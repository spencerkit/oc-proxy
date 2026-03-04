//! Module Overview
//! SQLite-backed request statistics store.
//! Persists per-request events and builds aggregated summaries on demand.

use crate::models::{
    ComparisonSummary, HourlyStatsPoint, LogEntry, RuleCardHourlyPoint, RuleCardStatsItem,
    StatsBreakdowns, StatsCountBreakdownItem, StatsRuleCountBreakdownItem, StatsRuleOption,
    StatsRuleTokenBreakdownItem, StatsSummaryResult, StatsTokenBreakdownItem,
};
use chrono::{DateTime, Duration, Timelike, Utc};
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, params_from_iter, Connection};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const DEFAULT_HOURS: u32 = 24;
const MAX_HOURS: u32 = 24 * 90;
const SCHEMA_VERSION: i64 = 1;

#[derive(Clone)]
pub struct StatsStore {
    db_path: PathBuf,
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone)]
enum RuleSelection {
    All,
    Empty,
    Selected(HashSet<String>),
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
    total_duration_ms: u64,
    total_cost: f64,
    currencies: HashSet<String>,
    hourly: BTreeMap<String, HourlyStatsPoint>,
    errors_by_status: HashMap<String, u64>,
    requests_by_protocol: HashMap<String, u64>,
    tokens_by_protocol: HashMap<String, u64>,
    requests_by_rule: HashMap<String, (String, u64)>,
    tokens_by_rule: HashMap<String, (String, u64)>,
}

impl StatsStore {
    pub fn new(file_path: PathBuf) -> Self {
        let conn = Connection::open(&file_path).unwrap_or_else(|_| {
            Connection::open_in_memory()
                .expect("open in-memory sqlite connection for stats should not fail")
        });
        Self {
            db_path: file_path,
            conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub fn initialize(&self) -> Result<(), String> {
        if let Some(parent) = self.db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("create stats dir failed: {e}"))?;
        }
        let conn = self
            .conn
            .lock()
            .map_err(|_| "stats sqlite lock poisoned".to_string())?;
        initialize_schema(&conn)?;
        Ok(())
    }

