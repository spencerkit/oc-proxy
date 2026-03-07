//! Module Overview
//! Application entrypoint that wires Tauri commands and shared state.
//! Keeps startup responsibilities minimal and delegates runtime logic to domain modules.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod app_state;
mod backup;
mod bootstrap;
mod commands;
mod config;
mod config_store;
mod domain;
mod log_store;
mod mappers;
mod models;
mod proxy;
mod quota;
mod remote_sync;
mod services;
mod stats_store;

use commands::{
    app_get_info, app_get_status, app_read_clipboard_text, app_renderer_ready,
    app_report_renderer_error, app_start_server, app_stop_server, config_export_groups,
    config_export_groups_clipboard, config_export_groups_folder, config_get, config_import_groups,
    config_import_groups_json, config_remote_rules_pull, config_remote_rules_upload, config_save,
    logs_clear, logs_list, logs_stats_clear, logs_stats_rule_cards, logs_stats_summary,
    quota_get_group, quota_get_rule, quota_test_draft,
};
use tauri::Manager;

#[tokio::main]
/// Application entrypoint for the Tauri runtime.
async fn main() {
    let app_name = "AI Open Router".to_string();
    let app_version = env!("CARGO_PKG_VERSION").to_string();

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_log::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| Ok(bootstrap::setup_app(app, &app_name, &app_version)?))
        .invoke_handler(tauri::generate_handler![
            app_get_info,
            app_get_status,
            app_start_server,
            app_stop_server,
            app_renderer_ready,
            app_report_renderer_error,
            config_get,
            config_save,
            config_export_groups,
            config_export_groups_folder,
            config_export_groups_clipboard,
            config_import_groups,
            config_import_groups_json,
            config_remote_rules_upload,
            config_remote_rules_pull,
            app_read_clipboard_text,
            logs_list,
            logs_clear,
            logs_stats_summary,
            logs_stats_rule_cards,
            logs_stats_clear,
            quota_get_rule,
            quota_get_group,
            quota_test_draft,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
