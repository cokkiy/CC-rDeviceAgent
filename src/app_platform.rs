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
use tracing::{debug, info, warn};

use agent_store::{AppHealthReportRecord, AppSessionRecord, StateStore};

use crate::grpc::app::{
    app_platform_server::{AppPlatform, AppPlatformServer},
    ConfigUpdate, DataMessage, GetConfigRequest, GetConfigResponse, HealthReport, HealthResponse,
    HeartbeatRequest, HeartbeatResponse, PublishDataRequest, PublishDataResponse,
    RegisterAppRequest, RegisterAppResponse, SubscribeDataRequest, UnregisterAppRequest,
    UnregisterAppResponse, WatchConfigRequest,
};

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
}

impl AppPlatformState {
    pub fn new(device_id: String, store: StateStore) -> Self {
        Self {
            device_id,
            store: Mutex::new(store),
            session_duration: Duration::from_secs(3600),
        }
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
            metadata_json: serde_json::to_string(&req.metadata)
                .unwrap_or_else(|_| "{}".into()),
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
        self.state.validate_session(&req.app_id, &req.session_token)?;

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
        self.state.validate_session(&req.app_id, &req.session_token)?;

        let record = AppHealthReportRecord {
            app_id: req.app_id.clone(),
            status: format!("{:?}", req.status),
            message: req.message.clone(),
            metrics_json: serde_json::to_string(&req.metrics)
                .unwrap_or_else(|_| "{}".into()),
            reported_at_unix_ms: now_unix_ms(),
        };

        self.state
            .store
            .lock()
            .unwrap()
            .insert_health_report(&record)
            .map_err(|e| Status::internal(e.to_string()))?;

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
        self.state.validate_session(&req.app_id, &req.session_token)?;

        debug!(
            app_id = %req.app_id,
            topic = %req.topic,
            bytes = req.payload.len(),
            "Data published"
        );

        // TODO(W2.4): route through DataRouter to MQTT
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
        self.state.validate_session(&req.app_id, &req.session_token)?;

        let (tx, rx) = mpsc::channel(16);
        info!(app_id = %req.app_id, keys = ?req.keys, "Config watch started");

        // TODO(W2.5): push real ConfigUpdate events from ConfigManager
        tokio::spawn(async move { drop(tx) });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type SubscribeDataStream = ReceiverStream<Result<DataMessage, Status>>;

    async fn subscribe_data(
        &self,
        request: Request<SubscribeDataRequest>,
    ) -> Result<Response<Self::SubscribeDataStream>, Status> {
        let req = request.into_inner();
        self.state.validate_session(&req.app_id, &req.session_token)?;

        let (tx, rx) = mpsc::channel(16);
        info!(app_id = %req.app_id, topics = ?req.topics, "Data subscription started");

        // TODO(W2.4): push real DataMessage events from DataRouter
        tokio::spawn(async move { drop(tx) });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_config(
        &self,
        request: Request<GetConfigRequest>,
    ) -> Result<Response<GetConfigResponse>, Status> {
        let req = request.into_inner();
        self.state.validate_session(&req.app_id, &req.session_token)?;

        // TODO(W2.5): read from ConfigManager
        Ok(Response::new(GetConfigResponse {
            config: HashMap::new(),
            version: 1,
        }))
    }

    async fn unregister_app(
        &self,
        request: Request<UnregisterAppRequest>,
    ) -> Result<Response<UnregisterAppResponse>, Status> {
        let req = request.into_inner();
        self.state.validate_session(&req.app_id, &req.session_token)?;

        self.state
            .store
            .lock()
            .unwrap()
            .revoke_app_session(&req.app_id)
            .map_err(|e| Status::internal(e.to_string()))?;

        info!(app_id = %req.app_id, "Application unregistered");

        Ok(Response::new(UnregisterAppResponse { success: true }))
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use agent_store::StateStore;

    fn make_service() -> AppPlatformService {
        let store = StateStore::open_in_memory().unwrap();
        let state = Arc::new(AppPlatformState::new("test-device".into(), store));
        AppPlatformService::new(state)
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
}
