//! Module Overview
//! Persistent config store load/save helpers.
//! Encapsulates disk I/O and provides a single source of truth for runtime config access.

use crate::config::migrator::migrate_config;
use crate::config::schema::normalize_config;
use crate::models::{
    default_config, default_group_failover_config, validate_config, Group, ProxyConfig, Rule,
};
use chrono::Utc;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use uuid::Uuid;

#[derive(Clone)]
pub struct ConfigStore {
    file_path: PathBuf,
    groups_db_path: PathBuf,
    groups_db: Arc<Mutex<Connection>>,
    config: Arc<RwLock<ProxyConfig>>,
    revision: Arc<AtomicU64>,
}

impl ConfigStore {
    /// Performs new.
    pub fn new(file_path: PathBuf) -> Self {
        let groups_db_path = file_path.with_file_name("providers.sqlite");
        let groups_db = Connection::open(&groups_db_path).unwrap_or_else(|_| {
            Connection::open_in_memory()
                .expect("open in-memory sqlite connection for config should not fail")
        });
        Self {
            file_path,
            groups_db_path,
            groups_db: Arc::new(Mutex::new(groups_db)),
            config: Arc::new(RwLock::new(default_config())),
            revision: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Initializes data for this module's workflow.
    pub fn initialize(&self) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create config dir failed: {e}"))?;
        }
        if let Some(parent) = self.groups_db_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create groups db dir failed: {e}"))?;
        }
        self.reopen_groups_db()?;
        self.initialize_groups_db()?;
        self.migrate_group_records_schema_if_needed()?;
        self.migrate_provider_records_schema_if_needed()?;

        if !self.file_path.exists() {
            let defaults = normalize_config_for_storage(default_config())?;
            self.save_config_to_db(&defaults)?;
            self.write_file(&defaults)?;
            self.set_in_memory(defaults);
            return Ok(());
        }

        let raw = std::fs::read_to_string(&self.file_path)
            .map_err(|e| format!("read config failed: {e}"))?;

        let parsed = serde_json::from_str::<serde_json::Value>(&raw)
            .unwrap_or_else(|_| serde_json::json!({}));
        let migrated = migrate_config(parsed)?;
        let mut normalized = normalize_config_for_storage(normalize_config(migrated)?)?;
        let (groups_from_db, providers_from_db) = self.load_groups_and_providers_from_db()?;
        let db_is_empty = groups_from_db.is_empty() && providers_from_db.is_empty();
        if !db_is_empty {
            normalized.groups = groups_from_db;
            normalized.providers = providers_from_db;
        }
        if db_is_empty {
            self.save_config_to_db(&normalized)?;
        }

        if let Err(err) = validate_config(&normalized) {
            let defaults = normalize_config_for_storage(default_config())?;
            self.save_config_to_db(&defaults)?;
            self.write_file(&defaults)?;
            self.set_in_memory(defaults);
            return Err(format!("config invalid, reset to default: {err}"));
        }

