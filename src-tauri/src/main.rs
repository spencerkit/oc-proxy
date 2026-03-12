//! Module Overview
//! Application entrypoint that wires Tauri commands and shared state.
//! Keeps startup responsibilities minimal and delegates runtime logic to domain modules.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use ai_open_router_tauri::bootstrap;
use ai_open_router_tauri::commands::build_invoke_handler;
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| Ok(bootstrap::setup_app(app, &app_name, &app_version)?))
        .invoke_handler(build_invoke_handler())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
