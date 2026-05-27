# Chain-of-Responsibility gRPC Middleware Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire Security Center enforcement into every gRPC StationControlService and FileTransferService call via a tonic middleware layer.

**Architecture:** A `tower::Layer`/`tower::Service` pair (`SecurityInterceptorLayer` / `SecurityInterceptor`) that intercepts every gRPC request, runs the pre-chain (identity extract → authn → authz + replay → audit entry), injects a `SecurityContext` into request extensions, calls the inner handler, then runs the post-chain (audit exit via async channel). Identity extraction uses mTLS peer certificate parsing (X.509 SAN/CN) with fallback to a `x-cc-principal` metadata header.

**Tech Stack:** tonic 0.14, tower, hyper, x509-parser, ring (existing), tokio

---

### Task 1: Add dependencies to agent-core

**Files:**
- Modify: `crates/agent-core/Cargo.toml`

- [ ] **Step 1: Add required dependencies**

Replace the content of `crates/agent-core/Cargo.toml`:

```toml
[package]
name = "agent-core"
version.workspace = true
edition.workspace = true
authors.workspace = true

[dependencies]
pal-core = { path = "../pal-core" }
ring = "0.17.14"
serde.workspace = true
serde_json.workspace = true
thiserror.workspace = true
tokio = { workspace = true }
tonic = { workspace = true }
tower = "0.5"
hyper = { version = "1", features = [] }
http = "1"
x509-parser = "0.17"
uuid = { workspace = true }
```

- [ ] **Step 2: Verify the dependency addition compiles**

Run: `cargo check -p agent-core`
Expected: `Finished` (no errors, no warnings)

- [ ] **Step 3: Commit**

```bash
git add crates/agent-core/Cargo.toml
git commit -m "build(agent-core): add tonic, tower, hyper, x509-parser, tokio deps"
```

---

### Task 2: Create SecurityContext extension type

**Files:**
- Create: `crates/agent-core/src/chain.rs`

- [ ] **Step 1: Write the SecurityContext struct and AuditSink trait**

Write `crates/agent-core/src/chain.rs`:

```rust
use std::sync::Arc;
use std::time::SystemTime;

use crate::security::AuditEvent;
use crate::security::Principal;
use crate::security::AuthMethod;
use crate::security::RequestContext;

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
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p agent-core`
Expected: `Finished`

- [ ] **Step 3: Commit**

```bash
git add crates/agent-core/src/chain.rs
git commit -m "feat(chain): add SecurityContext and AuditSink trait"
```

---

### Task 3: Create ResourceMapper

**Files:**
- Modify: `crates/agent-core/src/chain.rs`

- [ ] **Step 1: Write ResourceMapper with all 18 method paths**

Append to `crates/agent-core/src/chain.rs`:

```rust
use crate::security::{Action, Resource};

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
            ("/cc.grpc.v1.FileTransfer", "Upload") => {
                Some((Resource::FileTransfer, Action::Write))
            }
            ("/cc.grpc.v1.FileTransfer", "Download") => {
                Some((Resource::FileTransfer, Action::Read))
            }
            _ => None,
        }
    }
}
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p agent-core`
Expected: `Finished`

- [ ] **Step 3: Commit**

```bash
git add crates/agent-core/src/chain.rs
git commit -m "feat(chain): add ResourceMapper for gRPC method to Resource/Action mapping"
```

---

### Task 4: Create IdentityExtractor

**Files:**
- Modify: `crates/agent-core/src/chain.rs`

- [ ] **Step 1: Write IdentityExtractor**

Append to `crates/agent-core/src/chain.rs`:

```rust
use std::time::SystemTime;

use crate::security::DeviceIdentityBinding;
use crate::security::Role;

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
        if let Some(certs) = peer_certs {
            if let Some(principal) = self.try_extract_from_certs(certs) {
                return (principal, crate::security::AuthMethod::Mtls);
            }
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
                        x509_parser::extensions::GeneralName::DNSName(dns) => {
                            Some(dns.to_string())
                        }
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
            if let Some(rest) = name.strip_prefix("tenant-") {
                if let Some(tenant) = rest.split('.').next() {
                    return tenant.to_string();
                }
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
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p agent-core`
Expected: `Finished`

- [ ] **Step 3: Commit**

```bash
git add crates/agent-core/src/chain.rs
git commit -m "feat(chain): add IdentityExtractor for mTLS cert and metadata header"
```

---

### Task 5: Create AuditWriter

**Files:**
- Modify: `crates/agent-core/src/chain.rs`

- [ ] **Step 1: Write AuditWriter**

Append to `crates/agent-core/src/chain.rs`:

