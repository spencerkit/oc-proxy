use crate::app_state::SharedState;
use crate::models::{LogEntry, StatsSummaryResult};
use serde_json::json;
use tauri::State;

#[tauri::command]
pub async fn logs_list(
    state: State<'_, SharedState>,
    max: Option<usize>,
) -> Result<Vec<LogEntry>, String> {
    Ok(state.runtime.list_logs(max.unwrap_or(100)))
}

#[tauri::command]
pub async fn logs_clear(state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.runtime.clear_logs();
    Ok(json!({ "ok": true }))
}

#[tauri::command]
pub async fn logs_stats_summary(
    state: State<'_, SharedState>,
    hours: Option<u32>,
    rule_key: Option<String>,
) -> Result<StatsSummaryResult, String> {
    Ok(state.runtime.stats_summary(hours, rule_key))
}

#[tauri::command]
pub async fn logs_stats_clear(state: State<'_, SharedState>) -> Result<serde_json::Value, String> {
    state.runtime.clear_stats()?;
    Ok(json!({ "ok": true }))
}
