//! Module Overview
//! Application bootstrap orchestration.
//! Creates stores/services/runtime and prepares shared state used by Tauri commands.

use crate::app_state::{apply_launch_on_startup_setting, AppState, SharedState};
use crate::config_store::ConfigStore;
use crate::integration_store::IntegrationStore;
use crate::log_store::LogStore;
use crate::models::AppInfo;
use crate::proxy::ProxyRuntime;
use crate::stats_store::StatsStore;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Manager, Url, WindowEvent};

const DEBUG_FORCE_LOAD_FAILED_PAGE_ENV: &str = "AOR_DEBUG_FORCE_LOAD_FAILED_PAGE";

const RELEASE_WEBVIEW_HARDEN_SCRIPT: &str = r#"
(() => {
  const block = (e) => e.preventDefault();
  document.addEventListener('contextmenu', block, true);
  window.addEventListener('keydown', (e) => {
    const key = (e.key || '').toLowerCase();
    const withCmdOrCtrl = e.ctrlKey || e.metaKey;
    const withShift = e.shiftKey;
    const shouldBlock =
      key === 'f5' ||
      (withCmdOrCtrl && key === 'r') ||
      key === 'f12' ||
      (withCmdOrCtrl && withShift && key === 'i');
    if (shouldBlock) {
      e.preventDefault();
      e.stopPropagation();
    }
  }, true);
})();
"#;

const RENDERER_LOAD_FAILED_PAGE_HTML_TEMPLATE: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width,initial-scale=1" />
  <title>页面加载失败</title>
  <style>
    :root { color-scheme: dark; }
    body {
      margin: 0;
      min-height: 100vh;
      display: flex;
      align-items: center;
      justify-content: center;
      background: #0f172a;
      color: #e2e8f0;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    .card {
      width: min(92vw, 540px);
      border-radius: 12px;
      padding: 24px;
      background: #111827;
      border: 1px solid #334155;
      box-shadow: 0 16px 40px rgba(0, 0, 0, 0.35);
    }
    h1 { margin: 0 0 10px; font-size: 20px; }
    p { margin: 0; opacity: 0.9; line-height: 1.6; }
    button {
      margin-top: 18px;
      border: none;
      border-radius: 8px;
      padding: 10px 16px;
      font-size: 14px;
      cursor: pointer;
      background: #2563eb;
      color: #fff;
    }
  </style>
</head>
<body>
  <main class="card">
    <h1>页面加载失败</h1>
    <p>主文档或脚本加载异常，请检查网络与本地文件完整性后重试。</p>
    <button id="retry-btn" data-target="__RETRY_TARGET__">重新加载</button>
  </main>
  <script>
    document.addEventListener("contextmenu", function (event) { event.preventDefault(); }, true);
    window.addEventListener("keydown", function (event) {
      var key = (event.key || "").toLowerCase();
      var withCmdOrCtrl = event.ctrlKey || event.metaKey;
      var withShift = event.shiftKey;
      var shouldBlock =
        key === "f5" ||
        (withCmdOrCtrl && key === "r") ||
        key === "f12" ||
        (withCmdOrCtrl && withShift && key === "i");
      if (shouldBlock) {
        event.preventDefault();
        event.stopPropagation();
      }
    }, true);

    var retryBtn = document.getElementById("retry-btn");
    if (retryBtn) {
      retryBtn.addEventListener("click", function () {
        var target = retryBtn.getAttribute("data-target");
        if (target) {
          window.location.replace(target);
          return;
        }
        window.location.reload();
      });
    }
  </script>
</body>
</html>
"#;

/// Escapes html attribute content.
fn escape_html_attr(raw: &str) -> String {
    let mut escaped = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Percent-encodes utf8 bytes for data URL payload.
fn encode_data_url_payload(raw: &str) -> String {
    const HEX: [char; 16] = [
        '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'A', 'B', 'C', 'D', 'E', 'F',
    ];
    let mut encoded = String::with_capacity(raw.len() * 2);
    for byte in raw.as_bytes() {
        let value = *byte;
        let is_unreserved =
            value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            encoded.push(value as char);
            continue;
        }
        encoded.push('%');
        encoded.push(HEX[(value >> 4) as usize]);
        encoded.push(HEX[(value & 0x0f) as usize]);
    }
    encoded
}

