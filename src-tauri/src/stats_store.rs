//! Module Overview
//! Aggregated statistics store derived from request logs.
//! Maintains hourly buckets, persistence, retention pruning, and summary query APIs.

use crate::models::{
    ComparisonSummary, HourlyStatsPoint, LogEntry, RuleCardHourlyPoint, RuleCardStatsItem,
    StatsBreakdowns, StatsCountBreakdownItem, StatsRuleCountBreakdownItem, StatsRuleOption,
    StatsRuleTokenBreakdownItem, StatsSummaryResult, StatsTokenBreakdownItem,
};
use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration as StdDuration;

const RETENTION_DAYS: i64 = 90;
const DEFAULT_HOURS: u32 = 24;
const MAX_HOURS: u32 = 24 * 90;
const FLUSH_INTERVAL_MS: u64 = 1000;
const PERSISTED_STATS_VERSION: u8 = 2;

#[derive(Clone)]
pub struct StatsStore {
    file_path: PathBuf,
    inner: Arc<Mutex<HashMap<String, StatsBucket>>>,
    dirty: Arc<AtomicBool>,
    worker_started: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatsBucket {
    hour: String,
    group_id: Option<String>,
    group_name: Option<String>,
    rule_id: Option<String>,
    rule_name: Option<String>,
    entry_protocol: Option<String>,
    downstream_protocol: Option<String>,
    http_status: Option<u16>,
    requests: u64,
    errors: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedStats {
    version: u8,
    buckets: Vec<StatsBucket>,
}

#[derive(Debug, Clone)]
enum RuleSelection {
    All,
    Empty,
    Selected(HashSet<String>),
}

#[derive(Debug, Default, Clone)]
struct RuleCardAccumulator {
    requests: u64,
    input_tokens: u64,
    output_tokens: u64,
    hourly: BTreeMap<String, RuleCardHourlyPoint>,
}

#[derive(Debug, Clone, Copy)]
enum StatsDimension {
    Rule,
    Protocol,
    Status,
}

impl StatsDimension {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Protocol => "protocol",
            Self::Status => "status",
        }
    }
}

#[derive(Debug, Default, Clone)]
struct WindowAggregate {
    requests: u64,
    errors: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    hourly: BTreeMap<String, HourlyStatsPoint>,
    errors_by_status: HashMap<String, u64>,
    requests_by_protocol: HashMap<String, u64>,
    tokens_by_protocol: HashMap<String, u64>,
    requests_by_rule: HashMap<String, (String, u64)>,
    tokens_by_rule: HashMap<String, (String, u64)>,
}

