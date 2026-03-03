use chrono::Utc;

use crate::models::{Group, GroupsBackupPayload};

pub fn create_groups_backup_payload(groups: &[Group]) -> GroupsBackupPayload {
    GroupsBackupPayload {
        format: "ai-open-router-groups-backup".to_string(),
        version: 1,
        exported_at: Utc::now().to_rfc3339(),
        groups: groups.to_vec(),
    }
}

pub fn extract_groups_from_import_payload(input: &serde_json::Value) -> Result<Vec<Group>, String> {
    if let Some(arr) = input.as_array() {
        return serde_json::from_value::<Vec<Group>>(serde_json::Value::Array(arr.clone()))
            .map_err(|e| format!("Invalid groups array: {e}"));
    }

    if let Some(groups) = input.get("groups") {
        return serde_json::from_value::<Vec<Group>>(groups.clone())
            .map_err(|e| format!("Invalid groups field: {e}"));
    }

    if let Some(config) = input.get("config") {
        if let Some(groups) = config.get("groups") {
            return serde_json::from_value::<Vec<Group>>(groups.clone())
                .map_err(|e| format!("Invalid config.groups field: {e}"));
        }
    }

    Err("Invalid import JSON: expected a groups array".to_string())
}

pub fn backup_default_file_name() -> String {
    let now = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    format!("ai-open-router-groups-backup-{now}.json")
}

#[cfg(test)]
mod tests {
    use super::{
        backup_default_file_name, create_groups_backup_payload, extract_groups_from_import_payload,
    };
    use crate::models::{default_rule_quota_config, Group, Rule, RuleProtocol};
    use chrono::DateTime;
    use serde_json::json;
    use std::collections::HashMap;

    fn sample_group(id: &str, name: &str) -> Group {
        Group {
            id: id.to_string(),
            name: name.to_string(),
            models: vec![],
            active_rule_id: None,
            rules: vec![Rule {
                id: "r1".to_string(),
                name: "rule-1".to_string(),
                protocol: RuleProtocol::Anthropic,
                token: "t1".to_string(),
                api_address: "https://api.example.com".to_string(),
                default_model: "claude-3-7-sonnet".to_string(),
                model_mappings: HashMap::new(),
                quota: default_rule_quota_config(),
            }],
        }
    }

    #[test]
    fn create_groups_backup_payload_keeps_groups_and_metadata() {
        let groups = vec![sample_group("demo", "Demo")];
        let payload = create_groups_backup_payload(&groups);

        assert_eq!(payload.format, "ai-open-router-groups-backup");
        assert_eq!(payload.version, 1);
        assert_eq!(payload.groups.len(), groups.len());
        assert_eq!(payload.groups[0].id, groups[0].id);
        assert_eq!(payload.groups[0].name, groups[0].name);
        assert!(DateTime::parse_from_rfc3339(&payload.exported_at).is_ok());
    }

    #[test]
    fn extract_groups_from_import_payload_supports_root_groups_object() {
        let out = extract_groups_from_import_payload(&json!({
            "groups": [
                {
                    "id": "g1",
                    "name": "Group 1",
                    "models": [],
                    "activeRuleId": null,
                    "rules": []
                }
            ]
        }))
        .expect("payload should parse");
        assert_eq!(out[0].id, "g1");
    }

    #[test]
    fn extract_groups_from_import_payload_supports_groups_array_root() {
        let out = extract_groups_from_import_payload(&json!([
            {
                "id": "g2",
                "name": "Group 2",
                "models": [],
                "activeRuleId": null,
                "rules": []
            }
        ]))
        .expect("payload should parse");
        assert_eq!(out[0].id, "g2");
    }

    #[test]
    fn extract_groups_from_import_payload_supports_full_config_envelope() {
        let out = extract_groups_from_import_payload(&json!({
            "config": {
                "groups": [
                    {
                        "id": "g3",
                        "name": "Group 3",
                        "models": [],
                        "activeRuleId": null,
                        "rules": []
                    }
                ]
            }
        }))
        .expect("payload should parse");
        assert_eq!(out[0].id, "g3");
    }

    #[test]
    fn extract_groups_from_import_payload_rejects_invalid_payload() {
        let err =
            extract_groups_from_import_payload(&json!({ "invalid": true })).expect_err("must fail");
        assert!(err.contains("expected a groups array"));
    }

    #[test]
    fn backup_default_file_name_has_expected_shape() {
        let file_name = backup_default_file_name();
        assert!(file_name.starts_with("ai-open-router-groups-backup-"));
        assert!(file_name.ends_with(".json"));
    }
}
