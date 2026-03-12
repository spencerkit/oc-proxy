//! Module Overview
//! Tauri command module exports.
//! Collects command handlers so main entrypoint can register them declaratively.

#[macro_use]
mod app;
#[macro_use]
mod config;
#[macro_use]
mod integration;
#[macro_use]
mod logs;
#[macro_use]
mod provider;
#[macro_use]
mod quota;
#[macro_use]
mod remote;

pub use app::{
    app_get_info, app_get_status, app_read_clipboard_text, app_renderer_ready,
    app_report_renderer_error, app_start_server, app_stop_server,
};
pub use config::{
    config_export_groups, config_export_groups_clipboard, config_export_groups_folder, config_get,
    config_import_groups, config_import_groups_json, config_save,
};
pub use integration::{
    integration_add_target, integration_list_targets, integration_pick_directory,
    integration_read_agent_config, integration_remove_target, integration_update_target,
    integration_write_agent_config, integration_write_agent_config_source,
    integration_write_group_entry,
};
pub use logs::{
    logs_clear, logs_list, logs_stats_clear, logs_stats_rule_cards, logs_stats_summary,
};
pub use provider::provider_test_model;
pub use quota::{quota_get_group, quota_get_rule, quota_test_draft};
pub use remote::{config_remote_rules_pull, config_remote_rules_upload};

/// Build the Tauri invoke handler for the desktop runtime.
pub fn build_invoke_handler(
) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
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
        integration_list_targets,
        integration_pick_directory,
        integration_add_target,
        integration_update_target,
        integration_remove_target,
        integration_write_group_entry,
        integration_read_agent_config,
        integration_write_agent_config,
        integration_write_agent_config_source,
        app_read_clipboard_text,
        logs_list,
        logs_clear,
        logs_stats_summary,
        logs_stats_rule_cards,
        logs_stats_clear,
        provider_test_model,
        quota_get_rule,
        quota_get_group,
        quota_test_draft,
    ]
}
