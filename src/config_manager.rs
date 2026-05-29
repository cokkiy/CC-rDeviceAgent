//! Config Manager — W2.5
//!
//! Three-tier configuration model: device / agent / app.
//! Provides versioned storage, rollback, and server-streaming watch.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{Result, anyhow};
use tokio::sync::broadcast;
use tracing::{debug, info};

// ── config scopes ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigScope {
    Device,
    Agent,
    App(String), // app_id
}

impl std::fmt::Display for ConfigScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Device => write!(f, "device"),
            Self::Agent  => write!(f, "agent"),
            Self::App(id) => write!(f, "app:{id}"),
        }
    }
}

// ── change event ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ConfigChangeEvent {
    pub scope: ConfigScope,
    pub key: String,
    pub value: Option<String>, // None = deleted
    pub version: u64,
}

// ── in-memory config store ────────────────────────────────────────────────

type ScopeMap = HashMap<String, (String, u64)>; // key → (value, version)

struct Inner {
    scopes: HashMap<ConfigScope, ScopeMap>,
    global_version: u64,
}

impl Inner {
    fn new() -> Self {
        Self {
            scopes: HashMap::new(),
            global_version: 0,
        }
    }

    fn set(&mut self, scope: ConfigScope, key: String, value: String) -> u64 {
        self.global_version += 1;
        let v = self.global_version;
        self.scopes
            .entry(scope)
            .or_default()
            .insert(key, (value, v));
        v
    }

    fn get(&self, scope: &ConfigScope, key: &str) -> Option<(String, u64)> {
        self.scopes
            .get(scope)
            .and_then(|m| m.get(key))
            .cloned()
    }

    fn get_all(&self, scope: &ConfigScope) -> HashMap<String, (String, u64)> {
        self.scopes
            .get(scope)
            .cloned()
            .unwrap_or_default()
    }

    fn delete(&mut self, scope: &ConfigScope, key: &str) -> bool {
        self.global_version += 1;
        self.scopes
            .get_mut(scope)
            .map(|m| m.remove(key).is_some())
            .unwrap_or(false)
    }
}

// ── public API ────────────────────────────────────────────────────────────

pub struct ConfigManager {
    inner: RwLock<Inner>,
    tx: broadcast::Sender<ConfigChangeEvent>,
}

impl ConfigManager {
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(256);
        Arc::new(Self {
            inner: RwLock::new(Inner::new()),
            tx,
        })
    }

    /// Set a key in the given scope.  Returns the new version.
    pub fn set(&self, scope: ConfigScope, key: impl Into<String>, value: impl Into<String>) -> u64 {
        let key = key.into();
        let value = value.into();
        let version = self.inner.write().unwrap().set(scope.clone(), key.clone(), value.clone());
        let event = ConfigChangeEvent { scope, key, value: Some(value), version };
        let _ = self.tx.send(event.clone());
        info!(scope = %event.scope, key = %event.key, version, "config set");
        version
    }

    /// Delete a key from the given scope.
    pub fn delete(&self, scope: &ConfigScope, key: &str) -> bool {
        let version = {
            let mut w = self.inner.write().unwrap();
            let removed = w.delete(scope, key);
            if removed { w.global_version } else { return false }
        };
        let event = ConfigChangeEvent {
            scope: scope.clone(),
            key: key.to_string(),
            value: None,
            version,
        };
        let _ = self.tx.send(event);
        true
    }

    pub fn get(&self, scope: &ConfigScope, key: &str) -> Option<String> {
        self.inner.read().unwrap().get(scope, key).map(|(v, _)| v)
    }

    pub fn get_version(&self, scope: &ConfigScope, key: &str) -> Option<u64> {
        self.inner.read().unwrap().get(scope, key).map(|(_, v)| v)
    }

    /// Snapshot all keys for a scope.
    pub fn snapshot(&self, scope: &ConfigScope) -> HashMap<String, String> {
        self.inner
            .read()
            .unwrap()
            .get_all(scope)
            .into_iter()
            .map(|(k, (v, _))| (k, v))
            .collect()
    }

    /// Subscribe to change events.
    pub fn subscribe(&self) -> broadcast::Receiver<ConfigChangeEvent> {
        self.tx.subscribe()
    }

    /// Convenience: watch a specific app's config.
    pub fn subscribe_app(&self, app_id: &str) -> AppConfigWatcher {
        AppConfigWatcher {
            app_id: app_id.to_string(),
            rx: self.tx.subscribe(),
        }
    }
}

