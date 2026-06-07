#![cfg(test)]

//! Public test support utilities for integration tests.
//!
//! Provides:
//! - `RecordingPublisher` — a mock publisher that captures uplink data without a real MQTT broker
//! - `SpawnedAgent` — a running, AppPlatform-only agent listening on a temp UDS socket
//! - `spawn_app_platform_server` — factory that creates and starts a full AppPlatform server
//!
//! Only compiled when the `test-support` Cargo feature is enabled — invisible to
//! production binaries.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_core::chain::{AuditSink, AuditWriter};
use agent_core::security::{AuditEvent, BasicSecurityCenter, RbacPolicy, ReplayGuard};
use agent_store::StateStore;
use anyhow::{Context, Result};
use tokio::net::UnixListener;
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;

use crate::app_platform::{AppPlatformService, AppPlatformState, DynAppDataPublisher};
use crate::config_manager::ConfigManager;
use crate::data_router::DataRouter;
use crate::health_evaluator::HealthEvaluator;

// ── RecordingPublisher ──────────────────────────────────────────────────────

type PublishedAppData = Vec<(String, Vec<u8>)>;
type SharedPublishedAppData = Arc<Mutex<PublishedAppData>>;

/// A mock publisher for testing that records all published data in memory.
/// No data is published to a real MQTT broker.
#[derive(Clone, Default)]
pub struct RecordingPublisher {
    published: SharedPublishedAppData,
}

impl RecordingPublisher {
    /// Returns the recorded topic/payload pairs in publish order, clearing the
    /// internal buffer.
    pub fn drain(&self) -> Vec<(String, Vec<u8>)> {
        let mut guard = self.published.lock().unwrap();
        std::mem::take(&mut *guard)
    }

    /// Returns the current number of recorded entries.
    pub fn len(&self) -> usize {
        self.published.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[async_trait::async_trait]
impl DynAppDataPublisher for RecordingPublisher {
    async fn publish_app_data(&self, topic: String, payload: Vec<u8>) -> Result<()> {
        self.published.lock().unwrap().push((topic, payload));
        Ok(())
    }
}

// ── StoreAuditSink ──────────────────────────────────────────────────────────

struct StoreAuditSink {
    store: Arc<Mutex<StateStore>>,
}

impl AuditSink for StoreAuditSink {
    fn append_audit_event(&self, event: AuditEvent) -> Result<(), String> {
        self.store
            .lock()
            .map_err(|e| e.to_string())?
            .append_audit_event(event)
            .map_err(|e| e.to_string())
    }
}

// ── SpawnedAgent ────────────────────────────────────────────────────────────

/// A running, AppPlatform-only agent server.
///
/// Holds a handle to a tonic server on a UDS socket in a temp directory.
/// Signals shutdown on drop or via `shutdown()`.
    /// UDS socket file path.
    pub socket_path: PathBuf,
    /// Recorded publisher for post-mortem verification.
    pub publisher: RecordingPublisher,
    /// Shared state store for post-mortem audit queries.
    pub store: Arc<Mutex<StateStore>>,
    /// Server task handle; held in an `Option` so it can be taken by either
    /// `shutdown()` or `drop()` without conflict.
    server_handle: Option<tokio::task::JoinHandle<()>>,
    /// Send `true` to signal graceful server shutdown.
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    /// Temp directory holding the UDS socket; auto-removed when dropped.
    _temp_dir: tempfile::TempDir,
}

impl Drop for SpawnedAgent {
    fn drop(&mut self) {
        // Signal the server to stop; ignore errors (channel may already be closed).
        let _ = self.shutdown_tx.send(true);
        // Abort the server task so the socket is released promptly.
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
        // _temp_dir cleanup removes the socket file and directory automatically.
    }
}

impl SpawnedAgent {
    /// Gracefully stop the agent server and await completion.
    ///
    /// The socket file and temp directory are removed when `SpawnedAgent` is
    /// dropped (this method consumes `self`, so drop happens immediately after).
    pub async fn shutdown(mut self) {
        let _ = self.shutdown_tx.send(true);
        if let Some(handle) = self.server_handle.take() {
            let _ = handle.await;
        }
    }
}

// ── factory ─────────────────────────────────────────────────────────────────

/// Create and start a complete AppPlatform server.
///
/// This function:
/// 1. Creates an in-memory StateStore
/// 2. Assembles SecurityCenter, AuditWriter, DataRouter + RecordingPublisher,
///    ConfigManager, and HealthEvaluator
/// 3. Wires them into an `AppPlatformState`
/// 4. Binds and starts a tonic server on a temp UDS socket
///
/// Returns a `SpawnedAgent` with the socket path, recording publisher, store
/// handle, and shutdown control.
pub async fn spawn_app_platform_server() -> Result<SpawnedAgent> {
    let temp_dir = tempfile::TempDir::new().context("create temp dir for UDS socket")?;
    let socket_path = temp_dir.path().join("app.sock");

    let device_id = "test-device-e2e";

    // Step 1: create AppPlatformState (it wraps StateStore internally)
    let mut state = AppPlatformState::new(
        device_id.to_string(),
        StateStore::open_in_memory().context("open in-memory StateStore")?,
    );

    // Step 2: clone the shared store Arc so the audit sink writes to the same DB
    let store_arc = Arc::clone(state.store());

    // Step 3: build security / audit against the shared store
    let security_center = Arc::new(Mutex::new(BasicSecurityCenter::new(
        RbacPolicy::default(),
        ReplayGuard::new(Duration::from_secs(300)),
    )));
    let audit_sink: Arc<dyn AuditSink> = Arc::new(StoreAuditSink {
        store: Arc::clone(&store_arc),
    });
    let audit_writer = AuditWriter::new(audit_sink);

    // Step 4: remaining components
    let publisher = RecordingPublisher::default();
    let data_router = Arc::new(DataRouter::new(
        "test-tenant".to_string(),
        device_id.to_string(),
    ));
    let config_manager = ConfigManager::new();

    let (health_action_tx, _health_action_rx) =
        tokio::sync::mpsc::channel::<crate::health_evaluator::HealthAction>(64);
    let health_evaluator = HealthEvaluator::new(health_action_tx);

    // Step 5: wire everything into state
    state = state
        .with_session_duration(Duration::from_secs(3600))
        .with_data_router(data_router, Arc::new(publisher.clone()))
        .with_config_manager(config_manager)
        .with_health_evaluator(health_evaluator)
        .with_security(security_center, audit_writer);

    let state = Arc::new(state);

    // Step 6: bind UDS and start server
    let _ = tokio::fs::remove_file(&socket_path).await;
    let uds = UnixListener::bind(&socket_path)
        .with_context(|| format!("bind UDS at {}", socket_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))
            .context("set socket permissions")?;
    }

    let uds_stream = UnixListenerStream::new(uds);
    let service = AppPlatformService::new(state);
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    let server_handle = tokio::spawn(async move {
        if let Err(e) = Server::builder()
            .add_service(service.into_server())
            .serve_with_incoming_shutdown(uds_stream, async move {
                let _ = shutdown_rx.changed().await;
            })
            .await
        {
            tracing::error!(error = %e, "AppPlatform E2E server exited with error");
        }
    });

    // give the server a moment to start listening
    tokio::time::sleep(Duration::from_millis(50)).await;

    Ok(SpawnedAgent {
        socket_path,
        publisher,
        store: store_arc,
        server_handle: Some(server_handle),
        shutdown_tx,
        _temp_dir: temp_dir,
    })
}