impl StatsStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            inner: Arc::new(Mutex::new(HashMap::new())),
            dirty: Arc::new(AtomicBool::new(false)),
            worker_started: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Initialize stats storage from disk and start background flush worker.
    pub fn initialize(&self) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create stats dir failed: {e}"))?;
        }

        if !self.file_path.exists() {
            self.persist_locked(&HashMap::new())?;
            self.start_flush_worker();
            return Ok(());
        }

        let raw = std::fs::read_to_string(&self.file_path)
            .map_err(|e| format!("read stats file failed: {e}"))?;
        let parsed = serde_json::from_str::<PersistedStats>(&raw).unwrap_or(PersistedStats {
            version: PERSISTED_STATS_VERSION,
            buckets: vec![],
        });

        let mut next = HashMap::new();
        for bucket in parsed.buckets {
            if bucket.hour.trim().is_empty() {
                continue;
            }
            let key = bucket_key(
                &bucket.hour,
                bucket.group_id.as_deref(),
                bucket.rule_id.as_deref(),
                bucket.downstream_protocol.as_deref(),
                bucket.entry_protocol.as_deref(),
                bucket.http_status,
            );
            next.insert(key, bucket);
        }

        prune_old_locked(&mut next);
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| "stats lock poisoned".to_string())?;
            *guard = next.clone();
        }
        self.persist_locked(&next)?;
        self.start_flush_worker();
        Ok(())
    }

    /// Aggregate one finalized request log into hourly counters.
    ///
    /// Only `/oc/*` entries are included in proxy stats.
    pub fn append_log(&self, entry: &LogEntry) {
        if !entry.request_path.starts_with("/oc/") {
            return;
        }
        let Some(hour) = normalize_hour(&entry.timestamp) else {
            return;
        };

        {
            let mut guard = match self.inner.lock() {
                Ok(v) => v,
                Err(_) => return,
            };
            prune_old_locked(&mut guard);

            let key = bucket_key(
                &hour,
                entry.group_path.as_deref(),
                entry.rule_id.as_deref(),
                entry.downstream_protocol.as_deref(),
                entry.entry_protocol.as_deref(),
                entry.http_status,
            );
            let bucket = guard.entry(key).or_insert_with(|| StatsBucket {
                hour: hour.clone(),
                group_id: entry.group_path.clone(),
                group_name: entry.group_name.clone(),
                rule_id: entry.rule_id.clone(),
                rule_name: None,
                entry_protocol: entry.entry_protocol.clone(),
                downstream_protocol: entry.downstream_protocol.clone(),
                http_status: entry.http_status,
                requests: 0,
                errors: 0,
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            });

            bucket.requests += 1;
            if entry.status != "ok" {
                bucket.errors += 1;
            }
            if let Some(usage) = &entry.token_usage {
                bucket.input_tokens += usage.input_tokens;
                bucket.output_tokens += usage.output_tokens;
                bucket.cache_read_tokens += usage.cache_read_tokens;
                bucket.cache_write_tokens += usage.cache_write_tokens;
            }
        }

        self.dirty.store(true, Ordering::Release);
    }

    /// Build summary for a time window and optional rule filters.
    ///
    /// `rule_keys` supports multi-select semantics:
    /// - `None`: all rules
    /// - `Some([])`: empty selection (returns zero summary)
    /// - `Some([..])`: selected rules
    /// `rule_key` is kept for backward compatibility.
    pub fn summarize(
        &self,
        hours: Option<u32>,
        rule_keys: Option<Vec<String>>,
        rule_key: Option<String>,
        dimension: Option<String>,
        enable_comparison: Option<bool>,
    ) -> StatsSummaryResult {
        let requested_hours = hours.unwrap_or(DEFAULT_HOURS).clamp(1, MAX_HOURS);
        let dimension = normalize_dimension(dimension.as_deref());
        let selection = if matches!(dimension, StatsDimension::Rule) {
            normalize_rule_selection(rule_keys, rule_key.as_deref())
        } else {
            RuleSelection::All
        };
        let normalized_rule_keys = if matches!(dimension, StatsDimension::Rule) {
            selection_to_rule_keys(&selection)
        } else {
            None
        };
        let now = Utc::now();
        let window_start = now - Duration::hours(requested_hours as i64);
        let enable_comparison = enable_comparison.unwrap_or(false);

        let guard = match self.inner.lock() {
            Ok(v) => v,
            Err(_) => {
                return StatsSummaryResult {
                    dimension: dimension.as_str().to_string(),
                    hours: requested_hours,
                    rule_key,
                    rule_keys: normalized_rule_keys,
                    requests: 0,
                    errors: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    rpm: 0.0,
                    input_tpm: 0.0,
                    output_tpm: 0.0,
                    peak_rpm: 0.0,
                    peak_input_tpm: 0.0,
                    peak_output_tpm: 0.0,
                    comparison: if enable_comparison {
                        Some(ComparisonSummary {
                            requests_delta_pct: 0.0,
                            errors_delta_pct: 0.0,
                            rpm_delta_pct: 0.0,
                            input_tpm_delta_pct: 0.0,
                            output_tpm_delta_pct: 0.0,
                        })
                    } else {
                        None
                    },
                    breakdowns: Some(StatsBreakdowns {
                        errors_by_status: vec![],
                        requests_by_protocol: vec![],
                        tokens_by_protocol: vec![],
                        requests_by_rule: vec![],
                        tokens_by_rule: vec![],
                    }),
                    hourly: vec![],
                    options: vec![],
                }
            }
        };

        let mut options_map: BTreeMap<String, StatsRuleOption> = BTreeMap::new();

        for bucket in guard.values() {
            let Some(bucket_time) = parse_ts(&bucket.hour) else {
                continue;
            };
            if bucket_time < retention_cutoff() {
                continue;
            }

            if let (Some(group), Some(rule)) = (&bucket.group_id, &bucket.rule_id) {
                let option_key = format!("{group}::{rule}");
                let group_label = bucket.group_name.clone().unwrap_or_else(|| group.clone());
                let rule_label = bucket.rule_name.clone().unwrap_or_else(|| rule.clone());
                let label = format!("{group_label}-{rule_label}");
                options_map
                    .entry(option_key.clone())
                    .or_insert(StatsRuleOption {
                        key: option_key,
                        label,
                        group_id: group.clone(),
                        rule_id: rule.clone(),
                    });
            }
        }

        let current = aggregate_window(&guard, window_start, now, &selection, dimension);
        let (peak_rpm, peak_input_tpm, peak_output_tpm) = compute_peaks(&current.hourly);
        let rpm = rate_metric(current.requests, requested_hours);
        let input_tpm = rate_metric(current.input_tokens, requested_hours);
        let output_tpm = rate_metric(current.output_tokens, requested_hours);

        let comparison = if enable_comparison {
            let previous_start = window_start - Duration::hours(requested_hours as i64);
            let previous =
                aggregate_window(&guard, previous_start, window_start, &selection, dimension);
            Some(ComparisonSummary {
                requests_delta_pct: pct_delta(current.requests as f64, previous.requests as f64),
                errors_delta_pct: pct_delta(current.errors as f64, previous.errors as f64),
                rpm_delta_pct: pct_delta(rpm, rate_metric(previous.requests, requested_hours)),
                input_tpm_delta_pct: pct_delta(
                    input_tpm,
                    rate_metric(previous.input_tokens, requested_hours),
                ),
                output_tpm_delta_pct: pct_delta(
                    output_tpm,
                    rate_metric(previous.output_tokens, requested_hours),
                ),
            })
        } else {
            None
        };

        StatsSummaryResult {
            dimension: dimension.as_str().to_string(),
            hours: requested_hours,
            rule_key,
            rule_keys: normalized_rule_keys,
            requests: current.requests,
            errors: current.errors,
            input_tokens: current.input_tokens,
            output_tokens: current.output_tokens,
            cache_read_tokens: current.cache_read_tokens,
            cache_write_tokens: current.cache_write_tokens,
            rpm,
            input_tpm,
            output_tpm,
            peak_rpm,
            peak_input_tpm,
            peak_output_tpm,
            comparison,
            breakdowns: Some(build_breakdowns(&current)),
            hourly: current.hourly.into_values().collect(),
            options: options_map.into_values().collect(),
        }
    }

    /// Build compact per-rule stats for service-page rule cards in one group.
    pub fn summarize_rule_cards(
        &self,
        group_id: &str,
        hours: Option<u32>,
    ) -> Vec<RuleCardStatsItem> {
        let normalized_group_id = group_id.trim();
        if normalized_group_id.is_empty() {
            return vec![];
        }

        let requested_hours = hours.unwrap_or(DEFAULT_HOURS).clamp(1, MAX_HOURS);
        let cutoff = Utc::now() - Duration::hours(requested_hours as i64);

        let guard = match self.inner.lock() {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let mut map: BTreeMap<String, RuleCardAccumulator> = BTreeMap::new();
        for bucket in guard.values() {
            let Some(bucket_time) = parse_ts(&bucket.hour) else {
                continue;
            };
            if bucket_time < retention_cutoff() || bucket_time < cutoff {
                continue;
            }

            if bucket.group_id.as_deref() != Some(normalized_group_id) {
                continue;
            }
            let Some(rule_id) = bucket.rule_id.as_ref() else {
                continue;
            };

            let acc = map.entry(rule_id.clone()).or_default();
            acc.requests += bucket.requests;
            acc.input_tokens += bucket.input_tokens;
            acc.output_tokens += bucket.output_tokens;

            let point =
                acc.hourly
                    .entry(bucket.hour.clone())
                    .or_insert_with(|| RuleCardHourlyPoint {
                        hour: bucket.hour.clone(),
                        requests: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        tokens: 0,
                    });
            point.requests += bucket.requests;
            point.input_tokens += bucket.input_tokens;
            point.output_tokens += bucket.output_tokens;
            point.tokens = point.input_tokens + point.output_tokens;
        }

        map.into_iter()
            .map(|(rule_id, acc)| RuleCardStatsItem {
                group_id: normalized_group_id.to_string(),
                rule_id,
                requests: acc.requests,
                input_tokens: acc.input_tokens,
                output_tokens: acc.output_tokens,
                tokens: acc.input_tokens + acc.output_tokens,
                hourly: acc.hourly.into_values().collect(),
            })
            .collect()
    }

    /// Clear in-memory and persisted stats data.
    pub fn clear(&self) -> Result<(), String> {
        {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| "stats lock poisoned".to_string())?;
            guard.clear();
        }
        self.flush_now()
    }

    pub fn flush_now(&self) -> Result<(), String> {
        let snapshot = {
            let guard = self
                .inner
                .lock()
                .map_err(|_| "stats lock poisoned".to_string())?;
            guard.clone()
        };
        self.persist_locked(&snapshot)?;
        self.dirty.store(false, Ordering::Release);
        Ok(())
    }

    fn flush_if_dirty(&self) {
        if !self.dirty.swap(false, Ordering::AcqRel) {
            return;
        }
        let snapshot = match self.inner.lock() {
            Ok(v) => v.clone(),
            Err(_) => return,
        };
        let _ = self.persist_locked(&snapshot);
    }

    fn start_flush_worker(&self) {
        if self.worker_started.swap(true, Ordering::AcqRel) {
            return;
        }
        let store = self.clone();
        let _ = thread::Builder::new()
            .name("stats-flush-worker".to_string())
            .spawn(move || loop {
                thread::sleep(StdDuration::from_millis(FLUSH_INTERVAL_MS));
                store.flush_if_dirty();
            });
    }

    fn persist_locked(&self, data: &HashMap<String, StatsBucket>) -> Result<(), String> {
        let payload = PersistedStats {
            version: PERSISTED_STATS_VERSION,
            buckets: data.values().cloned().collect(),
        };
        let text = serde_json::to_string_pretty(&payload)
            .map_err(|e| format!("serialize stats failed: {e}"))?;
        std::fs::write(&self.file_path, text).map_err(|e| format!("write stats file failed: {e}"))
    }
}

