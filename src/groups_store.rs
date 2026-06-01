//! Device Groups Store - CRUD Operations for Device Groups
//!
//! This module provides persistent storage and CRUD operations for device groups.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use uuid::Uuid;

use crate::groups::{DeviceGroup, DeviceGroupFilter, DeviceGroupSortField, DeviceGroupStats};

/// In-memory cache for device groups
pub struct DeviceGroupCache {
    groups: RwLock<HashMap<Uuid, DeviceGroup>>,
    stats: RwLock<DeviceGroupStats>,
}

impl DeviceGroupCache {
    pub fn new() -> Self {
        Self {
            groups: RwLock::new(HashMap::new()),
            stats: RwLock::new(DeviceGroupStats::default()),
        }
    }

    pub fn get(&self, id: &Uuid) -> Option<DeviceGroup> {
        self.groups.read().ok()?.get(id).cloned()
    }

    pub fn get_by_name(&self, name: &str) -> Option<DeviceGroup> {
        self.groups
            .read()
            .ok()?
            .values()
            .find(|g| g.name == name)
            .cloned()
    }

    pub fn get_all(&self) -> Vec<DeviceGroup> {
        self.groups
            .read()
            .map(|groups| groups.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn insert(&self, group: DeviceGroup) {
        let mut groups = self.groups.write().unwrap();
        groups.insert(group.id, group.clone());
        drop(groups);
        self.update_stats();
    }

    pub fn update(&self, group: &DeviceGroup) -> Option<DeviceGroup> {
        let mut groups = self.groups.write().unwrap();
        if groups.contains_key(&group.id) {
            let old = groups.get(&group.id).cloned();
            groups.insert(group.id, group.clone());
            drop(groups);
            self.update_stats();
            old
        } else {
            None
        }
    }

    pub fn remove(&self, id: &Uuid) -> Option<DeviceGroup> {
        let mut groups = self.groups.write().unwrap();
        let removed = groups.remove(id);
        drop(groups);
        if removed.is_some() {
            self.update_stats();
        }
        removed
    }

    pub fn filter(&self, filter: &DeviceGroupFilter) -> Vec<DeviceGroup> {
        let groups: Vec<DeviceGroup> = self.get_all();

        let mut filtered: Vec<DeviceGroup> = groups
            .into_iter()
            .filter(|group| {
                if let Some(name) = &filter.name
                    && !group.name.to_lowercase().contains(&name.to_lowercase())
                {
                    return false;
                }
                if let Some(created_by) = &filter.created_by
                    && group.created_by.as_deref() != Some(created_by.as_str())
                {
                    return false;
                }
                true
            })
            .collect();

        // Sort
        let sort_by = filter.sort_by;
        let sort_desc = filter.sort_desc;
        filtered.sort_by(|a, b| {
            let cmp = match sort_by {
                DeviceGroupSortField::CreatedAt => a.created_at.cmp(&b.created_at),
                DeviceGroupSortField::UpdatedAt => a.updated_at.cmp(&b.updated_at),
                DeviceGroupSortField::Name => a.name.cmp(&b.name),
                DeviceGroupSortField::DeviceCount => a.device_ids.len().cmp(&b.device_ids.len()),
            };
            if sort_desc { cmp.reverse() } else { cmp }
        });

        // Pagination (limit 0 means no limit)
        let offset = filter.offset as usize;
        let limit = if filter.limit == 0 {
            usize::MAX
        } else {
            filter.limit as usize
        };
        filtered.into_iter().skip(offset).take(limit).collect()
    }

    fn update_stats(&self) {
        let groups = self.groups.read().unwrap();
        let mut stats = self.stats.write().unwrap();

        stats.total_groups = groups.len() as u64;
        let total_devices: usize = groups.values().map(|g| g.device_ids.len()).sum();
        stats.total_devices = total_devices as u64;
        stats.avg_devices_per_group = if groups.is_empty() {
            0.0
        } else {
            total_devices as f64 / groups.len() as f64
        };
    }

    pub fn stats(&self) -> DeviceGroupStats {
        self.stats.read().unwrap().clone()
    }
}

impl Default for DeviceGroupCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Device group store with SQLite persistence.
pub struct DeviceGroupStore {
    cache: Arc<DeviceGroupCache>,
    db_path: PathBuf,
}

impl DeviceGroupStore {
    /// Create a new device group store with the given database path.
    pub fn new(db_path: PathBuf) -> Result<Self> {
        let store = Self {
            cache: Arc::new(DeviceGroupCache::new()),
            db_path,
        };
        store.init_db()?;
        store.load_from_db()?;
        Ok(store)
    }

    /// Initialize the database schema.
    fn init_db(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("open device group store DB at {}", self.db_path.display()))?;

        migrate_group_device_schema(&conn)?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS device_groups (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                description TEXT,
                device_ids TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                created_by TEXT
            )",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_device_groups_name ON device_groups(name)",
            [],
        )?;

        Ok(())
    }

