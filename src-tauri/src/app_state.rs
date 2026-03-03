//! Module Overview
//! Shared app state coordination utilities.
//! Applies config updates and synchronizes proxy runtime lifecycle when settings change.

use crate::config_store::ConfigStore;
use crate::models::{AppInfo, ProxyConfig, ProxyStatus};
use crate::proxy::ProxyRuntime;
use std::sync::Arc;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt as _;

pub struct AppState {
    pub app_info: AppInfo,
    pub config_store: ConfigStore,
    pub runtime: ProxyRuntime,
}

pub type SharedState = Arc<AppState>;

fn has_server_setting_changed(prev: &ProxyConfig, next: &ProxyConfig) -> bool {
    prev.server.host != next.server.host
        || prev.server.port != next.server.port
        || prev.server.auth_enabled != next.server.auth_enabled
        || prev.server.local_bearer_token != next.server.local_bearer_token
}

pub async fn sync_runtime_config(
    state: &SharedState,
    prev: ProxyConfig,
    next: ProxyConfig,
) -> Result<(bool, ProxyStatus), String> {
    let mut restarted = false;
    let status_before = state.runtime.get_status();
    if status_before.running && has_server_setting_changed(&prev, &next) {
        state.runtime.stop().await?;
        state.runtime.start().await?;
        restarted = true;
    }

    Ok((restarted, state.runtime.get_status()))
}

pub fn apply_launch_on_startup_setting(app: &AppHandle, enabled: bool) {
    let autostart_manager = app.autolaunch();
    if enabled {
        let _ = autostart_manager.enable();
    } else {
        let _ = autostart_manager.disable();
    }
}
