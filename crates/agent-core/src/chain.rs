use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::SystemTime;

use tonic::Status;

use crate::security::Action;
use crate::security::AuditEvent;
use crate::security::AuthMethod;
use crate::security::BasicSecurityCenter;
use crate::security::Decision;
use crate::security::Principal;
use crate::security::RequestContext;
use crate::security::Resource;
use crate::security::Role;
use crate::security::SecurityCenter;

/// Persistence sink for audit events. Implemented by the host application
/// (wraps StateStore or test doubles).
pub trait AuditSink: Send + Sync {
    fn append_audit_event(&self, event: AuditEvent) -> Result<(), String>;
}

/// Injected into `tonic::Request::extensions()` so handlers can read
/// the authenticated identity and audit trail produced by the middleware.
#[derive(Debug, Clone)]
pub struct SecurityContext {
    pub principal: Principal,
    pub auth_method: AuthMethod,
    pub request_context: RequestContext,
    pub audit_entry: AuditEvent,
}

/// Wrapper around a slice of peer TLS certificates, used as the key for
/// accessing mTLS peer certs from `Request::extensions()`.
///
/// Tonic 0.14 removed `PeerCertificates`; this type serves the same role.
#[derive(Debug, Clone)]
pub struct PeerCerts(pub Vec<tonic::transport::Certificate>);

/// Maps a gRPC method path (e.g. "/cc.grpc.v1.StationControl/Reboot")
/// to a `(Resource, Action)` pair for RBAC decisions.
#[derive(Debug, Clone, Default)]
pub struct ResourceMapper;

impl ResourceMapper {
    pub fn map(&self, path: &str) -> Option<(Resource, Action)> {
        let (svc, method) = path.rsplit_once('/')?;
        match (svc, method) {
            ("/cc.grpc.v1.StationControl", "StartApp")
            | ("/cc.grpc.v1.StationControl", "CloseApp")
            | ("/cc.grpc.v1.StationControl", "RestartApp") => {
                Some((Resource::AppControl, Action::Execute))
            }
            ("/cc.grpc.v1.StationControl", "Reboot")
            | ("/cc.grpc.v1.StationControl", "Shutdown") => {
                Some((Resource::ControlCommand, Action::Execute))
            }
            ("/cc.grpc.v1.StationControl", "ExecuteCommand") => {
                Some((Resource::ControlCommand, Action::Execute))
            }
            ("/cc.grpc.v1.StationControl", "GetSystemState")
            | ("/cc.grpc.v1.StationControl", "GetAllProcessInfo")
            | ("/cc.grpc.v1.StationControl", "GetServerVersion")
            | ("/cc.grpc.v1.StationControl", "GetServicePath")
            | ("/cc.grpc.v1.StationControl", "GetAppLauncherPath")
            | ("/cc.grpc.v1.StationControl", "GetNetworkInterfaces")
            | ("/cc.grpc.v1.StationControl", "GetConnectionInformations")
            | ("/cc.grpc.v1.StationControl", "GetTcpListenerInfos")
            | ("/cc.grpc.v1.StationControl", "GetUdpListenerInfos")
            | ("/cc.grpc.v1.StationControl", "GetCurrentTelemetrySchema")
            | ("/cc.grpc.v1.StationControl", "GetTelemetryProfiles")
            | ("/cc.grpc.v1.StationControl", "CaptureScreen") => {
                Some((Resource::Telemetry, Action::Read))
            }
            ("/cc.grpc.v1.StationControl", "SetWatchingApp") => {
                Some((Resource::Configuration, Action::Write))
            }
            ("/cc.grpc.v1.StationControl", "ReplaceTelemetryProfiles") => {
                Some((Resource::Configuration, Action::Write))
            }
            ("/cc.grpc.v1.StationControl", "GetFileInfo")
            | ("/cc.grpc.v1.StationControl", "RenameFile") => {
                Some((Resource::FileTransfer, Action::Write))
            }
            ("/cc.grpc.v1.FileTransfer", "Upload") => Some((Resource::FileTransfer, Action::Write)),
            ("/cc.grpc.v1.FileTransfer", "Download") => {
                Some((Resource::FileTransfer, Action::Read))
            }
            _ => None,
        }
    }
}

