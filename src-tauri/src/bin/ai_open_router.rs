//! Headless server entrypoint for CLI usage.

use ai_open_router_tauri::app_state::AppState;
use ai_open_router_tauri::config_store::ConfigStore;
use ai_open_router_tauri::integration_store::IntegrationStore;
use ai_open_router_tauri::log_store::LogStore;
use ai_open_router_tauri::models::AppInfo;
use ai_open_router_tauri::proxy::ProxyRuntime;
use ai_open_router_tauri::stats_store::StatsStore;
use directories::ProjectDirs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn resolve_app_data_dir() -> Result<PathBuf, String> {
    if let Ok(value) = std::env::var("AOR_APP_DATA_DIR") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    let dirs = ProjectDirs::from("art", "shier", "aiopenrouter")
        .ok_or_else(|| "resolve app data dir failed".to_string())?;
    Ok(dirs.data_dir().to_path_buf())
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let app_data_dir = resolve_app_data_dir()?;
    std::fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("create app data dir failed: {e}"))?;

    let config_store = ConfigStore::new(app_data_dir.join("config.json"));
    let _ = config_store.initialize();
    let integration_store = IntegrationStore::new(app_data_dir.join("client-integrations.json"));
    let _ = integration_store.initialize();
    let log_store = LogStore::with_dev_log_file(100, Some(app_data_dir.join("proxy-dev-logs.jsonl")));
    let stats_store = StatsStore::new(app_data_dir.join("request-stats.sqlite"));
    let _ = stats_store.initialize();

    let runtime = ProxyRuntime::new(
        config_store.shared_config(),
        config_store.shared_revision(),
        log_store,
        stats_store,
    )?;

    let state = Arc::new(AppState {
        app_info: AppInfo {
            name: "AI Open Router".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        config_store,
        integration_store,
        runtime,
        renderer_ready: AtomicBool::new(false),
    });

    state.runtime.attach_shared_state(state.clone());
    let status = state.runtime.start().await?;

    if let Some(address) = status.address.as_deref() {
        println!("Management UI: {address}/management");
    }
    if let Some(lan) = status.lan_address.as_deref() {
        println!("LAN Access: {lan}/management");
    }

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| format!("wait for ctrl-c failed: {e}"))?;
    let _ = state.runtime.stop().await;
    Ok(())
}
