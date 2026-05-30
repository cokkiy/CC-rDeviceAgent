use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use async_stream::try_stream;
use futures_util::Stream;
use ring::digest;
use tokio::fs::{self, File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio::sync::watch;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Request, Response, Status, Streaming};
use tracing::{debug, error, info, warn};

use crate::app_platform::{AppPlatformService, AppPlatformState};
use crate::config::AppConfig;
use crate::config_manager::ConfigManager;
use crate::data_router::DataRouter;
use crate::grpc::cc::{
    AppControlResult, AppStartParameter, AppStartingResult, CaptureScreenChunk,
    CaptureScreenRequest, CloseAppRequest, CloseAppResponse, DownloadChunk, DownloadRequest, Empty,
    ExecuteCommandRequest, ExecuteCommandResponse, GetAllProcessInfoResponse,
    GetAppLauncherPathResponse, GetConnectionInformationsResponse,
    GetCurrentTelemetrySchemaResponse, GetFileInfoResponse, GetNetworkInterfacesResponse,
    GetServicePathResponse, GetTcpListenerInfosResponse, GetTelemetryProfilesResponse,
    GetUdpListenerInfosResponse, PathRef, RebootRequest, RenameFileRequest, RenameFileResponse,
    ReplaceTelemetryProfilesRequest, RestartAppRequest, RestartAppResponse, ServerVersionInfo,
    SetStateGatheringIntervalRequest, SetWatchingAppRequest, ShutdownRequest, StartAppRequest,
    StartAppResponse, TelemetryInclude, TelemetryIncludeDefinition, TelemetryProfile, UploadChunk,
    UploadResult, device_control_server::DeviceControl, device_control_server::DeviceControlServer,
    file_transfer_server::FileTransfer, file_transfer_server::FileTransferServer,
};
use crate::health_evaluator::HealthEvaluator;
use crate::platform;
use crate::state::{AppState, find_process_ids_by_name, terminate_process};
use crate::telemetry::{TelemetryProfileConfig, TelemetryScheduler};

