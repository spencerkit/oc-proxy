use crate::models::{HourlyStatsPoint, LogEntry, StatsRuleOption, StatsSummaryResult};
use chrono::{DateTime, Duration, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const RETENTION_DAYS: i64 = 90;
const DEFAULT_HOURS: u32 = 24;
const MAX_HOURS: u32 = 24 * 90;

#[derive(Clone)]
pub struct StatsStore {
    file_path: PathBuf,
    inner: Arc<Mutex<HashMap<String, StatsBucket>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatsBucket {
    hour: String,
    group_id: Option<String>,
    group_name: Option<String>,
    rule_id: Option<String>,
    rule_name: Option<String>,
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

impl StatsStore {
    pub fn new(file_path: PathBuf) -> Self {
        Self {
            file_path,
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn initialize(&self) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create stats dir failed: {e}"))?;
        }

        if !self.file_path.exists() {
            self.persist_locked(&HashMap::new())?;
            return Ok(());
        }

        let raw =
            std::fs::read_to_string(&self.file_path).map_err(|e| format!("read stats file failed: {e}"))?;
        let parsed = serde_json::from_str::<PersistedStats>(&raw).unwrap_or(PersistedStats {
            version: 1,
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
            );
            next.insert(key, bucket);
        }

        prune_old_locked(&mut next);
        {
            let mut guard = self.inner.lock().map_err(|_| "stats lock poisoned".to_string())?;
            *guard = next.clone();
        }
        self.persist_locked(&next)?;
        Ok(())
    }

    pub fn append_log(&self, entry: &LogEntry) {
        if !entry.request_path.starts_with("/oc/") {
            return;
        }
        let Some(hour) = normalize_hour(&entry.timestamp) else {
            return;
        };

        let next_snapshot = {
            let mut guard = match self.inner.lock() {
                Ok(v) => v,
                Err(_) => return,
            };
            prune_old_locked(&mut guard);

            let key = bucket_key(&hour, entry.group_path.as_deref(), entry.rule_id.as_deref());
            let bucket = guard.entry(key).or_insert_with(|| StatsBucket {
                hour: hour.clone(),
                group_id: entry.group_path.clone(),
                group_name: entry.group_name.clone(),
                rule_id: entry.rule_id.clone(),
                rule_name: None,
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
            guard.clone()
        };

        let _ = self.persist_locked(&next_snapshot);
    }

    pub fn summarize(&self, hours: Option<u32>, rule_key: Option<String>) -> StatsSummaryResult {
        let requested_hours = hours.unwrap_or(DEFAULT_HOURS).clamp(1, MAX_HOURS);
        let cutoff = Utc::now() - Duration::hours(requested_hours as i64);
        let (rule_group, rule_id) = parse_rule_key(rule_key.as_deref());

        let guard = match self.inner.lock() {
            Ok(v) => v,
            Err(_) => {
                return StatsSummaryResult {
                    hours: requested_hours,
                    rule_key,
                    requests: 0,
                    errors: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    hourly: vec![],
                    options: vec![],
                }
            }
        };

        let mut requests = 0u64;
        let mut errors = 0u64;
        let mut input_tokens = 0u64;
        let mut output_tokens = 0u64;
        let mut cache_read_tokens = 0u64;
        let mut cache_write_tokens = 0u64;
        let mut hourly_map: BTreeMap<String, HourlyStatsPoint> = BTreeMap::new();
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
                options_map.entry(option_key.clone()).or_insert(StatsRuleOption {
                    key: option_key,
                    label,
                    group_id: group.clone(),
                    rule_id: rule.clone(),
                });
            }

            if bucket_time < cutoff {
                continue;
            }

            if let (Some(expect_group), Some(expect_rule)) = (&rule_group, &rule_id) {
                if bucket.group_id.as_deref() != Some(expect_group.as_str())
                    || bucket.rule_id.as_deref() != Some(expect_rule.as_str())
                {
                    continue;
                }
            }

            requests += bucket.requests;
            errors += bucket.errors;
            input_tokens += bucket.input_tokens;
            output_tokens += bucket.output_tokens;
            cache_read_tokens += bucket.cache_read_tokens;
            cache_write_tokens += bucket.cache_write_tokens;

            let point = hourly_map
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
        }

        StatsSummaryResult {
            hours: requested_hours,
            rule_key,
            requests,
            errors,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
            hourly: hourly_map.into_values().collect(),
            options: options_map.into_values().collect(),
        }
    }

    pub fn clear(&self) -> Result<(), String> {
        let snapshot = {
            let mut guard = self.inner.lock().map_err(|_| "stats lock poisoned".to_string())?;
            guard.clear();
            guard.clone()
        };
        self.persist_locked(&snapshot)
    }

    fn persist_locked(&self, data: &HashMap<String, StatsBucket>) -> Result<(), String> {
        let payload = PersistedStats {
            version: 1,
            buckets: data.values().cloned().collect(),
        };
        let text =
            serde_json::to_string_pretty(&payload).map_err(|e| format!("serialize stats failed: {e}"))?;
        std::fs::write(&self.file_path, text).map_err(|e| format!("write stats file failed: {e}"))
    }
}

fn parse_rule_key(rule_key: Option<&str>) -> (Option<String>, Option<String>) {
    let Some(rule_key) = rule_key else {
        return (None, None);
    };
    let mut parts = rule_key.splitn(2, "::");
    let group = parts.next().unwrap_or_default().trim();
    let rule = parts.next().unwrap_or_default().trim();
    if group.is_empty() || rule.is_empty() {
        return (None, None);
    }
    (Some(group.to_string()), Some(rule.to_string()))
}

fn normalize_hour(ts: &str) -> Option<String> {
    let mut dt = parse_ts(ts)?;
    dt = dt
        .with_minute(0)?
        .with_second(0)?
        .with_nanosecond(0)?;
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
    data.retain(|_, bucket| parse_ts(&bucket.hour).map(|dt| dt >= cutoff).unwrap_or(false));
}

fn bucket_key(hour: &str, group_id: Option<&str>, rule_id: Option<&str>) -> String {
    format!(
        "{}::{}::{}",
        hour,
        group_id.unwrap_or("_"),
        rule_id.unwrap_or("_")
    )
}
