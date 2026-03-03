//! Module Overview
//! Application bootstrap orchestration.
//! Creates stores/services/runtime and prepares shared state used by Tauri commands.

use crate::app_state::{apply_launch_on_startup_setting, AppState, SharedState};
use crate::config_store::ConfigStore;
use crate::log_store::LogStore;
use crate::models::AppInfo;
use crate::proxy::ProxyRuntime;
use crate::stats_store::StatsStore;
use std::sync::Arc;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, WindowEvent};

fn create_tray(app: &AppHandle) -> Result<(), String> {
    let show_hide = MenuItem::with_id(
        app,
        "toggle-window",
        "Show/Hide AI Open Router",
        true,
        None::<&str>,
    )
    .map_err(|e| format!("create tray menu failed: {e}"))?;
    let quit = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)
        .map_err(|e| format!("create tray menu failed: {e}"))?;
    let menu = Menu::with_items(app, &[&show_hide, &quit])
        .map_err(|e| format!("build menu failed: {e}"))?;

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("AI Open Router")
        .on_menu_event(|app, event| {
            let window = app.get_webview_window("main");
            match event.id().as_ref() {
                "toggle-window" => {
                    if let Some(w) = window {
                        let visible = w.is_visible().unwrap_or(true);
                        if visible {
                            let _ = w.hide();
                        } else {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                }
                "quit" => {
                    std::process::exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                if let Some(window) = tray.app_handle().get_webview_window("main") {
                    let visible = window.is_visible().unwrap_or(true);
                    if visible {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        });

    let tray_icon = Image::from_bytes(include_bytes!("../../assets/icon.png"))
        .map_err(|e| format!("load tray icon failed: {e}"))?;
    builder = builder.icon(tray_icon);

    builder
        .build(app)
        .map_err(|e| format!("create tray icon failed: {e}"))?;

    Ok(())
}

fn setup_close_to_tray(app: &AppHandle, state: SharedState, tray_ready: bool) {
    if let Some(window) = app.get_webview_window("main") {
        let window_for_event = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let close_to_tray = state.config_store.get().ui.close_to_tray;
                if close_to_tray && tray_ready {
                    api.prevent_close();
                    let _ = window_for_event.hide();
                }
            }
        });
    }
}

pub fn setup_app(app: &mut App, app_name: &str, app_version: &str) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("resolve app_data_dir failed: {e}"))?;

    std::fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("create app data dir failed: {e}"))?;

    let config_path = app_data_dir.join("config.json");
    let config_store = ConfigStore::new(config_path);
    let _ = config_store.initialize();

    let log_store = LogStore::new(100);
    let stats_path = app_data_dir.join("request-stats.json");
    let stats_store = StatsStore::new(stats_path);
    let _ = stats_store.initialize();
    let runtime = ProxyRuntime::new(
        config_store.shared_config(),
        config_store.shared_revision(),
        log_store,
        stats_store,
    )?;

    let state = Arc::new(AppState {
        app_info: AppInfo {
            name: app_name.to_string(),
            version: app_version.to_string(),
        },
        config_store,
        runtime,
    });

    apply_launch_on_startup_setting(app.handle(), state.config_store.get().ui.launch_on_startup);

    let runtime_clone = state.runtime.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(err) = runtime_clone.start().await {
            eprintln!("proxy auto-start failed: {err}");
        }
    });

    let tray_ready = if state.config_store.get().ui.close_to_tray {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| create_tray(app.handle()))) {
            Ok(Ok(())) => true,
            Ok(Err(err)) => {
                eprintln!("tray icon disabled: {err}");
                false
            }
            Err(_) => {
                eprintln!("tray icon disabled: appindicator runtime unavailable");
                false
            }
        }
    } else {
        false
    };
    setup_close_to_tray(app.handle(), state.clone(), tray_ready);

    app.manage(state);
    Ok(())
}
