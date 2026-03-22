//! Tauri command handlers for remote management authentication.

use crate::app_state::SharedState;
use crate::models::AuthSessionStatus;
use crate::services::config_service;
use tauri::State;

#[tauri::command]
/// IPC command: returns remote management auth session status for desktop runtime.
pub async fn auth_get_session_status(
    state: State<'_, SharedState>,
) -> Result<AuthSessionStatus, String> {
    Ok(config_service::auth_session_status(&state, false, true))
}

#[tauri::command]
/// IPC command: performs a best-effort login no-op for desktop runtime compatibility.
pub async fn auth_login(
    state: State<'_, SharedState>,
    _password: String,
) -> Result<AuthSessionStatus, String> {
    Ok(config_service::auth_session_status(&state, false, true))
}

#[tauri::command]
/// IPC command: performs a best-effort logout no-op for desktop runtime compatibility.
pub async fn auth_logout(state: State<'_, SharedState>) -> Result<AuthSessionStatus, String> {
    Ok(config_service::auth_session_status(&state, false, true))
}
