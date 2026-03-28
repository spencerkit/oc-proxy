//! Module Overview
//! Tauri command handlers for renderer IPC invocations.
//! Performs boundary-level argument handling and delegates business logic to runtime/services.

use crate::app_state::SharedState;
use crate::models::{AppInfo, ClipboardTextResult, ProxyStatus};
use tauri::{AppHandle, State, Window};
use tauri_plugin_clipboard_manager::ClipboardExt;

/// Writes renderer diagnostics log line.
fn log_renderer_event(level: &str, event: &str, window_label: &str, message: &str) {
    eprintln!("[renderer][{level}] event={event} window={window_label} message={message}");
}

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

#[tauri::command]
/// Opens an external URL with the system default handler.
pub async fn app_open_external_url(url: String) -> Result<(), String> {
    tauri_plugin_opener::open_url(url, None::<&str>)
        .map_err(|error| format!("open external url failed: {error}"))
}

#[tauri::command]
/// Marks renderer as ready for watchdog logic.
pub async fn app_renderer_ready(
    state: State<'_, SharedState>,
    window: Window,
) -> Result<(), String> {
    state.set_renderer_ready(true);
    log_renderer_event(
        "info",
        "renderer_ready",
        window.label(),
        "renderer boot completed",
    );
    Ok(())
}

#[tauri::command]
/// Receives renderer runtime error telemetry.
pub async fn app_report_renderer_error(
    _state: State<'_, SharedState>,
    window: Window,
    kind: String,
    message: String,
    stack: Option<String>,
    source: Option<String>,
) -> Result<(), String> {
    let stack_preview = stack
        .as_deref()
        .map(|value| value.chars().take(240).collect::<String>())
        .unwrap_or_else(|| "-".to_string());
    let source_text = source.unwrap_or_else(|| "-".to_string());
    log_renderer_event(
        "error",
        "renderer_runtime_error",
        window.label(),
        &format!("kind={kind} source={source_text} message={message} stack={stack_preview}"),
    );
    Ok(())
}
