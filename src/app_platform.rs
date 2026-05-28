//! Southbound IPC service for payload applications
//!
//! This module implements the AppPlatform gRPC service that allows payload applications
//! to register, publish data, subscribe to configuration, and report health status.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};
use tracing::{debug, info};

use crate::grpc::app::{
    app_platform_server::{AppPlatform, AppPlatformServer},
    ConfigUpdate, DataMessage, GetConfigRequest, GetConfigResponse, HealthReport, HealthResponse,
    HeartbeatRequest, HeartbeatResponse, PublishDataRequest, PublishDataResponse,
    RegisterAppRequest, RegisterAppResponse, SubscribeDataRequest, UnregisterAppRequest,
    UnregisterAppResponse, WatchConfigRequest,
};

/// Application session information
#[derive(Debug, Clone)]
#[allow(dead_code)]
struct AppSession {
    app_id: String,
    app_name: String,
    app_version: String,
    session_token: String,
    capabilities: Vec<String>,
    metadata: HashMap<String, String>,
    registered_at: SystemTime,
    expires_at: SystemTime,
    last_heartbeat: SystemTime,
}

/// Application platform state
pub struct AppPlatformState {
    device_id: String,
    sessions: RwLock<HashMap<String, AppSession>>,
    session_duration_secs: u64,
}

impl AppPlatformState {
    pub fn new(device_id: String) -> Self {
        Self {
            device_id,
            sessions: RwLock::new(HashMap::new()),
            session_duration_secs: 3600, // 1 hour default
        }
    }

    fn generate_app_id(&self, app_name: &str) -> String {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        format!("{}_{}", app_name, timestamp)
    }

    fn generate_session_token(&self) -> String {
        use ring::rand::{SecureRandom, SystemRandom};
        let rng = SystemRandom::new();
        let mut token = [0u8; 32];
        rng.fill(&mut token).unwrap();
        base16::encode_lower(&token)
    }

    fn validate_session(&self, app_id: &str, session_token: &str) -> Result<(), Status> {
        let sessions = self.sessions.read().unwrap();
        let session = sessions
            .get(app_id)
            .ok_or_else(|| Status::unauthenticated("Invalid app_id"))?;

        if session.session_token != session_token {
            return Err(Status::unauthenticated("Invalid session_token"));
        }

        let now = SystemTime::now();
        if now > session.expires_at {
            return Err(Status::unauthenticated("Session expired"));
        }

        Ok(())
    }

    fn extend_session(&self, app_id: &str) -> Result<SystemTime, Status> {
        let mut sessions = self.sessions.write().unwrap();
        let session = sessions
            .get_mut(app_id)
            .ok_or_else(|| Status::not_found("Session not found"))?;

        let now = SystemTime::now();
        let new_expiry = now + std::time::Duration::from_secs(self.session_duration_secs);
        session.expires_at = new_expiry;
        session.last_heartbeat = now;

        Ok(new_expiry)
    }
}

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

        let app_id = self.state.generate_app_id(&req.app_name);
        let session_token = self.state.generate_session_token();
        let now = SystemTime::now();
        let expires_at = now + std::time::Duration::from_secs(self.state.session_duration_secs);

        let session = AppSession {
            app_id: app_id.clone(),
            app_name: req.app_name.clone(),
            app_version: req.app_version.clone(),
            session_token: session_token.clone(),
            capabilities: req.capabilities.clone(),
            metadata: req.metadata.clone(),
            registered_at: now,
            expires_at,
            last_heartbeat: now,
        };

        self.state
            .sessions
            .write()
            .unwrap()
            .insert(app_id.clone(), session);

        info!(
            app_id = %app_id,
            app_name = %req.app_name,
            app_version = %req.app_version,
            capabilities = ?req.capabilities,
            "Application registered"
        );

        Ok(Response::new(RegisterAppResponse {
            app_id,
            session_token,
            session_expires_at: expires_at
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
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

        let new_expiry = self.state.extend_session(&req.app_id)?;

        debug!(app_id = %req.app_id, "Heartbeat received");

        Ok(Response::new(HeartbeatResponse {
            session_valid: true,
            session_expires_at: new_expiry
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        }))
    }

    async fn report_health(
        &self,
        request: Request<HealthReport>,
    ) -> Result<Response<HealthResponse>, Status> {
        let req = request.into_inner();

        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        info!(
            app_id = %req.app_id,
            status = ?req.status,
            message = %req.message,
            "Health report received"
        );

        // TODO: Store health status and trigger health evaluator

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
            payload_size = req.payload.len(),
            "Data published"
        );

        // TODO: Route data to backend via MQTT or other transport

        let message_id = format!("msg_{}", uuid::Uuid::new_v4());

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

        info!(
            app_id = %req.app_id,
            keys = ?req.keys,
            "Config watch started"
        );

        // TODO: Implement actual config watching
        // For now, just keep the stream open
        tokio::spawn(async move {
            // Stream will close when tx is dropped
            let _ = tx;
        });

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

        info!(
            app_id = %req.app_id,
            topics = ?req.topics,
            "Data subscription started"
        );

        // TODO: Implement actual data subscription
        tokio::spawn(async move {
            let _ = tx;
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn get_config(
        &self,
        request: Request<GetConfigRequest>,
    ) -> Result<Response<GetConfigResponse>, Status> {
        let req = request.into_inner();

        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        debug!(
            app_id = %req.app_id,
            keys = ?req.keys,
            "Config snapshot requested"
        );

        // TODO: Implement actual config retrieval
        let config = HashMap::new();

        Ok(Response::new(GetConfigResponse {
            config,
            version: 1,
        }))
    }

    async fn unregister_app(
        &self,
        request: Request<UnregisterAppRequest>,
    ) -> Result<Response<UnregisterAppResponse>, Status> {
        let req = request.into_inner();

        self.state
            .validate_session(&req.app_id, &req.session_token)?;

        self.state.sessions.write().unwrap().remove(&req.app_id);

        info!(app_id = %req.app_id, "Application unregistered");

        Ok(Response::new(UnregisterAppResponse { success: true }))
    }
}