pub async fn run(
    config_path: Option<PathBuf>,
    mut shutdown: watch::Receiver<bool>,
    console_telemetry: bool,
) -> Result<()> {
    let (config, resolved_config_path) = AppConfig::load_with_path(config_path.as_deref())?;
    let control_tls = config.control.tls.clone();
    let service_path = std::env::current_exe().context("resolve current executable")?;
    let state = Arc::new(AppState::new(config, resolved_config_path, service_path)?);
    let listen_addr = state.listen_addr()?;

    info!(
        device_id = state.device_id(),
        listen_addr = %listen_addr,
        watched_processes = state.watched_processes().len(),
        console_telemetry,
        "starting CC-rDeviceAgent"
    );

    if console_telemetry {
        tokio::spawn(console_telemetry_task(Arc::clone(&state)));
    }

    if state.mqtt_status_enabled() {
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            let Some(mqtt_client) = state_clone.mqtt_client().cloned() else {
                return;
            };

            info!("MQTT status publisher started");

            loop {
                let status = crate::mqtt::DeviceStatus::online(state_clone.device_id().to_string());
                if let Err(error) = mqtt_client.publish_status(&status).await {
                    warn!(error = %error, "failed to publish MQTT status");
                }

                tokio::time::sleep(Duration::from_secs(state_clone.interval_seconds().max(1)))
                    .await;
            }
        });
    }

    if state.mqtt_telemetry_enabled() {
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            info!("MQTT telemetry publisher started");
            let Some(mqtt_client) = state_clone.mqtt_client().cloned() else {
                return;
            };

            let mut profiles_rx = state_clone.telemetry_profile_receiver();
            let start = tokio::time::Instant::now();
            let mut scheduler =
                TelemetryScheduler::new(&profiles_rx.borrow().profiles, elapsed_ms(start));

            loop {
                if scheduler.is_empty() {
                    if profiles_rx.changed().await.is_err() {
                        break;
                    }
                    scheduler =
                        TelemetryScheduler::new(&profiles_rx.borrow().profiles, elapsed_ms(start));
                    continue;
                }

                let now_ms = elapsed_ms(start);
                let Some(deadline_ms) = scheduler.next_deadline_ms() else {
                    continue;
                };
                let wait_ms = deadline_ms.saturating_sub(now_ms);

                tokio::select! {
                    changed = profiles_rx.changed() => {
                        if changed.is_err() {
                            break;
                        }
                        scheduler = TelemetryScheduler::new(
                            &profiles_rx.borrow().profiles,
                            elapsed_ms(start),
                        );
                        continue;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(wait_ms.max(1))) => {}
                }

                let now_ms = elapsed_ms(start);
                let due_collect = scheduler.due_collection_indices(now_ms);
                if !due_collect.is_empty() {
                    let includes = scheduler.collection_includes(&due_collect);
                    let collected = state_clone.collect_telemetry_sections(&includes).await;
                    let profiles_version = profiles_rx.borrow().version;
                    for bundle in scheduler.collect_due_bundles(
                        &due_collect,
                        now_ms,
                        &collected,
                        state_clone.device_id(),
                        profiles_version,
                    ) {
                        if let Err(error) = mqtt_client.publish_telemetry(&bundle).await {
                            warn!(error = %error, "failed to publish telemetry via MQTT");
                        }
                    }
                }
            }
        });
    }

    // Start MQTT command listener if MQTT is enabled
    if state.mqtt_enabled()
        && let Some(mqtt_client) = state.mqtt_client()
    {
        let mqtt_client = mqtt_client.clone();
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            match mqtt_client.subscribe_commands().await {
                Ok(mut command_rx) => {
                    info!("MQTT command listener started");
                    while let Some(command) = command_rx.recv().await {
                        // Handle the command and send ack
                        let ack = state_clone.handle_mqtt_command(&command);
                        if let Err(e) = mqtt_client.publish_command_ack(&ack).await {
                            warn!("Failed to publish command ack: {:?}", e);
                        }
                    }
                    info!("MQTT command listener ended");
                }
                Err(e) => {
                    error!("Failed to subscribe to MQTT commands: {:?}", e);
                }
            }
        });
    }

    // Build the security middleware chain
    use agent_core::chain::{
        AuditWriter, IdentityExtractor, ResourceMapper, SecurityInterceptorLayer,
    };
    use agent_core::security::{BasicSecurityCenter, RbacPolicy, ReplayGuard};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct StoreAuditSink {
        store: Mutex<agent_store::StateStore>,
    }
    impl agent_core::chain::AuditSink for StoreAuditSink {
        fn append_audit_event(
            &self,
            event: agent_core::security::AuditEvent,
        ) -> Result<(), String> {
            self.store
                .lock()
                .map_err(|e| e.to_string())?
                .append_audit_event(event)
                .map_err(|e| e.to_string())
        }
    }

    // Open StateStore for audit persistence next to the service binary.
    let store_path = state
        .service_path()
        .parent()
        .map(|p| p.join("state.db"))
        .unwrap_or_else(|| std::path::PathBuf::from("state.db"));
    let state_store = Mutex::new(
        agent_store::StateStore::open(&store_path)
            .map_err(|e| anyhow::anyhow!("open state store at {}: {e}", store_path.display()))?,
    );
    let audit_sink: Arc<dyn agent_core::chain::AuditSink> =
        Arc::new(StoreAuditSink { store: state_store });
    let audit_writer = AuditWriter::new(audit_sink);
    let security_center = Arc::new(Mutex::new(BasicSecurityCenter::new(
        RbacPolicy::default(),
        ReplayGuard::new(Duration::from_secs(300)),
    )));
    let identity_extractor = IdentityExtractor::new(state.device_id().to_string());
    let resource_mapper = ResourceMapper;
    let security_layer = SecurityInterceptorLayer::new(
        Arc::clone(&security_center),
        audit_writer,
        identity_extractor,
        resource_mapper,
        state.device_id().to_string(),
    );

    // Start southbound IPC server for payload applications if enabled
    let _app_platform_handle = if state.config().app_platform.enabled {
        let app_platform_store = agent_store::StateStore::open(&store_path)
            .map_err(|e| anyhow::anyhow!("open state store for app platform: {e}"))?;
        let config_store = agent_store::StateStore::open(&store_path)
            .map_err(|e| anyhow::anyhow!("open state store for config manager: {e}"))?;
        let config_manager = ConfigManager::new_with_store(config_store);
        let data_router = Arc::new(DataRouter::new(
            "default".to_string(),
            state.device_id().to_string(),
        ));
        let (health_action_tx, mut health_action_rx) = tokio::sync::mpsc::channel(64);
        let health_evaluator = HealthEvaluator::new(health_action_tx);
        tokio::spawn(async move {
            while let Some(action) = health_action_rx.recv().await {
                warn!(
                    app_id = %action.app_id,
                    action = ?action.action,
                    failures = action.consecutive_failures,
                    "App health action emitted"
                );
            }
        });
        let mut app_platform_state =
            AppPlatformState::new(state.device_id().to_string(), app_platform_store)
                .with_session_duration(Duration::from_secs(
                    state.config().app_platform.session_duration_secs.max(1),
                ))
                .with_config_manager(config_manager)
                .with_health_evaluator(health_evaluator);
        if let Some(mqtt_client) = state.mqtt_client().cloned() {
            app_platform_state =
                app_platform_state.with_data_router(data_router, Arc::new(mqtt_client));
        }
        let app_platform_state = Arc::new(app_platform_state);
        let app_platform_service = AppPlatformService::new(Arc::clone(&app_platform_state));
        let socket_path = state.config().app_platform.socket_path.clone();

        info!(socket_path = %socket_path, "Starting southbound IPC server");

        let shutdown_clone = shutdown.clone();
        Some(tokio::spawn(async move {
            #[allow(unused_mut)]
            let mut shutdown_clone = shutdown_clone;
            #[cfg(unix)]
            {
                use tokio::net::UnixListener;
                use tokio_stream::wrappers::UnixListenerStream;

                // Ensure parent directory exists
                if let Some(parent) = std::path::Path::new(&socket_path).parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }

                // Remove existing socket file
                let _ = tokio::fs::remove_file(&socket_path).await;

                let uds = UnixListener::bind(&socket_path)
                    .context("bind Unix socket for app platform")?;
                let uds_stream = UnixListenerStream::new(uds);

                info!(socket_path = %socket_path, "Southbound IPC server listening");

                Server::builder()
                    .add_service(app_platform_service.into_server())
                    .serve_with_incoming_shutdown(uds_stream, async move {
                        let _ = shutdown_clone.changed().await;
                    })
                    .await
                    .context("run southbound IPC server")
            }

            #[cfg(windows)]
            {
                // TODO: Implement Named Pipe server for Windows
                anyhow::bail!("Windows Named Pipe support not yet implemented")
            }
        }))
    } else {
        info!("Southbound IPC server disabled");
        None
    };

    let mut server = Server::builder();
    if control_tls.enabled {
        server = server
            .tls_config(build_grpc_tls_config(&control_tls)?)
            .context("configure gRPC mTLS")?;
    }

    server
        .layer(security_layer)
        .add_service(DeviceControlServer::new(DeviceControlService {
            state: Arc::clone(&state),
        }))
        .add_service(FileTransferServer::new(FileTransferService {
            state: Arc::clone(&state),
        }))
        .serve_with_shutdown(listen_addr, async move {
            let _ = shutdown.changed().await;
        })
        .await
        .context("run gRPC server")
}

