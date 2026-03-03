use crate::app_state::SharedState;
use crate::models::{AppInfo, ClipboardTextResult, ProxyStatus};
use tauri::{AppHandle, State};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[tauri::command]
pub async fn app_get_info(state: State<'_, SharedState>) -> Result<AppInfo, String> {
    Ok(state.app_info.clone())
}

#[tauri::command]
pub async fn app_get_status(state: State<'_, SharedState>) -> Result<ProxyStatus, String> {
    Ok(state.runtime.get_status())
}

#[tauri::command]
pub async fn app_start_server(state: State<'_, SharedState>) -> Result<ProxyStatus, String> {
    state.runtime.start().await
}

#[tauri::command]
pub async fn app_stop_server(state: State<'_, SharedState>) -> Result<ProxyStatus, String> {
    state.runtime.stop().await
}

#[tauri::command]
pub async fn app_read_clipboard_text(app: AppHandle) -> Result<ClipboardTextResult, String> {
    let text = app
        .clipboard()
        .read_text()
        .map_err(|e| format!("read clipboard failed: {e}"))?;
    Ok(ClipboardTextResult { text })
}