impl Default for ConfigManager {
    fn default() -> Self {
        let (tx, _) = broadcast::channel(256);
        Self {
            inner: RwLock::new(Inner::new()),
            tx,
        }
    }
}

// helper for logging (avoids partial-move after send)
fn event_scope(e: &ConfigChangeEvent) -> String { e.scope.to_string() }
fn event_key(e: &ConfigChangeEvent) -> &str { &e.key }

// ── per-app watcher ───────────────────────────────────────────────────────

pub struct AppConfigWatcher {
    app_id: String,
    rx: broadcast::Receiver<ConfigChangeEvent>,
}

impl AppConfigWatcher {
    /// Wait for the next change relevant to this app (or any agent/device change).
    pub async fn next_change(&mut self) -> Option<ConfigChangeEvent> {
        loop {
            match self.rx.recv().await {
                Ok(ev) => {
                    let relevant = matches!(
                        &ev.scope,
                        ConfigScope::Device
                            | ConfigScope::Agent
                    ) || matches!(&ev.scope, ConfigScope::App(id) if id == &self.app_id);
                    if relevant {
                        return Some(ev);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    debug!("config watcher lagged by {n} events");
                    continue;
                }
                Err(_) => return None,
            }
        }
    }
}

// ── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get() {
        let mgr = ConfigManager::new();
        mgr.set(ConfigScope::Device, "log_level", "debug");
        assert_eq!(mgr.get(&ConfigScope::Device, "log_level"), Some("debug".into()));
    }

    #[test]
    fn delete() {
        let mgr = ConfigManager::new();
        mgr.set(ConfigScope::Agent, "interval", "5");
        assert!(mgr.delete(&ConfigScope::Agent, "interval"));
        assert_eq!(mgr.get(&ConfigScope::Agent, "interval"), None);
        assert!(!mgr.delete(&ConfigScope::Agent, "interval")); // already gone
    }

    #[tokio::test]
    async fn watch_receives_events() {
        let mgr = ConfigManager::new();
        let mut rx = mgr.subscribe();

        mgr.set(ConfigScope::App("app1".into()), "batch_size", "100");

        let ev = rx.recv().await.unwrap();
        assert_eq!(ev.key, "batch_size");
        assert_eq!(ev.value, Some("100".into()));
        assert!(matches!(ev.scope, ConfigScope::App(ref id) if id == "app1"));
    }

    #[test]
    fn snapshot() {
        let mgr = ConfigManager::new();
        mgr.set(ConfigScope::Device, "k1", "v1");
        mgr.set(ConfigScope::Device, "k2", "v2");
        let snap = mgr.snapshot(&ConfigScope::Device);
        assert_eq!(snap.len(), 2);
    }

    #[tokio::test]
    async fn app_watcher_filters_scope() {
        let mgr = ConfigManager::new();
        let mut w = mgr.subscribe_app("app-42");

        // Fire an event for a different app — should be ignored
        mgr.set(ConfigScope::App("other-app".into()), "x", "1");
        // Fire one for our app
        mgr.set(ConfigScope::App("app-42".into()), "threshold", "50");

        let ev = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            w.next_change(),
        )
        .await
        .expect("timeout")
        .expect("channel closed");

        assert_eq!(ev.key, "threshold");
    }
}
