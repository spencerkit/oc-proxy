//! Module Overview
//! Config migration logic between schema versions.
//! Transforms legacy config payloads to the current schema in a deterministic way.

use serde_json::{Map, Value};

pub const CURRENT_CONFIG_VERSION: u32 = 2;

pub fn migrate_config(input: Value) -> Result<Value, String> {
    let mut root = ensure_object_root(input);
    let mut version = detect_config_version(&root);

    if version > CURRENT_CONFIG_VERSION {
        return Err(format!(
            "configVersion {version} is newer than supported version {CURRENT_CONFIG_VERSION}"
        ));
    }

    while version < CURRENT_CONFIG_VERSION {
        root = match version {
            1 => migrate_v1_to_v2(root),
            _ => {
                return Err(format!(
                    "missing migrator for configVersion {version} -> {}",
                    version + 1
                ));
            }
        };
        version += 1;
    }

    if let Some(obj) = root.as_object_mut() {
        obj.insert(
            "configVersion".to_string(),
            Value::Number((CURRENT_CONFIG_VERSION as u64).into()),
        );
    }

    Ok(root)
}

fn ensure_object_root(input: Value) -> Value {
    if input.is_object() {
        input
    } else {
        Value::Object(Map::new())
    }
}

fn detect_config_version(root: &Value) -> u32 {
    root.get("configVersion")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(1)
}

fn migrate_v1_to_v2(mut root: Value) -> Value {
    let Some(obj) = root.as_object_mut() else {
        return Value::Object(Map::new());
    };

    let locale = obj
        .get("ui")
        .and_then(Value::as_object)
        .and_then(|ui| ui.get("locale"))
        .and_then(Value::as_str)
        .unwrap_or("en-US");
    let default_locale_mode = if locale == "zh-CN" { "manual" } else { "auto" };
    let ui = obj
        .entry("ui".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(ui_obj) = ui.as_object_mut() {
        if !ui_obj.contains_key("localeMode") {
            ui_obj.insert(
                "localeMode".to_string(),
                Value::String(default_locale_mode.to_string()),
            );
        }
    }

    let remote = obj
        .entry("remoteGit".to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    if let Some(remote_obj) = remote.as_object_mut() {
        let repo_url = remote_obj
            .get("repoUrl")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let token = remote_obj
            .get("token")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !remote_obj.contains_key("enabled") {
            remote_obj.insert(
                "enabled".to_string(),
                Value::Bool(!repo_url.trim().is_empty() || !token.trim().is_empty()),
            );
        }
        let branch = remote_obj
            .get("branch")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if branch.trim().is_empty() {
            remote_obj.insert("branch".to_string(), Value::String("main".to_string()));
        }
    }

    obj.insert("configVersion".to_string(), Value::Number(2u64.into()));
    root
}

#[cfg(test)]
mod tests {
    use super::{migrate_config, CURRENT_CONFIG_VERSION};
    use serde_json::json;

    #[test]
    fn migrate_defaults_missing_version_to_current() {
        let migrated = migrate_config(json!({})).expect("migration should succeed");
        assert_eq!(migrated["configVersion"], CURRENT_CONFIG_VERSION);
    }

    #[test]
    fn migrate_v1_to_v2_fills_locale_mode_and_remote_defaults() {
        let migrated = migrate_config(json!({
            "ui": {
                "locale": "zh-CN"
            },
            "remoteGit": {
                "repoUrl": "https://github.com/demo/repo.git",
                "token": "tok"
            }
        }))
        .expect("migration should succeed");

        assert_eq!(migrated["configVersion"], 2);
        assert_eq!(migrated["ui"]["localeMode"], "manual");
        assert_eq!(migrated["remoteGit"]["enabled"], true);
        assert_eq!(migrated["remoteGit"]["branch"], "main");
    }

    #[test]
    fn migrate_rejects_future_version() {
        let err = migrate_config(json!({
            "configVersion": CURRENT_CONFIG_VERSION + 1
        }))
        .expect_err("future version must fail");
        assert!(err.contains("newer than supported"));
    }

    #[test]
    fn migrate_is_idempotent_on_current_version() {
        let input = json!({
            "configVersion": CURRENT_CONFIG_VERSION,
            "ui": { "locale": "en-US", "localeMode": "auto" },
            "remoteGit": { "enabled": false, "repoUrl": "", "token": "", "branch": "main" }
        });
        let migrated = migrate_config(input.clone()).expect("migration should succeed");
        assert_eq!(migrated["configVersion"], CURRENT_CONFIG_VERSION);
        assert_eq!(migrated["ui"]["localeMode"], "auto");
        assert_eq!(migrated["remoteGit"]["branch"], "main");
    }
}
