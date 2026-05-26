use std::path::Path;
use std::time::SystemTime;

use agent_core::security::{Action, AuditChain, AuditEvent, Resource};
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SecurityKeyRecord {
    pub name: String,
    pub purpose: String,
    pub provider: String,
    pub reference: String,
    pub security_level: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RbacPolicyRecord {
    pub role: String,
    pub resource: Resource,
    pub action: Action,
    pub allowed: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FileTransferTaskRecord {
    pub task_id: String,
    pub file_name: String,
    pub direction: String,
    pub state: String,
    pub offset: i64,
    pub file_sha256: Option<String>,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct AuditEventFilter {
    pub principal: Option<String>,
    pub action: Option<Action>,
    pub resource: Option<Resource>,
    pub result: Option<String>,
    pub since: Option<SystemTime>,
    pub until: Option<SystemTime>,
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
        let rows = stmt.query_map([], row_to_audit_event)?;

        let mut chain = AuditChain::default();
        for row in rows {
            chain.push_stored(row?);
        }
        Ok(chain)
    }

    pub fn query_audit_events(&self, filter: &AuditEventFilter) -> StoreResult<Vec<AuditEvent>> {
        let mut stmt = self.connection.prepare(
            "SELECT event_json, result, prev_hash, hash
             FROM audit_events
             WHERE (?1 IS NULL OR principal = ?1)
               AND (?2 IS NULL OR action = ?2)
               AND (?3 IS NULL OR resource = ?3)
               AND (?4 IS NULL OR result = ?4)
               AND (?5 IS NULL OR timestamp_unix_ms >= ?5)
               AND (?6 IS NULL OR timestamp_unix_ms <= ?6)
             ORDER BY sequence ASC",
        )?;
        let action = filter.action.map(|action| format!("{action:?}"));
        let resource = filter.resource.map(|resource| format!("{resource:?}"));
        let since = filter.since.map(system_time_to_unix_ms);
        let until = filter.until.map(system_time_to_unix_ms);
        let rows = stmt.query_map(
            params![
                filter.principal.as_deref(),
                action.as_deref(),
                resource.as_deref(),
                filter.result.as_deref(),
                since,
                until,
            ],
            row_to_audit_event,
        )?;

        let mut events = Vec::new();
        for row in rows {
            events.push(row?);
        }
        Ok(events)
    }

    pub fn upsert_security_key(&self, record: &SecurityKeyRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO security_keys(name, purpose, provider, reference, security_level)
             VALUES(?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(name) DO UPDATE SET
               purpose = excluded.purpose,
               provider = excluded.provider,
               reference = excluded.reference,
               security_level = excluded.security_level,
               updated_at = CURRENT_TIMESTAMP",
            params![
                record.name,
                record.purpose,
                record.provider,
                record.reference,
                record.security_level
            ],
        )?;
        Ok(())
    }

    pub fn load_security_key(&self, name: &str) -> StoreResult<Option<SecurityKeyRecord>> {
        self.connection
            .query_row(
                "SELECT name, purpose, provider, reference, security_level
                 FROM security_keys
                 WHERE name = ?1",
                params![name],
                |row| {
                    Ok(SecurityKeyRecord {
                        name: row.get(0)?,
                        purpose: row.get(1)?,
                        provider: row.get(2)?,
                        reference: row.get(3)?,
                        security_level: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    pub fn upsert_rbac_policy(&self, record: &RbacPolicyRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO rbac_policies(role, resource, action, allowed)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(role, resource, action) DO UPDATE SET
               allowed = excluded.allowed,
               updated_at = CURRENT_TIMESTAMP",
            params![
                record.role,
                format!("{:?}", record.resource),
                format!("{:?}", record.action),
                record.allowed
            ],
        )?;
        Ok(())
    }

    pub fn load_rbac_policies(&self) -> StoreResult<Vec<RbacPolicyRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT role, resource, action, allowed
             FROM rbac_policies
             ORDER BY role, resource, action",
        )?;
        let rows = stmt.query_map([], |row| {
            let resource: String = row.get(1)?;
            let action: String = row.get(2)?;
            Ok(RbacPolicyRecord {
                role: row.get(0)?,
                resource: parse_resource(&resource).map_err(to_sql_conversion_error)?,
                action: parse_action(&action).map_err(to_sql_conversion_error)?,
                allowed: row.get(3)?,
            })
        })?;

        let mut policies = Vec::new();
        for row in rows {
            policies.push(row?);
        }
        Ok(policies)
    }

    pub fn try_insert_replay_nonce(
        &self,
        tenant_id: &str,
        device_id: &str,
        principal: &str,
        action: Action,
        nonce: &str,
        timestamp: SystemTime,
    ) -> StoreResult<bool> {
        let inserted = self.connection.execute(
            "INSERT OR IGNORE INTO replay_nonces(
                tenant_id, device_id, principal, action, nonce, timestamp_unix_ms
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                tenant_id,
                device_id,
                principal,
                format!("{action:?}"),
                nonce,
                system_time_to_unix_ms(timestamp)
            ],
        )?;
        Ok(inserted == 1)
    }

    pub fn prune_replay_nonces(&self, older_than: SystemTime) -> StoreResult<usize> {
        self.connection
            .execute(
                "DELETE FROM replay_nonces WHERE timestamp_unix_ms < ?1",
                params![system_time_to_unix_ms(older_than)],
            )
            .map_err(StoreError::from)
    }

    pub fn upsert_file_transfer_task(&self, record: &FileTransferTaskRecord) -> StoreResult<()> {
        self.connection.execute(
            "INSERT INTO file_transfer_tasks(
                task_id, file_name, direction, state, offset, file_sha256
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(task_id) DO UPDATE SET
               file_name = excluded.file_name,
               direction = excluded.direction,
               state = excluded.state,
               offset = excluded.offset,
               file_sha256 = excluded.file_sha256,
               updated_at = CURRENT_TIMESTAMP",
            params![
                record.task_id,
                record.file_name,
                record.direction,
                record.state,
                record.offset,
                record.file_sha256
            ],
        )?;
        Ok(())
    }

    pub fn load_file_transfer_tasks(&self) -> StoreResult<Vec<FileTransferTaskRecord>> {
        let mut stmt = self.connection.prepare(
            "SELECT task_id, file_name, direction, state, offset, file_sha256
             FROM file_transfer_tasks
             ORDER BY updated_at ASC, task_id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FileTransferTaskRecord {
                task_id: row.get(0)?,
                file_name: row.get(1)?,
                direction: row.get(2)?,
                state: row.get(3)?,
                offset: row.get(4)?,
                file_sha256: row.get(5)?,
            })
        })?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row?);
        }
        Ok(tasks)
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

CREATE TABLE IF NOT EXISTS security_keys (
    name TEXT PRIMARY KEY,
    purpose TEXT NOT NULL,
    provider TEXT NOT NULL,
    reference TEXT NOT NULL,
    security_level TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS rbac_policies (
    role TEXT NOT NULL,
    resource TEXT NOT NULL,
    action TEXT NOT NULL,
    allowed INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(role, resource, action)
);

CREATE TABLE IF NOT EXISTS replay_nonces (
    tenant_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    principal TEXT NOT NULL,
    action TEXT NOT NULL,
    nonce TEXT NOT NULL,
    timestamp_unix_ms INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY(tenant_id, device_id, principal, action, nonce)
);

CREATE INDEX IF NOT EXISTS idx_replay_nonces_timestamp
ON replay_nonces(timestamp_unix_ms);

CREATE TABLE IF NOT EXISTS file_transfer_tasks (
    task_id TEXT PRIMARY KEY,
    file_name TEXT NOT NULL,
    direction TEXT NOT NULL,
    state TEXT NOT NULL,
    offset INTEGER NOT NULL DEFAULT 0,
    file_sha256 TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS capability_profile_cache (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    profile_json TEXT NOT NULL,
    detected_at_unix_ms INTEGER NOT NULL
);
";

fn row_to_audit_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEvent> {
    let event_json: String = row.get(0)?;
    let mut event: AuditEvent = serde_json::from_str(&event_json).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })?;
    event.result = row.get(1)?;
    event.prev_hash = row.get(2)?;
    event.hash = row.get(3)?;
    Ok(event)
}

fn system_time_to_unix_ms(time: SystemTime) -> i64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or_default()
}