```rust
use std::sync::Arc;

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
            while let Some(mut event) = exit_rx.recv().await {
                // Finalize the hash in the background context before persisting
                // (the entry's hash is already computed before it's sent)
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
    /// Returns an error string on failure.
    pub fn write_entry(&self, mut event: AuditEvent) -> Result<AuditEvent, String> {
        // Compute the hash now so it's embedded before handler runs
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
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p agent-core`
Expected: `Finished`

- [ ] **Step 3: Commit**

```bash
git add crates/agent-core/src/chain.rs
git commit -m "feat(chain): add AuditWriter with sync entry and async exit channel"
```

---

### Task 6: Create SecurityInterceptor and SecurityInterceptorLayer

**Files:**
- Modify: `crates/agent-core/src/chain.rs`

- [ ] **Step 1: Write SecurityInterceptorLayer and SecurityInterceptor**

Append to `crates/agent-core/src/chain.rs`:

```rust
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};

use tonic::Status;

use crate::security::Action;
use crate::security::AuditEvent;
use crate::security::BasicSecurityCenter;
use crate::security::Decision;
use crate::security::RequestContext;
use crate::security::SecurityCenter;

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

impl<S> Service<hyper::Request<hyper::body::Incoming>> for SecurityInterceptor<S>
where
    S: Service<
            hyper::Request<hyper::body::Incoming>,
            Response = tonic::Response<tonic::body::BoxBody>,
        > + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
{
    type Response = tonic::Response<tonic::body::BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, mut request: hyper::Request<hyper::body::Incoming>) -> Self::Future {
        let mut inner = self.inner.clone();
        let security_center = Arc::clone(&self.security_center);
        let audit_writer = self.audit_writer.clone();
        let identity_extractor = self.identity_extractor.clone();
        let resource_mapper = self.resource_mapper.clone();
        let station_id = self.station_id.clone();

        Box::pin(async move {
            let now = SystemTime::now();
            let method_path = request.uri().path().to_string();
            let trace_id = uuid::Uuid::new_v4().to_string();

            // Step 1: Extract identity.
            // Peer certs are stored in request extensions by tonic's TLS acceptor
            // under the tonic::transport::PeerCertificates newtype.
            let peer_certs = request
                .extensions()
                .get::<tonic::transport::PeerCertificates>()
                .map(|certs| certs.0.as_slice());

            // Tonic puts headers in a MetadataMap extension key.
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
                        return Err(
                            Status::invalid_argument(format!("security check failed: {err}"))
                                .into(),
                        );
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
```

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p agent-core`
Expected: `Finished`

- [ ] **Step 3: Commit**

```bash
git add crates/agent-core/src/chain.rs
git commit -m "feat(chain): add SecurityInterceptorLayer and SecurityInterceptor"
```

---

### Task 7: Register module and write unit tests

**Files:**
- Modify: `crates/agent-core/src/lib.rs`
- Modify: `crates/agent-core/src/chain.rs` (append tests)

- [ ] **Step 1: Register the chain module**

Edit `crates/agent-core/src/lib.rs` — add `pub mod chain;`:

```rust
pub mod chain;
pub mod command_policy;
pub mod error;
pub mod security;