fn normalize_rule_selection(
    rule_keys: Option<Vec<String>>,
    legacy_rule_key: Option<&str>,
) -> RuleSelection {
    if let Some(rule_keys) = rule_keys {
        if rule_keys.is_empty() {
            return RuleSelection::Empty;
        }
        let set: HashSet<String> = rule_keys
            .into_iter()
            .filter_map(|key| normalize_rule_key(&key))
            .collect();
        if set.is_empty() {
            RuleSelection::Empty
        } else {
            RuleSelection::Selected(set)
        }
    } else if let Some(single) = legacy_rule_key.and_then(normalize_rule_key) {
        let mut set = HashSet::new();
        set.insert(single);
        RuleSelection::Selected(set)
    } else {
        RuleSelection::All
    }
}

fn normalize_rule_key(rule_key: &str) -> Option<String> {
    let mut parts = rule_key.splitn(2, "::");
    let group = parts.next().unwrap_or_default().trim();
    let rule = parts.next().unwrap_or_default().trim();
    if group.is_empty() || rule.is_empty() {
        return None;
    }
    Some(format!("{group}::{rule}"))
}

fn selection_to_rule_keys(selection: &RuleSelection) -> Option<Vec<String>> {
    match selection {
        RuleSelection::All => None,
        RuleSelection::Empty => Some(vec![]),
        RuleSelection::Selected(set) => {
            let mut items: Vec<String> = set.iter().cloned().collect();
            items.sort();
            Some(items)
        }
    }
}