/// Builds fallback html page data URL.
fn build_load_failed_page_data_url(retry_target: &str) -> String {
    let html = RENDERER_LOAD_FAILED_PAGE_HTML_TEMPLATE
        .replace("__RETRY_TARGET__", &escape_html_attr(retry_target));
    let payload = encode_data_url_payload(&html);
    format!("data:text/html;charset=utf-8,{payload}")
}

/// Creates tray for this module's workflow.
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

/// Performs setup close to tray.
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

/// Applies release-only webview hardening policy.
fn apply_release_webview_hardening(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    if let Some(window) = app.get_webview_window("main") {
        if let Err(err) = window.eval(RELEASE_WEBVIEW_HARDEN_SCRIPT) {
            eprintln!("[renderer][warn] event=release_harden_inject_failed message={err}");
        }
    }
}

/// Navigates to renderer load failure fallback page.
fn show_renderer_load_failed_page(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let retry_target = window
            .url()
            .map(|value| value.to_string())
            .unwrap_or_else(|_| "tauri://localhost".to_string());
        let data_url = build_load_failed_page_data_url(&retry_target);
        match Url::parse(&data_url) {
            Ok(url) => {
                if let Err(err) = window.navigate(url) {
                    eprintln!(
                        "[renderer][error] event=load_failed_page_navigate_failed message={err}"
                    );
                }
            }
            Err(err) => {
                eprintln!(
                    "[renderer][error] event=load_failed_page_url_parse_failed message={err}"
                );
            }
        }
    }
}

/// Returns whether debug mode should force fallback page.
fn should_force_load_failed_page_for_debug() -> bool {
    if !cfg!(debug_assertions) {
        return false;
    }
    match std::env::var(DEBUG_FORCE_LOAD_FAILED_PAGE_ENV) {
        Ok(raw) => {
            let normalized = raw.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        }
        Err(_) => false,
    }
}

/// Spawns debug-only forced fallback page task.
fn spawn_debug_forced_load_failed_page(app: AppHandle) {
    if !should_force_load_failed_page_for_debug() {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        eprintln!(
            "[renderer][warn] event=debug_force_load_failed_page window=main message=forced by {DEBUG_FORCE_LOAD_FAILED_PAGE_ENV}"
        );
        show_renderer_load_failed_page(&app);
    });
}

/// Starts watchdog for renderer boot timeout.
fn spawn_renderer_boot_watchdog(app: AppHandle, state: SharedState) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        if state.is_renderer_ready() {
            return;
        }
        eprintln!(
            "[renderer][error] event=renderer_boot_timeout window=main message=renderer did not report ready in 10s"
        );
        show_renderer_load_failed_page(&app);
    });
}

/// Performs setup app.
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
    let integration_store = IntegrationStore::new(app_data_dir.join("client-integrations.json"));
    let _ = integration_store.initialize();

    let log_store = LogStore::with_dev_log_file(
        100,
        if cfg!(debug_assertions) {
            Some(app_data_dir.join("proxy-dev-logs.jsonl"))
        } else {
            None
        },
    );
    let stats_path = app_data_dir.join("request-stats.sqlite");
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
        integration_store,
        runtime,
        renderer_ready: AtomicBool::new(false),
    });

    apply_launch_on_startup_setting(app.handle(), state.config_store.get().ui.launch_on_startup);

    if state.config_store.get().ui.auto_start_server {
        let runtime_clone = state.runtime.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(err) = runtime_clone.start().await {
                eprintln!("proxy auto-start failed: {err}");
            }
        });
    }

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
    apply_release_webview_hardening(app.handle());
    spawn_debug_forced_load_failed_page(app.handle().clone());
    spawn_renderer_boot_watchdog(app.handle().clone(), state.clone());

    app.manage(state);
    Ok(())
}
