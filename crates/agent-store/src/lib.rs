use std::path::Path;

use agent_core::security::{AuditChain, AuditEvent};
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

    pub fn append_audit_event(&self, mut event: AuditEvent) -> StoreResult<()> {
        let prev_hash = self
            .connection
            .query_row(
                "SELECT hash FROM audit_events ORDER BY sequence DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_default();
        event.prev_hash = prev_hash;
        event.hash = event.calculate_hash(&event.prev_hash);
        let timestamp_ms = event
            .timestamp
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
            .unwrap_or_default();
        let event_json = serde_json::to_string(&event)?;
        let sequence = self
            .connection
            .query_row(
                "SELECT COALESCE(MAX(sequence), 0) + 1 FROM audit_events",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .unwrap_or(1);

        self.connection.execute(
            "INSERT INTO audit_events(
                id, sequence, timestamp_unix_ms, tenant_id, device_id, principal, action, resource,
                target, params_digest, result, trace_id, prev_hash, hash, event_json
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                event.event_id,
                sequence,
                timestamp_ms,
                event.tenant_id,
                event.device_id,
                event.principal,
                format!("{:?}", event.action),
                format!("{:?}", event.resource),
                event.target,
                event.params_digest,
                event.result,
                event.trace_id,
                event.prev_hash,
                event.hash,
                event_json,
            ],
        )?;

        Ok(())
    }

    pub fn load_audit_chain(&self) -> StoreResult<AuditChain> {
        let mut stmt = self.connection.prepare(
            "SELECT event_json, result, prev_hash, hash
             FROM audit_events
             ORDER BY sequence ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let event_json: String = row.get(0)?;
            let mut event: AuditEvent = serde_json::from_str(&event_json).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?;
            event.result = row.get(1)?;
            event.prev_hash = row.get(2)?;
            event.hash = row.get(3)?;
            Ok(event)
        })?;

        let mut chain = AuditChain::default();
        for row in rows {
            chain.push_stored(row?);
        }
        Ok(chain)
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
    sequence INTEGER NOT NULL UNIQUE,
    timestamp_unix_ms INTEGER NOT NULL,
    tenant_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    principal TEXT NOT NULL,
    action TEXT NOT NULL,
    resource TEXT NOT NULL,
    target TEXT NOT NULL,
    params_digest TEXT NOT NULL DEFAULT '',
    result TEXT NOT NULL,
    trace_id TEXT NOT NULL,
    prev_hash TEXT NOT NULL,
    hash TEXT NOT NULL,
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

    #[test]
    fn stores_audit_events_as_verifiable_chain() {
        use agent_core::security::{Action, AuditEvent, Principal, Resource, Role};
        use std::time::{Duration, SystemTime};

        let store = StateStore::open_in_memory().unwrap();
        let principal = Principal::new("tenant-a", "device-1", "operator-user", Role::Operator);

        store
            .append_audit_event(AuditEvent::new(
                "event-1",
                SystemTime::UNIX_EPOCH + Duration::from_secs(1_000),
                principal.clone(),
                Action::Execute,
                Resource::ControlCommand,
                "process:nginx",
                "success",
                "trace-1",
            ))
            .unwrap();
        store
            .append_audit_event(AuditEvent::new(
                "event-2",
                SystemTime::UNIX_EPOCH + Duration::from_secs(1_001),
                principal,
                Action::Read,
                Resource::Telemetry,
                "telemetry:cpu",
                "success",
                "trace-1",
            ))
            .unwrap();

        let chain = store.load_audit_chain().unwrap();
        assert!(chain.verify());
        assert_eq!(chain.events().len(), 2);
    }

    #[test]
    fn audit_chain_verification_fails_after_stored_event_tampering() {
        use agent_core::security::{Action, AuditEvent, Principal, Resource, Role};
        use std::time::{Duration, SystemTime};

        let store = StateStore::open_in_memory().unwrap();
        let principal = Principal::new("tenant-a", "device-1", "operator-user", Role::Operator);

        store
            .append_audit_event(AuditEvent::new(
                "event-1",
                SystemTime::UNIX_EPOCH + Duration::from_secs(1_000),
                principal,
                Action::Execute,
                Resource::ControlCommand,
                "process:nginx",
                "success",
                "trace-1",
            ))
            .unwrap();

        store
            .connection
            .execute(
                "UPDATE audit_events SET result = 'failed' WHERE id = 'event-1'",
                [],
            )
            .unwrap();

        let chain = store.load_audit_chain().unwrap();
        assert!(!chain.verify());
    }
}