fn parse_action(value: &str) -> Result<Action, String> {
    match value {
        "Read" => Ok(Action::Read),
        "Execute" => Ok(Action::Execute),
        "Write" => Ok(Action::Write),
        "Manage" => Ok(Action::Manage),
        _ => Err(format!("unknown action {value}")),
    }
}

fn parse_resource(value: &str) -> Result<Resource, String> {
    match value {
        "Telemetry" => Ok(Resource::Telemetry),
        "ControlCommand" => Ok(Resource::ControlCommand),
        "FileTransfer" => Ok(Resource::FileTransfer),
        "Configuration" => Ok(Resource::Configuration),
        "Upgrade" => Ok(Resource::Upgrade),
        "AppControl" => Ok(Resource::AppControl),
        "SecurityPolicy" => Ok(Resource::SecurityPolicy),
        _ => Err(format!("unknown resource {value}")),
    }
}

fn to_sql_conversion_error(error: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_core::security::{Action, Resource};
    use std::time::{Duration, SystemTime};

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

    #[test]
    fn stores_security_key_metadata_and_rbac_policy() {
        let store = StateStore::open_in_memory().unwrap();

        store
            .upsert_security_key(&SecurityKeyRecord {
                name: "audit-root".to_string(),
                purpose: "audit-chain".to_string(),
                provider: "pal-key-store".to_string(),
                reference: "audit-root".to_string(),
                security_level: "file-backed".to_string(),
            })
            .unwrap();
        store
            .upsert_rbac_policy(&RbacPolicyRecord {
                role: "operator".to_string(),
                resource: Resource::ControlCommand,
                action: Action::Execute,
                allowed: true,
            })
            .unwrap();

        assert_eq!(
            store
                .load_security_key("audit-root")
                .unwrap()
                .unwrap()
                .purpose,
            "audit-chain"
        );
        assert_eq!(store.load_rbac_policies().unwrap().len(), 1);
    }

    #[test]
    fn replay_nonce_persistence_rejects_duplicates_and_prunes_expired() {
        let store = StateStore::open_in_memory().unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
        let old = SystemTime::UNIX_EPOCH + Duration::from_secs(100);

        assert!(
            store
                .try_insert_replay_nonce(
                    "tenant-a",
                    "device-1",
                    "operator",
                    Action::Execute,
                    "n1",
                    now
                )
                .unwrap()
        );
        assert!(
            !store
                .try_insert_replay_nonce(
                    "tenant-a",
                    "device-1",
                    "operator",
                    Action::Execute,
                    "n1",
                    now
                )
                .unwrap()
        );
        store
            .try_insert_replay_nonce("tenant-a", "device-1", "operator", Action::Read, "old", old)
            .unwrap();
        assert_eq!(
            store
                .prune_replay_nonces(now - Duration::from_secs(300))
                .unwrap(),
            1
        );
    }

    #[test]
    fn stores_file_transfer_task_and_queries_audit_events() {
        use agent_core::security::{AuditEvent, Principal, Role};

        let store = StateStore::open_in_memory().unwrap();
        store
            .upsert_file_transfer_task(&FileTransferTaskRecord {
                task_id: "upload-1".to_string(),
                file_name: "uploads/app.bin".to_string(),
                direction: "upload".to_string(),
                state: "completed".to_string(),
                offset: 128,
                file_sha256: Some("abc".to_string()),
            })
            .unwrap();
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

        let tasks = store.load_file_transfer_tasks().unwrap();
        assert_eq!(tasks[0].offset, 128);

        let events = store
            .query_audit_events(&AuditEventFilter {
                principal: Some("operator-user".to_string()),
                action: Some(Action::Execute),
                result: Some("success".to_string()),
                ..AuditEventFilter::default()
            })
            .unwrap();
        assert_eq!(events.len(), 1);
    }
}