    /// Load groups from the database into cache.
    fn load_from_db(&self) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("open device group store DB at {}", self.db_path.display()))?;

        let mut stmt = conn.prepare(
            "SELECT id, name, description, device_ids, created_at, updated_at, created_by
             FROM device_groups ORDER BY created_at DESC",
        )?;

        let row_data_list: Vec<GroupRowData> = stmt
            .query_map([], |row| {
                let created_at_str: String = row.get(4)?;
                let updated_at_str: String = row.get(5)?;

                Ok(GroupRowData {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    device_ids: row.get(3)?,
                    created_at: created_at_str,
                    updated_at: updated_at_str,
                    created_by: row.get(6)?,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();

        for data in row_data_list {
            if let Ok(group) = self.row_data_to_group(data) {
                self.cache.insert(group);
            }
        }

        Ok(())
    }

    fn row_data_to_group(&self, data: GroupRowData) -> Result<DeviceGroup> {
        let device_ids: Vec<String> = serde_json::from_str(&data.device_ids).unwrap_or_default();

        let created_at = DateTime::parse_from_rfc3339(&data.created_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());
        let updated_at = DateTime::parse_from_rfc3339(&data.updated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

        Ok(DeviceGroup {
            id: Uuid::parse_str(&data.id).unwrap_or_else(|_| Uuid::new_v4()),
            name: data.name,
            description: data.description,
            device_ids,
            created_at,
            updated_at,
            created_by: data.created_by,
        })
    }

    // CRUD Operations

    /// Create a new device group.
    pub fn create(&self, group: &DeviceGroup) -> Result<()> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("open device group store DB at {}", self.db_path.display()))?;

        let device_ids_json = serde_json::to_string(&group.device_ids)?;

        conn.execute(
            "INSERT INTO device_groups (id, name, description, device_ids, created_at, updated_at, created_by)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                group.id.to_string(),
                group.name,
                group.description,
                device_ids_json,
                group.created_at.to_rfc3339(),
                group.updated_at.to_rfc3339(),
                group.created_by,
            ],
        )?;

        self.cache.insert(group.clone());
        Ok(())
    }

    /// Get a device group by ID.
    pub fn get(&self, id: &Uuid) -> Option<DeviceGroup> {
        self.cache.get(id)
    }

    /// Get a device group by name.
    pub fn get_by_name(&self, name: &str) -> Option<DeviceGroup> {
        self.cache.get_by_name(name)
    }

    /// Get all device groups.
    pub fn get_all(&self) -> Vec<DeviceGroup> {
        self.cache.get_all()
    }

    /// Update a device group.
    pub fn update(&self, group: &DeviceGroup) -> Result<Option<DeviceGroup>> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("open device group store DB at {}", self.db_path.display()))?;

        let device_ids_json = serde_json::to_string(&group.device_ids)?;

        let affected = conn.execute(
            "UPDATE device_groups SET name = ?2, description = ?3, device_ids = ?4,
                                    updated_at = ?5, created_by = ?6
             WHERE id = ?1",
            params![
                group.id.to_string(),
                group.name,
                group.description,
                device_ids_json,
                group.updated_at.to_rfc3339(),
                group.created_by,
            ],
        )?;

        if affected > 0 {
            self.cache.update(group);
            Ok(self.cache.get(&group.id))
        } else {
            Ok(None)
        }
    }

    /// Delete a device group by ID.
    pub fn delete(&self, id: &Uuid) -> Result<Option<DeviceGroup>> {
        let conn = Connection::open(&self.db_path)
            .with_context(|| format!("open device group store DB at {}", self.db_path.display()))?;

        let affected = conn.execute("DELETE FROM device_groups WHERE id = ?1", [id.to_string()])?;

        if affected > 0 {
            Ok(self.cache.remove(id))
        } else {
            Ok(None)
        }
    }

    /// Filter device groups by criteria.
    pub fn filter(&self, filter: &DeviceGroupFilter) -> Vec<DeviceGroup> {
        self.cache.filter(filter)
    }

    /// Get device group statistics.
    pub fn stats(&self) -> DeviceGroupStats {
        self.cache.stats()
    }
}

