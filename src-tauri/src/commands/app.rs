//! Module Overview
//! Tauri command handlers for renderer IPC invocations.
//! Performs boundary-level argument handling and delegates business logic to runtime/services.

use crate::app_state::SharedState;
use crate::models::{AppInfo, ClipboardTextResult, ProxyStatus};
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[tauri::command]
/// Performs app get info.
pub async fn app_get_info(state: State<'_, SharedState>) -> Result<AppInfo, String> {
    Ok(state.app_info.clone())
}

#[tauri::command]
/// Performs app get status.
pub async fn app_get_status(state: State<'_, SharedState>) -> Result<ProxyStatus, String> {
    Ok(state.runtime.get_status())
}

#[tauri::command]
/// Performs app start server.
pub async fn app_start_server(state: State<'_, SharedState>) -> Result<ProxyStatus, String> {
    state.runtime.start().await
}

#[tauri::command]
/// Performs app stop server.
pub async fn app_stop_server(state: State<'_, SharedState>) -> Result<ProxyStatus, String> {
    state.runtime.stop().await
}

#[tauri::command]
/// Performs app read clipboard text.
pub async fn app_read_clipboard_text(app: AppHandle) -> Result<ClipboardTextResult, String> {
    let text = app
        .clipboard()
        .read_text()
        .map_err(|e| format!("read clipboard failed: {e}"))?;
    Ok(ClipboardTextResult { text })
}