        self.write_file(&normalized)?;
        self.set_in_memory(normalized);
        Ok(())
    }

    /// Performs get.
    pub fn get(&self) -> ProxyConfig {
        self.config.read().expect("config rwlock poisoned").clone()
    }

    /// Saves data for this module's workflow.
    pub fn save(&self, next_config: serde_json::Value) -> Result<ProxyConfig, String> {
        let migrated = migrate_config(next_config)?;
        let normalized = normalize_config_for_storage(normalize_config(migrated)?)?;
        validate_config(&normalized)?;
        self.save_config_to_db(&normalized)?;
        self.write_file(&normalized)?;
        self.set_in_memory(normalized.clone());
        Ok(normalized)
    }

    /// Saves config for this module's workflow.
    pub fn save_config(&self, next_config: ProxyConfig) -> Result<ProxyConfig, String> {
        let normalized = normalize_config_for_storage(next_config)?;
        validate_config(&normalized)?;
        self.save_config_to_db(&normalized)?;
        self.write_file(&normalized)?;
        self.set_in_memory(normalized.clone());
        Ok(normalized)
    }

    /// Writes file for this module's workflow.
    fn write_file(&self, cfg: &ProxyConfig) -> Result<(), String> {
        let mut storage_cfg = cfg.clone();
        storage_cfg.groups = vec![];
        storage_cfg.providers = vec![];
        let text = serde_json::to_string_pretty(&storage_cfg)
            .map_err(|e| format!("serialize config failed: {e}"))?;
        std::fs::write(&self.file_path, text).map_err(|e| format!("write config failed: {e}"))
    }

    /// Initializes groups db for this module's workflow.
    fn initialize_groups_db(&self) -> Result<(), String> {
        let conn = self
            .groups_db
            .lock()
            .map_err(|_| "groups db lock poisoned".to_string())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS group_records (
                group_id TEXT PRIMARY KEY,
                group_name TEXT NOT NULL,
                models_json TEXT NOT NULL,
                active_provider_id TEXT,
                provider_ids_json TEXT NOT NULL,
                group_json TEXT,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS provider_records (
                provider_id TEXT PRIMARY KEY,
                provider_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )
        .map_err(|e| format!("create groups db schema failed: {e}"))?;
        Ok(())
    }

    /// Migrates legacy group table schema to include serialized group payload storage.
    fn migrate_group_records_schema_if_needed(&self) -> Result<(), String> {
        let conn = self
            .groups_db
            .lock()
            .map_err(|_| "groups db lock poisoned".to_string())?;
        let group_columns = table_columns(&conn, "group_records")?;
        if group_columns.contains("group_json") {
            return Ok(());
        }

        conn.execute("ALTER TABLE group_records ADD COLUMN group_json TEXT", [])
            .map_err(|e| format!("migrate group_records schema failed: {e}"))?;
        Ok(())
    }

    /// Migrates legacy provider table schema `(group_id, provider_id)` to global provider storage.
    ///
    /// NOTE:
    /// SQLite does not support altering an existing primary key definition in-place.
    /// We therefore rename the legacy table, create a new one with the target
    /// constraints, backfill normalized data, and finally drop the legacy table.
    fn migrate_provider_records_schema_if_needed(&self) -> Result<(), String> {
        let mut conn = self
            .groups_db
            .lock()
            .map_err(|_| "groups db lock poisoned".to_string())?;
        let provider_columns = table_columns(&conn, "provider_records")?;
        if !provider_columns.contains("group_id") {
            return Ok(());
        }

        let legacy_groups = load_groups_from_legacy_relational_tables(&conn)?;
        let (next_groups, next_providers) = normalize_groups_and_providers(legacy_groups, vec![]);

        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_provider_records_group_id;
             DROP TABLE IF EXISTS provider_records_legacy;
             ALTER TABLE provider_records RENAME TO provider_records_legacy;
             CREATE TABLE provider_records (
                provider_id TEXT PRIMARY KEY,
                provider_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
             );",
        )
        .map_err(|e| format!("migrate provider_records schema failed: {e}"))?;

        persist_groups_and_providers(&mut conn, &next_groups, &next_providers)?;
        conn.execute("DROP TABLE IF EXISTS provider_records_legacy", [])
            .map_err(|e| format!("cleanup provider_records legacy table failed: {e}"))?;
        Ok(())
    }

    /// Performs reopen groups db.
    fn reopen_groups_db(&self) -> Result<(), String> {
        let next_conn = Connection::open(&self.groups_db_path)
            .map_err(|e| format!("open groups db failed: {e}"))?;
        let mut conn = self
            .groups_db
            .lock()
            .map_err(|_| "groups db lock poisoned".to_string())?;
        *conn = next_conn;
        Ok(())
    }

    /// Loads groups/providers from db for this module's workflow.
    fn load_groups_and_providers_from_db(&self) -> Result<(Vec<Group>, Vec<Rule>), String> {
        let conn = self
            .groups_db
            .lock()
            .map_err(|_| "groups db lock poisoned".to_string())?;
        load_groups_and_providers_from_relational_tables(&conn)
    }

    /// Saves config groups/providers to db for this module's workflow.
    fn save_config_to_db(&self, config: &ProxyConfig) -> Result<(), String> {
        let mut conn = self
            .groups_db
            .lock()
            .map_err(|_| "groups db lock poisoned".to_string())?;
        persist_groups_and_providers(&mut conn, &config.groups, &config.providers)
    }

    /// Performs set in memory.
    fn set_in_memory(&self, cfg: ProxyConfig) {
        if let Ok(mut guard) = self.config.write() {
            *guard = cfg;
            let _ = self.revision.fetch_add(1, Ordering::Release);
        }
    }

    /// Performs path.
    pub fn path(&self) -> &Path {
        &self.file_path
    }

    /// Performs shared config.
    pub fn shared_config(&self) -> Arc<RwLock<ProxyConfig>> {
        self.config.clone()
    }

    /// Performs shared revision.
    pub fn shared_revision(&self) -> Arc<AtomicU64> {
        self.revision.clone()
    }
}