fn migrate_group_device_schema(conn: &Connection) -> Result<()> {
    if table_exists(conn, "station_groups")? && !table_exists(conn, "device_groups")? {
        conn.execute("ALTER TABLE station_groups RENAME TO device_groups", [])?;
    }

    if table_exists(conn, "device_groups")?
        && column_exists(conn, "device_groups", "station_ids")?
        && !column_exists(conn, "device_groups", "device_ids")?
    {
        conn.execute(
            "ALTER TABLE device_groups RENAME COLUMN station_ids TO device_ids",
            [],
        )?;
    }

    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        [table],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let names = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for name in names {
        if name? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

#[derive(Debug)]
struct GroupRowData {
    id: String,
    name: String,
    description: Option<String>,
    device_ids: String,
    created_at: String,
    updated_at: String,
    created_by: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env::temp_dir;

    #[test]
    fn test_device_group_crud() {
        let db_path = temp_dir().join(format!("device_group_test_{}.db", Uuid::new_v4()));
        let store = DeviceGroupStore::new(db_path.clone()).unwrap();

        let mut group = DeviceGroup::new("Test Group");
        group = group.with_description("A test device group");
        group = group.created_by("test-user");
        group = group.add_device("device-1").add_device("device-2");

        // Create
        store.create(&group).unwrap();
        assert!(store.get(&group.id).is_some());

        // Read
        let retrieved = store.get(&group.id).unwrap();
        assert_eq!(retrieved.name, "Test Group");
        assert_eq!(
            retrieved.description,
            Some("A test device group".to_string())
        );
        assert_eq!(retrieved.device_ids.len(), 2);

        // Update
        let mut updated = retrieved;
        updated = updated.add_device("device-3");
        store.update(&updated).unwrap();

        let retrieved2 = store.get(&group.id).unwrap();
        assert_eq!(retrieved2.device_ids.len(), 3);

        // Delete
        store.delete(&group.id).unwrap();
        assert!(store.get(&group.id).is_none());

        // Clean up
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn test_device_group_filter() {
        let db_path = temp_dir().join(format!("device_group_filter_test_{}.db", Uuid::new_v4()));
        let store = DeviceGroupStore::new(db_path.clone()).unwrap();

        // Create groups with different names
        for i in 0..5 {
            let mut group = DeviceGroup::new(format!("Group {}", i));
            group = group.add_devices(["device-a", "device-b"]);
            store.create(&group).unwrap();
        }

        // Verify groups are in cache
        let all_groups = store.get_all();
        assert_eq!(all_groups.len(), 5, "Should have 5 groups in cache");

        // Filter by name
        let filter = DeviceGroupFilter {
            name: Some("Group 0".to_string()),
            ..Default::default()
        };
        let filtered = store.filter(&filter);
        assert_eq!(filtered.len(), 1, "Should have 1 matching group");

        // Filter by name partial match
        let filter = DeviceGroupFilter {
            name: Some("Group".to_string()),
            ..Default::default()
        };
        let filtered = store.filter(&filter);
        assert_eq!(filtered.len(), 5, "Should have 5 matching groups");

        // Clean up
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn test_device_group_stats() {
        let db_path = temp_dir().join(format!("device_group_stats_test_{}.db", Uuid::new_v4()));
        let store = DeviceGroupStore::new(db_path.clone()).unwrap();

        // Create groups with devices
        for i in 0..3 {
            let mut group = DeviceGroup::new(format!("Group {}", i));
            group = group.add_devices([format!("device-{}", i), format!("device-{}", i + 10)]);
            store.create(&group).unwrap();
        }

        let stats = store.stats();
        assert_eq!(stats.total_groups, 3);
        assert_eq!(stats.total_devices, 6);
        assert_eq!(stats.avg_devices_per_group, 2.0);

        // Clean up
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn migrates_legacy_station_groups() {
        let db_path = temp_dir().join(format!("device_group_migration_{}.db", Uuid::new_v4()));
        {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "
                CREATE TABLE station_groups (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    description TEXT,
                    station_ids TEXT NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    created_by TEXT
                );
                ",
            )
            .unwrap();
        }

        let _store = DeviceGroupStore::new(db_path.clone()).unwrap();
        let conn = Connection::open(&db_path).unwrap();
        assert!(table_exists(&conn, "device_groups").unwrap());
        assert!(!table_exists(&conn, "station_groups").unwrap());
        assert!(column_exists(&conn, "device_groups", "device_ids").unwrap());
        assert!(!column_exists(&conn, "device_groups", "station_ids").unwrap());

        std::fs::remove_file(db_path).ok();
    }
}