pub use error::{AgentError, AgentResult};
```

- [ ] **Step 2: Add unit tests to chain.rs**

Append to `crates/agent-core/src/chain.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::{Action, Principal, Resource, Role};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime};

    // ---- ResourceMapper tests ----

    #[test]
    fn mapper_covers_all_station_control_methods() {
        let mapper = ResourceMapper::default();
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
        let mapper = ResourceMapper::default();
        assert_eq!(
            mapper.map("/cc.grpc.v1.Unknown/DoSomething"),
            None
        );
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
        headers.insert(
            "x-cc-principal",
            header_value.parse().unwrap(),
        );
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

    #[test]
    fn extractor_infers_tenant_from_dns_san_pattern() {
        let extractor = IdentityExtractor::new("default-tenant");
        let dns_names = vec!["device-1.tenant-acme.cc-devices.io".to_string()];
        assert_eq!(extractor.infer_tenant(&dns_names), "default-tenant");
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

    #[test]
    fn audit_writer_entry_is_synchronous() {
        let sink = Arc::new(TestAuditSink {
            events: Mutex::new(Vec::new()),
        });
        let writer = AuditWriter::new(Arc::clone(&sink) as Arc<dyn AuditSink>);
        let event = test_audit_event("entry-1");
        let persisted = writer.write_entry(event).expect("entry write should succeed");
        assert!(!persisted.hash.is_empty());
        assert_eq!(sink.events.lock().unwrap().len(), 1);
    }

    #[test]
    fn audit_writer_exit_is_async() {
        let sink = Arc::new(TestAuditSink {
            events: Mutex::new(Vec::new()),
        });
        let writer = AuditWriter::new(Arc::clone(&sink) as Arc<dyn AuditSink>);
        let event = test_audit_event("exit-1");
        writer.write_exit(event);

        // Give the background task a moment to drain the channel
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(sink.events.lock().unwrap().len(), 1);
    }
}
```

- [ ] **Step 3: Run tests and verify they pass**

Run: `cargo test -p agent-core -- chain::tests`
Expected: all tests pass

- [ ] **Step 4: Run full workspace tests**

Run: `cargo test --workspace --all-targets`
Expected: all tests pass, no regressions

- [ ] **Step 5: Commit**

```bash
git add crates/agent-core/src/lib.rs crates/agent-core/src/chain.rs
git commit -m "test(chain): add unit tests for ResourceMapper, IdentityExtractor, AuditWriter"
```

---

### Task 8: Wire middleware into app.rs and simplify handlers

**Files:**
- Modify: `src/app.rs`

- [ ] **Step 1: Verify agent-core compiles with new deps**

Run: `cargo check -p agent-core`
Expected: `Finished`

- [ ] **Step 2: Wire SecurityInterceptorLayer into the gRPC server**

In `src/app.rs`, locate the server builder section (around line 181-198). Replace:

```rust
    let mut server = Server::builder();
    if control_tls.enabled {
        server = server
            .tls_config(build_grpc_tls_config(&control_tls)?)
            .context("configure gRPC mTLS")?;
    }

    server
        .add_service(StationControlServer::new(StationControlService {
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
```

With:

```rust
    // Build the security middleware chain
    use agent_core::chain::{
        AuditWriter, IdentityExtractor, ResourceMapper, SecurityInterceptorLayer,
    };
    use agent_core::security::{BasicSecurityCenter, RbacPolicy, ReplayGuard};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    struct StoreAuditSink {
        store: Arc<agent_store::StateStore>,
    }
    impl agent_core::chain::AuditSink for StoreAuditSink {
        fn append_audit_event(
            &self,
            event: agent_core::security::AuditEvent,
        ) -> Result<(), String> {
            self.store.append_audit_event(event).map_err(|e| e.to_string())
        }
    }

    // Open StateStore for audit persistence next to the service binary.
    let store_path = state
        .service_path()
        .parent()
        .map(|p| p.join("state.db"))
        .unwrap_or_else(|| std::path::PathBuf::from("state.db"));
    let state_store = Arc::new(
        agent_store::StateStore::open(&store_path)
            .map_err(|e| anyhow::anyhow!("open state store at {}: {e}", store_path.display()))?,
    );
    let audit_sink: Arc<dyn agent_core::chain::AuditSink> =
        Arc::new(StoreAuditSink {
            store: Arc::clone(&state_store),
        });
    let audit_writer = AuditWriter::new(audit_sink);
    let security_center = Arc::new(Mutex::new(BasicSecurityCenter::new(
        RbacPolicy::default(),
        ReplayGuard::new(Duration::from_secs(300)),
    )));
    let identity_extractor =
        IdentityExtractor::new(state.station_id().to_string());
    let resource_mapper = ResourceMapper::default();
    let security_layer = SecurityInterceptorLayer::new(
        Arc::clone(&security_center),
        audit_writer,
        identity_extractor,
        resource_mapper,
        state.station_id().to_string(),
    );

    let mut server = Server::builder();
    if control_tls.enabled {
        server = server
            .tls_config(build_grpc_tls_config(&control_tls)?)
            .context("configure gRPC mTLS")?;
    }

    server
        .layer(security_layer)
        .add_service(StationControlServer::new(StationControlService {
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
```

- [ ] **Step 3: Simplify ExecuteCommand to use CommandPolicy**

Replace the `execute_command` method in `impl StationControl for StationControlService` (around line 526-537):

```rust
    async fn execute_command(
        &self,
        request: Request<ExecuteCommandRequest>,
    ) -> Result<Response<ExecuteCommandResponse>, Status> {
        use agent_core::command_policy::CommandPolicy;
        use agent_core::security::Principal;

        let sec = request
            .extensions()
            .get::<agent_core::chain::SecurityContext>()
            .ok_or_else(|| Status::internal("security context missing"))?;

        let req = request.into_inner();
        let policy = CommandPolicy::default();
        let validated = policy
            .validate(&sec.principal, &req.command_id, &req.params)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        // For now, only restart_process is supported; other commands
        // require new CommandTemplate registration in the policy.
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
```

- [ ] **Step 4: Compile check**

Run: `cargo check`
Expected: `Finished` — may need to fix import paths or type issues

- [ ] **Step 5: Run full test suite**

Run: `cargo test --workspace --all-targets`
Expected: all tests pass

- [ ] **Step 6: Run clippy and fmt**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```
Expected: both pass

- [ ] **Step 7: Commit**

```bash
git add src/app.rs crates/agent-core/Cargo.toml Cargo.lock
git commit -m "feat(chain): wire security middleware into gRPC server, un-stub ExecuteCommand"
```