    pub fn append_log(&self, entry: &LogEntry) {
        if !entry.request_path.starts_with("/oc/") {
            return;
        }

        let Some(ts) = parse_ts(&entry.timestamp) else {
            return;
        };
        let Some(hour) = normalize_hour(&entry.timestamp) else {
            return;
        };

        let (input_tokens, output_tokens, cache_read_tokens, cache_write_tokens) =
            if let Some(usage) = &entry.token_usage {
                (
                    usage.input_tokens as i64,
                    usage.output_tokens as i64,
                    usage.cache_read_tokens as i64,
                    usage.cache_write_tokens as i64,
                )
            } else {
                (0, 0, 0, 0)
            };
        let errors = if entry.status == "ok" { 0_i64 } else { 1_i64 };
        let (
            total_cost,
            currency,
            input_price_snapshot,
            output_price_snapshot,
            cache_input_price_snapshot,
            cache_output_price_snapshot,
        ) = if let Some(cost) = &entry.cost_snapshot {
            (
                Some(cost.total_cost),
                Some(cost.currency.clone()),
                Some(cost.input_price_per_m),
                Some(cost.output_price_per_m),
                Some(cost.cache_input_price_per_m),
                Some(cost.cache_output_price_per_m),
            )
        } else {
            (None, None, None, None, None, None)
        };

        let Ok(conn) = self.conn.lock() else {
            return;
        };

        let _ = conn.execute(
            "INSERT INTO request_events (
                ts_epoch_ms, hour, group_id, group_name, rule_id, entry_protocol,
                downstream_protocol, http_status, errors, input_tokens, output_tokens,
                cache_read_tokens, cache_write_tokens, duration_ms, total_cost, currency,
                input_price_snapshot, output_price_snapshot, cache_input_price_snapshot, cache_output_price_snapshot
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                ts.timestamp_millis(),
                hour,
                entry.group_path,
                entry.group_name,
                entry.rule_id,
                entry.entry_protocol,
                entry.downstream_protocol,
                entry.http_status.map(i64::from),
                errors,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                entry.duration_ms as i64,
                total_cost,
                currency,
                input_price_snapshot,
                output_price_snapshot,
                cache_input_price_snapshot,
                cache_output_price_snapshot
            ],
        );
    }

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
        if matches!(selection, RuleSelection::Empty) {
            return empty_summary(dimension, requested_hours, rule_key, normalized_rule_keys);
        }

        let now = Utc::now();
        let window_start = now - Duration::hours(requested_hours as i64);
        let enable_comparison = enable_comparison.unwrap_or(false);

        let guard = match self.conn.lock() {
            Ok(v) => v,
            Err(_) => {
                return empty_summary(dimension, requested_hours, rule_key, normalized_rule_keys);
            }
        };

        let options = query_rule_options(&guard).unwrap_or_default();
        let current = aggregate_window(&guard, window_start, now, &selection, dimension)
            .unwrap_or_default();

        let (peak_input_tps, peak_output_tps) = compute_peaks(&current.hourly);
        let current_duration_seconds =
            duration_seconds_metric(current.total_duration_ms, current.requests);
        let input_tps = token_speed_metric(current.input_tokens, current_duration_seconds);
        let output_tps = token_speed_metric(current.output_tokens, current_duration_seconds);

        let comparison = if enable_comparison {
            let previous_start = window_start - Duration::hours(requested_hours as i64);
            aggregate_window(&guard, previous_start, window_start, &selection, dimension)
                .ok()
                .map(|previous| {
                    let previous_duration_seconds =
                        duration_seconds_metric(previous.total_duration_ms, previous.requests);
                    ComparisonSummary {
                        requests_delta_pct: pct_delta(
                            current.requests as f64,
                            previous.requests as f64,
                        ),
                        errors_delta_pct: pct_delta(current.errors as f64, previous.errors as f64),
                        total_cost_delta_pct: pct_delta(current.total_cost, previous.total_cost),
                        input_tps_delta_pct: pct_delta(
                            input_tps,
                            token_speed_metric(previous.input_tokens, previous_duration_seconds),
                        ),
                        output_tps_delta_pct: pct_delta(
                            output_tps,
                            token_speed_metric(previous.output_tokens, previous_duration_seconds),
                        ),
                    }
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
            total_cost: current.total_cost,
            cost_currency: resolve_single_currency(&current.currencies),
            input_tps,
            output_tps,
            peak_input_tps,
            peak_output_tps,
            comparison,
            breakdowns: Some(build_breakdowns(&current)),
            hourly: current.hourly.into_values().collect(),
            options,
        }
    }

    pub fn summarize_rule_cards(&self, group_id: &str, hours: Option<u32>) -> Vec<RuleCardStatsItem> {
        let normalized_group_id = group_id.trim();
        if normalized_group_id.is_empty() {
            return vec![];
        }
        let requested_hours = hours.unwrap_or(DEFAULT_HOURS).clamp(1, MAX_HOURS);
        let now = Utc::now();
        let window_start = now - Duration::hours(requested_hours as i64);
        let start_ms = window_start.timestamp_millis();
        let end_ms = now.timestamp_millis();

        let guard = match self.conn.lock() {
            Ok(v) => v,
            Err(_) => return vec![],
        };

        let mut totals_stmt = match guard.prepare(
            "SELECT rule_id,
                    COUNT(*) AS requests,
                    SUM(input_tokens) AS input_tokens,
                    SUM(output_tokens) AS output_tokens,
                    SUM(COALESCE(total_cost, 0)) AS total_cost
             FROM request_events
             WHERE ts_epoch_ms >= ?1 AND ts_epoch_ms < ?2
               AND group_id = ?3
               AND rule_id IS NOT NULL
             GROUP BY rule_id",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return vec![],
        };

        let totals_rows = match totals_stmt.query_map(
            params![start_ms, end_ms, normalized_group_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)? as u64,
                    row.get::<_, i64>(2)? as u64,
                    row.get::<_, i64>(3)? as u64,
                    row.get::<_, f64>(4)?,
                ))
            },
        ) {
            Ok(rows) => rows,
            Err(_) => return vec![],
        };

        let mut totals_map: BTreeMap<String, (u64, u64, u64, f64)> = BTreeMap::new();
        for row in totals_rows.flatten() {
            totals_map.insert(row.0, (row.1, row.2, row.3, row.4));
        }

        let mut hourly_stmt = match guard.prepare(
            "SELECT rule_id, hour,
                    COUNT(*) AS requests,
                    SUM(input_tokens) AS input_tokens,
                    SUM(output_tokens) AS output_tokens
             FROM request_events
             WHERE ts_epoch_ms >= ?1 AND ts_epoch_ms < ?2
               AND group_id = ?3
               AND rule_id IS NOT NULL
             GROUP BY rule_id, hour
             ORDER BY hour ASC",
        ) {
            Ok(stmt) => stmt,
            Err(_) => return vec![],
        };

        let mut points: BTreeMap<String, Vec<RuleCardHourlyPoint>> = BTreeMap::new();
        if let Ok(rows) = hourly_stmt.query_map(
            params![start_ms, end_ms, normalized_group_id],
            |row| {
                let input_tokens = row.get::<_, i64>(3)? as u64;
                let output_tokens = row.get::<_, i64>(4)? as u64;
                Ok((
                    row.get::<_, String>(0)?,
                    RuleCardHourlyPoint {
                        hour: row.get::<_, String>(1)?,
                        requests: row.get::<_, i64>(2)? as u64,
                        input_tokens,
                        output_tokens,
                        tokens: input_tokens + output_tokens,
                    },
                ))
            },
        ) {
            for row in rows.flatten() {
                points.entry(row.0).or_default().push(row.1);
            }
        }

        totals_map
            .into_iter()
            .map(|(rule_id, (requests, input_tokens, output_tokens, total_cost))| RuleCardStatsItem {
                group_id: normalized_group_id.to_string(),
                rule_id: rule_id.clone(),
                requests,
                input_tokens,
                output_tokens,
                tokens: input_tokens + output_tokens,
                total_cost,
                hourly: points.remove(&rule_id).unwrap_or_default(),
            })
            .collect()
    }

    pub fn clear(&self) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "stats sqlite lock poisoned".to_string())?;
        conn.execute("DELETE FROM request_events", [])
            .map_err(|e| format!("clear stats events failed: {e}"))?;
        Ok(())
    }
}

