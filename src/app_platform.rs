//! Southbound IPC service for payload applications (W2.1 + W2.2)
//!
//! AppPlatformService implements the AppPlatform gRPC service. Sessions are
//! persisted to StateStore so they survive agent restarts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use agent_store::{AppHealthReportRecord, AppManifestRecord, AppSessionRecord, StateStore};

use crate::app_lifecycle::AppLifecycleHandle;
use crate::config_manager::{ConfigManager, ConfigScope};
use crate::data_router::{AsyncPublish, DataRouter};
use crate::grpc::app::{
    ConfigUpdate, DataMessage, GetConfigRequest, GetConfigResponse, HealthReport, HealthResponse,
    HeartbeatRequest, HeartbeatResponse, PublishDataRequest, PublishDataResponse,
    RegisterAppRequest, RegisterAppResponse, SubscribeDataRequest, UnregisterAppRequest,
    UnregisterAppResponse, WatchConfigRequest,
    app_platform_server::{AppPlatform, AppPlatformServer},
};
use crate::health_evaluator::{HealthEvaluator, HealthStatus};
use crate::mqtt::MqttClient;

// ── helpers ──────────────────────────────────────────────────────────────────

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn generate_session_token() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes).expect("rng fill");
    base16::encode_lower(&bytes)
}

/// SHA-256 the token before storing — never persist raw bearer tokens.
fn hash_token(token: &str) -> String {
    use ring::digest::{SHA256, digest};
    let d = digest(&SHA256, token.as_bytes());
    base16::encode_lower(d.as_ref())
}

fn generate_app_id(app_name: &str) -> String {
    format!("{}_{}", app_name, now_unix_ms())
}

// ── state ─────────────────────────────────────────────────────────────────────

pub struct AppPlatformState {
    device_id: String,
    store: Mutex<StateStore>,
    session_duration: Duration,
    data_router: Option<Arc<DataRouter>>,
    config_manager: Option<Arc<ConfigManager>>,
    health_evaluator: Option<Arc<HealthEvaluator>>,
    app_data_publisher: Option<Arc<dyn DynAppDataPublisher>>,
    lifecycle: Option<AppLifecycleHandle>,
}

impl AppPlatformState {
    pub fn new(device_id: String, store: StateStore) -> Self {
        Self {
            device_id,
            store: Mutex::new(store),
            session_duration: Duration::from_secs(3600),
            data_router: None,
            config_manager: None,
            health_evaluator: None,
            app_data_publisher: None,
            lifecycle: None,
        }
    }

    pub fn with_session_duration(mut self, session_duration: Duration) -> Self {
        self.session_duration = session_duration;
        self
    }

    pub fn with_data_router(
        mut self,
        data_router: Arc<DataRouter>,
        publisher: Arc<dyn DynAppDataPublisher>,
    ) -> Self {
        self.data_router = Some(data_router);
        self.app_data_publisher = Some(publisher);
        self
    }

    pub fn with_config_manager(mut self, config_manager: Arc<ConfigManager>) -> Self {
        self.config_manager = Some(config_manager);
        self
    }

    pub fn with_health_evaluator(mut self, health_evaluator: Arc<HealthEvaluator>) -> Self {
        self.health_evaluator = Some(health_evaluator);
        self
    }

    pub fn with_lifecycle(mut self, lifecycle: AppLifecycleHandle) -> Self {
        self.lifecycle = Some(lifecycle);
        self
    }

