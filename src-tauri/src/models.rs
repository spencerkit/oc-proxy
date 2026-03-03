//! Module Overview
//! Application-wide data models shared across runtime, commands, and renderer IPC.
//! Defines config, proxy status, logs, stats, and quota-related transport structures.

pub use crate::api::dto::*;
#[allow(unused_imports)]
pub use crate::config::schema::{default_config, default_metrics, default_remote_git_config};
pub use crate::config::validator::validate_config;
pub use crate::domain::entities::*;