/// Extracts a `Principal` from a gRPC request.
///
/// Priority:
/// 1. mTLS peer certificate — parses X.509 SAN dNSName / CN for device identity.
/// 2. `x-cc-principal` metadata header — JSON {tenant_id, device_id, subject, role}.
/// 3. Falls back to `AuthMethod::Anonymous`.
#[derive(Debug, Clone, Default)]
pub struct IdentityExtractor {
    pub default_tenant: String,
}

impl IdentityExtractor {
    pub fn new(default_tenant: impl Into<String>) -> Self {
        Self {
            default_tenant: default_tenant.into(),
        }
    }

    /// Build a `Principal` and `AuthMethod` from request data.
    ///
    /// `peer_certs` — slice of DER-encoded certificates from
    ///   `request.extensions().get::<tonic::transport::PeerCertificates>()`.
    /// `headers` — HTTP headers from `request.headers()`, checked for the
    ///   `x-cc-principal` fallback header.
    pub fn extract(
        &self,
        peer_certs: Option<&[tonic::transport::Certificate]>,
        headers: &http::HeaderMap,
    ) -> (crate::security::Principal, crate::security::AuthMethod) {
        // Try mTLS certificate first
        if let Some(certs) = peer_certs
            && let Some(principal) = self.try_extract_from_certs(certs)
        {
            return (principal, crate::security::AuthMethod::Mtls);
        }

        // Fall back to metadata header
        if let Some(principal) = self.try_extract_from_header(headers) {
            return (principal, crate::security::AuthMethod::SessionToken);
        }

        // Anonymous — will be denied by RBAC
        let anon = crate::security::Principal::new(
            &self.default_tenant,
            "unknown",
            "anonymous",
            Role::Readonly,
        );
        (anon, AuthMethod::Anonymous)
    }

