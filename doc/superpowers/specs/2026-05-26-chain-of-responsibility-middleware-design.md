# Chain-of-Responsibility gRPC Middleware Design

Date: 2026-05-26
Status: Approved
Scope: Phase 1 (W1.4) — Wire Security Center enforcement across gRPC control plane

## Goal

Implement a tonic middleware layer that enforces the architecture-specified chain of responsibility on every `StationControlService` and `FileTransferService` gRPC call:

```
Request → IdentityExtract → AuthN → AuthZ + Replay → AuditEntry → Handler → AuditExit → Response
```

Currently these handlers do not call `SecurityCenter` at all. This change makes Security Center the mandatory gateway for all northbound gRPC traffic.

## Architecture

```
gRPC Request
    │
    ▼
┌──────────────────────────────────────────────┐
│  SecurityInterceptor (tonic Service impl)     │
│                                               │
│  Pre-chain:                                    │
│  1. IdentityExtractor                          │
│     ├─ mTLS peer cert → parse SAN/CN          │
│     └─ fallback: x-cc-principal metadata      │
│  2. SecurityCenter.authenticate()              │
│  3. ResourceMapper (method → Resource+Action)  │
│  4. SecurityCenter.authorize() (RBAC+replay)   │
│  5. AuditWriter.write_entry() — synchronous    │
│  6. Inject SecurityContext into extensions     │
│                                               │
│  ▼ inner.call(request)                        │
│                                               │
│  Post-chain:                                   │
│  7. AuditWriter.write_exit() — async channel   │
│     └─ Drop guard: writes "failed" on panic    │
└──────────────────────────────────────────────┘
```

## Components

### New file: `crates/agent-core/src/chain.rs`

| Type | Purpose |
|------|---------|
| `SecurityInterceptorLayer` | Tonic `Layer` — constructs `SecurityInterceptor` per connection |
| `SecurityInterceptor` | Tonic `Service` — runs the full pre/post chain |
| `IdentityExtractor` | Parses X.509 peer certificate or `x-cc-principal` header |
| `ResourceMapper` | Maps gRPC method path → `(Resource, Action)` |
| `AuditWriter` | Sync entry writes + async channel for exit writes |
| `SecurityContext` | Injected into `Request::extensions()` for handlers |

### Dependencies

- `x509-parser` — pure-Rust DER certificate parsing (no OpenSSL)

### Identity Extraction

Priority order:
1. **mTLS peer certificate**: Read `tonic::Request::extensions().get::<PeerCertificates>()`, parse first cert as DER, extract CN from subject and dNSName entries from SAN extension. Build `DeviceIdentityBinding`. Tenant inferred from dNSName pattern `*.tenant-<name>.*` or defaults to `"default"`. Role defaults to `Operator`. AuthMethod: `Mtls`.
2. **Metadata header**: Read `x-cc-principal` header, deserialize JSON `{tenant_id, device_id, subject, role}`. AuthMethod: `SessionToken`.
3. **Anonymous**: Neither available → `AuthMethod::Anonymous`. All subsequent chain steps will deny access.

### Resource Mapping

Full method path → (Resource, Action). See table in design discussion. Covers all 16 `StationControl` methods + 2 `FileTransfer` methods.

### AuditWriter

- `write_entry()`: Synchronous `StateStore::append_audit_event()`. Blocks until SQLite confirms write. Failure denies the request with `Internal`.
- `write_exit()`: Sends event into `mpsc::UnboundedSender`. Background task drains channel and batch-writes to SQLite. Full channel → drop + `tracing::error!` + counter increment.
- Entry event is written BEFORE handler runs, so even panics leave an audit trail.
- `AuditGuard`: Drop-based guard on the response future that writes `result = "failed"` if the future is dropped without explicit finalization.

### SecurityContext (injected into Request extensions)

```rust
struct SecurityContext {
    principal: Principal,
    auth_method: AuthMethod,
    request_context: RequestContext,
    audit_entry: AuditEvent,
}
```

Handlers access via `request.extensions().get::<SecurityContext>()`.

### Wiring in app.rs

```rust
let security_center = Arc::new(Mutex::new(
    BasicSecurityCenter::new(RbacPolicy::default(), ReplayGuard::new(Duration::from_secs(300)))
));
let audit_writer = AuditWriter::new(Arc::clone(&state_store));

Server::builder()
    .layer(SecurityInterceptorLayer::new(
        security_center,
        audit_writer,
        station_id,
    ))
    .add_service(StationControlServer::new(service))
    .add_service(FileTransferServer::new(file_service))
    .serve_with_shutdown(...)
```

## Error Handling

| Step failure | gRPC Status | Audit written? |
|-------------|-------------|----------------|
| No identity | `Unauthenticated` | No |
| RBAC deny | `PermissionDenied` | Yes (entry only, result=denied) |
| Replay detected | `AlreadyExists` | Yes (entry only, result=denied) |
| Timestamp skew | `InvalidArgument` | Yes (entry only, result=denied) |
| Audit entry write fails | `Internal` | No (disk full, can't write) |
| Handler panics | `Internal` | Yes (entry + "failed" exit via AuditGuard) |
| Handler error | Per-handler | Yes (entry + exit with handler's status) |

## Handler Changes

Handlers are simplified. They read `SecurityContext` from extensions instead of doing their own auth:

- Remove no-op or missing auth checks
- `ExecuteCommand`: un-stubbed — now wired to `CommandPolicy` + PAL `ProcessManager`
- `FileTransferService`: same pattern — reads `SecurityContext`, no longer trusts unauthenticated streams

## What Stays Unchanged

- **DesktopAgent** — separate loopback server with token auth
- **MQTT command path** — keeps its own `CommandPolicy::validate()` call
- **SecurityCenter trait** — no changes
- **StateStore schema** — no changes
- **Config** — `TlsConfig` unchanged

## Test Plan

### Unit tests (in `chain.rs`)

1. IdentityExtractor: parse DER cert with SAN/CN → `Principal` with `AuthMethod::Mtls`
2. IdentityExtractor: metadata header fallback → `Principal` with `AuthMethod::SessionToken`
3. IdentityExtractor: no cert, no header → `AuthMethod::Anonymous`
4. ResourceMapper: all 18 method paths map to correct `(Resource, Action)` pairs
5. AuditWriter: synchronous entry write succeeds, exit via channel arrives in store
6. AuditWriter: exit dropped when channel full, error logged

### Integration tests (in `chain.rs`)

7. Full chain: authenticated Operator calls `GetSystemState` → success
8. Full chain: Readonly calls `Shutdown` → `PermissionDenied`
9. Full chain: same nonce twice → second call `AlreadyExists`
10. Full chain: after any call, `audit_events` table has both entry and exit rows with linked hashes
11. Full chain: anonymous request → `Unauthenticated`

## Files Changed

| File | Change |
|------|--------|
| `crates/agent-core/src/chain.rs` | **New** — ~500 lines |
| `crates/agent-core/src/lib.rs` | Add `pub mod chain;` |
| `Cargo.toml` (workspace or agent-core) | Add `x509-parser` dependency |
| `src/app.rs` | Wire layer, simplify handlers, un-stub ExecuteCommand (~50 lines changed) |
| `crates/agent-core/Cargo.toml` | Add `x509-parser`, `tokio` (for channel) |