/// Normalizes groups/providers into global provider storage and provider-id links.
fn normalize_groups_and_providers(
    groups: Vec<Group>,
    providers: Vec<Rule>,
) -> (Vec<Group>, Vec<Rule>) {
    let mut provider_map: HashMap<String, Rule> = HashMap::new();
    let mut provider_order: Vec<String> = Vec::new();
    for provider in providers {
        let provider_id = provider.id.trim().to_string();
        if provider_id.is_empty() {
            continue;
        }
        if provider_map.contains_key(&provider_id) {
            continue;
        }
        provider_order.push(provider_id.clone());
        provider_map.insert(provider_id, provider);
    }
    let has_global_providers = !provider_order.is_empty();

    let mut normalized_groups = Vec::new();
    for group in groups {
        let scoped_provider_ids: HashSet<String> = if has_global_providers {
            group
                .provider_ids
                .iter()
                .map(|provider_id| provider_id.trim())
                .filter(|provider_id| !provider_id.is_empty())
                .map(|provider_id| provider_id.to_string())
                .collect()
        } else {
            group
                .providers
                .iter()
                .map(|provider| provider.id.trim())
                .filter(|provider_id| !provider_id.is_empty())
                .map(|provider_id| provider_id.to_string())
                .collect()
        };
        let mut group_provider_id_remap: HashMap<String, String> = HashMap::new();
        for provider in &group.providers {
            let provider_id = provider.id.trim().to_string();
            if provider_id.is_empty() {
                continue;
            }
            if !scoped_provider_ids.contains(&provider_id) {
                continue;
            }
            if let Some(existing_provider) = provider_map.get(&provider_id) {
                let existing_json = serde_json::to_string(existing_provider).unwrap_or_default();
                let incoming_json = serde_json::to_string(provider).unwrap_or_default();
                if existing_json != incoming_json {
                    let next_provider_id = alloc_unique_provider_id(&provider_id, &provider_map);
                    let mut next_provider = provider.clone();
                    next_provider.id = next_provider_id.clone();
                    provider_order.push(next_provider_id.clone());
                    provider_map.insert(next_provider_id.clone(), next_provider);
                    group_provider_id_remap.insert(provider_id, next_provider_id);
                }
                continue;
            }
            provider_order.push(provider_id.clone());
            provider_map.insert(provider_id, provider.clone());
        }

        let raw_provider_ids = if has_global_providers {
            group.provider_ids.clone()
        } else {
            group
                .providers
                .iter()
                .map(|provider| provider.id.clone())
                .collect()
        };
        let mut seen_ids = HashSet::new();
        let mut provider_ids = Vec::new();
        for provider_id in raw_provider_ids {
            let mut normalized_id = provider_id.trim().to_string();
            if let Some(remapped_id) = group_provider_id_remap.get(&normalized_id) {
                normalized_id = remapped_id.clone();
            }
            if normalized_id.is_empty() || seen_ids.contains(&normalized_id) {
                continue;
            }
            if provider_map.contains_key(&normalized_id) {
                seen_ids.insert(normalized_id.clone());
                provider_ids.push(normalized_id);
            }
        }

        let providers_for_group: Vec<Rule> = provider_ids
            .iter()
            .filter_map(|provider_id| provider_map.get(provider_id).cloned())
            .collect();
        let active_provider_id = group
            .active_provider_id
            .filter(|active| provider_ids.iter().any(|provider_id| provider_id == active));

        normalized_groups.push(Group {
            id: group.id,
            name: group.name,
            models: group.models,
            provider_ids,
            active_provider_id,
            providers: providers_for_group,
            failover: group.failover,
        });
    }

    let normalized_providers: Vec<Rule> = provider_order
        .into_iter()
        .filter_map(|provider_id| provider_map.get(&provider_id).cloned())
        .collect();

    (normalized_groups, normalized_providers)
}

/// Allocates a non-conflicting provider id for a conflicting provider payload.
fn alloc_unique_provider_id(provider_id: &str, provider_map: &HashMap<String, Rule>) -> String {
    let base = provider_id.trim();
    if !base.is_empty() {
        for idx in 1..1000 {
            let candidate = format!("{base}-{}", idx + 1);
            if !provider_map.contains_key(&candidate) {
                return candidate;
            }
        }
    }
    loop {
        let candidate = Uuid::new_v4().to_string();
        if !provider_map.contains_key(&candidate) {
            return candidate;
        }
    }
}

/// Normalizes full config structure for this module's workflow.
fn normalize_config_for_storage(config: ProxyConfig) -> Result<ProxyConfig, String> {
    let (groups, providers) = normalize_groups_and_providers(config.groups, config.providers);
    Ok(ProxyConfig {
        groups,
        providers,
        ..config
    })
}