fn normalize_dimension(dimension: Option<&str>) -> StatsDimension {
    match dimension.unwrap_or_default().trim() {
        "protocol" => StatsDimension::Protocol,
        "status" => StatsDimension::Status,
        _ => StatsDimension::Rule,
    }
}

fn rate_metric(total: u64, hours: u32) -> f64 {
    let minutes = (hours as f64) * 60.0;
    if minutes <= 0.0 {
        0.0
    } else {
        total as f64 / minutes
    }
}

fn pct_delta(current: f64, previous: f64) -> f64 {
    if previous.abs() <= f64::EPSILON {
        if current.abs() <= f64::EPSILON {
            0.0
        } else {
            100.0
        }
    } else {
        ((current - previous) / previous.abs()) * 100.0
    }
}

fn normalize_protocol(value: Option<&str>) -> String {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn normalize_http_status(value: Option<u16>) -> String {
    value
        .map(|status| status.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn compute_peaks(hourly: &BTreeMap<String, HourlyStatsPoint>) -> (f64, f64, f64) {
    let mut peak_rpm: f64 = 0.0;
    let mut peak_input_tpm: f64 = 0.0;
    let mut peak_output_tpm: f64 = 0.0;

    for point in hourly.values() {
        peak_rpm = peak_rpm.max(point.requests as f64 / 60.0);
        peak_input_tpm = peak_input_tpm.max(point.input_tokens as f64 / 60.0);
        peak_output_tpm = peak_output_tpm.max(point.output_tokens as f64 / 60.0);
    }

    (peak_rpm, peak_input_tpm, peak_output_tpm)
}

fn aggregate_window(
    data: &HashMap<String, StatsBucket>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    selection: &RuleSelection,
    dimension: StatsDimension,
) -> WindowAggregate {
    let mut aggregate = WindowAggregate::default();
    let cutoff = retention_cutoff();

    for bucket in data.values() {
        let Some(bucket_time) = parse_ts(&bucket.hour) else {
            continue;
        };
        if bucket_time < cutoff || bucket_time < start || bucket_time >= end {
            continue;
        }
        if !should_include_bucket(bucket, selection) {
            continue;
        }

        aggregate.requests += bucket.requests;
        aggregate.errors += bucket.errors;
        aggregate.input_tokens += bucket.input_tokens;
        aggregate.output_tokens += bucket.output_tokens;
        aggregate.cache_read_tokens += bucket.cache_read_tokens;
        aggregate.cache_write_tokens += bucket.cache_write_tokens;

        let point = aggregate
            .hourly
            .entry(bucket.hour.clone())
            .or_insert_with(|| HourlyStatsPoint {
                hour: bucket.hour.clone(),
                requests: 0,
                errors: 0,
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
            });
        point.requests += bucket.requests;
        point.errors += bucket.errors;
        point.input_tokens += bucket.input_tokens;
        point.output_tokens += bucket.output_tokens;
        point.cache_read_tokens += bucket.cache_read_tokens;
        point.cache_write_tokens += bucket.cache_write_tokens;

        let protocol_key = normalize_protocol(bucket.downstream_protocol.as_deref());
        *aggregate
            .requests_by_protocol
            .entry(protocol_key.clone())
            .or_insert(0) += bucket.requests;
        *aggregate
            .tokens_by_protocol
            .entry(protocol_key.clone())
            .or_insert(0) += bucket.input_tokens + bucket.output_tokens;

        if bucket.errors > 0 {
            let status_key = normalize_http_status(bucket.http_status);
            let error_key = if matches!(dimension, StatsDimension::Protocol) {
                format!("{protocol_key} · {status_key}")
            } else {
                status_key
            };
            *aggregate.errors_by_status.entry(error_key).or_insert(0) += bucket.errors;
        }

        if let (Some(group), Some(rule)) = (&bucket.group_id, &bucket.rule_id) {
            let key = format!("{group}::{rule}");
            let group_label = bucket.group_name.as_deref().unwrap_or(group);
            let rule_label = bucket.rule_name.as_deref().unwrap_or(rule);
            let label = format!("{group_label}-{rule_label}");

            let request_entry = aggregate
                .requests_by_rule
                .entry(key.clone())
                .or_insert_with(|| (label.clone(), 0));
            request_entry.1 += bucket.requests;

            let token_entry = aggregate
                .tokens_by_rule
                .entry(key)
                .or_insert_with(|| (label, 0));
            token_entry.1 += bucket.input_tokens + bucket.output_tokens;
        }
    }

    aggregate
}

fn build_breakdowns(aggregate: &WindowAggregate) -> StatsBreakdowns {
    StatsBreakdowns {
        errors_by_status: build_count_breakdown(&aggregate.errors_by_status, aggregate.errors),
        requests_by_protocol: build_count_breakdown(
            &aggregate.requests_by_protocol,
            aggregate.requests,
        ),
        tokens_by_protocol: build_token_breakdown(
            &aggregate.tokens_by_protocol,
            aggregate.input_tokens + aggregate.output_tokens,
        ),
        requests_by_rule: build_ranked_count_breakdown(
            &aggregate.requests_by_rule,
            aggregate.requests,
        ),
        tokens_by_rule: build_ranked_token_breakdown(
            &aggregate.tokens_by_rule,
            aggregate.input_tokens + aggregate.output_tokens,
        ),
    }
}

fn build_count_breakdown(
    values: &HashMap<String, u64>,
    total: u64,
) -> Vec<StatsCountBreakdownItem> {
    let mut items: Vec<StatsCountBreakdownItem> = values
        .iter()
        .map(|(key, count)| StatsCountBreakdownItem {
            key: key.clone(),
            count: *count,
            ratio: if total == 0 {
                0.0
            } else {
                (*count as f64) / (total as f64)
            },
        })
        .collect();
    items.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
    items
}

fn build_token_breakdown(
    values: &HashMap<String, u64>,
    total: u64,
) -> Vec<StatsTokenBreakdownItem> {
    let mut items: Vec<StatsTokenBreakdownItem> = values
        .iter()
        .map(|(key, tokens)| StatsTokenBreakdownItem {
            key: key.clone(),
            tokens: *tokens,
            ratio: if total == 0 {
                0.0
            } else {
                (*tokens as f64) / (total as f64)
            },
        })
        .collect();
    items.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.key.cmp(&b.key)));
    items
}

fn build_ranked_count_breakdown(
    values: &HashMap<String, (String, u64)>,
    total: u64,
) -> Vec<StatsRuleCountBreakdownItem> {
    let mut items: Vec<StatsRuleCountBreakdownItem> = values
        .iter()
        .map(|(key, (label, count))| StatsRuleCountBreakdownItem {
            key: key.clone(),
            label: label.clone(),
            count: *count,
            ratio: if total == 0 {
                0.0
            } else {
                (*count as f64) / (total as f64)
            },
        })
        .collect();
    items.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
    items
}

fn build_ranked_token_breakdown(
    values: &HashMap<String, (String, u64)>,
    total: u64,
) -> Vec<StatsRuleTokenBreakdownItem> {
    let mut items: Vec<StatsRuleTokenBreakdownItem> = values
        .iter()
        .map(|(key, (label, tokens))| StatsRuleTokenBreakdownItem {
            key: key.clone(),
            label: label.clone(),
            tokens: *tokens,
            ratio: if total == 0 {
                0.0
            } else {
                (*tokens as f64) / (total as f64)
            },
        })
        .collect();
    items.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.key.cmp(&b.key)));
    items
}

