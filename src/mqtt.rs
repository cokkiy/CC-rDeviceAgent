//! MQTT client module for CC-rDeviceAgent
//!
//! This module implements MQTT telemetry publishing for the device service.
//! It publishes telemetry data to the CC-Aggregator via MQTT and
//! subscribes to command messages from the CC-Aggregator.

use anyhow::{Context, Result};
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, Transport};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, error, info, warn};

use crate::telemetry::TelemetryBundle;

use crate::config::TlsConfig;

/// Command received from the Aggregator via MQTT
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub command_id: String,
    pub command: String,
    pub params: serde_json::Value,
    pub timestamp: i64,
}

/// Acknowledgment response for a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandAck {
    pub command_id: String,
    pub success: bool,
    pub message: String,
    pub timestamp: i64,
}

/// Internal message types for the MQTT worker
enum MqttWorkerMsg {
    /// Request to subscribe to a topic, with a channel to send the command receiver
    Subscribe {
        topic: String,
        response_tx: mpsc::Sender<Result<mpsc::Receiver<Command>, anyhow::Error>>,
    },
}

/// MQTT client wrapper for the device service
pub struct MqttClient {
    client: AsyncClient,
    device_id: String,
    worker_tx: mpsc::Sender<MqttWorkerMsg>,
}

impl Clone for MqttClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            device_id: self.device_id.clone(),
            worker_tx: self.worker_tx.clone(),
        }
    }
}

impl MqttClient {
    /// Create a new MQTT client connection
    pub fn new(broker_host: &str, broker_port: u16, device_id: &str) -> Result<Self> {
        Self::new_with_tls_config(broker_host, broker_port, device_id, None)
    }