fn build_grpc_tls_config(tls: &crate::config::TlsConfig) -> Result<ServerTlsConfig> {
    let cert = read_required_file(tls.cert_path.as_deref(), "control.tls.cert_path")?;
    let key = read_required_file(tls.key_path.as_deref(), "control.tls.key_path")?;
    let ca = read_required_file(tls.ca_cert_path.as_deref(), "control.tls.ca_cert_path")?;
    let config = ServerTlsConfig::new()
        .identity(Identity::from_pem(cert, key))
        .client_ca_root(Certificate::from_pem(ca))
        .client_auth_optional(!tls.require_client_auth);
    Ok(config)
}

fn read_required_file(path: Option<&Path>, field: &str) -> Result<Vec<u8>> {
    let path = path.ok_or_else(|| anyhow::anyhow!("{field} is required"))?;
    std::fs::read(path).with_context(|| format!("read {field} from {}", path.display()))
}

#[derive(Clone)]
struct DeviceControlService {
    state: Arc<AppState>,
}

#[tonic::async_trait]
impl DeviceControl for DeviceControlService {
    type CaptureScreenStream =
        Pin<Box<dyn Stream<Item = Result<CaptureScreenChunk, Status>> + Send>>;

    async fn start_app(
        &self,
        request: Request<StartAppRequest>,
    ) -> Result<Response<StartAppResponse>, Status> {
        let mut results = Vec::new();
        for app in request.into_inner().apps {
            results.push(start_one_app(app).await);
        }

        Ok(Response::new(StartAppResponse { results }))
    }

    async fn close_app(
        &self,
        request: Request<CloseAppRequest>,
    ) -> Result<Response<CloseAppResponse>, Status> {
        let mut results = Vec::new();
        for pid in request.into_inner().process_ids {
            let result = match terminate_process(pid).map_err(status_from_error)? {
                true => AppControlResult::Closed,
                false => AppControlResult::NotRunning,
            };
            results.push(result as i32);
        }

        Ok(Response::new(CloseAppResponse { results }))
    }

    async fn restart_app(
        &self,
        request: Request<RestartAppRequest>,
    ) -> Result<Response<RestartAppResponse>, Status> {
        let mut results = Vec::new();
        for app in request.into_inner().apps {
            for pid in find_process_ids_by_name(&effective_process_name(&app)) {
                if let Err(error) = terminate_process(pid) {
                    warn!(pid, error = %error, "failed to terminate process during restart");
                }
            }

            results.push(start_one_app(app).await);
        }

        Ok(Response::new(RestartAppResponse { results }))
    }

    async fn reboot(&self, request: Request<RebootRequest>) -> Result<Response<Empty>, Status> {
        platform::reboot(request.into_inner().force).map_err(status_from_error)?;
        Ok(Response::new(Empty {}))
    }

    async fn shutdown(
        &self,
        _request: Request<ShutdownRequest>,
    ) -> Result<Response<Empty>, Status> {
        platform::shutdown().map_err(status_from_error)?;
        Ok(Response::new(Empty {}))
    }

