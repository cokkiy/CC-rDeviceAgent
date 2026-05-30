//! CC-rDeviceAgent Payload Application SDK
//!
//! Provides a high-level async client for payload applications to communicate
//! with the agent platform via the southbound IPC channel.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use app_sdk::AppClient;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let mut client = AppClient::connect_uds(
//!         "/var/run/cc-rdeviceagent/app.sock",
//!         "my-app",
//!         "1.0.0",
//!         vec!["metrics".into()],
//!     ).await?;
//!
//!     // Periodic heartbeat
//!     client.heartbeat().await?;
//!
//!     // Publish data uplink
//!     client.publish("sensors/temperature", b"23.5").await?;
//!
//!     // Watch configuration
//!     let mut watcher = client.watch_config(vec![]).await?;
//!     while let Some(update) = watcher.recv().await {
//!         println!("Config changed: {} = {:?}", update.key, update.value);
//!     }
//!
//!     client.unregister().await?;
//!     Ok(())
//! }
//! ```

mod proto {
    tonic::include_proto!("cc.app.v1");
}

use std::collections::HashMap;

#[cfg(unix)]
use anyhow::anyhow;
use anyhow::{Context, Result};
use tokio::sync::mpsc;
use tonic::transport::Channel;
use tracing::info;

use proto::{
    HealthReport, HeartbeatRequest, PublishDataRequest, RegisterAppRequest, SubscribeDataRequest,
    UnregisterAppRequest, WatchConfigRequest, app_platform_client::AppPlatformClient,
};

pub use proto::{ConfigUpdate, DataMessage, HealthStatus};

// ── connection helpers ────────────────────────────────────────────────────

#[cfg(unix)]
async fn connect_uds_channel(socket_path: &str) -> Result<Channel> {
    use hyper_util::rt::TokioIo;
    use tokio::net::UnixStream;
    use tonic::transport::Endpoint;

    let path = socket_path.to_owned();
    let channel = Endpoint::try_from("http://[::]:0")?
        .connect_with_connector(tower::service_fn(move |_| {
            let path = path.clone();
            async move {
                let stream = UnixStream::connect(&path).await.map_err(|e| anyhow!(e))?;
                Ok::<_, anyhow::Error>(TokioIo::new(stream))
            }
        }))
        .await
        .context("connect to agent socket")?;
    Ok(channel)
}

// ── AppClient ─────────────────────────────────────────────────────────────

/// Registered session held by the SDK.
struct Session {
    app_id: String,
    session_token: String,
}

/// High-level client for payload applications.
pub struct AppClient {
    inner: AppPlatformClient<Channel>,
    session: Session,
}

impl AppClient {
    /// Connect via Unix Domain Socket and register with the agent.
    #[cfg(unix)]
    pub async fn connect_uds(
        socket_path: &str,
        app_name: &str,
        app_version: &str,
        capabilities: Vec<String>,
    ) -> Result<Self> {
        let channel = connect_uds_channel(socket_path).await?;
        Self::register_with_channel(channel, app_name, app_version, capabilities).await
    }

    /// Connect via TCP address (for testing or Windows TCP fallback).
    pub async fn connect_tcp(
        addr: &str,
        app_name: &str,
        app_version: &str,
        capabilities: Vec<String>,
    ) -> Result<Self> {
        let channel = Channel::from_shared(addr.to_owned())
            .context("invalid address")?
            .connect()
            .await
            .context("connect to agent")?;
        Self::register_with_channel(channel, app_name, app_version, capabilities).await
    }

    async fn register_with_channel(
        channel: Channel,
        app_name: &str,
        app_version: &str,
        capabilities: Vec<String>,
    ) -> Result<Self> {
        let mut inner = AppPlatformClient::new(channel);
        let resp = inner
            .register_app(RegisterAppRequest {
                app_name: app_name.to_string(),
                app_version: app_version.to_string(),
                capabilities,
                metadata: HashMap::new(),
            })
            .await
            .context("register_app")?
            .into_inner();

        info!(
            app_id = %resp.app_id,
            device_id = %resp.device_id,
            "Registered with agent"
        );

        Ok(Self {
            inner,
            session: Session {
                app_id: resp.app_id,
                session_token: resp.session_token,
            },
        })
    }

    pub fn app_id(&self) -> &str {
        &self.session.app_id
    }

    /// Send a heartbeat to keep the session alive.
    pub async fn heartbeat(&mut self) -> Result<bool> {
        let resp = self
            .inner
            .heartbeat(HeartbeatRequest {
                app_id: self.session.app_id.clone(),
                session_token: self.session.session_token.clone(),
            })
            .await
            .context("heartbeat")?
            .into_inner();
        Ok(resp.session_valid)
    }

    /// Report application health status.
    pub async fn report_health(
        &mut self,
        status: HealthStatus,
        message: &str,
        metrics: HashMap<String, String>,
    ) -> Result<()> {
        self.inner
            .report_health(HealthReport {
                app_id: self.session.app_id.clone(),
                session_token: self.session.session_token.clone(),
                status: status as i32,
                message: message.to_string(),
                metrics,
            })
            .await
            .context("report_health")?;
        Ok(())
    }

    /// Publish data to the backend via the agent.
    pub async fn publish(&mut self, topic: &str, payload: impl Into<Vec<u8>>) -> Result<String> {
        let resp = self
            .inner
            .publish_data(PublishDataRequest {
                app_id: self.session.app_id.clone(),
                session_token: self.session.session_token.clone(),
                topic: topic.to_string(),
                payload: payload.into(),
                metadata: HashMap::new(),
            })
            .await
            .context("publish_data")?
            .into_inner();
        Ok(resp.message_id)
    }

    /// Start watching configuration changes.
    /// Returns a channel receiver; each item is a `ConfigUpdate`.
    pub async fn watch_config(
        &mut self,
        keys: Vec<String>,
    ) -> Result<mpsc::Receiver<ConfigUpdate>> {
        let (tx, rx) = mpsc::channel(64);
        let mut stream = self
            .inner
            .watch_config(WatchConfigRequest {
                app_id: self.session.app_id.clone(),
                session_token: self.session.session_token.clone(),
                keys,
            })
            .await
            .context("watch_config")?
            .into_inner();

        tokio::spawn(async move {
            while let Ok(Some(update)) = stream.message().await {
                if tx.send(update).await.is_err() {
                    break;
                }
            }
        });

        Ok(rx)
    }

    /// Subscribe to downlink data from the backend.
    pub async fn subscribe_data(
        &mut self,
        topics: Vec<String>,
    ) -> Result<mpsc::Receiver<DataMessage>> {
        let (tx, rx) = mpsc::channel(64);
        let mut stream = self
            .inner
            .subscribe_data(SubscribeDataRequest {
                app_id: self.session.app_id.clone(),
                session_token: self.session.session_token.clone(),
                topics,
            })
            .await
            .context("subscribe_data")?
            .into_inner();

        tokio::spawn(async move {
            while let Ok(Some(msg)) = stream.message().await {
                if tx.send(msg).await.is_err() {
                    break;
                }
            }
        });

        Ok(rx)
    }

    /// Unregister from the agent (graceful shutdown).
    pub async fn unregister(mut self) -> Result<()> {
        self.inner
            .unregister_app(UnregisterAppRequest {
                app_id: self.session.app_id.clone(),
                session_token: self.session.session_token.clone(),
            })
            .await
            .context("unregister_app")?;
        info!(app_id = %self.session.app_id, "Unregistered from agent");
        Ok(())
    }
}