    fn validate_session(&self, app_id: &str, token: &str) -> Result<(), Status> {
        let store = self.store.lock().unwrap();
        let rec = store
            .load_app_session(app_id)
            .map_err(|e| Status::internal(e.to_string()))?
            .ok_or_else(|| Status::unauthenticated("unknown app_id"))?;

        if rec.revoked {
            return Err(Status::unauthenticated("session revoked"));
        }
        if rec.session_token_hash != hash_token(token) {
            return Err(Status::unauthenticated("invalid session_token"));
        }
        if now_unix_ms() > rec.expires_at_unix_ms {
            return Err(Status::unauthenticated("session expired"));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
pub trait DynAppDataPublisher: Send + Sync {
    async fn publish_app_data(&self, topic: String, payload: Vec<u8>) -> Result<()>;
}

#[async_trait::async_trait]
impl DynAppDataPublisher for MqttClient {
    async fn publish_app_data(&self, topic: String, payload: Vec<u8>) -> Result<()> {
        MqttClient::publish_app_data(self, topic, payload).await
    }
}

impl<T> AsyncPublish for Arc<T>
where
    T: DynAppDataPublisher + ?Sized,
{
    async fn publish(&self, topic: String, payload: Vec<u8>) -> Result<()> {
        self.publish_app_data(topic, payload).await
    }
}

// ── service ───────────────────────────────────────────────────────────────────

pub struct AppPlatformService {
    state: Arc<AppPlatformState>,
}

impl AppPlatformService {
    pub fn new(state: Arc<AppPlatformState>) -> Self {
        Self { state }
    }

    pub fn into_server(self) -> AppPlatformServer<Self> {
        AppPlatformServer::new(self)
    }
}

#[tonic::async_trait]
impl AppPlatform for AppPlatformService {
    async fn register_app(
        &self,
        request: Request<RegisterAppRequest>,
    ) -> Result<Response<RegisterAppResponse>, Status> {
        let req = request.into_inner();
        if req.app_name.is_empty() {
            return Err(Status::invalid_argument("app_name is required"));
        }

        let app_id = generate_app_id(&req.app_name);
        let token = generate_session_token();
        let now = now_unix_ms();
        let expires = now + self.state.session_duration.as_millis() as i64;

        let record = AppSessionRecord {
            app_id: app_id.clone(),
            app_name: req.app_name.clone(),
            app_version: req.app_version.clone(),
            session_token_hash: hash_token(&token),
            capabilities_json: serde_json::to_string(&req.capabilities)
                .unwrap_or_else(|_| "[]".into()),
            metadata_json: serde_json::to_string(&req.metadata).unwrap_or_else(|_| "{}".into()),
            device_id: self.state.device_id.clone(),
            registered_at_unix_ms: now,
            expires_at_unix_ms: expires,
            last_heartbeat_unix_ms: now,
            revoked: false,
        };

        self.state
            .store
            .lock()
            .unwrap()
            .upsert_app_session(&record)
            .map_err(|e| Status::internal(e.to_string()))?;

        let manifest_json = serde_json::json!({
            "kind": "registry",
            "app_id": app_id,
            "app_name": req.app_name,
            "version": req.app_version,
            "capabilities": req.capabilities,
            "metadata": req.metadata,
            "device_id": self.state.device_id,
            "registered_at_unix_ms": now,
        })
        .to_string();

        self.state
            .store
            .lock()
            .unwrap()
            .upsert_app_manifest(&AppManifestRecord {
                app_id: app_id.clone(),
                version: req.app_version.clone(),
                manifest_json,
            })
            .map_err(|e| Status::internal(e.to_string()))?;

        if let Some(lifecycle) = self.state.lifecycle.as_ref() {
            lifecycle
                .register(&app_id, &req.app_name, &req.app_version)
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }

        info!(
            app_id = %app_id,
            app_name = %req.app_name,
            app_version = %req.app_version,
            "Application registered"
        );

        Ok(Response::new(RegisterAppResponse {
            app_id,
            session_token: token,
            session_expires_at: expires / 1000,
            device_id: self.state.device_id.clone(),
        }))
    }

    async fn heartbeat(
        &self,
        request: Request<HeartbeatRequest>,
    ) -> Result<Response<HeartbeatResponse>, Status> {
        let req = request.into_inner();
        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        let now = now_unix_ms();
        let new_exp = now + self.state.session_duration.as_millis() as i64;

        self.state
            .store
            .lock()
            .unwrap()
            .touch_app_session(&req.app_id, new_exp, now)
            .map_err(|e| Status::internal(e.to_string()))?;

        debug!(app_id = %req.app_id, "Heartbeat");

        Ok(Response::new(HeartbeatResponse {
            session_valid: true,
            session_expires_at: new_exp / 1000,
        }))
    }

    async fn report_health(
        &self,
        request: Request<HealthReport>,
    ) -> Result<Response<HealthResponse>, Status> {
        let req = request.into_inner();
        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        let record = AppHealthReportRecord {
            app_id: req.app_id.clone(),
            status: serde_json::to_string(&HealthStatus::from(req.status))
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
            message: req.message.clone(),
            metrics_json: serde_json::to_string(&req.metrics).unwrap_or_else(|_| "{}".into()),
            reported_at_unix_ms: now_unix_ms(),
        };

        self.state
            .store
            .lock()
            .unwrap()
            .insert_health_report(&record)
            .map_err(|e| Status::internal(e.to_string()))?;

        if let Some(evaluator) = self.state.health_evaluator.as_ref() {
            evaluator
                .report(&req.app_id, HealthStatus::from(req.status))
                .await;
        }

        info!(
            app_id = %req.app_id,
            status = ?req.status,
            "Health report received"
        );

        Ok(Response::new(HealthResponse { accepted: true }))
    }

    async fn publish_data(
        &self,
        request: Request<PublishDataRequest>,
    ) -> Result<Response<PublishDataResponse>, Status> {
        let req = request.into_inner();
        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        debug!(
            app_id = %req.app_id,
            topic = %req.topic,
            bytes = req.payload.len(),
            "Data published"
        );

        if let (Some(router), Some(publisher)) = (
            self.state.data_router.as_ref(),
            self.state.app_data_publisher.as_ref(),
        ) {
            router
                .publish_uplink(&req.app_id, &req.topic, req.payload, Arc::clone(publisher))
                .await
                .map_err(|e| Status::internal(e.to_string()))?;
        }
        let message_id = format!("msg_{}", now_unix_ms());

        Ok(Response::new(PublishDataResponse {
            accepted: true,
            message_id,
        }))
    }

    type WatchConfigStream = ReceiverStream<Result<ConfigUpdate, Status>>;

    async fn watch_config(
        &self,
        request: Request<WatchConfigRequest>,
    ) -> Result<Response<Self::WatchConfigStream>, Status> {
        let req = request.into_inner();
        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        let (tx, rx) = mpsc::channel(16);
        info!(app_id = %req.app_id, keys = ?req.keys, "Config watch started");

        if let Some(config_manager) = self.state.config_manager.as_ref() {
            let mut watcher = config_manager.subscribe_app(&req.app_id);
            let keys = req.keys;
            tokio::spawn(async move {
                while let Some(event) = watcher.next_change().await {
                    if !keys.is_empty() && !keys.iter().any(|key| key == &event.key) {
                        continue;
                    }
                    let change_type = if event.value.is_some() {
                        "updated".to_string()
                    } else {
                        "deleted".to_string()
                    };
                    let update = ConfigUpdate {
                        key: event.key,
                        value: event.value.unwrap_or_default(),
                        version: event.version.min(i64::MAX as u64) as i64,
                        change_type,
                    };
                    if tx.send(Ok(update)).await.is_err() {
                        break;
                    }
                }
            });
        } else {
            tokio::spawn(async move { drop(tx) });
        }

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type SubscribeDataStream = ReceiverStream<Result<DataMessage, Status>>;

    async fn subscribe_data(
        &self,
        request: Request<SubscribeDataRequest>,
    ) -> Result<Response<Self::SubscribeDataStream>, Status> {
        let req = request.into_inner();
        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        let (tx, rx) = mpsc::channel(16);
        info!(app_id = %req.app_id, topics = ?req.topics, "Data subscription started");

        if let Some(router) = self.state.data_router.as_ref() {
            let mut downlink_rx = router.downlink_registry().subscribe(&req.app_id);
            let topics = req.topics;
            tokio::spawn(async move {
                while let Some((topic, payload)) = downlink_rx.recv().await {
                    if !topics.is_empty() && !topics.iter().any(|filter| filter == &topic) {
                        continue;
                    }
                    let msg = DataMessage {
                        topic,
                        payload,
                        timestamp: now_unix_ms() / 1000,
                        metadata: HashMap::new(),
                    };
                    if tx.send(Ok(msg)).await.is_err() {
                        break;
                    }
                }
            });
        } else {
            tokio::spawn(async move { drop(tx) });
        }

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_config(
        &self,
        request: Request<GetConfigRequest>,
    ) -> Result<Response<GetConfigResponse>, Status> {
        let req = request.into_inner();
        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        let (config, version) = self
            .state
            .config_manager
            .as_ref()
            .map(|manager| {
                let scope = ConfigScope::App(req.app_id.clone());
                let snapshot = manager.snapshot(&scope);
                let filtered = if req.keys.is_empty() {
                    snapshot
                } else {
                    snapshot
                        .into_iter()
                        .filter(|(key, _)| req.keys.iter().any(|wanted| wanted == key))
                        .collect()
                };
                let v = manager.global_version() as i64;
                (filtered, v)
            })
            .unwrap_or_default();
        Ok(Response::new(GetConfigResponse { config, version }))
    }

    async fn unregister_app(
        &self,
        request: Request<UnregisterAppRequest>,
    ) -> Result<Response<UnregisterAppResponse>, Status> {
        let req = request.into_inner();
        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        self.state
            .store
            .lock()
            .unwrap()
            .revoke_app_session(&req.app_id)
            .map_err(|e| Status::internal(e.to_string()))?;

        if let Some(lifecycle) = self.state.lifecycle.as_ref() {
            if let Err(error) = lifecycle.stop(&req.app_id).await {
                tracing::warn!(
                    app_id = %req.app_id,
                    error = %error,
                    "failed to stop lifecycle app during unregister"
                );
            }
        }

        info!(app_id = %req.app_id, "Application unregistered");

        Ok(Response::new(UnregisterAppResponse { success: true }))
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    use agent_store::StateStore;
    use pal_core::PlatformBuilder;
    use tokio_stream::StreamExt;

    fn make_service() -> AppPlatformService {
        let store = StateStore::open_in_memory().unwrap();
        let state = Arc::new(AppPlatformState::new("test-device".into(), store));
        AppPlatformService::new(state)
    }

    fn mock_lifecycle() -> crate::app_lifecycle::AppLifecycleHandle {
        let context = pal_mock::MockPlatformBuilder::default().build().unwrap();
        crate::app_lifecycle::spawn_lifecycle_manager(&context)
    }

    #[derive(Default)]
    struct RecordingPublisher {
        published: StdMutex<Vec<(String, Vec<u8>)>>,
    }

    #[async_trait::async_trait]
    impl DynAppDataPublisher for RecordingPublisher {
        async fn publish_app_data(&self, topic: String, payload: Vec<u8>) -> Result<()> {
            self.published.lock().unwrap().push((topic, payload));
            Ok(())
        }
    }

    async fn registered_session(svc: &AppPlatformService, name: &str) -> RegisterAppResponse {
        svc.register_app(Request::new(RegisterAppRequest {
            app_name: name.into(),
            app_version: "1.0.0".into(),
            capabilities: vec![],
            metadata: HashMap::new(),
        }))
        .await
        .unwrap()
        .into_inner()
    }

    #[tokio::test]
    async fn register_and_heartbeat() {
        let svc = make_service();

        let resp = svc
            .register_app(Request::new(RegisterAppRequest {
                app_name: "test-app".into(),
                app_version: "1.0.0".into(),
                capabilities: vec!["metrics".into()],
                metadata: HashMap::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert!(!resp.app_id.is_empty());
        assert!(!resp.session_token.is_empty());
        assert_eq!(resp.device_id, "test-device");

        let hb = svc
            .heartbeat(Request::new(HeartbeatRequest {
                app_id: resp.app_id.clone(),
                session_token: resp.session_token.clone(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert!(hb.session_valid);
        assert!(hb.session_expires_at > 0);
    }

    #[tokio::test]
    async fn register_app_updates_lifecycle_and_manifest_store() {
        let store = StateStore::open_in_memory().unwrap();
        let lifecycle = mock_lifecycle();
        let state = Arc::new(
            AppPlatformState::new("test-device".into(), store).with_lifecycle(lifecycle.clone()),
        );
        let svc = AppPlatformService::new(Arc::clone(&state));

        let resp = svc
            .register_app(Request::new(RegisterAppRequest {
                app_name: "managed-app".into(),
                app_version: "1.2.3".into(),
                capabilities: vec!["metrics".into()],
                metadata: HashMap::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(
            lifecycle.get_state(&resp.app_id).await,
            Some(crate::app_lifecycle::AppState::Registered)
        );
        let manifest = state
            .store
            .lock()
            .unwrap()
            .load_app_manifest(&resp.app_id)
            .unwrap()
            .expect("manifest persisted");
        assert_eq!(manifest.version, "1.2.3");
        assert!(manifest.manifest_json.contains("\"kind\":\"registry\""));
    }

    #[tokio::test]
    async fn invalid_token_rejected() {
        let svc = make_service();

        let resp = svc
            .register_app(Request::new(RegisterAppRequest {
                app_name: "app2".into(),
                app_version: "1.0".into(),
                capabilities: vec![],
                metadata: HashMap::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        let err = svc
            .heartbeat(Request::new(HeartbeatRequest {
                app_id: resp.app_id.clone(),
                session_token: "wrong-token".into(),
            }))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn unregister_revokes_session() {
        let svc = make_service();

        let resp = svc
            .register_app(Request::new(RegisterAppRequest {
                app_name: "app3".into(),
                app_version: "0.1".into(),
                capabilities: vec![],
                metadata: HashMap::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        svc.unregister_app(Request::new(UnregisterAppRequest {
            app_id: resp.app_id.clone(),
            session_token: resp.session_token.clone(),
        }))
        .await
        .unwrap();

        let err = svc
            .heartbeat(Request::new(HeartbeatRequest {
                app_id: resp.app_id,
                session_token: resp.session_token,
            }))
            .await
            .unwrap_err();

        assert_eq!(err.code(), tonic::Code::Unauthenticated);
    }

    #[tokio::test]
    async fn publish_data_routes_through_data_router() {
        let store = StateStore::open_in_memory().unwrap();
        let router = Arc::new(DataRouter::new("tenant-a".into(), "test-device".into()));
        let publisher = Arc::new(RecordingPublisher::default());
        let state = Arc::new(
            AppPlatformState::new("test-device".into(), store)
                .with_data_router(router, publisher.clone()),
        );
        let svc = AppPlatformService::new(state);
        let session = registered_session(&svc, "router-app").await;

        svc.publish_data(Request::new(PublishDataRequest {
            app_id: session.app_id,
            session_token: session.session_token,
            topic: "metrics".into(),
            payload: b"ok".to_vec(),
            metadata: HashMap::new(),
        }))
        .await
        .unwrap();

        let published = publisher.published.lock().unwrap();
        assert_eq!(published.len(), 1);
        assert!(
            published[0]
                .0
                .starts_with("tenant-a/test-device/apps/router-app_")
        );
        assert!(published[0].0.ends_with("/metrics"));
        assert_eq!(published[0].1, b"ok");
    }

    #[tokio::test]
    async fn get_config_reads_app_scope() {
        let store = StateStore::open_in_memory().unwrap();
        let config_manager = ConfigManager::new();
        let state = Arc::new(
            AppPlatformState::new("test-device".into(), store)
                .with_config_manager(Arc::clone(&config_manager)),
        );
        let svc = AppPlatformService::new(state);
        let session = registered_session(&svc, "config-app").await;

        config_manager.set(ConfigScope::App(session.app_id.clone()), "threshold", "42");

        let resp = svc
            .get_config(Request::new(GetConfigRequest {
                app_id: session.app_id,
                session_token: session.session_token,
                keys: vec!["threshold".into()],
            }))
            .await
            .unwrap()
            .into_inner();

        assert_eq!(resp.config.get("threshold"), Some(&"42".to_string()));
    }

    #[tokio::test]
    async fn subscribe_data_receives_downlink() {
        let store = StateStore::open_in_memory().unwrap();
        let router = Arc::new(DataRouter::new("tenant-a".into(), "test-device".into()));
        let publisher = Arc::new(RecordingPublisher::default());
        let state = Arc::new(
            AppPlatformState::new("test-device".into(), store)
                .with_data_router(Arc::clone(&router), publisher),
        );
        let svc = AppPlatformService::new(state);
        let session = registered_session(&svc, "downlink-app").await;

        let mut stream = svc
            .subscribe_data(Request::new(SubscribeDataRequest {
                app_id: session.app_id.clone(),
                session_token: session.session_token,
                topics: vec!["cmd".into()],
            }))
            .await
            .unwrap()
            .into_inner();

        router
            .downlink_registry()
            .deliver(&session.app_id, "cmd".into(), b"run".to_vec())
            .await;

        let msg = tokio::time::timeout(std::time::Duration::from_secs(1), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(msg.topic, "cmd");
        assert_eq!(msg.payload, b"run");
    }
}