fn initialize_schema(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS app_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS request_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ts_epoch_ms INTEGER NOT NULL,
            hour TEXT NOT NULL,
            group_id TEXT,
            group_name TEXT,
            rule_id TEXT,
            entry_protocol TEXT,
            downstream_protocol TEXT,
            http_status INTEGER,
            errors INTEGER NOT NULL DEFAULT 0,
            input_tokens INTEGER NOT NULL DEFAULT 0,
            output_tokens INTEGER NOT NULL DEFAULT 0,
            cache_read_tokens INTEGER NOT NULL DEFAULT 0,
            cache_write_tokens INTEGER NOT NULL DEFAULT 0,
            duration_ms INTEGER NOT NULL DEFAULT 0,
            total_cost REAL,
            currency TEXT,
            input_price_snapshot REAL,
            output_price_snapshot REAL,
            cache_input_price_snapshot REAL,
            cache_output_price_snapshot REAL
        );
        CREATE INDEX IF NOT EXISTS idx_request_events_ts ON request_events(ts_epoch_ms);
        CREATE INDEX IF NOT EXISTS idx_request_events_provider_time ON request_events(group_id, rule_id, ts_epoch_ms);
        CREATE INDEX IF NOT EXISTS idx_request_events_protocol_time ON request_events(downstream_protocol, ts_epoch_ms);
        CREATE INDEX IF NOT EXISTS idx_request_events_status_time ON request_events(http_status, ts_epoch_ms);",
    )
    .map_err(|e| format!("create stats sqlite schema failed: {e}"))?;

    conn.execute(
        "INSERT INTO app_meta(key, value, updated_at)
         VALUES('stats_schema_version', ?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
        params![SCHEMA_VERSION.to_string(), Utc::now().timestamp_millis()],
    )
    .map_err(|e| format!("upsert stats schema version failed: {e}"))?;
    Ok(())
}