    async fn get_system_state(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<crate::grpc::cc::DeviceSystemState>, Status> {
        Ok(Response::new(self.state.system_state()))
    }

    async fn set_watching_app(
        &self,
        request: Request<SetWatchingAppRequest>,
    ) -> Result<Response<Empty>, Status> {
        self.state
            .set_watched_processes(request.into_inner().process_names);
        Ok(Response::new(Empty {}))
    }

    async fn set_state_gathering_interval(
        &self,
        request: Request<SetStateGatheringIntervalRequest>,
    ) -> Result<Response<Empty>, Status> {
        let interval = request.into_inner().interval_seconds;
        if interval <= 0 {
            return Err(Status::invalid_argument(
                "interval_seconds must be positive",
            ));
        }

        self.state.set_interval_seconds(interval as u64);
        Ok(Response::new(Empty {}))
    }

    async fn get_telemetry_profiles(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetTelemetryProfilesResponse>, Status> {
        Ok(Response::new(GetTelemetryProfilesResponse {
            schema_version: self.state.telemetry_schema().schema_version,
            profiles_version: self.state.telemetry_profiles_version(),
            profiles: self
                .state
                .telemetry_profiles()
                .into_iter()
                .map(proto_telemetry_profile_from_config)
                .collect(),
        }))
    }

    async fn replace_telemetry_profiles(
        &self,
        request: Request<ReplaceTelemetryProfilesRequest>,
    ) -> Result<Response<GetTelemetryProfilesResponse>, Status> {
        let profiles = request
            .into_inner()
            .profiles
            .into_iter()
            .map(config_telemetry_profile_from_proto)
            .collect::<Result<Vec<_>, _>>()?;
        let state = self
            .state
            .replace_telemetry_profiles(profiles)
            .map_err(status_from_error)?;

        Ok(Response::new(GetTelemetryProfilesResponse {
            schema_version: self.state.telemetry_schema().schema_version,
            profiles_version: state.version,
            profiles: state
                .profiles
                .into_iter()
                .map(proto_telemetry_profile_from_config)
                .collect(),
        }))
    }

    async fn get_current_telemetry_schema(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetCurrentTelemetrySchemaResponse>, Status> {
        let schema = self.state.telemetry_schema();
        Ok(Response::new(GetCurrentTelemetrySchemaResponse {
            schema_version: schema.schema_version,
            supported_includes: schema
                .supported_includes
                .into_iter()
                .map(|item| TelemetryIncludeDefinition {
                    include: proto_include_from_key(&item.key) as i32,
                    key: item.key,
                    label: item.label,
                    description: item.description,
                })
                .collect(),
        }))
    }

    async fn capture_screen(
        &self,
        _request: Request<CaptureScreenRequest>,
    ) -> Result<Response<Self::CaptureScreenStream>, Status> {
        Err(Status::unimplemented(
            "Screen capture has been moved to a standalone application. \
             This feature will be available as a payload app in Phase 2.",
        ))
    }

    async fn get_file_info(
        &self,
        request: Request<PathRef>,
    ) -> Result<Response<GetFileInfoResponse>, Status> {
        let response = self
            .state
            .file_info(&request.into_inner().name)
            .map_err(status_from_error)?;
        Ok(Response::new(response))
    }

    async fn get_all_process_info(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetAllProcessInfoResponse>, Status> {
        Ok(Response::new(self.state.all_process_info()))
    }

    async fn get_server_version(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<ServerVersionInfo>, Status> {
        Ok(Response::new(self.state.server_version()))
    }

    async fn get_service_path(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetServicePathResponse>, Status> {
        Ok(Response::new(GetServicePathResponse {
            path: self.state.service_path().display().to_string(),
        }))
    }

    async fn get_app_launcher_path(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetAppLauncherPathResponse>, Status> {
        Ok(Response::new(GetAppLauncherPathResponse {
            path: self.state.launcher_proxy_path().to_string(),
        }))
    }

    async fn rename_file(
        &self,
        request: Request<RenameFileRequest>,
    ) -> Result<Response<RenameFileResponse>, Status> {
        let request = request.into_inner();
        let old_path = resolve_managed_file_path(&self.state, &request.old_name)?;
        let new_path = resolve_managed_file_path(&self.state, &request.new_name)?;

        if !old_path.exists() {
            return Ok(Response::new(RenameFileResponse { ok: false }));
        }

        if new_path.exists() {
            remove_existing_path(&new_path)
                .await
                .map_err(status_from_error)?;
        }

        fs::rename(&old_path, &new_path)
            .await
            .with_context(|| format!("rename {} -> {}", old_path.display(), new_path.display()))
            .map_err(status_from_error)?;

        Ok(Response::new(RenameFileResponse { ok: true }))
    }

    async fn get_network_interfaces(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetNetworkInterfacesResponse>, Status> {
        debug!("gRPC: get_network_interfaces called");
        let response = self.state.network_interfaces();
        debug!(
            "gRPC: returning {} network interfaces",
            response.items.len()
        );
        Ok(Response::new(response))
    }

    async fn get_connection_informations(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetConnectionInformationsResponse>, Status> {
        let response = self.state.connection_infos().map_err(status_from_error)?;
        Ok(Response::new(response))
    }

    async fn get_tcp_listener_infos(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetTcpListenerInfosResponse>, Status> {
        let response = self.state.tcp_listener_infos().map_err(status_from_error)?;
        Ok(Response::new(response))
    }

    async fn get_udp_listener_infos(
        &self,
        _request: Request<Empty>,
    ) -> Result<Response<GetUdpListenerInfosResponse>, Status> {
        let response = self.state.udp_listener_infos().map_err(status_from_error)?;
        Ok(Response::new(response))
    }

    async fn execute_command(
        &self,
        request: Request<ExecuteCommandRequest>,
    ) -> Result<Response<ExecuteCommandResponse>, Status> {
        use agent_core::command_policy::CommandPolicy;

        // Extract context before consuming request
        let sec = request
            .extensions()
            .get::<agent_core::chain::SecurityContext>()
            .ok_or_else(|| Status::internal("security context missing"))?
            .clone();

        let req = request.into_inner();

        // Parse the command string as JSON {command_id, params}, or treat
        // the raw string as a process_name for "restart_process".
        let (command_id, params): (String, serde_json::Value) =
            match serde_json::from_str::<serde_json::Value>(&req.command) {
                Ok(obj) if obj.is_object() => (
                    obj.get("command_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("restart_process")
                        .to_string(),
                    obj.get("params").cloned().unwrap_or(serde_json::json!({})),
                ),
                _ => (
                    "restart_process".to_string(),
                    serde_json::json!({"process_name": req.command}),
                ),
            };

        let policy = CommandPolicy::default();
        let validated = policy
            .validate(&sec.principal, &command_id, &params)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        match validated.command_id.as_str() {
            "restart_process" => {
                let process_name = validated
                    .argument("process_name")
                    .ok_or_else(|| Status::invalid_argument("process_name required"))?;
                let pids = crate::state::find_process_ids_by_name(process_name);
                if pids.is_empty() {
                    return Ok(Response::new(ExecuteCommandResponse {
                        exit_code: 1,
                        stdout: String::new(),
                        stderr: format!("process '{process_name}' not found"),
                    }));
                }
                let mut exit_code = 0;
                for pid in &pids {
                    if let Err(e) = crate::state::terminate_process(*pid) {
                        exit_code = 1;
                        tracing::warn!(pid, error = %e, "failed to terminate process");
                    }
                }
                Ok(Response::new(ExecuteCommandResponse {
                    exit_code,
                    stdout: format!("terminated {} pids for '{process_name}'", pids.len()),
                    stderr: String::new(),
                }))
            }
            other => Ok(Response::new(ExecuteCommandResponse {
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("command '{other}' is registered but has no runtime handler"),
            })),
        }
    }
}

#[derive(Clone)]
struct FileTransferService {
    state: Arc<AppState>,
}

#[tonic::async_trait]
impl FileTransfer for FileTransferService {
    type DownloadStream = Pin<Box<dyn Stream<Item = Result<DownloadChunk, Status>> + Send>>;

    async fn upload(
        &self,
        request: Request<Streaming<UploadChunk>>,
    ) -> Result<Response<UploadResult>, Status> {
        let mut stream = request.into_inner();
        let mut current_path: Option<PathBuf> = None;
        let mut current_file: Option<File> = None;

        while let Some(chunk) = stream.message().await? {
            if chunk.file_name.trim().is_empty() {
                return Err(Status::invalid_argument("file_name is required"));
            }

            let path = resolve_managed_file_path(&self.state, &chunk.file_name)?;
            ensure_upload_limits(
                &self.state,
                &path,
                chunk.position as u64,
                chunk.data.len() as u64,
            )?;
            if current_path.as_ref() != Some(&path) || current_file.is_none() {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .await
                        .with_context(|| format!("create directory {}", parent.display()))
                        .map_err(status_from_error)?;
                }

                current_file = Some(
                    OpenOptions::new()
                        .create(true)
                        .truncate(false)
                        .read(true)
                        .write(true)
                        .open(&path)
                        .await
                        .with_context(|| format!("open upload file {}", path.display()))
                        .map_err(status_from_error)?,
                );
                current_path = Some(path.clone());
            }

            let Some(file) = current_file.as_mut() else {
                return Err(Status::internal("upload stream lost its active file"));
            };

            file.seek(SeekFrom::Start(chunk.position as u64))
                .await
                .with_context(|| format!("seek upload file {}", path.display()))
                .map_err(status_from_error)?;
            file.write_all(&chunk.data)
                .await
                .with_context(|| format!("write upload file {}", path.display()))
                .map_err(status_from_error)?;

            if chunk.close_after_write {
                finalize_upload(current_path.take(), current_file.take()).await?;
            }
        }

        let finalized_path = current_path.clone();
        finalize_upload(current_path, current_file).await?;

        let message = if let Some(path) = finalized_path {
            let sha256 = sha256_file_hex(&path).await.map_err(status_from_error)?;
            format!("upload completed; sha256={sha256}")
        } else {
            "upload completed".to_string()
        };

        Ok(Response::new(UploadResult { ok: true, message }))
    }

    async fn download(
        &self,
        request: Request<DownloadRequest>,
    ) -> Result<Response<Self::DownloadStream>, Status> {
        let request = request.into_inner();
        let path = resolve_managed_file_path(&self.state, &request.file_name)?;
        let metadata = fs::metadata(&path)
            .await
            .with_context(|| format!("read metadata {}", path.display()))
            .map_err(status_from_error)?;
        let total_length = metadata.len();
        if total_length > self.state.max_file_transfer_bytes() {
            return Err(Status::resource_exhausted(
                "file exceeds transfer size limit",
            ));
        }
        let start_position = request.start_position.max(0) as u64;

        let output = try_stream! {
            let mut file = File::open(&path)
                .await
                .with_context(|| format!("open download file {}", path.display()))
                .map_err(status_from_error)?;
            file.seek(SeekFrom::Start(start_position))
                .await
                .with_context(|| format!("seek download file {}", path.display()))
                .map_err(status_from_error)?;

            let mut position = start_position;
            loop {
                let mut buffer = vec![0u8; 64 * 1024];
                let read = file.read(&mut buffer)
                    .await
                    .with_context(|| format!("read download file {}", path.display()))
                    .map_err(status_from_error)?;

                if read == 0 {
                    yield DownloadChunk {
                        position: position as i64,
                        data: Vec::new(),
                        length: 0,
                        completed: true,
                    };
                    break;
                }

                buffer.truncate(read);
                position += read as u64;
                let completed = position >= total_length;

                yield DownloadChunk {
                    position: (position - read as u64) as i64,
                    data: buffer,
                    length: read as i32,
                    completed,
                };

                if completed {
                    break;
                }
            }
        };

        Ok(Response::new(Box::pin(output)))
    }
}

async fn start_one_app(app: AppStartParameter) -> AppStartingResult {
    let process_name = effective_process_name(&app);
    if process_name.is_empty() || app.app_path.trim().is_empty() {
        return AppStartingResult {
            param_id: app.param_id,
            process_id: 0,
            process_name,
            control_result: AppControlResult::FailToStart as i32,
            result: "app_path is required".to_string(),
        };
    }

    if !app.allow_multi_instance
        && let Some(existing) = find_process_ids_by_name(&process_name).into_iter().next()
    {
        return AppStartingResult {
            param_id: app.param_id,
            process_id: existing,
            process_name,
            control_result: AppControlResult::AlreadyRunning as i32,
            result: "process already running".to_string(),
        };
    }

    let mut command = tokio::process::Command::new(&app.app_path);
    if let Some(parent) = Path::new(&app.app_path).parent() {
        command.current_dir(parent);
    }

    if !app.arguments.trim().is_empty() {
        let Some(arguments) = shlex::split(&app.arguments) else {
            return AppStartingResult {
                param_id: app.param_id,
                process_id: 0,
                process_name,
                control_result: AppControlResult::FailToStart as i32,
                result: "failed to parse arguments".to_string(),
            };
        };
        command.args(arguments);
    }

    match command.spawn() {
        Ok(child) => AppStartingResult {
            param_id: app.param_id,
            process_id: child.id().unwrap_or_default() as i32,
            process_name,
            control_result: AppControlResult::Started as i32,
            result: "started".to_string(),
        },
        Err(error) => AppStartingResult {
            param_id: app.param_id,
            process_id: 0,
            process_name,
            control_result: AppControlResult::FailToStart as i32,
            result: error.to_string(),
        },
    }
}

fn elapsed_ms(start: tokio::time::Instant) -> u64 {
    start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn proto_include_from_config(include: crate::telemetry::TelemetryInclude) -> TelemetryInclude {
    match include {
        crate::telemetry::TelemetryInclude::RuntimeBasic => TelemetryInclude::RuntimeBasic,
        crate::telemetry::TelemetryInclude::RuntimeSystem => TelemetryInclude::RuntimeSystem,
        crate::telemetry::TelemetryInclude::RuntimeApps => TelemetryInclude::RuntimeApps,
        crate::telemetry::TelemetryInclude::RuntimeNetwork => TelemetryInclude::RuntimeNetwork,
        crate::telemetry::TelemetryInclude::RuntimeStorage => TelemetryInclude::RuntimeStorage,
    }
}

fn proto_include_from_key(key: &str) -> TelemetryInclude {
    match key {
        "runtime_basic" => TelemetryInclude::RuntimeBasic,
        "runtime_system" => TelemetryInclude::RuntimeSystem,
        "runtime_apps" => TelemetryInclude::RuntimeApps,
        "runtime_network" => TelemetryInclude::RuntimeNetwork,
        "runtime_storage" => TelemetryInclude::RuntimeStorage,
        _ => TelemetryInclude::Unspecified,
    }
}

fn config_include_from_proto(include: i32) -> Result<crate::telemetry::TelemetryInclude, Status> {
    match TelemetryInclude::try_from(include).unwrap_or(TelemetryInclude::Unspecified) {
        TelemetryInclude::RuntimeBasic => Ok(crate::telemetry::TelemetryInclude::RuntimeBasic),
        TelemetryInclude::RuntimeSystem => Ok(crate::telemetry::TelemetryInclude::RuntimeSystem),
        TelemetryInclude::RuntimeApps => Ok(crate::telemetry::TelemetryInclude::RuntimeApps),
        TelemetryInclude::RuntimeNetwork => Ok(crate::telemetry::TelemetryInclude::RuntimeNetwork),
        TelemetryInclude::RuntimeStorage => Ok(crate::telemetry::TelemetryInclude::RuntimeStorage),
        TelemetryInclude::Unspecified => Err(Status::invalid_argument(
            "telemetry include cannot be unspecified",
        )),
    }
}

fn proto_telemetry_profile_from_config(profile: TelemetryProfileConfig) -> TelemetryProfile {
    TelemetryProfile {
        id: profile.id,
        name: profile.name,
        enabled: profile.enabled,
        collection_interval_ms: profile.collection_interval_ms as i64,
        includes: profile
            .includes
            .into_iter()
            .map(|include| proto_include_from_config(include) as i32)
            .collect(),
    }
}

fn config_telemetry_profile_from_proto(
    profile: TelemetryProfile,
) -> Result<TelemetryProfileConfig, Status> {
    let includes = profile
        .includes
        .into_iter()
        .map(config_include_from_proto)
        .collect::<Result<Vec<_>, _>>()?;

    if profile.collection_interval_ms <= 0 {
        return Err(Status::invalid_argument(
            "collection_interval_ms must be positive",
        ));
    }

    Ok(TelemetryProfileConfig {
        id: profile.id,
        name: profile.name,
        enabled: profile.enabled,
        collection_interval_ms: profile.collection_interval_ms as u64,
        includes,
    })
}

fn effective_process_name(app: &AppStartParameter) -> String {
    if !app.process_name.trim().is_empty() {
        return app.process_name.trim().to_string();
    }

    Path::new(&app.app_path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string()
}

async fn finalize_upload(path: Option<PathBuf>, file: Option<File>) -> Result<(), Status> {
    if let Some(mut file) = file {
        file.flush()
            .await
            .context("flush upload file")
            .map_err(status_from_error)?;
        file.sync_all()
            .await
            .context("sync upload file")
            .map_err(status_from_error)?;
    }

    if let Some(path) = path {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mode = if looks_executable(&path).await.unwrap_or(false) {
                0o777
            } else {
                0o666
            };

            fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
                .await
                .with_context(|| format!("set permissions {}", path.display()))
                .map_err(status_from_error)?;
        }
    }

    Ok(())
}

async fn looks_executable(path: &Path) -> Result<bool> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if matches!(extension, "sh" | "py" | "pl" | "run") {
        return Ok(true);
    }

    let mut file = File::open(path)
        .await
        .with_context(|| format!("open file for executable check {}", path.display()))?;
    let mut buffer = [0u8; 4];
    let read = file
        .read(&mut buffer)
        .await
        .with_context(|| format!("read file for executable check {}", path.display()))?;

    Ok((read >= 2 && &buffer[..2] == b"#!")
        || (read >= 4 && buffer == [0x7f, b'E', b'L', b'F'])
        || (read >= 2 && &buffer[..2] == b"MZ"))
}

async fn sha256_file_hex(path: &Path) -> Result<String> {
    let mut file = File::open(path)
        .await
        .with_context(|| format!("open file for sha256 {}", path.display()))?;
    let mut context = digest::Context::new(&digest::SHA256);
    let mut buffer = vec![0u8; 64 * 1024];

    loop {
        let read = file
            .read(&mut buffer)
            .await
            .with_context(|| format!("read file for sha256 {}", path.display()))?;
        if read == 0 {
            break;
        }
        context.update(&buffer[..read]);
    }

    Ok(hex_encode(context.finish().as_ref()))
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

async fn remove_existing_path(path: &Path) -> Result<()> {
    let metadata = fs::metadata(path)
        .await
        .with_context(|| format!("read metadata {}", path.display()))?;

    if metadata.is_dir() {
        fs::remove_dir_all(path)
            .await
            .with_context(|| format!("remove directory {}", path.display()))?;
    } else {
        fs::remove_file(path)
            .await
            .with_context(|| format!("remove file {}", path.display()))?;
    }

    Ok(())
}

fn resolve_managed_file_path(state: &AppState, input: &str) -> Result<PathBuf, Status> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument("file path is required"));
    }

    let root = managed_file_root(state)?;
    platform::context()
        .and_then(|context| {
            context
                .path_resolver
                .resolve_managed_path(&root, Path::new(trimmed))
                .context("resolve managed file path through PAL")
        })
        .map_err(|error| Status::permission_denied(error.to_string()))
}

fn managed_file_root(state: &AppState) -> Result<PathBuf, Status> {
    let mut root = state
        .service_path()
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| Status::internal("service path has no parent directory"))?;
    root.push("managed-files");
    Ok(root)
}

fn ensure_upload_limits(
    state: &AppState,
    path: &Path,
    position: u64,
    chunk_len: u64,
) -> Result<(), Status> {
    let projected_size = position.saturating_add(chunk_len);
    if projected_size > state.max_file_transfer_bytes() {
        return Err(Status::resource_exhausted("upload exceeds file size limit"));
    }

    let disk_info = platform::context()
        .and_then(|context| {
            context
                .disk_space
                .query(path)
                .context("query disk space through PAL")
        })
        .map_err(|error| Status::internal(error.to_string()))?;
    let required_free = state
        .min_file_transfer_free_bytes()
        .saturating_add(chunk_len);
    if disk_info.available_bytes < required_free {
        return Err(Status::resource_exhausted(
            "insufficient free disk space for upload",
        ));
    }

    Ok(())
}

async fn console_telemetry_task(state: Arc<AppState>) {
    let mut ticker = tokio::time::interval(Duration::from_secs(state.interval_seconds()));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;

        let (running, apps) = state.running_and_apps_state().await;
        let network = state.network_statistics();

        let watched = apps
            .items
            .iter()
            .map(|item| {
                let process_name = item
                    .process
                    .as_ref()
                    .map(|process| process.process_monitor_name.as_str())
                    .unwrap_or("<unknown>");
                format!(
                    "{}:running={},proc_count={},cpu={:.2},memory={}",
                    process_name, item.is_running, item.proc_count, item.cpu, item.current_memory
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");

        let network_summary = network
            .interface_statistics
            .iter()
            .map(|item| {
                format!(
                    "{} rx/s={:.0} tx/s={:.0}",
                    item.if_name, item.bytes_received_per_sec, item.bytes_sented_per_sec
                )
            })
            .collect::<Vec<_>>()
            .join(" | ");

        println!(
            "[telemetry] device={} cpu={:.2}% memory={} proc_count={} tcp_connections={} udp_listeners={} watched=[{}] net=[{}]",
            running.device_id,
            running.cpu,
            running.current_memory,
            running.proc_count,
            network.current_connections,
            network.udp_listeners,
            watched,
            network_summary
        );

        let next_seconds = state.interval_seconds().max(1);
        ticker = tokio::time::interval(Duration::from_secs(next_seconds));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    }
}

fn status_from_error(error: anyhow::Error) -> Status {
    Status::internal(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_state() -> AppState {
        AppState::new(
            AppConfig::default(),
            PathBuf::from("/tmp/CC-rDeviceAgent.test.toml"),
            PathBuf::from("/tmp/cc-rdeviceagent"),
        )
        .expect("state")
    }

    #[test]
    fn managed_file_path_allows_relative_paths_under_service_directory() {
        let state = make_state();
        let path = resolve_managed_file_path(&state, "uploads/app.bin").expect("path");

        assert_eq!(
            path,
            PathBuf::from("/tmp/managed-files").join("uploads/app.bin")
        );
    }

    #[test]
    fn managed_file_path_rejects_absolute_and_parent_paths() {
        let state = make_state();

        assert!(resolve_managed_file_path(&state, "/etc/passwd").is_err());
        assert!(resolve_managed_file_path(&state, "../outside").is_err());
        assert!(resolve_managed_file_path(&state, "nested/../../outside").is_err());
    }

    #[test]
    fn upload_limits_reject_oversized_chunks() {
        let mut config = AppConfig::default();
        config.service.file_transfer.max_file_bytes = 4;
        let state = AppState::new(
            config,
            PathBuf::from("/tmp/CC-rDeviceAgent.test.toml"),
            PathBuf::from("/tmp/cc-rdeviceagent"),
        )
        .expect("state");

        let error = ensure_upload_limits(&state, Path::new("/tmp/upload.bin"), 2, 3).unwrap_err();
        assert_eq!(error.code(), tonic::Code::ResourceExhausted);
    }

    #[tokio::test]
    async fn sha256_file_digest_matches_known_value() {
        let path = std::env::temp_dir().join(format!(
            "cc-rdeviceagent-sha256-test-{}",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::write(&path, b"abc").await.unwrap();

        let digest = sha256_file_hex(&path).await.unwrap();

        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = tokio::fs::remove_file(path).await;
    }
}
