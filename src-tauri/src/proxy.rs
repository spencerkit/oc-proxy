//! Module Overview
//! Proxy runtime bootstrap and lifecycle management for the Axum server.
//! Owns shared state objects and exposes start/stop/status/log access APIs.

use crate::log_store::LogStore;
use crate::models::{LogEntry, ProxyConfig, ProxyStatus};
use crate::stats_store::StatsStore;
use axum::routing::{get, post};
use axum::Router;
use reqwest::Client;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

mod net;
mod observability;
mod pipeline;
mod routing;
mod stream_bridge;

const MAX_REQUEST_BODY_BYTES: usize = 10 * 1024 * 1024;
const MAX_STREAM_LOG_BODY_BYTES: usize = 256 * 1024;
const NON_STREAM_REQUEST_TIMEOUT_MS: u64 = 60_000;
const MESSAGES_TO_RESPONSES_NON_STREAM_REQUEST_TIMEOUT_MS: u64 = 300_000;
const STREAM_REQUEST_TIMEOUT_MS: u64 = 600_000;
const UPSTREAM_CONNECT_TIMEOUT_MS: u64 = 10_000;

#[derive(Clone)]
pub struct ProxyRuntime {
    inner: Arc<ProxyRuntimeInner>,
}

struct ProxyRuntimeInner {
    config: Arc<RwLock<ProxyConfig>>,
    config_revision: Arc<AtomicU64>,
    route_index: Arc<RwLock<routing::RouteIndex>>,
    route_index_revision: Arc<AtomicU64>,
    log_store: LogStore,
    stats_store: StatsStore,
    metrics: Arc<observability::MetricsState>,
    server: Mutex<Option<RunningServer>>,
    client: Client,
}

struct RunningServer {
    address: String,
    lan_address: Option<String>,
    shutdown: Option<oneshot::Sender<()>>,
    handle: JoinHandle<()>,
}

#[derive(Clone)]
struct ServiceState {
    config: Arc<RwLock<ProxyConfig>>,
    config_revision: Arc<AtomicU64>,
    route_index: Arc<RwLock<routing::RouteIndex>>,
    route_index_revision: Arc<AtomicU64>,
    log_store: LogStore,
    stats_store: StatsStore,
    metrics: Arc<observability::MetricsState>,
    client: Client,
}

impl ProxyRuntime {
    /// Construct proxy runtime with shared config/stores and prebuilt route index.
    pub fn new(
        config: Arc<RwLock<ProxyConfig>>,
        config_revision: Arc<AtomicU64>,
        log_store: LogStore,
        stats_store: StatsStore,
    ) -> Result<Self, String> {
        let initial_route_index = config
            .read()
            .map_err(|_| "config lock poisoned".to_string())
            .map(|cfg| routing::build_route_index(&cfg))?;
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_millis(
                UPSTREAM_CONNECT_TIMEOUT_MS,
            ))
            .build()
            .map_err(|e| format!("create http client failed: {e}"))?;

