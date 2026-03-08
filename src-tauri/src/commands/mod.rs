//! Module Overview
//! Tauri command module exports.
//! Collects command handlers so main entrypoint can register them declaratively.

mod app;
mod config;
mod integration;
mod logs;
mod provider;
mod quota;
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
    integration_write_agent_config, integration_write_group_entry,
};
pub use logs::{
    logs_clear, logs_list, logs_stats_clear, logs_stats_rule_cards, logs_stats_summary,
};
pub use provider::provider_test_model;
pub use quota::{quota_get_group, quota_get_rule, quota_test_draft};
pub use remote::{config_remote_rules_pull, config_remote_rules_upload};