fn query_rule_options(conn: &Connection) -> Result<Vec<StatsRuleOption>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT group_id, group_name, rule_id
             FROM request_events
             WHERE group_id IS NOT NULL AND rule_id IS NOT NULL
             ORDER BY group_id ASC, rule_id ASC",
        )
        .map_err(|e| format!("prepare rule options query failed: {e}"))?;

    let rows = stmt
        .query_map([], |row| {
            let group_id = row.get::<_, String>(0)?;
            let group_name = row.get::<_, Option<String>>(1)?.unwrap_or_else(|| group_id.clone());
            let rule_id = row.get::<_, String>(2)?;
            let key = format!("{group_id}::{rule_id}");
            Ok(StatsRuleOption {
                key,
                label: format!("{group_name}-{rule_id}"),
                group_id,
                rule_id,
            })
        })
        .map_err(|e| format!("query rule options failed: {e}"))?;

    Ok(rows.flatten().collect())
}

fn aggregate_window(
    conn: &Connection,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    selection: &RuleSelection,
    dimension: StatsDimension,
) -> Result<WindowAggregate, String> {
    if matches!(selection, RuleSelection::Empty) {
        return Ok(WindowAggregate::default());
    }

    let mut aggregate = WindowAggregate::default();
    let start_ms = start.timestamp_millis();
    let end_ms = end.timestamp_millis();

    let (filter_sql, mut params_values) = build_rule_filter(selection);
    let hourly_sql = format!(
        "SELECT hour,
                COUNT(*) AS requests,
                SUM(errors) AS errors,
                SUM(input_tokens) AS input_tokens,
                SUM(output_tokens) AS output_tokens,
                SUM(cache_read_tokens) AS cache_read_tokens,
                SUM(cache_write_tokens) AS cache_write_tokens,
                SUM(duration_ms) AS total_duration_ms,
                SUM(COALESCE(total_cost, 0)) AS total_cost
         FROM request_events
         WHERE ts_epoch_ms >= ?1 AND ts_epoch_ms < ?2{filter_sql}
         GROUP BY hour
         ORDER BY hour ASC"
    );
    let mut args = vec![SqlValue::Integer(start_ms), SqlValue::Integer(end_ms)];
    args.append(&mut params_values.clone());

    let mut hourly_stmt = conn
        .prepare(&hourly_sql)
        .map_err(|e| format!("prepare hourly stats query failed: {e}"))?;
    let rows = hourly_stmt
        .query_map(params_from_iter(args), |row| {
            Ok(HourlyStatsPoint {
                hour: row.get::<_, String>(0)?,
                requests: row.get::<_, i64>(1)? as u64,
                errors: row.get::<_, i64>(2)? as u64,
                input_tokens: row.get::<_, i64>(3)? as u64,
                output_tokens: row.get::<_, i64>(4)? as u64,
                cache_read_tokens: row.get::<_, i64>(5)? as u64,
                cache_write_tokens: row.get::<_, i64>(6)? as u64,
                total_duration_ms: row.get::<_, i64>(7)? as u64,
                total_cost: row.get::<_, f64>(8)?,
                input_tps: 0.0,
                output_tps: 0.0,
            })
        })
        .map_err(|e| format!("query hourly stats failed: {e}"))?;

    for row in rows.flatten() {
        let duration_seconds = duration_seconds_metric(row.total_duration_ms, row.requests);
        let mut point = row;
        point.input_tps = token_speed_metric(point.input_tokens, duration_seconds);
        point.output_tps = token_speed_metric(point.output_tokens, duration_seconds);
        aggregate.requests += point.requests;
        aggregate.errors += point.errors;
        aggregate.input_tokens += point.input_tokens;
        aggregate.output_tokens += point.output_tokens;
        aggregate.cache_read_tokens += point.cache_read_tokens;
        aggregate.cache_write_tokens += point.cache_write_tokens;
        aggregate.total_duration_ms += point.total_duration_ms;
        aggregate.total_cost += point.total_cost;
        aggregate.hourly.insert(point.hour.clone(), point);
    }

    let currency_sql = format!(
        "SELECT COALESCE(NULLIF(TRIM(currency), ''), '') AS currency
         FROM request_events
         WHERE ts_epoch_ms >= ?1 AND ts_epoch_ms < ?2
           AND total_cost IS NOT NULL{filter_sql}"
    );
    let mut currency_args = vec![SqlValue::Integer(start_ms), SqlValue::Integer(end_ms)];
    currency_args.append(&mut params_values.clone());
    let mut currency_stmt = conn
        .prepare(&currency_sql)
        .map_err(|e| format!("prepare currency query failed: {e}"))?;
    let currency_rows = currency_stmt
        .query_map(params_from_iter(currency_args), |row| row.get::<_, String>(0))
        .map_err(|e| format!("query currencies failed: {e}"))?;
    for row in currency_rows.flatten() {
        let currency = row.trim();
        if !currency.is_empty() {
            aggregate.currencies.insert(currency.to_string());
        }
    }

    let protocol_sql = format!(
        "SELECT COALESCE(NULLIF(TRIM(downstream_protocol), ''), 'unknown') AS protocol,
                COUNT(*) AS requests,
                SUM(input_tokens + output_tokens) AS tokens
         FROM request_events
         WHERE ts_epoch_ms >= ?1 AND ts_epoch_ms < ?2{filter_sql}
         GROUP BY protocol"
    );
    let mut protocol_args = vec![SqlValue::Integer(start_ms), SqlValue::Integer(end_ms)];
    protocol_args.append(&mut params_values.clone());
    let mut protocol_stmt = conn
        .prepare(&protocol_sql)
        .map_err(|e| format!("prepare protocol breakdown query failed: {e}"))?;
    let protocol_rows = protocol_stmt
        .query_map(params_from_iter(protocol_args), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, i64>(2)? as u64,
            ))
        })
        .map_err(|e| format!("query protocol breakdown failed: {e}"))?;
    for row in protocol_rows.flatten() {
        aggregate.requests_by_protocol.insert(row.0.clone(), row.1);
        aggregate.tokens_by_protocol.insert(row.0, row.2);
    }

    let status_sql = format!(
        "SELECT COALESCE(CAST(http_status AS TEXT), 'unknown') AS status_key,
                COALESCE(NULLIF(TRIM(downstream_protocol), ''), 'unknown') AS protocol_key,
                SUM(errors) AS errors
         FROM request_events
         WHERE ts_epoch_ms >= ?1 AND ts_epoch_ms < ?2 AND errors > 0{filter_sql}
         GROUP BY status_key, protocol_key"
    );
    let mut status_args = vec![SqlValue::Integer(start_ms), SqlValue::Integer(end_ms)];
    status_args.append(&mut params_values.clone());
    let mut status_stmt = conn
        .prepare(&status_sql)
        .map_err(|e| format!("prepare status breakdown query failed: {e}"))?;
    let status_rows = status_stmt
        .query_map(params_from_iter(status_args), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? as u64,
            ))
        })
        .map_err(|e| format!("query status breakdown failed: {e}"))?;
    for row in status_rows.flatten() {
        let key = if matches!(dimension, StatsDimension::Protocol) {
            format!("{} · {}", row.1, row.0)
        } else {
            row.0
        };
        *aggregate.errors_by_status.entry(key).or_insert(0) += row.2;
    }

    let rule_sql = format!(
        "SELECT group_id, COALESCE(group_name, group_id) AS group_label, rule_id,
                COUNT(*) AS requests, SUM(input_tokens + output_tokens) AS tokens
         FROM request_events
         WHERE ts_epoch_ms >= ?1 AND ts_epoch_ms < ?2
           AND group_id IS NOT NULL AND rule_id IS NOT NULL{filter_sql}
         GROUP BY group_id, group_label, rule_id"
    );
    let mut rule_args = vec![SqlValue::Integer(start_ms), SqlValue::Integer(end_ms)];
    rule_args.append(&mut params_values);
    let mut rule_stmt = conn
        .prepare(&rule_sql)
        .map_err(|e| format!("prepare rule breakdown query failed: {e}"))?;
    let rule_rows = rule_stmt
        .query_map(params_from_iter(rule_args), |row| {
            let group_id = row.get::<_, String>(0)?;
            let group_label = row.get::<_, String>(1)?;
            let rule_id = row.get::<_, String>(2)?;
            Ok((
                format!("{group_id}::{rule_id}"),
                format!("{group_label}-{rule_id}"),
                row.get::<_, i64>(3)? as u64,
                row.get::<_, i64>(4)? as u64,
            ))
        })
        .map_err(|e| format!("query rule breakdown failed: {e}"))?;
    for row in rule_rows.flatten() {
        aggregate
            .requests_by_rule
            .insert(row.0.clone(), (row.1.clone(), row.2));
        aggregate.tokens_by_rule.insert(row.0, (row.1, row.3));
    }

    Ok(aggregate)
}