        Ok(Self {
            inner: Arc::new(ProxyRuntimeInner {
                config,
                config_revision: config_revision.clone(),
                route_index: Arc::new(RwLock::new(initial_route_index)),
                route_index_revision: Arc::new(AtomicU64::new(
                    config_revision.load(Ordering::Acquire),
                )),
                log_store,
                stats_store,
                metrics: Arc::new(observability::MetricsState::new()),
                server: Mutex::new(None),
                client,
            }),
        })
    }

    /// Start Axum proxy server if not already running.
    pub async fn start(&self) -> Result<ProxyStatus, String> {
        if self.is_running() {
            return Ok(self.get_status());
        }

        let config = self
            .inner
            .config
            .read()
            .map_err(|_| "config lock poisoned".to_string())?
            .clone();

        let (listener, bound_host) =
            net::bind_proxy_listener(&config.server.host, config.server.port).await?;

        let (tx, rx) = oneshot::channel();
        let service_state = ServiceState {
            config: self.inner.config.clone(),
            config_revision: self.inner.config_revision.clone(),
            route_index: self.inner.route_index.clone(),
            route_index_revision: self.inner.route_index_revision.clone(),
            log_store: self.inner.log_store.clone(),
            stats_store: self.inner.stats_store.clone(),
            metrics: self.inner.metrics.clone(),
            client: self.inner.client.clone(),
        };

        let app = Router::new()
            .route("/healthz", get(pipeline::healthz))
            .route("/metrics-lite", get(pipeline::metrics_lite))
            .route("/oc/:group_id", post(pipeline::handle_proxy_root))
            .route("/oc/:group_id/*suffix", post(pipeline::handle_proxy_suffix))
            .with_state(service_state);

        self.inner.metrics.mark_started();

        let handle = tokio::spawn(async move {
            let server = axum::serve(listener, app);
            let graceful = server.with_graceful_shutdown(async {
                let _ = rx.await;
            });
            let _ = graceful.await;
        });

        let address = format!(
            "http://{}:{}",
            net::public_host_for_status(&bound_host),
            config.server.port
        );
        let lan_address = match bound_host.as_str() {
            "0.0.0.0" | "::" => {
                net::detect_local_ipv4().map(|ip| format!("http://{}:{}", ip, config.server.port))
            }
            _ => None,
        };
        let running = RunningServer {
            address,
            lan_address,
            shutdown: Some(tx),
            handle,
        };

        self.inner
            .server
            .lock()
            .map_err(|_| "server lock poisoned".to_string())?
            .replace(running);

        Ok(self.get_status())
    }

    /// Stop proxy server and return latest runtime status snapshot.
    pub async fn stop(&self) -> Result<ProxyStatus, String> {
        let running = self
            .inner
            .server
            .lock()
            .map_err(|_| "server lock poisoned".to_string())?
            .take();

        if let Some(mut srv) = running {
            if let Some(tx) = srv.shutdown.take() {
                let _ = tx.send(());
            }
            let _ =
                tokio::time::timeout(std::time::Duration::from_millis(2500), &mut srv.handle).await;
        }

        self.inner.metrics.mark_stopped();

        Ok(self.get_status())
    }

    /// Read lightweight runtime status including metrics and listen addresses.
    pub fn get_status(&self) -> ProxyStatus {
        let running_guard = self.inner.server.lock();
        let (running, address, lan_address) = if let Ok(guard) = running_guard {
            if let Some(srv) = guard.as_ref() {
                (true, Some(srv.address.clone()), srv.lan_address.clone())
            } else {
                (false, None, None)
            }
        } else {
            (false, None, None)
        };

        let metrics = self.inner.metrics.snapshot();

        ProxyStatus {
            running,
            address,
            lan_address,
            metrics,
        }
    }

    /// List in-memory request logs.
    pub fn list_logs(&self, max: usize) -> Vec<LogEntry> {
        self.inner.log_store.list(max)
    }

    /// Clear in-memory request logs.
    pub fn clear_logs(&self) {
        self.inner.log_store.clear();
    }

    /// Query aggregated stats from `StatsStore`.
    pub fn stats_summary(
        &self,
        hours: Option<u32>,
        rule_keys: Option<Vec<String>>,
        rule_key: Option<String>,
    ) -> crate::models::StatsSummaryResult {
        self.inner.stats_store.summarize(hours, rule_keys, rule_key)
    }

    /// Query compact rule-card stats for one group.
    pub fn stats_rule_cards(
        &self,
        group_id: String,
        hours: Option<u32>,
    ) -> Vec<crate::models::RuleCardStatsItem> {
        self.inner
            .stats_store
            .summarize_rule_cards(&group_id, hours)
    }

    /// Clear aggregated stats data.
    pub fn clear_stats(&self) -> Result<(), String> {
        self.inner.stats_store.clear()
    }

    fn is_running(&self) -> bool {
        self.inner
            .server
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::observability::{extract_token_usage, StreamTokenAccumulator};
    use super::pipeline::{build_upstream_body, resolve_request_timeout_ms};
    use super::routing::{
        detect_entry_protocol, resolve_target_model, resolve_upstream_path, EntryEndpoint,
        EntryProtocol, PathEntry,
    };
    use super::{
        MESSAGES_TO_RESPONSES_NON_STREAM_REQUEST_TIMEOUT_MS, NON_STREAM_REQUEST_TIMEOUT_MS,
    };
    use crate::models::{default_rule_quota_config, Group, Rule, RuleProtocol};
    use serde_json::{json, Value};
    use std::collections::HashMap;

    #[test]
    fn extract_token_usage_reads_nested_response_usage_payload() {
        let payload = json!({
            "response": {
                "usage": {
                    "input_tokens": 44,
                    "output_tokens": 12,
                    "cache_read_input_tokens": 9
                }
            }
        });

        let usage = extract_token_usage(&payload).expect("usage should exist");
        assert_eq!(usage.input_tokens, 44);
        assert_eq!(usage.output_tokens, 12);
        assert_eq!(usage.cache_read_tokens, 9);
        assert_eq!(usage.cache_write_tokens, 0);
    }

    #[test]
    fn extract_token_usage_maps_openai_prompt_and_cache_fields() {
        let payload = json!({
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 20,
                "prompt_tokens_details": {
                    "cached_tokens": 30,
                    "cache_creation_tokens": 5
                }
            }
        });

        let usage = extract_token_usage(&payload).expect("usage should exist");
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 20);
        assert_eq!(usage.cache_read_tokens, 30);
        assert_eq!(usage.cache_write_tokens, 5);
    }

    #[test]
    fn extract_token_usage_reads_message_and_delta_usage_payloads() {
        let message_payload = json!({
            "message": {
                "usage": {
                    "input_tokens": 7,
                    "output_tokens": 3
                }
            }
        });
        let delta_payload = json!({
            "delta": {
                "usage": {
                    "input_tokens": 11,
                    "output_tokens": 4
                }
            }
        });

        let message_usage =
            extract_token_usage(&message_payload).expect("message usage should exist");
        assert_eq!(message_usage.input_tokens, 7);
        assert_eq!(message_usage.output_tokens, 3);

        let delta_usage = extract_token_usage(&delta_payload).expect("delta usage should exist");
        assert_eq!(delta_usage.input_tokens, 11);
        assert_eq!(delta_usage.output_tokens, 4);
    }

    #[test]
    fn stream_token_accumulator_aggregates_usage_from_chunked_sse() {
        let mut acc = StreamTokenAccumulator::default();
        acc.consume_chunk(
            b"event: message_delta\ndata: {\"usage\":{\"input_tokens\":7,\"output_tokens\":3}}\n\n",
        );
        acc.consume_chunk(
            b"data: {\"usage\":{\"input_tokens\":9,\"output_tokens\":3,\"cache_read_input_tokens\":1}}\n",
        );
        acc.consume_chunk(
            b"\ndata: {\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":4,\"prompt_tokens_details\":{\"cache_creation_tokens\":2}}}\n\n",
        );
        acc.consume_chunk(b"data: [DONE]\n\n");

        let usage = acc.into_token_usage().expect("usage should be captured");
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 4);
        assert_eq!(usage.cache_read_tokens, 1);
        assert_eq!(usage.cache_write_tokens, 2);
    }

    #[test]
    fn stream_token_accumulator_returns_none_when_usage_missing() {
        let mut acc = StreamTokenAccumulator::default();
        acc.consume_chunk(b"event: ping\ndata: hello\n\n");
        acc.consume_chunk(b": keep-alive\n\n");
        assert!(acc.into_token_usage().is_none());
    }

    #[test]
    fn resolve_target_model_uses_group_and_rule_mapping() {
        let mut mappings = HashMap::new();
        mappings.insert("m1".to_string(), "gpt-x".to_string());
        let rule = Rule {
            id: "r1".to_string(),
            name: "rule".to_string(),
            protocol: RuleProtocol::Openai,
            token: "t".to_string(),
            api_address: "https://api.example.com".to_string(),
            default_model: "fallback".to_string(),
            model_mappings: mappings,
            quota: default_rule_quota_config(),
        };
        let group = Group {
            id: "g1".to_string(),
            name: "Group".to_string(),
            models: vec!["m1".to_string()],
            active_rule_id: Some("r1".to_string()),
            rules: vec![rule.clone()],
        };

        let model = resolve_target_model(&rule, &group.models, &json!({ "model": "m1" }));
        assert_eq!(model, "gpt-x");
    }

    #[test]
    fn resolve_target_model_falls_back_to_default_model_when_unmatched() {
        let rule = Rule {
            id: "r1".to_string(),
            name: "rule".to_string(),
            protocol: RuleProtocol::Openai,
            token: "t".to_string(),
            api_address: "https://api.example.com".to_string(),
            default_model: "fallback".to_string(),
            model_mappings: HashMap::new(),
            quota: default_rule_quota_config(),
        };
        let group = Group {
            id: "g1".to_string(),
            name: "Group".to_string(),
            models: vec!["m1".to_string()],
            active_rule_id: Some("r1".to_string()),
            rules: vec![rule.clone()],
        };

        let model = resolve_target_model(&rule, &group.models, &json!({ "model": "unknown" }));
        assert_eq!(model, "fallback");
    }

    #[test]
    fn build_upstream_body_passes_through_request_shape_with_target_model() {
        let entry = PathEntry {
            protocol: EntryProtocol::Openai,
            endpoint: EntryEndpoint::Responses,
        };
        let out = build_upstream_body(
            &entry,
            &RuleProtocol::Openai,
            &json!({
                "model": "gpt-4.1",
                "input": "hello from responses"
            }),
            false,
            "gpt-4.1",
        )
        .expect("mapping should succeed");

        assert_eq!(out["model"], "gpt-4.1");
        assert_eq!(out["input"], "hello from responses");
    }

    #[test]
    fn build_upstream_body_forces_non_stream_for_messages_to_responses() {
        let entry = PathEntry {
            protocol: EntryProtocol::Anthropic,
            endpoint: EntryEndpoint::Messages,
        };
        let out = build_upstream_body(
            &entry,
            &RuleProtocol::Openai,
            &json!({
                "model": "claude-3-5-sonnet",
                "stream": true,
                "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }]
            }),
            true,
            "gpt-4.1",
        )
        .expect("mapping should succeed");

        assert_eq!(out["model"], "gpt-4.1");
        assert_eq!(out["stream"], false);
        assert_eq!(out["input"][0]["type"], "message");
    }

    #[test]
    fn build_upstream_body_drops_max_output_tokens_for_messages_to_responses() {
        let entry = PathEntry {
            protocol: EntryProtocol::Anthropic,
            endpoint: EntryEndpoint::Messages,
        };
        let out = build_upstream_body(
            &entry,
            &RuleProtocol::Openai,
            &json!({
                "model": "claude-3-5-sonnet",
                "max_tokens": 1234,
                "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }]
            }),
            true,
            "gpt-4.1",
        )
        .expect("mapping should succeed");

        assert_eq!(out["model"], "gpt-4.1");
        assert!(out.get("max_output_tokens").is_none());
    }

    #[test]
    fn build_upstream_body_sets_auto_tool_choice_for_messages_to_responses_when_tools_exist() {
        let entry = PathEntry {
            protocol: EntryProtocol::Anthropic,
            endpoint: EntryEndpoint::Messages,
        };
        let out = build_upstream_body(
            &entry,
            &RuleProtocol::Openai,
            &json!({
                "model": "claude-3-5-sonnet",
                "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }],
                "tools": [{
                    "name": "Read",
                    "input_schema": {
                        "type": "object",
                        "properties": {
                            "file_path": { "type": "string" }
                        },
                        "required": ["file_path"]
                    }
                }]
            }),
            true,
            "gpt-4.1",
        )
        .expect("mapping should succeed");

        assert_eq!(out["tool_choice"], "auto");
        assert_eq!(out["parallel_tool_calls"], true);
    }

    #[test]
    fn build_upstream_body_defaults_stream_true_when_anthropic_stream_missing() {
        let entry = PathEntry {
            protocol: EntryProtocol::Anthropic,
            endpoint: EntryEndpoint::Messages,
        };
        let out = build_upstream_body(
            &entry,
            &RuleProtocol::OpenaiCompletion,
            &json!({
                "model": "claude-3-5-sonnet",
                "messages": [{ "role": "user", "content": [{ "type": "text", "text": "hello" }] }]
            }),
            true,
            "gpt-4.1",
        )
        .expect("mapping should succeed");

        assert_eq!(out["model"], "gpt-4.1");
        assert_eq!(out["stream"], true);
    }

    #[test]
    fn resolve_request_timeout_ms_extends_messages_to_responses_non_stream_requests() {
        let entry = PathEntry {
            protocol: EntryProtocol::Anthropic,
            endpoint: EntryEndpoint::Messages,
        };

        let timeout = resolve_request_timeout_ms(false, &entry, &RuleProtocol::Openai);
        assert_eq!(timeout, MESSAGES_TO_RESPONSES_NON_STREAM_REQUEST_TIMEOUT_MS);
    }

    #[test]
    fn resolve_request_timeout_ms_keeps_default_for_other_non_stream_requests() {
        let entry = PathEntry {
            protocol: EntryProtocol::Anthropic,
            endpoint: EntryEndpoint::Messages,
        };

        let timeout = resolve_request_timeout_ms(false, &entry, &RuleProtocol::OpenaiCompletion);
        assert_eq!(timeout, NON_STREAM_REQUEST_TIMEOUT_MS);
    }

    #[test]
    fn detect_entry_protocol_supports_v1_and_non_v1_downstream_paths() {
        assert!(detect_entry_protocol("/chat/completions").is_some());
        assert!(detect_entry_protocol("/responses").is_some());
        assert!(detect_entry_protocol("/messages").is_some());
        assert!(detect_entry_protocol("/v1/chat/completions").is_some());
        assert!(detect_entry_protocol("/v1/responses").is_some());
        assert!(detect_entry_protocol("/v1/messages").is_some());
    }

    #[test]
    fn resolve_upstream_path_uses_rule_protocol_enum_directly() {
        assert_eq!(
            resolve_upstream_path(&RuleProtocol::Anthropic),
            "/v1/messages"
        );
        assert_eq!(resolve_upstream_path(&RuleProtocol::Openai), "/responses");
        assert_eq!(
            resolve_upstream_path(&RuleProtocol::OpenaiCompletion),
            "/chat/completions"
        );
    }

    #[test]
    fn contract_extract_token_usage_snapshot() {
        let input: Value = serde_json::from_str(include_str!(
            "contract_fixtures/proxy/extract_token_usage.input.json"
        ))
        .expect("contract input must be valid json");
        let expected: Value = serde_json::from_str(include_str!(
            "contract_fixtures/proxy/extract_token_usage.expected.json"
        ))
        .expect("contract expected must be valid json");

        let usage = extract_token_usage(&input).expect("usage should exist");
        let actual = json!({
            "input_tokens": usage.input_tokens,
            "output_tokens": usage.output_tokens,
            "cache_read_tokens": usage.cache_read_tokens,
            "cache_write_tokens": usage.cache_write_tokens,
        });
        assert_eq!(actual, expected);
    }
}