fn should_include_bucket(bucket: &StatsBucket, selection: &RuleSelection) -> bool {
    match selection {
        RuleSelection::All => true,
        RuleSelection::Empty => false,
        RuleSelection::Selected(set) => {
            let (Some(group_id), Some(rule_id)) = (&bucket.group_id, &bucket.rule_id) else {
                return false;
            };
            let key = format!("{group_id}::{rule_id}");
            set.contains(&key)
        }
    }
}

fn normalize_hour(ts: &str) -> Option<String> {
    let mut dt = parse_ts(ts)?;
    dt = dt.with_minute(0)?.with_second(0)?.with_nanosecond(0)?;
    Some(dt.to_rfc3339())
}

fn parse_ts(ts: &str) -> Option<DateTime<Utc>> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn retention_cutoff() -> DateTime<Utc> {
    Utc::now() - Duration::days(RETENTION_DAYS)
}

fn prune_old_locked(data: &mut HashMap<String, StatsBucket>) {
    let cutoff = retention_cutoff();
    data.retain(|_, bucket| {
        parse_ts(&bucket.hour)
            .map(|dt| dt >= cutoff)
            .unwrap_or(false)
    });
}

fn bucket_key(
    hour: &str,
    group_id: Option<&str>,
    rule_id: Option<&str>,
    downstream_protocol: Option<&str>,
    entry_protocol: Option<&str>,
    http_status: Option<u16>,
) -> String {
    format!(
        "{}::{}::{}::{}::{}::{}",
        hour,
        group_id.unwrap_or("_"),
        rule_id.unwrap_or("_"),
        downstream_protocol.unwrap_or("_"),
        entry_protocol.unwrap_or("_"),
        http_status
            .map(|status| status.to_string())
            .unwrap_or_else(|| "_".to_string())
    )
}