    /// Create a new MQTT client connection with optional TLS client authentication.
    pub fn new_with_tls_config(
        broker_host: &str,
        broker_port: u16,
        device_id: &str,
        tls: Option<&TlsConfig>,
    ) -> Result<Self> {
        let client_id = format!("cc-device-{}", device_id);
        let mut mqttoptions = MqttOptions::new(client_id.clone(), (broker_host, broker_port));
        mqttoptions.set_keep_alive(30);
        mqttoptions.set_clean_session(true);
        if let Some(tls) = tls
            && tls.enabled
        {
            let ca = read_tls_file(tls.ca_cert_path.as_ref(), "mqtt.tls.ca_cert_path")?;
            let cert = read_tls_file(tls.cert_path.as_ref(), "mqtt.tls.cert_path")?;
            let key = read_tls_file(tls.key_path.as_ref(), "mqtt.tls.key_path")?;
            mqttoptions.set_transport(Transport::tls(ca, Some((cert, key)), None));
        }

        let (client, eventloop) = AsyncClient::builder(mqttoptions).capacity(100).build();
        let (worker_tx, mut worker_rx) = mpsc::channel::<MqttWorkerMsg>(100);

        let client_for_worker = client.clone();

        // Spawn background task to handle event loop and subscriptions
        tokio::spawn(async move {
            let mut eventloop = eventloop;
            // Map of topic -> command sender
            let handlers: Arc<Mutex<HashMap<String, mpsc::Sender<Command>>>> =
                Arc::new(Mutex::new(HashMap::new()));

            loop {
                tokio::select! {
                    // Handle messages from the main client (subscription requests)
                    msg = worker_rx.recv() => {
                        match msg {
                            Some(MqttWorkerMsg::Subscribe { topic, response_tx }) => {
                                // Subscribe to the topic
                                match client_for_worker.subscribe(&topic, QoS::AtLeastOnce).await {
                                    Ok(()) => {
                                        debug!("Subscribed to topic: {}", topic);

                                        // Create a channel for this subscription
                                        let (tx, rx) = mpsc::channel::<Command>(100);

                                        // Store the handler
                                        let mut handlers_lock = handlers.lock().await;
                                        handlers_lock.insert(topic.clone(), tx);

                                        let _ = response_tx.send(Ok(rx)).await;
                                    }
                                    Err(e) => {
                                        error!("Failed to subscribe to {}: {:?}", topic, e);
                                        let _ = response_tx.send(Err(anyhow::anyhow!("Subscribe failed: {:?}", e))).await;
                                    }
                                }
                            }
                            None => {
                                debug!("Worker channel closed, stopping worker");
                                break;
                            }
                        }
                    }
                    // Poll the event loop for incoming messages
                    notification = eventloop.poll() => {
                        match notification {
                            Ok(event) => {
                                if let Event::Incoming(Packet::Publish(publish)) = event {
                                    let topic = String::from_utf8_lossy(&publish.topic).to_string();

                                    // Look up handler for this topic
                                    let handlers_lock = handlers.lock().await;
                                    if let Some(tx) = handlers_lock.get(&topic)
                                        && let Ok(cmd) = serde_json::from_slice::<Command>(&publish.payload) {
                                            debug!("Received command for topic {}: {:?}", topic, cmd);
                                            if tx.send(cmd).await.is_err() {
                                                warn!("Handler for topic {} dropped", topic);
                                            }
                                        }
                                }
                            }
                            Err(e) => {
                                error!("MQTT event loop error: {:?}", e);
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                        }
                    }
                }
            }

            info!("MQTT worker task ending");
        });

        info!(
            "MQTT client created: broker={}:{}, device_id={}",
            broker_host, broker_port, device_id
        );

        Ok(Self {
            client,
            device_id: device_id.to_string(),
            worker_tx,
        })
    }

    /// Subscribe to command topic and return a receiver for command messages.
    pub async fn subscribe_commands(&self) -> Result<mpsc::Receiver<Command>> {
        let command_topic = format!("cc/{}/command", self.device_id);

        let (response_tx, mut response_rx) =
            mpsc::channel::<Result<mpsc::Receiver<Command>, anyhow::Error>>(1);

        self.worker_tx
            .send(MqttWorkerMsg::Subscribe {
                topic: command_topic,
                response_tx,
            })
            .await
            .context("Failed to send subscribe request to worker")?;

        let result = response_rx
            .recv()
            .await
            .context("Worker dropped response channel")??;

        Ok(result)
    }

    /// Publish a command acknowledgment to the aggregator
    pub async fn publish_command_ack(&self, ack: &CommandAck) -> Result<()> {
        let topic = format!("cc/{}/command/ack", self.device_id);
        let payload = serde_json::to_vec(ack)?;

        self.client
            .publish(&topic, QoS::AtLeastOnce, false, payload)
            .await
            .context("Failed to publish command ack")?;

        debug!("Published command ack for command_id: {}", ack.command_id);
        Ok(())
    }

    /// Get the device ID
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Publish telemetry data
    pub async fn publish_telemetry(&self, telemetry: &TelemetryBundle) -> Result<()> {
        let topic = format!("cc/{}/telemetry", self.device_id);
        let payload = serde_json::to_vec(telemetry)?;

        self.client
            .publish(&topic, QoS::AtLeastOnce, false, payload)
            .await
            .context("Failed to publish telemetry")?;

        debug!("Published telemetry for device: {}", self.device_id);
        Ok(())
    }

    /// Publish device status
    pub async fn publish_status(&self, status: &DeviceStatus) -> Result<()> {
        let topic = format!("cc/{}/status", self.device_id);
        let payload = serde_json::to_vec(status)?;

        self.client
            .publish(&topic, QoS::AtLeastOnce, true, payload)
            .await
            .context("Failed to publish status")?;

        debug!("Published status for device: {}", self.device_id);
        Ok(())
    }

    /// Publish device descriptor
    pub async fn publish_descriptor(&self, descriptor: &DeviceDescriptor) -> Result<()> {
        let topic = format!("cc/{}/descriptor/announce", self.device_id);
        let payload = serde_json::to_vec(descriptor)?;

        self.client
            .publish(&topic, QoS::AtLeastOnce, true, payload)
            .await
            .context("Failed to publish descriptor")?;

        info!("Published device descriptor for: {}", self.device_id);
        Ok(())
    }
}

fn read_tls_file(path: Option<&std::path::PathBuf>, field: &str) -> Result<Vec<u8>> {
    let path = path.ok_or_else(|| anyhow::anyhow!("{field} is required when TLS is enabled"))?;
    std::fs::read(path).with_context(|| format!("read {field} from {}", path.display()))
}

/// Device status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceStatus {
    pub device_id: String,
    pub online: bool,
    pub last_seen: i64,
    pub version: Option<String>,
    pub alert: Option<String>,
}

impl DeviceStatus {
    pub fn online(device_id: String) -> Self {
        Self {
            device_id,
            online: true,
            last_seen: chrono::Utc::now().timestamp_millis(),
            version: None,
            alert: None,
        }
    }
}

/// Device descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceDescriptor {
    pub device_id: String,
    pub descriptors: Vec<TelemetryDescriptor>,
}

impl DeviceDescriptor {
    pub fn new(device_id: String) -> Self {
        Self {
            device_id,
            descriptors: Vec::new(),
        }
    }

    pub fn add_descriptor(&mut self, descriptor: TelemetryDescriptor) {
        self.descriptors.push(descriptor);
    }
}

/// Telemetry descriptor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryDescriptor {
    pub key: String,
    pub name: String,
    pub name_en: String,
    pub description: String,
    pub value_type: String,
    pub unit: String,
    pub range: TelemetryRange,
    pub update_interval_ms: u32,
    pub aggregation: Vec<String>,
    pub alert: Option<AlertThreshold>,
}

/// Telemetry range
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryRange {
    pub min: f64,
    pub max: f64,
}

/// Alert threshold
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThreshold {
    pub warning: f64,
    pub critical: f64,
}
