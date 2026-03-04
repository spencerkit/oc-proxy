//! Module Overview
//! Tauri command handlers for renderer IPC invocations.
//! Performs boundary-level argument handling and delegates business logic to runtime/services.

use crate::app_state::SharedState;
use crate::models::{LogEntry, RuleCardStatsItem, StatsSummaryResult};
use serde_json::json;
use tauri::State;

#[tauri::command]
/// Performs logs list.
pub async fn logs_list(
    state: State<'_, SharedState>,
    max: Option<usize>,
) -> Result<Vec<LogEntry>, String> {
    Ok(state.runtime.list_logs(max.unwrap_or(100)))
}

#[tauri::command]
/// Performs logs clear.
pub async fn logs_clear(state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.runtime.clear_logs();
    Ok(json!({ "ok": true }))
}

#[tauri::command]
/// Performs logs stats summary.
pub async fn logs_stats_summary(
    state: State<'_, SharedState>,
    hours: Option<u32>,
    rule_keys: Option<Vec<String>>,
    rule_key: Option<String>,
    dimension: Option<String>,
    enable_comparison: Option<bool>,
) -> Result<StatsSummaryResult, String> {
    Ok(state
        .runtime
        .stats_summary(hours, rule_keys, rule_key, dimension, enable_comparison))
}

#[tauri::command]
/// Performs logs stats rule cards.
pub async fn logs_stats_rule_cards(
    state: State<'_, SharedState>,
    group_id: String,
    hours: Option<u32>,
) -> Result<Vec<RuleCardStatsItem>, String> {
    Ok(state.runtime.stats_rule_cards(group_id, hours))
}

#[tauri::command]
/// Performs logs stats clear.
pub async fn logs_stats_clear(state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.runtime.clear_stats()?;
    Ok(json!({ "ok": true }))
}