fn build_rule_filter(selection: &RuleSelection) -> (String, Vec<SqlValue>) {
    match selection {
        RuleSelection::All => (String::new(), vec![]),
        RuleSelection::Empty => (" AND 1 = 0".to_string(), vec![]),
        RuleSelection::Selected(set) => {
            let mut keys: Vec<String> = set.iter().cloned().collect();
            keys.sort();
            let mut placeholders = Vec::with_capacity(keys.len());
            let mut values = Vec::with_capacity(keys.len());
            for (index, key) in keys.iter().enumerate() {
                placeholders.push(format!("?{}", index + 3));
                values.push(SqlValue::Text(key.clone()));
            }
            (
                format!(
                    " AND (COALESCE(group_id, '') || '::' || COALESCE(rule_id, '')) IN ({})",
                    placeholders.join(", ")
                ),
                values,
            )
        }
    }
}

fn empty_summary(
    dimension: StatsDimension,
    requested_hours: u32,
    rule_key: Option<String>,
    rule_keys: Option<Vec<String>>,
) -> StatsSummaryResult {
    StatsSummaryResult {
        dimension: dimension.as_str().to_string(),
        hours: requested_hours,
        rule_key,
        rule_keys,
        requests: 0,
        errors: 0,
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        total_cost: 0.0,
        cost_currency: None,
        input_tps: 0.0,
        output_tps: 0.0,
        peak_input_tps: 0.0,
        peak_output_tps: 0.0,
        comparison: None,
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

fn duration_seconds_metric(total_duration_ms: u64, requests: u64) -> f64 {
    if total_duration_ms > 0 {
        return total_duration_ms as f64 / 1000.0;
    }
    if requests > 0 {
        return requests as f64 * 0.001;
    }
    1.0
}

fn token_speed_metric(total_tokens: u64, duration_seconds: f64) -> f64 {
    if duration_seconds <= 0.0 {
        0.0
    } else {
        total_tokens as f64 / duration_seconds
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

fn compute_peaks(hourly: &BTreeMap<String, HourlyStatsPoint>) -> (f64, f64) {
    let mut peak_input_tps: f64 = 0.0;
    let mut peak_output_tps: f64 = 0.0;
    for point in hourly.values() {
        let duration_seconds = duration_seconds_metric(point.total_duration_ms, point.requests);
        peak_input_tps = peak_input_tps.max(token_speed_metric(point.input_tokens, duration_seconds));
        peak_output_tps =
            peak_output_tps.max(token_speed_metric(point.output_tokens, duration_seconds));
    }
    (peak_input_tps, peak_output_tps)
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

fn resolve_single_currency(currencies: &HashSet<String>) -> Option<String> {
    if currencies.is_empty() {
        return None;
    }
    if currencies.len() == 1 {
        return currencies.iter().next().cloned();
    }
    Some("MIXED".to_string())
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