/// Persists groups/providers into sqlite tables.
fn persist_groups_and_providers(
    conn: &mut Connection,
    groups: &[Group],
    providers: &[Rule],
) -> Result<(), String> {
    let tx = conn
        .transaction()
        .map_err(|e| format!("begin groups transaction failed: {e}"))?;
    tx.execute("DELETE FROM group_records", [])
        .map_err(|e| format!("clear group_records failed: {e}"))?;
    tx.execute("DELETE FROM provider_records", [])
        .map_err(|e| format!("clear provider_records failed: {e}"))?;

    let now = Utc::now().timestamp_millis();
    for group in groups {
        let models_json = serde_json::to_string(&group.models)
            .map_err(|e| format!("serialize group models failed: {e}"))?;
        let provider_ids_json = serde_json::to_string(&group.provider_ids)
            .map_err(|e| format!("serialize provider ids failed: {e}"))?;
        tx.execute(
            "INSERT INTO group_records(group_id, group_name, models_json, active_provider_id, provider_ids_json, group_json, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                group.id,
                group.name,
                models_json,
                group.active_provider_id,
                provider_ids_json,
                serde_json::to_string(group)
                    .map_err(|e| format!("serialize group failed: {e}"))?,
                now
            ],
        )
        .map_err(|e| format!("insert group record failed: {e}"))?;
    }

    for provider in providers {
        let provider_json = serde_json::to_string(provider)
            .map_err(|e| format!("serialize provider failed: {e}"))?;
        tx.execute(
            "INSERT INTO provider_records(provider_id, provider_json, updated_at)
             VALUES(?1, ?2, ?3)",
            params![provider.id, provider_json, now],
        )
        .map_err(|e| format!("insert provider record failed: {e}"))?;
    }

    tx.commit()
        .map_err(|e| format!("commit groups transaction failed: {e}"))?;
    Ok(())
}

/// Loads groups/providers from new relational tables.
fn load_groups_and_providers_from_relational_tables(
    conn: &Connection,
) -> Result<(Vec<Group>, Vec<Rule>), String> {
    let provider_columns = table_columns(conn, "provider_records")?;
    let provider_query = select_records_with_soft_delete_filter(
        "SELECT provider_id, provider_json FROM provider_records",
        &provider_columns,
        Some("ORDER BY rowid ASC"),
    );
    let mut provider_stmt = conn
        .prepare(&provider_query)
        .map_err(|e| format!("prepare provider_records query failed: {e}"))?;
    let provider_rows = provider_stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("query provider_records failed: {e}"))?;
    let mut provider_map: HashMap<String, Rule> = HashMap::new();
    let mut providers = Vec::new();
    for row in provider_rows {
        let (provider_id, raw_json) =
            row.map_err(|e| format!("read provider_records row failed: {e}"))?;
        let provider = serde_json::from_str::<Rule>(&raw_json)
            .map_err(|e| format!("parse provider_json failed: {e}"))?;
        provider_map.insert(provider_id, provider.clone());
        providers.push(provider);
    }

    let group_columns = table_columns(conn, "group_records")?;
    let group_query = if group_columns.contains("group_json") {
        select_records_with_soft_delete_filter(
            "SELECT group_json FROM group_records",
            &group_columns,
            Some("ORDER BY rowid ASC"),
        )
    } else {
        select_records_with_soft_delete_filter(
            "SELECT group_id, group_name, models_json, active_provider_id, provider_ids_json FROM group_records",
            &group_columns,
            Some("ORDER BY rowid ASC"),
        )
    };
    let mut group_stmt = conn
        .prepare(&group_query)
        .map_err(|e| format!("prepare group_records query failed: {e}"))?;
    let mut groups = Vec::new();
    if group_columns.contains("group_json") {
        let group_rows = group_stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| format!("query group_records failed: {e}"))?;
        for row in group_rows {
            let raw_group_json = row.map_err(|e| format!("read group_records row failed: {e}"))?;
            let mut group = serde_json::from_str::<Group>(&raw_group_json)
                .map_err(|e| format!("parse group_json failed: {e}"))?;
            group.providers = group
                .provider_ids
                .iter()
                .filter_map(|provider_id| provider_map.get(provider_id).cloned())
                .collect();
            groups.push(group);
        }
    } else {
        let group_rows = group_stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| format!("query group_records failed: {e}"))?;
        for row in group_rows {
            let (group_id, group_name, models_json, active_provider_id, provider_ids_json) =
                row.map_err(|e| format!("read group_records row failed: {e}"))?;
            let models = serde_json::from_str::<Vec<String>>(&models_json)
                .map_err(|e| format!("parse models_json failed: {e}"))?;
            let provider_ids = serde_json::from_str::<Vec<String>>(&provider_ids_json)
                .map_err(|e| format!("parse provider_ids_json failed: {e}"))?;
            let providers_for_group = provider_ids
                .iter()
                .filter_map(|provider_id| provider_map.get(provider_id).cloned())
                .collect();
            groups.push(Group {
                id: group_id,
                name: group_name,
                models,
                provider_ids,
                active_provider_id,
                providers: providers_for_group,
                failover: default_group_failover_config(),
            });
        }
    }

    Ok((groups, providers))
}

