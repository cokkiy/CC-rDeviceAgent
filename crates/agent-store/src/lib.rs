use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub const LATEST_SCHEMA_VERSION: i64 = 1;

pub struct StateStore {
    connection: Connection,
}

impl StateStore {
    pub fn open(path: impl AsRef<Path>) -> StoreResult<Self> {
        let connection = Connection::open(path)?;
        let store = Self { connection };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> StoreResult<Self> {
        let connection = Connection::open_in_memory()?;
        let store = Self { connection };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    pub fn schema_version(&self) -> StoreResult<i64> {
        let version = self
            .connection
            .query_row(
                "SELECT version FROM schema_version WHERE id = 1",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(version)
    }

    pub fn save_capability_profile(
        &self,
        profile: &pal_core::CapabilityProfile,
    ) -> StoreResult<()> {
        let json = serde_json::to_string(profile)?;
        self.connection.execute(
            "INSERT INTO capability_profile_cache(id, profile_json, detected_at_unix_ms)
             VALUES(1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET
               profile_json = excluded.profile_json,
               detected_at_unix_ms = excluded.detected_at_unix_ms",
            params![
                json,
                profile.detected_at_unix_ms.min(i64::MAX as u64) as i64
            ],
        )?;
        Ok(())
    }

    pub fn load_capability_profile(&self) -> StoreResult<Option<pal_core::CapabilityProfile>> {
        let raw = self
            .connection
            .query_row(
                "SELECT profile_json FROM capability_profile_cache WHERE id = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        raw.map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(StoreError::from)
    }

    pub fn backup_to(&self, path: impl AsRef<Path>) -> StoreResult<()> {
        let escaped = path.as_ref().display().to_string().replace('\'', "''");
        self.connection
            .execute_batch(&format!("VACUUM main INTO '{escaped}'"))?;
        Ok(())
    }

    fn configure(&self) -> StoreResult<()> {
        self.connection.pragma_update(None, "journal_mode", "WAL")?;
        self.connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(())
    }

    fn migrate(&self) -> StoreResult<()> {
        self.connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS schema_version (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                version INTEGER NOT NULL,
                applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );

            INSERT INTO schema_version(id, version)
            VALUES(1, 0)
            ON CONFLICT(id) DO NOTHING;
            ",
        )?;

        if self.schema_version()? < 1 {
            self.connection.execute_batch(SCHEMA_V1)?;
            self.connection.execute(
                "UPDATE schema_version SET version = ?1, applied_at = CURRENT_TIMESTAMP WHERE id = 1",
                params![LATEST_SCHEMA_VERSION],
            )?;
        }

        Ok(())
    }
}

const SCHEMA_V1: &str = "
CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL,
    state TEXT NOT NULL,
    payload_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS config_versions (
    scope TEXT NOT NULL,
    key TEXT NOT NULL,
    version INTEGER NOT NULL,
    value_json TEXT NOT NULL,
    signature TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(scope, key, version)
);

CREATE TABLE IF NOT EXISTS app_manifests (
    app_id TEXT PRIMARY KEY,
    version TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    action TEXT NOT NULL,
    target TEXT NOT NULL,
    result TEXT NOT NULL,
    event_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS upgrade_state (
    id TEXT PRIMARY KEY,
    target_version TEXT NOT NULL,
    state TEXT NOT NULL,
    state_json TEXT NOT NULL DEFAULT '{}',
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS key_refs (
    name TEXT PRIMARY KEY,
    provider TEXT NOT NULL,
    reference TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS capability_profile_cache (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    profile_json TEXT NOT NULL,
    detected_at_unix_ms INTEGER NOT NULL
);
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_empty_database() {
        let store = StateStore::open_in_memory().unwrap();
        assert_eq!(store.schema_version().unwrap(), LATEST_SCHEMA_VERSION);
    }

    #[test]
    fn stores_capability_profile() {
        let store = StateStore::open_in_memory().unwrap();
        let mut profile = pal_core::CapabilityProfile::current_platform();
        profile.has_cgroup_v2 = true;
        store.save_capability_profile(&profile).unwrap();
        let loaded = store.load_capability_profile().unwrap().unwrap();
        assert!(loaded.has_cgroup_v2);
    }
}
