//! Data Router — W2.4
//!
//! Routes data between payload applications and the backend MQTT broker.
//! - Uplink:   app → agent → MQTT  (`{tenant}/{device_id}/apps/{app_id}/{topic}`)
//! - Downlink: MQTT → agent → app  (broadcast to subscribed app streams)

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{Result, anyhow};
use tokio::sync::mpsc;
use tracing::debug;

use crate::mqtt::MqttClient;

// ── topic mapping ─────────────────────────────────────────────────────────

pub fn app_uplink_topic(tenant: &str, device_id: &str, app_id: &str, topic: &str) -> String {
    format!("{tenant}/{device_id}/apps/{app_id}/{topic}")
}

/// Validate that `topic` does not attempt to escape the app's namespace
/// and does not contain MQTT wildcard characters (`#`, `+`).
pub fn validate_app_topic(topic: &str) -> Result<()> {
    if topic.is_empty() {
        return Err(anyhow!("topic must not be empty"));
    }
    if topic.contains("..") || topic.starts_with('/') {
        return Err(anyhow!("invalid topic: {topic}"));
    }
    if topic.contains('#') || topic.contains('+') {
        return Err(anyhow!("topic must not contain MQTT wildcard characters: {topic}"));
    }
    Ok(())
}

// ── downlink fan-out ──────────────────────────────────────────────────────

type DownlinkSender = mpsc::Sender<(String, Vec<u8>)>;

/// Fan-out registry: maps app_id → list of active subscriber channels.
#[derive(Default, Clone)]
pub struct DownlinkRegistry {
    inner: Arc<RwLock<HashMap<String, Vec<DownlinkSender>>>>,
}

impl DownlinkRegistry {
    pub fn subscribe(&self, app_id: &str) -> mpsc::Receiver<(String, Vec<u8>)> {
        let (tx, rx) = mpsc::channel(64);
        self.inner
            .write()
            .unwrap()
            .entry(app_id.to_string())
            .or_default()
            .push(tx);
        rx
    }

    /// Fan-out a message to all subscribers of `app_id`.
    pub async fn deliver(&self, app_id: &str, topic: String, payload: Vec<u8>) {
        let senders: Vec<_> = {
            let guard = self.inner.read().unwrap();
            guard.get(app_id).cloned().unwrap_or_default()
        };
        let mut dead = vec![];
        for (i, tx) in senders.iter().enumerate() {
            if tx.send((topic.clone(), payload.clone())).await.is_err() {
                dead.push(i);
            }
        }
        if !dead.is_empty() {
            let mut guard = self.inner.write().unwrap();
            if let Some(v) = guard.get_mut(app_id) {
                for i in dead.iter().rev() {
                    v.swap_remove(*i);
                }
            }
        }
    }

    pub fn remove_app(&self, app_id: &str) {
        self.inner.write().unwrap().remove(app_id);
    }
}

// ── uplink publisher ──────────────────────────────────────────────────────

/// Thin wrapper over the MQTT client for uplink data.
pub struct DataRouter {
    tenant: String,
    device_id: String,
    downlink: DownlinkRegistry,
}

impl DataRouter {
    pub fn new(tenant: String, device_id: String) -> Self {
        Self {
            tenant,
            device_id,
            downlink: DownlinkRegistry::default(),
        }
    }

    pub fn downlink_registry(&self) -> &DownlinkRegistry {
        &self.downlink
    }

    /// Build the full MQTT topic for an uplink message from `app_id`.
    pub fn uplink_topic(&self, app_id: &str, topic: &str) -> Result<String> {
        validate_app_topic(topic)?;
        Ok(app_uplink_topic(
            &self.tenant,
            &self.device_id,
            app_id,
            topic,
        ))
    }

    /// Publish uplink data.  Caller provides the MQTT publish function
    /// so this module stays decoupled from the MQTT client type.
    pub async fn publish_uplink(
        &self,
        app_id: &str,
        topic: &str,
        payload: Vec<u8>,
        publish: impl AsyncPublish,
    ) -> Result<()> {
        let full_topic = self.uplink_topic(app_id, topic)?;
        debug!(app_id, topic = %full_topic, bytes = payload.len(), "uplink");
        publish.publish(full_topic, payload).await
    }
}

#[allow(async_fn_in_trait)]
pub trait AsyncPublish {
    async fn publish(&self, topic: String, payload: Vec<u8>) -> Result<()>;
}

impl AsyncPublish for MqttClient {
    async fn publish(&self, topic: String, payload: Vec<u8>) -> Result<()> {
        self.publish_app_data(topic, payload).await
    }
}

// ── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_mapping() {
        let t = app_uplink_topic("acme", "dev-01", "weather-app", "metrics");
        assert_eq!(t, "acme/dev-01/apps/weather-app/metrics");
    }

    #[test]
    fn topic_validation() {
        assert!(validate_app_topic("metrics").is_ok());
        assert!(validate_app_topic("sensor/temperature").is_ok());
        assert!(validate_app_topic("").is_err());
        assert!(validate_app_topic("../secrets").is_err());
        assert!(validate_app_topic("/absolute").is_err());
        assert!(validate_app_topic("sensor/#").is_err());
        assert!(validate_app_topic("sensor/+/temp").is_err());
    }

    #[tokio::test]
    async fn downlink_fanout() {
        let reg = DownlinkRegistry::default();
        let mut rx1 = reg.subscribe("app-1");
        let mut rx2 = reg.subscribe("app-1");

        reg.deliver("app-1", "cmd".into(), b"hello".to_vec()).await;

        let (t1, p1) = rx1.recv().await.unwrap();
        let (t2, p2) = rx2.recv().await.unwrap();
        assert_eq!(t1, "cmd");
        assert_eq!(p1, b"hello");
        assert_eq!(t2, "cmd");
        assert_eq!(p2, b"hello");
    }
}