/// Loads groups from legacy relational tables where provider table still used `(group_id, provider_id)`.
fn load_groups_from_legacy_relational_tables(conn: &Connection) -> Result<Vec<Group>, String> {
    let provider_columns = table_columns(conn, "provider_records")?;
    let provider_query = select_records_with_soft_delete_filter(
        "SELECT group_id, provider_id, provider_json FROM provider_records",
        &provider_columns,
        None,
    );
    let mut provider_stmt = conn
        .prepare(&provider_query)
        .map_err(|e| format!("prepare provider_records query failed: {e}"))?;
    let provider_rows = provider_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|e| format!("query provider_records failed: {e}"))?;
    let mut provider_map: HashMap<(String, String), Rule> = HashMap::new();
    for row in provider_rows {
        let (group_id, provider_id, raw_json) =
            row.map_err(|e| format!("read provider_records row failed: {e}"))?;
        let provider = serde_json::from_str::<Rule>(&raw_json)
            .map_err(|e| format!("parse provider_json failed: {e}"))?;
        provider_map.insert((group_id, provider_id), provider);
    }

    let group_columns = table_columns(conn, "group_records")?;
    let group_query = select_records_with_soft_delete_filter(
        "SELECT group_id, group_name, models_json, active_provider_id, provider_ids_json FROM group_records",
        &group_columns,
        Some("ORDER BY rowid ASC"),
    );
    let mut group_stmt = conn
        .prepare(&group_query)
        .map_err(|e| format!("prepare group_records query failed: {e}"))?;
    let group_rows = group_stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|e| format!("query group_records failed: {e}"))?;

    let mut groups = Vec::new();
    for row in group_rows {
        let (group_id, group_name, models_json, active_provider_id, provider_ids_json) =
            row.map_err(|e| format!("read group_records row failed: {e}"))?;
        let models = serde_json::from_str::<Vec<String>>(&models_json)
            .map_err(|e| format!("parse models_json failed: {e}"))?;
        let provider_ids = serde_json::from_str::<Vec<String>>(&provider_ids_json)
            .map_err(|e| format!("parse provider_ids_json failed: {e}"))?;
        let providers = provider_ids
            .iter()
            .filter_map(|provider_id| provider_map.get(&(group_id.clone(), provider_id.clone())))
            .cloned()
            .collect();
        groups.push(Group {
            id: group_id,
            name: group_name,
            models,
            provider_ids,
            active_provider_id,
            providers,
            failover: default_group_failover_config(),
        });
    }
    Ok(groups)
}

/// Collects table column names for this module's workflow.
fn table_columns(conn: &Connection, table_name: &str) -> Result<HashSet<String>, String> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table_name})"))
        .map_err(|e| format!("prepare table info query failed for {table_name}: {e}"))?;
    let column_rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("query table info failed for {table_name}: {e}"))?;
    let mut columns = HashSet::new();
    for column_row in column_rows {
        let column_name =
            column_row.map_err(|e| format!("read table info row failed for {table_name}: {e}"))?;
        columns.insert(column_name.to_ascii_lowercase());
    }
    Ok(columns)
}