    fn try_extract_from_certs(
        &self,
        certs: &[tonic::transport::Certificate],
    ) -> Option<crate::security::Principal> {
        let der = certs.first()?.as_ref();
        let (_, parsed) = x509_parser::parse_x509_certificate(der).ok()?;

        let tbs = &parsed.tbs_certificate;

        let cn = tbs
            .subject
            .iter_common_name()
            .next()
            .and_then(|cn| cn.as_str().ok())
            .map(String::from);

        let dns_names: Vec<String> = parsed
            .subject_alternative_name()
            .ok()
            .flatten()
            .map(|san| {
                san.value
                    .general_names
                    .iter()
                    .filter_map(|name| match name {
                        x509_parser::extensions::GeneralName::DNSName(dns) => Some(dns.to_string()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();

        let device_id = dns_names
            .first()
            .cloned()
            .or_else(|| cn.clone())
            .unwrap_or_else(|| "unknown-device".to_string());

        let tenant = self.infer_tenant(&dns_names);

        Some(crate::security::Principal::new(
            tenant,
            device_id.clone(),
            device_id,
            Role::Operator,
        ))
    }

    fn try_extract_from_header(
        &self,
        headers: &http::HeaderMap,
    ) -> Option<crate::security::Principal> {
        let header = headers.get("x-cc-principal")?;
        let value = header.to_str().ok()?;
        let parsed: PrincipalHeader = serde_json::from_str(value).ok()?;

        let role = match parsed.role.as_deref() {
            Some("admin") => Role::Admin,
            Some("readonly") => Role::Readonly,
            _ => Role::Operator,
        };

        Some(crate::security::Principal::new(
            parsed.tenant_id.as_deref().unwrap_or(&self.default_tenant),
            parsed.device_id.as_deref().unwrap_or("unknown"),
            parsed.subject.as_deref().unwrap_or("unknown"),
            role,
        ))
    }

    fn infer_tenant(&self, dns_names: &[String]) -> String {
        for name in dns_names {
            // Pattern: <prefix>.tenant-<tenant>.<suffix>
            if let Some(rest) = name.strip_prefix("tenant-")
                && let Some(tenant) = rest.split('.').next()
            {
                return tenant.to_string();
            }
        }
        self.default_tenant.clone()
    }
}

#[derive(Debug, serde::Deserialize)]
struct PrincipalHeader {
    tenant_id: Option<String>,
    device_id: Option<String>,
    subject: Option<String>,
    role: Option<String>,
}

/// Writes audit events: synchronously for entry events (ensuring at least
/// the entry is recorded before the handler runs), and asynchronously
/// through a fire-and-forget channel for exit events.
#[derive(Clone)]
pub struct AuditWriter {
    sink: Arc<dyn AuditSink>,
    exit_tx: tokio::sync::mpsc::UnboundedSender<AuditEvent>,
}

impl AuditWriter {
    /// Create a new AuditWriter and spawn a background task to drain the
    /// exit channel.
    pub fn new(sink: Arc<dyn AuditSink>) -> Self {
        let (exit_tx, mut exit_rx) = tokio::sync::mpsc::unbounded_channel::<AuditEvent>();
        let bg_sink = Arc::clone(&sink);
        tokio::spawn(async move {
            while let Some(event) = exit_rx.recv().await {
                if let Err(err) = bg_sink.append_audit_event(event) {
                    tracing::error!(
                        target: "security.audit",
                        error = %err,
                        "failed to persist audit exit event"
                    );
                }
            }
        });

        Self { sink, exit_tx }
    }

    /// Synchronously write an audit entry event.
    /// Returns the event (with hash populated) on success.
    pub fn write_entry(&self, mut event: AuditEvent) -> Result<AuditEvent, String> {
        event.prev_hash = String::new();
        event.hash = event.calculate_hash(&event.prev_hash);
        self.sink.append_audit_event(event.clone())?;
        Ok(event)
    }

    /// Fire-and-forget an audit exit event through the background channel.
    /// Drops the event silently if the channel is full or disconnected.
    pub fn write_exit(&self, event: AuditEvent) {
        if self.exit_tx.send(event).is_err() {
            tracing::error!(
                target: "security.audit",
                "audit exit channel closed, event dropped"
            );
        }
    }
}

/// Tonic `Layer` that wraps every service call with the security chain.
#[derive(Clone)]
pub struct SecurityInterceptorLayer {
    security_center: Arc<Mutex<BasicSecurityCenter>>,
    audit_writer: AuditWriter,
    identity_extractor: IdentityExtractor,
    resource_mapper: ResourceMapper,
    station_id: String,
}

impl SecurityInterceptorLayer {
    pub fn new(
        security_center: Arc<Mutex<BasicSecurityCenter>>,
        audit_writer: AuditWriter,
        identity_extractor: IdentityExtractor,
        resource_mapper: ResourceMapper,
        station_id: impl Into<String>,
    ) -> Self {
        Self {
            security_center,
            audit_writer,
            identity_extractor,
            resource_mapper,
            station_id: station_id.into(),
        }
    }
}

impl<S> tower::Layer<S> for SecurityInterceptorLayer {
    type Service = SecurityInterceptor<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityInterceptor {
            inner,
            security_center: Arc::clone(&self.security_center),
            audit_writer: self.audit_writer.clone(),
            identity_extractor: self.identity_extractor.clone(),
            resource_mapper: self.resource_mapper.clone(),
            station_id: self.station_id.clone(),
        }
    }
}

/// Tonic `Service` that runs the full security chain around every request.
#[derive(Clone)]
pub struct SecurityInterceptor<S> {
    inner: S,
    security_center: Arc<Mutex<BasicSecurityCenter>>,
    audit_writer: AuditWriter,
    identity_extractor: IdentityExtractor,
    resource_mapper: ResourceMapper,
    station_id: String,
}

impl<S> tower::Service<http::Request<tonic::body::Body>> for SecurityInterceptor<S>
where
    S: tower::Service<
            http::Request<tonic::body::Body>,
            Response = http::Response<tonic::body::Body>,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = http::Response<tonic::body::Body>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, mut request: http::Request<tonic::body::Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let security_center = Arc::clone(&self.security_center);
        let audit_writer = self.audit_writer.clone();
        let identity_extractor = self.identity_extractor.clone();
        let resource_mapper = self.resource_mapper.clone();
        let _station_id = self.station_id.clone();

        Box::pin(async move {
            let now = SystemTime::now();
            let method_path = request.uri().path().to_string();
            let trace_id = uuid::Uuid::new_v4().to_string();

            // Step 1: Extract identity.
            // Peer certs are stored in request extensions by tonic's TLS acceptor
            // or injected manually via PeerCerts.
            let peer_certs = request
                .extensions()
                .get::<PeerCerts>()
                .map(|certs| certs.0.as_slice());

            // For the metadata header fallback, read directly from HTTP headers.
            let (principal, auth_method) =
                identity_extractor.extract(peer_certs, request.headers());

            // Anonymous requests are denied immediately
            if auth_method == AuthMethod::Anonymous {
                return Err(Status::unauthenticated(
                    "no valid mTLS certificate or x-cc-principal header",
                )
                .into());
            }

            // Step 2: Map resource
            let Some((resource, action)) = resource_mapper.map(&method_path) else {
                return Err(Status::unimplemented(format!(
                    "no security mapping for method {method_path}"
                ))
                .into());
            };

            // Step 3: Build request context
            let nonce = trace_id.clone();
            let request_context = RequestContext::new(
                principal.clone(),
                resource,
                action,
                auth_method,
                now,
                &nonce,
                &trace_id,
            );

            // Step 4: Authorize (RBAC + replay)
            {
                let mut sc = security_center
                    .lock()
                    .map_err(|_| Status::internal("security center lock poisoned"))?;
                match sc.authorize(&request_context, now) {
                    Ok(Decision::Deny) => {
                        let deny_event = AuditEvent::new(
                            trace_id.clone(),
                            now,
                            principal.clone(),
                            action,
                            resource,
                            &method_path,
                            "denied",
                            &trace_id,
                        );
                        let _ = audit_writer.write_entry(deny_event);
                        return Err(Status::permission_denied("rbac deny").into());
                    }
                    Err(err) => {
                        let err_event = AuditEvent::new(
                            trace_id.clone(),
                            now,
                            principal.clone(),
                            action,
                            resource,
                            &method_path,
                            "denied",
                            &trace_id,
                        );
                        let _ = audit_writer.write_entry(err_event);
                        return Err(Status::invalid_argument(format!(
                            "security check failed: {err}"
                        ))
                        .into());
                    }
                    Ok(Decision::Allow) => { /* proceed */ }
                }
            }

            // Step 5: Write audit entry (sync)
            let entry_event = AuditEvent::new(
                trace_id.clone(),
                now,
                principal.clone(),
                action,
                resource,
                &method_path,
                "in_progress",
                &trace_id,
            );
            let entry_event = match audit_writer.write_entry(entry_event) {
                Ok(event) => event,
                Err(err) => {
                    tracing::error!(
                        target = "security.audit",
                        error = %err,
                        "failed to write audit entry, denying request"
                    );
                    return Err(Status::internal("audit entry write failed").into());
                }
            };

            // Step 6: Inject SecurityContext into request extensions so
            // handlers can read the authenticated identity.
            let sec_ctx = SecurityContext {
                principal: principal.clone(),
                auth_method,
                request_context: request_context.clone(),
                audit_entry: entry_event.clone(),
            };
            request.extensions_mut().insert(sec_ctx);

            // Step 7: Call the inner handler
            let response = inner.call(request).await;

            // Step 8: Write audit exit (async fire-and-forget)
            let result = match &response {
                Ok(_) => "success",
                Err(_) => "failed",
            };
            let exit_event = AuditEvent::new(
                trace_id,
                now,
                principal,
                action,
                resource,
                &method_path,
                result,
                &entry_event.event_id,
            );
            audit_writer.write_exit(exit_event);

            response.map_err(Into::into)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{Action, Principal, Resource, Role};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime};

    // ---- ResourceMapper tests ----

    #[test]
    fn mapper_covers_all_station_control_methods() {
        let mapper = ResourceMapper;
        assert_eq!(
            mapper.map("/cc.grpc.v1.StationControl/StartApp"),
            Some((Resource::AppControl, Action::Execute))
        );
        assert_eq!(
            mapper.map("/cc.grpc.v1.StationControl/Reboot"),
            Some((Resource::ControlCommand, Action::Execute))
        );
        assert_eq!(
            mapper.map("/cc.grpc.v1.StationControl/GetSystemState"),
            Some((Resource::Telemetry, Action::Read))
        );
        assert_eq!(
            mapper.map("/cc.grpc.v1.StationControl/SetWatchingApp"),
            Some((Resource::Configuration, Action::Write))
        );
        assert_eq!(
            mapper.map("/cc.grpc.v1.FileTransfer/Upload"),
            Some((Resource::FileTransfer, Action::Write))
        );
        assert_eq!(
            mapper.map("/cc.grpc.v1.FileTransfer/Download"),
            Some((Resource::FileTransfer, Action::Read))
        );
    }

    #[test]
    fn mapper_returns_none_for_unknown_method() {
        let mapper = ResourceMapper;
        assert_eq!(mapper.map("/cc.grpc.v1.Unknown/DoSomething"), None);
    }

    // ---- IdentityExtractor tests ----

    #[test]
    fn extractor_returns_anonymous_with_no_cert_and_no_header() {
        let extractor = IdentityExtractor::new("default-tenant");
        let headers = http::HeaderMap::new();
        let (principal, auth_method) = extractor.extract(None, &headers);
        assert_eq!(auth_method, AuthMethod::Anonymous);
        assert_eq!(principal.tenant_id, "default-tenant");
    }

    #[test]
    fn extractor_parses_metadata_header() {
        let extractor = IdentityExtractor::new("default-tenant");
        let mut headers = http::HeaderMap::new();
        let header_value = serde_json::json!({
            "tenant_id": "tenant-x",
            "device_id": "device-1",
            "subject": "admin-user",
            "role": "admin"
        })
        .to_string();
        headers.insert("x-cc-principal", header_value.parse().unwrap());
        let (principal, auth_method) = extractor.extract(None, &headers);
        assert_eq!(auth_method, AuthMethod::SessionToken);
        assert_eq!(principal.tenant_id, "tenant-x");
        assert_eq!(principal.device_id, "device-1");
        assert_eq!(principal.role, Role::Admin);
    }

    #[test]
    fn extractor_defaults_role_to_operator_for_header_without_role() {
        let extractor = IdentityExtractor::new("default-tenant");
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "x-cc-principal",
            r#"{"tenant_id":"t1","device_id":"d1","subject":"u1"}"#
                .parse()
                .unwrap(),
        );
        let (principal, _) = extractor.extract(None, &headers);
        assert_eq!(principal.role, Role::Operator);
    }

    // ---- AuditWriter tests ----

    struct TestAuditSink {
        events: Mutex<Vec<AuditEvent>>,
    }

    impl AuditSink for TestAuditSink {
        fn append_audit_event(&self, event: AuditEvent) -> Result<(), String> {
            self.events.lock().unwrap().push(event);
            Ok(())
        }
    }

    fn test_principal() -> Principal {
        Principal::new("tenant-a", "device-1", "test-user", Role::Operator)
    }

    fn test_audit_event(id: &str) -> AuditEvent {
        AuditEvent::new(
            id,
            SystemTime::UNIX_EPOCH + Duration::from_secs(1000),
            test_principal(),
            Action::Execute,
            Resource::ControlCommand,
            "test:target",
            "success",
            "trace-1",
        )
    }

    #[tokio::test]
    async fn audit_writer_entry_is_synchronous() {
        let sink = Arc::new(TestAuditSink {
            events: Mutex::new(Vec::new()),
        });
        let writer = AuditWriter::new(Arc::clone(&sink) as Arc<dyn AuditSink>);
        let event = test_audit_event("entry-1");
        let persisted = writer
            .write_entry(event)
            .expect("entry write should succeed");
        assert!(!persisted.hash.is_empty());
        assert_eq!(sink.events.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn audit_writer_exit_is_async() {
        let sink = Arc::new(TestAuditSink {
            events: Mutex::new(Vec::new()),
        });
        let writer = AuditWriter::new(Arc::clone(&sink) as Arc<dyn AuditSink>);
        let event = test_audit_event("exit-1");
        writer.write_exit(event);

        // Give the background task a moment to drain the channel
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(sink.events.lock().unwrap().len(), 1);
    }
}
