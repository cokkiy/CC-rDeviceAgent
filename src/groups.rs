//! Device Groups Module
//!
//! This module provides device group management for organizing devices
//! into logical groups for batch operations and targeting.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A device group for organizing devices.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceGroup {
    /// Unique identifier
    pub id: Uuid,
    /// Group name
    pub name: String,
    /// Group description
    pub description: Option<String>,
    /// Device IDs belonging to this group
    #[serde(default)]
    pub device_ids: Vec<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last update timestamp
    pub updated_at: DateTime<Utc>,
    /// Creator identifier
    pub created_by: Option<String>,
}

impl DeviceGroup {
    /// Create a new device group
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            description: None,
            device_ids: Vec::new(),
            created_at: now,
            updated_at: now,
            created_by: None,
        }
    }

    /// Set the description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the creator
    pub fn created_by(mut self, creator: impl Into<String>) -> Self {
        self.created_by = Some(creator.into());
        self
    }

    /// Add a device to the group
    pub fn add_device(mut self, device_id: impl Into<String>) -> Self {
        let device_id = device_id.into();
        if !self.device_ids.contains(&device_id) {
            self.device_ids.push(device_id);
            self.updated_at = Utc::now();
        }
        self
    }

    /// Add multiple devices to the group
    pub fn add_devices(mut self, device_ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        for device_id in device_ids {
            let device_id = device_id.into();
            if !self.device_ids.contains(&device_id) {
                self.device_ids.push(device_id);
            }
        }
        self.updated_at = Utc::now();
        self
    }

    /// Remove a device from the group
    pub fn remove_device(mut self, device_id: &str) -> Self {
        self.device_ids.retain(|id| id != device_id);
        self.updated_at = Utc::now();
        self
    }

    /// Check if the group contains a device
    pub fn contains_device(&self, device_id: &str) -> bool {
        self.device_ids.iter().any(|id| id == device_id)
    }

    /// Get the number of devices in the group
    pub fn device_count(&self) -> usize {
        self.device_ids.len()
    }
}

/// Filter options for querying device groups.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceGroupFilter {
    /// Filter by name (partial match)
    pub name: Option<String>,
    /// Filter by creator
    pub created_by: Option<String>,
    /// Sort by field
    #[serde(default)]
    pub sort_by: DeviceGroupSortField,
    /// Sort ascending or descending
    #[serde(default = "default_sort_desc")]
    pub sort_desc: bool,
    /// Pagination offset
    #[serde(default)]
    pub offset: u64,
    /// Pagination limit
    #[serde(default = "default_limit")]
    pub limit: u64,
}

fn default_sort_desc() -> bool {
    true
}

fn default_limit() -> u64 {
    50
}

/// Fields that can be used for sorting device groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum DeviceGroupSortField {
    #[default]
    CreatedAt,
    UpdatedAt,
    Name,
    DeviceCount,
}

/// Statistics about device groups.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceGroupStats {
    /// Total number of groups
    pub total_groups: u64,
    /// Total devices across all groups
    pub total_devices: u64,
    /// Average devices per group
    pub avg_devices_per_group: f64,
}