/// Builds query SQL and applies soft-delete filters when legacy columns exist.
fn select_records_with_soft_delete_filter(
    base_query: &str,
    columns: &HashSet<String>,
    suffix: Option<&str>,
) -> String {
    let mut filters: Vec<&str> = Vec::new();
    if columns.contains("is_deleted") {
        filters.push("COALESCE(is_deleted, 0) = 0");
    }
    if columns.contains("deleted") {
        filters.push("COALESCE(deleted, 0) = 0");
    }
    if columns.contains("deleted_at") {
        filters.push("(deleted_at IS NULL OR CAST(deleted_at AS TEXT) = '' OR CAST(deleted_at AS TEXT) = '0')");
    }
    if columns.contains("active") {
        filters.push("COALESCE(active, 1) = 1");
    }
    if columns.contains("is_active") {
        filters.push("COALESCE(is_active, 1) = 1");
    }

    let mut query = base_query.to_string();
    if !filters.is_empty() {
        query.push_str(" WHERE ");
        query.push_str(&filters.join(" AND "));
    }
    if let Some(suffix_clause) = suffix {
        query.push(' ');
        query.push_str(suffix_clause);
    }
    query
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::{
        default_rule_cost_config, default_rule_quota_config, Rule, RuleProtocol,
    };
    use std::collections::HashMap;
    use uuid::Uuid;

    /// Performs sample group.
    fn sample_group() -> Group {
        Group {
            id: "group-1".to_string(),
            name: "group-1".to_string(),
            models: vec!["gpt-4o-mini".to_string()],
            provider_ids: vec!["provider-1".to_string()],
            active_provider_id: Some("provider-1".to_string()),
            providers: vec![Rule {
                id: "provider-1".to_string(),
                name: "provider-1".to_string(),
                protocol: RuleProtocol::Openai,
                token: "test-token".to_string(),
                api_address: "https://api.openai.com".to_string(),
                website: String::new(),
                default_model: "gpt-4o-mini".to_string(),
                model_mappings: HashMap::new(),
                header_passthrough_allow: Vec::new(),
                header_passthrough_deny: Vec::new(),
                quota: default_rule_quota_config(),
                cost: default_rule_cost_config(),
            }],
            failover: default_group_failover_config(),
        }
    }

    fn sample_provider(id: &str) -> Rule {
        Rule {
            id: id.to_string(),
            name: id.to_string(),
            protocol: RuleProtocol::Openai,
            token: "test-token".to_string(),
            api_address: "https://api.openai.com".to_string(),
            website: String::new(),
            default_model: "gpt-4o-mini".to_string(),
            model_mappings: HashMap::new(),
            header_passthrough_allow: Vec::new(),
            header_passthrough_deny: Vec::new(),
            quota: default_rule_quota_config(),
            cost: default_rule_cost_config(),
        }
    }

    #[test]
    /// Initializes imports groups from config file into sqlite for this module's workflow.
    fn initialize_imports_groups_from_config_file_into_sqlite() {
        let temp_dir = std::env::temp_dir().join(format!("config-store-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let config_path = temp_dir.join("config.json");
        let db_path = temp_dir.join("providers.sqlite");

        let mut cfg = default_config();
        cfg.groups = vec![sample_group()];
        let raw = serde_json::to_string_pretty(&cfg).expect("serialize config");
        std::fs::write(&config_path, raw).expect("write config");

        let store = ConfigStore::new(config_path.clone());
        store.initialize().expect("initialize config store");

        let in_memory = store.get();
        assert_eq!(in_memory.groups.len(), 1);
        assert_eq!(in_memory.groups[0].providers.len(), 1);

        let config_raw = std::fs::read_to_string(&config_path).expect("read config");
        let config_json: serde_json::Value =
            serde_json::from_str(&config_raw).expect("parse config json");
        assert_eq!(
            config_json.get("groups"),
            Some(&serde_json::Value::Array(vec![]))
        );
        assert_eq!(
            config_json.get("providers"),
            Some(&serde_json::Value::Array(vec![]))
        );

        let conn = Connection::open(&db_path).expect("open providers sqlite");
        let persisted_ids: String = conn
            .query_row(
                "SELECT provider_ids_json FROM group_records WHERE group_id = 'group-1'",
                [],
                |row| row.get(0),
            )
            .expect("query provider_ids_json");
        let ids: Vec<String> = serde_json::from_str(&persisted_ids).expect("decode provider ids");
        assert_eq!(ids, vec!["provider-1".to_string()]);
        let provider_json: String = conn
            .query_row(
                "SELECT provider_json FROM provider_records WHERE provider_id = 'provider-1'",
                [],
                |row| row.get(0),
            )
            .expect("query provider json");
        let provider: crate::domain::entities::Rule =
            serde_json::from_str(&provider_json).expect("decode provider");
        assert_eq!(provider.id, "provider-1");
    }

    #[test]
    fn initialize_loads_group_failover_config_from_sqlite() {
        let temp_dir = std::env::temp_dir().join(format!("config-store-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let config_path = temp_dir.join("config.json");

        let mut cfg = default_config();
        let mut group = sample_group();
        group.failover.enabled = true;
        group.failover.failure_threshold = 5;
        group.failover.cooldown_seconds = 90;
        cfg.groups = vec![group];
        let raw = serde_json::to_string_pretty(&cfg).expect("serialize config");
        std::fs::write(&config_path, raw).expect("write config");

        let first_store = ConfigStore::new(config_path.clone());
        first_store.initialize().expect("first initialize");

        let second_store = ConfigStore::new(config_path.clone());
        second_store.initialize().expect("second initialize");
        let loaded = second_store.get();
        assert_eq!(loaded.groups.len(), 1);
        assert!(loaded.groups[0].failover.enabled);
        assert_eq!(loaded.groups[0].failover.failure_threshold, 5);
        assert_eq!(loaded.groups[0].failover.cooldown_seconds, 90);
    }

    #[test]
    /// Initializes loads groups from sqlite when config groups empty for this module's workflow.
    fn initialize_loads_groups_from_sqlite_when_config_groups_empty() {
        let temp_dir = std::env::temp_dir().join(format!("config-store-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let config_path = temp_dir.join("config.json");

        let mut cfg = default_config();
        cfg.groups = vec![sample_group()];
        let raw = serde_json::to_string_pretty(&cfg).expect("serialize config");
        std::fs::write(&config_path, raw).expect("write config");

        let first_store = ConfigStore::new(config_path.clone());
        first_store.initialize().expect("first initialize");

        let second_store = ConfigStore::new(config_path.clone());
        second_store.initialize().expect("second initialize");
        let loaded = second_store.get();
        assert_eq!(loaded.groups.len(), 1);
        assert_eq!(loaded.groups[0].id, "group-1");
        assert_eq!(loaded.groups[0].providers[0].id, "provider-1");
    }

    #[test]
    /// Initializes keeps groups empty on first run when no groups are configured.
    fn initialize_keeps_groups_empty_when_no_groups_configured() {
        let temp_dir = std::env::temp_dir().join(format!("config-store-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let config_path = temp_dir.join("config.json");

        let store = ConfigStore::new(config_path.clone());
        store.initialize().expect("initialize config store");
        let cfg = store.get();
        assert!(cfg.groups.is_empty());
    }

    #[test]
    /// Keeps groups empty after manual deletion.
    fn initialize_keeps_groups_empty_after_manual_group_deletion() {
        let temp_dir = std::env::temp_dir().join(format!("config-store-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let config_path = temp_dir.join("config.json");

        let first_store = ConfigStore::new(config_path.clone());
        first_store.initialize().expect("first initialize");
        let mut cfg = first_store.get();
        cfg.groups.clear();
        first_store.save_config(cfg).expect("save cleared groups");

        let second_store = ConfigStore::new(config_path.clone());
        second_store.initialize().expect("second initialize");
        let cfg_after = second_store.get();
        assert!(cfg_after.groups.is_empty());
    }

    #[test]
    fn normalize_config_for_storage_ignores_unlinked_group_providers_when_global_providers_present()
    {
        let linked_provider = sample_provider("provider-1");
        let stale_provider = sample_provider("provider-stale");
        let mut cfg = default_config();
        cfg.providers = vec![linked_provider.clone()];
        cfg.groups = vec![Group {
            id: "group-1".to_string(),
            name: "group-1".to_string(),
            models: vec!["gpt-4o-mini".to_string()],
            provider_ids: vec!["provider-1".to_string()],
            active_provider_id: Some("provider-1".to_string()),
            providers: vec![linked_provider.clone(), stale_provider],
            failover: default_group_failover_config(),
        }];

        let normalized = normalize_config_for_storage(cfg).expect("normalize config");
        assert_eq!(normalized.providers.len(), 1);
        assert_eq!(normalized.providers[0].id, "provider-1");
        assert_eq!(
            normalized.groups[0].provider_ids,
            vec!["provider-1".to_string()]
        );
        assert_eq!(normalized.groups[0].providers.len(), 1);
        assert_eq!(normalized.groups[0].providers[0].id, "provider-1");
    }

    #[test]
    fn normalize_config_for_storage_backfills_group_providers_when_global_providers_missing() {
        let linked_provider = sample_provider("provider-1");
        let mut cfg = default_config();
        cfg.groups = vec![Group {
            id: "group-1".to_string(),
            name: "group-1".to_string(),
            models: vec!["gpt-4o-mini".to_string()],
            provider_ids: vec![],
            active_provider_id: Some("provider-1".to_string()),
            providers: vec![linked_provider],
            failover: default_group_failover_config(),
        }];

        let normalized = normalize_config_for_storage(cfg).expect("normalize config");
        assert_eq!(normalized.providers.len(), 1);
        assert_eq!(normalized.providers[0].id, "provider-1");
        assert_eq!(
            normalized.groups[0].provider_ids,
            vec!["provider-1".to_string()]
        );
        assert_eq!(normalized.groups[0].providers.len(), 1);
        assert_eq!(normalized.groups[0].providers[0].id, "provider-1");
    }

    #[test]
    /// Loads groups from relational tables filters soft-deleted rows when legacy columns exist.
    fn load_groups_from_relational_tables_filters_soft_deleted_rows() {
        let conn = Connection::open_in_memory().expect("open in-memory sqlite");
        conn.execute_batch(
            "CREATE TABLE group_records (
                group_id TEXT PRIMARY KEY,
                group_name TEXT NOT NULL,
                models_json TEXT NOT NULL,
                active_provider_id TEXT,
                provider_ids_json TEXT NOT NULL,
                is_deleted INTEGER DEFAULT 0
            );
            CREATE TABLE provider_records (
                group_id TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                provider_json TEXT NOT NULL,
                is_deleted INTEGER DEFAULT 0
            );",
        )
        .expect("create tables");

        let models_json = serde_json::to_string(&vec!["gpt-4o-mini"]).expect("serialize models");
        let provider_ids_json =
            serde_json::to_string(&vec!["provider-1".to_string()]).expect("serialize provider ids");
        let provider_json =
            serde_json::to_string(&sample_group().providers[0]).expect("serialize provider json");

        conn.execute(
            "INSERT INTO group_records(group_id, group_name, models_json, active_provider_id, provider_ids_json, is_deleted)
             VALUES(?1, ?2, ?3, ?4, ?5, 0)",
            params![
                "group-active",
                "group-active",
                models_json.clone(),
                Some("provider-1".to_string()),
                provider_ids_json.clone()
            ],
        )
        .expect("insert active group");
        conn.execute(
            "INSERT INTO group_records(group_id, group_name, models_json, active_provider_id, provider_ids_json, is_deleted)
             VALUES(?1, ?2, ?3, ?4, ?5, 1)",
            params![
                "group-deleted",
                "group-deleted",
                models_json,
                Some("provider-1".to_string()),
                provider_ids_json
            ],
        )
        .expect("insert deleted group");

        conn.execute(
            "INSERT INTO provider_records(group_id, provider_id, provider_json, is_deleted)
             VALUES(?1, ?2, ?3, 0)",
            params!["group-active", "provider-1", provider_json.clone()],
        )
        .expect("insert active provider");
        conn.execute(
            "INSERT INTO provider_records(group_id, provider_id, provider_json, is_deleted)
             VALUES(?1, ?2, ?3, 1)",
            params!["group-active", "provider-deleted", provider_json],
        )
        .expect("insert deleted provider");

        let loaded = load_groups_from_legacy_relational_tables(&conn).expect("load groups");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "group-active");
        assert_eq!(loaded[0].providers.len(), 1);
        assert_eq!(loaded[0].providers[0].id, "provider-1");
    }

    #[test]
    /// Initializes migrates legacy provider_records schema by rebuilding constraints in-place.
    fn initialize_migrates_legacy_provider_records_schema() {
        let temp_dir = std::env::temp_dir().join(format!("config-store-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let config_path = temp_dir.join("config.json");
        let db_path = temp_dir.join("providers.sqlite");

        let cfg = default_config();
        let raw = serde_json::to_string_pretty(&cfg).expect("serialize config");
        std::fs::write(&config_path, raw).expect("write config");

        let conn = Connection::open(&db_path).expect("open sqlite");
        conn.execute_batch(
            "CREATE TABLE group_records (
                group_id TEXT PRIMARY KEY,
                group_name TEXT NOT NULL,
                models_json TEXT NOT NULL,
                active_provider_id TEXT,
                provider_ids_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE provider_records (
                group_id TEXT NOT NULL,
                provider_id TEXT NOT NULL,
                provider_json TEXT NOT NULL
            );
            CREATE INDEX idx_provider_records_group_id ON provider_records(group_id);",
        )
        .expect("create legacy tables");

        let models_json = serde_json::to_string(&vec!["gpt-4o-mini"]).expect("serialize models");
        let provider_ids_json =
            serde_json::to_string(&vec!["provider-1".to_string()]).expect("serialize provider ids");
        let provider_json =
            serde_json::to_string(&sample_group().providers[0]).expect("serialize provider json");

        conn.execute(
            "INSERT INTO group_records(group_id, group_name, models_json, active_provider_id, provider_ids_json, updated_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "group-1",
                "group-1",
                models_json,
                Some("provider-1".to_string()),
                provider_ids_json,
                Utc::now().timestamp_millis()
            ],
        )
        .expect("insert legacy group");
        conn.execute(
            "INSERT INTO provider_records(group_id, provider_id, provider_json)
             VALUES(?1, ?2, ?3)",
            params!["group-1", "provider-1", provider_json],
        )
        .expect("insert legacy provider");
        drop(conn);

        let store = ConfigStore::new(config_path);
        store.initialize().expect("initialize config store");
        let loaded = store.get();
        assert_eq!(loaded.groups.len(), 1);
        assert_eq!(loaded.providers.len(), 1);
        assert_eq!(
            loaded.groups[0].provider_ids,
            vec!["provider-1".to_string()]
        );

        let conn = Connection::open(&db_path).expect("reopen sqlite");
        let provider_columns = table_columns(&conn, "provider_records").expect("table columns");
        assert!(provider_columns.contains("provider_id"));
        assert!(!provider_columns.contains("group_id"));

        let provider_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM provider_records WHERE provider_id = 'provider-1'",
                [],
                |row| row.get(0),
            )
            .expect("query migrated provider count");
        assert_eq!(provider_count, 1);

        let legacy_table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'provider_records_legacy'",
                [],
                |row| row.get(0),
            )
            .expect("query legacy table count");
        assert_eq!(legacy_table_count, 0);
    }
}
