# Phase 2: Application Platform Implementation Status

**Version**: 1.0  
**Last Updated**: 2025-05-28  
**Phase Duration**: 8 weeks  
**Target Release**: v1.0  
**Current Status**: ⚠️ NOT STARTED

---

## Executive Summary

Phase 2 focuses on transforming CC-rDeviceAgent into an application platform by implementing:
1. **Southbound IPC channel** for payload applications
2. **Application lifecycle management** (registry, installation, monitoring)
3. **Data routing** between applications and backend
4. **Configuration management** (device/agent/app three-tier model)
5. **OTA Upgrade Engine design** and application-level prototype
6. **Device → Device terminology migration** (unified naming)

**Overall Progress**: 0/9 work packages completed (0%)

---

## Phase 2 Objectives

| Objective | Description | Status |
|-----------|-------------|--------|
| **Application Platform** | Enable payload apps to run on devices using Agent as runtime | ⚠️ Not Started |
| **IPC Foundation** | Southbound gRPC over UDS/Named Pipe with RBAC & audit | ⚠️ Not Started |
| **Lifecycle Management** | App registration, installation, start/stop, monitoring, restart | ⚠️ Not Started |
| **Data Routing** | Bidirectional data flow: app ↔ agent ↔ backend | ⚠️ Not Started |
| **Config Management** | Three-tier config (device/agent/app) with versioning & rollback | ⚠️ Not Started |
| **OTA Design** | Complete Upgrade Engine design + app-level prototype | ⚠️ Not Started |
| **Terminology Migration** | Unified `device` naming across codebase | ⚠️ Not Started |
| **SDK Delivery** | Rust SDK + sample app demonstrating platform capabilities | ⚠️ Not Started |

---

## Work Package Status

### W2.0: Device → Device Terminology Migration (5 days) 【NEW】

**Status**: ⚠️ NOT STARTED  
**Priority**: P0 (Blocking for v1.0)  
**Owner**: TBD

#### Scope

Unified terminology migration to eliminate legacy `device/Device/DEVICE` naming and establish `device/Device/DEVICE` as the standard across:
- Protocol definitions (proto files)
- Rust codebase (symbols, types, modules)
- Configuration files and templates
- Database schema and migrations
- MQTT topics and telemetry
- Documentation and deployment artifacts
- RBAC resource mappings
- Audit chain event types

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **Scan & Inventory** | Full codebase scan for `device/Device/DEVICE` occurrences | ⚠️ Not Started | - |
| **Proto Migration** | `DeviceControl` → `DeviceControl` in `proto/cc.proto` | ⚠️ Not Started | - |
| **Rust Symbol Migration** | `device_id` → `device_id`, `Device*` → `Device*` types | ⚠️ Not Started | Proto migration |
| **Config Migration** | `service.device_id` → `service.device_id` with deprecation warning | ⚠️ Not Started | - |
| **MQTT Migration** | client_id, topics, payload fields use `device_id` | ⚠️ Not Started | - |
| **Database Migration** | `device_*` tables → `device_*` with data migration script | ⚠️ Not Started | - |
| **Batch/Group/Tag** | Domain model renaming to Device semantics | ⚠️ Not Started | Database migration |
| **Deployment Artifacts** | README, Docker, systemd templates updated | ⚠️ Not Started | - |
| **RBAC/Audit Mapping** | gRPC method paths `DeviceControl` → `DeviceControl` | ⚠️ Not Started | Proto migration |
| **Compatibility Strategy** | Backward compatibility plan for old configs/data | ⚠️ Not Started | - |

#### Deliverables

- [ ] Device naming migration PR
- [ ] Config/database migration guide
- [ ] Compatibility documentation
- [ ] Regression test suite for migration

#### Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Breaking changes for existing deployments | High | Provide migration scripts + deprecation warnings |
| Incomplete migration leaves mixed terminology | Medium | Automated scanning + CI checks for old terms |
| Database migration data loss | High | Backup validation + rollback procedure |

---

### W2.1: Southbound IPC Channel (5 days)

**Status**: ⚠️ NOT STARTED  
**Priority**: P0  
**Owner**: TBD

#### Scope

Establish local IPC channel for payload applications to communicate with Agent using:
- gRPC over Unix Domain Socket (Linux/macOS) or Named Pipe (Windows)
- PAL `IpcServer` abstraction for cross-platform support
- Session-based authentication with short-lived tokens
- Permission controls to reject unregistered applications

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **Southbound gRPC Server** | Implement gRPC server using PAL `IpcServer` | ⚠️ Not Started | PAL IpcServer (Phase 0) |
| **Protocol Definition** | Define `AppRegistry`, `AppData`, `AppConfig`, `AppUpdate`, `AppHealth` services | ⚠️ Not Started | - |
| **UDS/Named Pipe Permissions** | File permissions (Linux/macOS) and ACL (Windows) design | ⚠️ Not Started | - |
| **Connection Management** | Session tracking, token generation, expiration, revocation | ⚠️ Not Started | - |
| **Access Control** | Reject unregistered apps, validate session tokens | ⚠️ Not Started | Connection mgmt |
| **Minimal Connectivity Test** | End-to-end test: app connects, authenticates, makes RPC | ⚠️ Not Started | All above |

#### Deliverables

- [ ] Southbound IPC framework
- [ ] Protobuf service definitions
- [ ] Minimal connectivity test suite

#### Dependencies

- ✅ PAL `IpcServer` trait (completed in Phase 0)
- ⚠️ Security Center for token generation (Phase 1)

---

### W2.2: App Registry (5 days)

**Status**: ⚠️ NOT STARTED  
**Priority**: P0  
**Owner**: TBD

#### Scope

Application registration and identity management:
- Registration handshake protocol
- App ID assignment and session token issuance
- Capability declaration and discovery
- Persistent registry in State Store
- Session renewal, revocation, replay protection

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **Registration Flow** | Startup handshake: app → agent registration request | ⚠️ Not Started | IPC channel |
| **Identity Assignment** | Generate App ID + Session Token + bind to device_id | ⚠️ Not Started | Security Center |
| **Capability Declaration** | Apps declare required capabilities (network, storage, etc.) | ⚠️ Not Started | - |
| **Registry Persistence** | Store app manifest in State Store `applications` table | ⚠️ Not Started | State Store schema |
| **Session Management** | Token renewal, expiration, revocation, replay protection | ⚠️ Not Started | Identity assignment |
| **RBAC & Audit Integration** | Register/renew/revoke operations logged to Audit Chain | ⚠️ Not Started | Phase 1 security |

#### Deliverables

- [ ] App Registry module
- [ ] State Store schema for applications
- [ ] Registration API documentation

---

### W2.3: App Lifecycle (8 days)

**Status**: ⚠️ NOT STARTED  
**Priority**: P0  
**Owner**: TBD

#### Scope

Complete application lifecycle management:
- State machine: Registered → Installed → Running → Stopped → Uninstalled
- Installation (extract, verify, configure)
- Start/stop via PAL `ProcessManager`
- Health monitoring and auto-restart with exponential backoff
- Resource isolation via PAL `ResourceLimiter`
- Log collection (stdout/stderr → Observability Hub)

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **Lifecycle State Machine** | Implement state transitions with persistence | ⚠️ Not Started | App Registry |
| **App Installation** | Extract package, verify signature/hash, configure | ⚠️ Not Started | File Transfer (Phase 1) |
| **Start/Stop** | Launch/terminate apps via PAL `ProcessManager` | ⚠️ Not Started | PAL ProcessManager |
| **Health Monitoring** | Periodic health checks, detect crashes | ⚠️ Not Started | App Health API |
| **Auto-Restart** | Exponential backoff restart policy | ⚠️ Not Started | Health monitoring |
| **Resource Isolation** | Apply CPU/memory/disk quotas via PAL `ResourceLimiter` | ⚠️ Not Started | PAL ResourceLimiter |
| **Log Collection** | Capture stdout/stderr, route to Observability Hub | ⚠️ Not Started | Observability Hub |
| **Cross-Platform Support** | Linux primary; Windows/macOS compile + capability degradation | ⚠️ Not Started | PAL CapabilityProfile |

#### Deliverables

- [ ] App Lifecycle module
- [ ] Lifecycle state machine tests
- [ ] Resource isolation tests

#### Dependencies

- ✅ PAL `ProcessManager` (Phase 0)
- ✅ PAL `ResourceLimiter` (Phase 0)
- ⚠️ PAL `CapabilityProfile` for degradation (Phase 0)

---

### W2.4: Data Router (6 days)

**Status**: ⚠️ NOT STARTED  
**Priority**: P0  
**Owner**: TBD

#### Scope

Bidirectional data routing between applications and backend:
- Uplink: app → agent → backend (MQTT)
- Downlink: backend → agent → app (gRPC streaming)
- Topic mapping: `{tenant}/{device_id}/apps/{app_id}/{topic}`
- Namespace isolation (prevent cross-app access)
- Traffic shaping (rate limiting, quotas)
- Offline queue reuse

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **Uplink Routing** | App data → Agent → MQTT backend | ⚠️ Not Started | IPC channel + MQTT |
| **Downlink Routing** | Backend → Agent → App (server-streaming) | ⚠️ Not Started | IPC channel |
| **Topic Mapping** | Default template: `{tenant}/{device_id}/apps/{app_id}/{topic}` | ⚠️ Not Started | - |
| **Namespace Isolation** | Enforce app_id boundaries, reject cross-app access | ⚠️ Not Started | RBAC |
| **Traffic Shaping** | Rate limiting and quota enforcement per app | ⚠️ Not Started | - |
| **Offline Queue** | Reuse existing offline queue for app data | ⚠️ Not Started | Telemetry offline queue |
| **Metrics & Tracing** | Instrument data routing with OpenTelemetry | ⚠️ Not Started | Observability Hub |

#### Deliverables

- [ ] Data Router module
- [ ] Topic mapping configuration
- [ ] Namespace isolation tests

---

### W2.5: Config Manager / Config Watcher (10 days)

**Status**: ⚠️ NOT STARTED  
**Priority**: P0  
**Owner**: TBD

#### Scope

Three-tier configuration management:
- **Device config**: Hardware-specific settings
- **Agent config**: Agent runtime parameters
- **App config**: Per-application configuration

Features:
- Versioning and rollback
- Activation policies (reconnect/restart/next-upgrade)
- Server-streaming watch API for apps
- Default value merging
- Signature verification via Security Center

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **Three-Tier Model** | Implement device/agent/app config layers | ⚠️ Not Started | State Store schema |
| **Activation Policies** | Reconnect/restart/next-upgrade strategies | ⚠️ Not Started | - |
| **Versioning & Rollback** | Config version tracking, rollback to previous version | ⚠️ Not Started | State Store |
| **Config Watch API** | Server-streaming gRPC for app config subscription | ⚠️ Not Started | IPC channel |
| **Default Merging** | Merge user config with defaults | ⚠️ Not Started | - |
| **Signature Verification** | Verify config signatures via Security Center | ⚠️ Not Started | Security Center |
| **Multi-Key Transactions** | Reserved for future, not blocking v1.0 | ⚠️ Not Started | - |

#### Deliverables

- [ ] Config Manager module
- [ ] Config Watcher module
- [ ] Config versioning tests
- [ ] Config watch API tests

---

### W2.6: App Health & Runtime Control (5 days) 【NEW】

**Status**: ⚠️ NOT STARTED  
**Priority**: P1  
**Owner**: TBD

#### Scope

Application health monitoring and runtime control foundation:
- Health reporting API for apps
- Health Evaluator collects and persists health status
- Failure threshold policies trigger restart/rollback/alert
- Reserve hooks for FR-10 runtime control (reload, pause/resume, parameter injection)

**Note**: Phase 2 delivers minimal health loop only; full App Control Service deferred to Phase 3.

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **Health Reporting API** | Apps report health status via gRPC | ⚠️ Not Started | IPC channel |
| **Health Evaluator** | Collect health data, persist to State Store | ⚠️ Not Started | State Store schema |
| **Failure Policies** | Consecutive failure thresholds → restart/rollback/alert | ⚠️ Not Started | Health Evaluator |
| **Runtime Control Hooks** | Reserve RBAC & audit hooks for reload/pause/resume | ⚠️ Not Started | RBAC framework |
| **Minimal Health Loop** | End-to-end: app reports unhealthy → agent restarts app | ⚠️ Not Started | All above |

#### Deliverables

- [ ] App Health API
- [ ] Health Evaluator module
- [ ] Runtime control design document (hooks only)

---

### W2.7: Upgrade Engine Design & App-Level Prototype (10 days) 【NEW/ADVANCED】

**Status**: ⚠️ NOT STARTED  
**Priority**: P0 (Design), P1 (Prototype)  
**Owner**: TBD

#### Scope

**Design Phase** (advanced from Phase 3 to reduce risk):
- Complete OTA state machine design
- Upgrade package format specification
- PAL `Bootloader` trait design (RAUC/UEFI BCD/app-level fallback)
- State persistence schema
- `UpgradeStrategy` trait for application/system/config upgrades

**Prototype Phase**:
- Application-level upgrade implementation
- Manifest parsing, signature/hash verification
- Anti-rollback protection
- Staging, backup, activation
- Health check and rollback on failure

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **OTA State Machine Design** | Received → Validated → Downloading → Verifying → PreCheck → Staging → ReadyToActivate → Activating → PostCheck → Committed/RolledBack/Failed | ⚠️ Not Started | - |
| **Package Format Spec** | `tar.zst + manifest.json + Ed25519 signature` | ⚠️ Not Started | - |
| **PAL Bootloader Design** | Trait design for RAUC/UEFI BCD/app-level fallback | ⚠️ Not Started | - |
| **State Persistence Schema** | State Store schema for upgrade tracking | ⚠️ Not Started | State Store |
| **UpgradeStrategy Trait** | Abstraction for application/system/config strategies | ⚠️ Not Started | - |
| **App Upgrade Prototype** | Manifest parse, verify, stage, backup, activate, health check, rollback | ⚠️ Not Started | File Transfer + Security |
| **Design Review** | Architecture + Security + Platform team review | ⚠️ Not Started | Design complete |

#### Deliverables

- [ ] OTA Design Document
- [ ] Application-level upgrade prototype
- [ ] Design review approval

#### Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| OTA complexity underestimated | High | Early design phase in Phase 2 provides buffer |
| Platform-specific bootloader integration | High | PAL abstraction + fallback strategies |
| Upgrade failure causes device brick | Critical | A/B partitioning + health checks + rollback |

---

### W2.8: SDK & Sample Application (4 days) 【NEW】

**Status**: ⚠️ NOT STARTED  
**Priority**: P0  
**Owner**: TBD

#### Scope

Deliver SDK for payload application developers:
- **Rust SDK**: Registration, data reporting, config subscription, health reporting, update queries
- **Sample app**: Demonstrates full platform capabilities
- **Python SDK**: Interface design or alpha prototype (not blocking v1.0)
- **Documentation**: SDK API reference and usage guide

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **Rust SDK Core** | Registration, data, config, health, update APIs | ⚠️ Not Started | IPC protocol |
| **Sample Application** | Demo app covering main SDK features | ⚠️ Not Started | Rust SDK |
| **Python SDK Design** | Interface design or alpha prototype | ⚠️ Not Started | Rust SDK |
| **API Documentation** | SDK reference docs and usage examples | ⚠️ Not Started | SDK complete |

#### Deliverables

- [ ] Rust SDK (crate published)
- [ ] Sample payload application
- [ ] SDK usage guide

---

### W2.9: End-to-End Integration & Testing (4 days)

**Status**: ⚠️ NOT STARTED  
**Priority**: P0  
**Owner**: TBD

#### Scope

Comprehensive testing of Phase 2 deliverables:
- Full application lifecycle E2E test (register → install → run → upgrade → uninstall)
- Device → Device migration regression tests
- Performance baseline (concurrent apps, IPC throughput, memory, disk)
- Security regression (unregistered apps, forged tokens, cross-app access, invalid upgrade packages)

#### Tasks

| Task | Description | Status | Blocker |
|------|-------------|--------|---------|
| **Lifecycle E2E Test** | App registration → upgrade complete flow | ⚠️ Not Started | All W2.1-W2.8 |
| **Migration Regression** | Verify Device → Device migration works | ⚠️ Not Started | W2.0 |
| **Performance Baseline** | Measure app concurrency, IPC throughput, memory, disk | ⚠️ Not Started | All modules |
| **Security Regression** | Test unregistered app, forged token, cross-app access, bad upgrade | ⚠️ Not Started | All modules |

#### Deliverables

- [ ] E2E test report
- [ ] Performance baseline report
- [ ] Security test report

---

## Key Milestones

| Week | Milestone | Status |
|------|-----------|--------|
| **W1** | Device → Device migration complete, config/DB compatibility verified | ⚠️ Not Started |
| **W2** | Southbound IPC + App Registry complete | ⚠️ Not Started |
| **W4** | App Lifecycle + Data Router complete | ⚠️ Not Started |
| **W5** | Config Manager / Config Watcher complete | ⚠️ Not Started |
| **W6** | App Health minimal loop complete, OTA design review passed | ⚠️ Not Started |
| **W7** | OTA app-level prototype + Rust SDK sample complete | ⚠️ Not Started |
| **W8** | **v1.0 Release**: Application platform GA + OTA design ready + Device naming unified | ⚠️ Not Started |

---

## Acceptance Criteria

### Functional Requirements

- [ ] All business terminology unified to `device` (no `device/Device/DEVICE` except in historical ADR/migration docs)
- [ ] Old `service.device_id` config migrates to `service.device_id` without data loss
- [ ] Old database `device_*` / `device_id` data migrates to `device_*` / `device_id` without data loss
- [ ] `DeviceControl` gRPC paths correctly mapped in RBAC & Audit Chain
- [ ] Application registration/start/stop/upgrade complete loop functional
- [ ] Bidirectional data channel operational (app ↔ agent ↔ backend)
- [ ] Three-tier config model (device/agent/app) operational
- [ ] App health reporting and failure recovery minimal loop operational
- [ ] OTA design document approved by architecture review
- [ ] Application-level upgrade prototype demonstrable
- [ ] E2E test passes: app register → install → run → upgrade → uninstall

### Non-Functional Requirements

| Metric | Target | Status |
|--------|--------|--------|
| **Concurrent Apps** | ≥ 10 apps running simultaneously | ⚠️ Not Measured |
| **IPC Throughput** | ≥ 10 MB/s per app | ⚠️ Not Measured |
| **Agent Memory** | ≤ 100 MB with 10 apps | ⚠️ Not Measured |
| **Offline Queue** | ≥ 10,000 messages per app | ⚠️ Not Measured |
| **Config Propagation** | ≤ 5s from backend to app | ⚠️ Not Measured |
| **App Restart Time** | ≤ 10s (including health check) | ⚠️ Not Measured |

### Security Requirements

- [ ] Unregistered apps cannot access IPC channel
- [ ] Forged session tokens rejected
- [ ] Cross-app data access blocked (namespace isolation)
- [ ] Invalid upgrade package signatures rejected
- [ ] All app lifecycle operations logged to Audit Chain
- [ ] Config signature verification enforced

---

## Dependencies

### From Phase 0 (Foundation)

- ✅ PAL `IpcServer` trait
- ✅ PAL `ProcessManager` trait
- ✅ PAL `ResourceLimiter` trait
- ✅ State Store with SQLite
- ✅ Observability Hub (OpenTelemetry)
- ⚠️ PAL `CapabilityProfile` for degradation

### From Phase 1 (Security)

- ✅ Security Center (token generation, signature verification)
- ✅ RBAC framework
- ✅ Audit Chain
- ✅ File Transfer Service (for app package delivery)
- ✅ mTLS northbound channel

### External Dependencies

- Rust toolchain ≥ 1.75
- Protobuf compiler
- SQLite ≥ 3.35
- OpenSSL/rustls for TLS

---

## Risks & Mitigation

| Risk | Probability | Impact | Mitigation | Owner |
|------|-------------|--------|------------|-------|
| **Device → Device migration breaks existing deployments** | Medium | High | Provide migration scripts, deprecation warnings, rollback procedure | TBD |
| **IPC performance bottleneck** | Medium | Medium | Early performance testing, optimize serialization, consider shared memory | TBD |
| **OTA design complexity underestimated** | High | High | Advanced design phase in Phase 2 (instead of Phase 3) provides buffer | TBD |
| **Cross-platform PAL gaps** | Medium | Medium | Linux-first delivery, Windows/macOS capability degradation | TBD |
| **App lifecycle state machine edge cases** | Medium | Medium | Comprehensive state transition tests, fault injection | TBD |
| **Config versioning conflicts** | Low | Medium | Clear versioning strategy, conflict resolution policy | TBD |
| **SDK API instability** | Medium | Low | Semantic versioning, deprecation policy, early feedback from sample app | TBD |

---

## Team & Resources

### Recommended Team Structure

| Role | Responsibility | FTE |
|------|----------------|-----|
| **Tech Lead** | Architecture, design review, risk management | 1.0 |
| **Backend Engineer** | IPC, Data Router, Config Manager | 1.5 |
| **Platform Engineer** | App Lifecycle, PAL integration, cross-platform | 1.5 |
| **Security Engineer** | RBAC integration, audit, signature verification | 0.5 |
| **OTA Specialist** | Upgrade Engine design, prototype | 1.0 |
| **SDK Engineer** | Rust SDK, sample app, documentation | 1.0 |
| **QA Engineer** | E2E testing, performance testing, security testing | 1.0 |

**Total**: ~7.5 FTE for 8 weeks

---

## Next Actions

### Immediate (Week 1)

1. ✅ **Desktop Agent Removal Complete** - Embedded Desktop Agent has been removed from the codebase (ADR-0007)
2. **Assign Phase 2 Tech Lead** and form team
3. **Kickoff meeting**: Review Phase 2 scope, dependencies, risks
4. **Start W2.0**: Begin Device → Device terminology scan and migration planning
5. **Finalize OTA design scope**: Confirm which bootloader integrations are in scope for v1.0

### Short-term (Week 2-3)

1. **Complete W2.0**: Device → Device migration PR merged
2. **Start W2.1**: Southbound IPC channel implementation
3. **Start W2.2**: App Registry design and implementation
4. **OTA design kickoff**: Begin state machine and package format design

### Medium-term (Week 4-6)

1. **Complete W2.1-W2.3**: IPC, Registry, Lifecycle operational
2. **Start W2.4-W2.5**: Data Router and Config Manager
3. **OTA design review**: Architecture + Security + Platform team approval
4. **SDK development**: Begin Rust SDK implementation

---

## References

- [Action Plan v2.0](./action_plan-v2.0-zh.md) - Phase 2 detailed work breakdown
- [Architecture Design](./architecture-zh.md) - System architecture and component design
- [Requirements v1.0](./requirements_v1.0-zh.md) - Functional requirements FR-1 through FR-10
- [Phase 1 Security Status](./phase1-security-implementation-status.md) - Security foundation dependencies

---

## Change Log

| Date | Version | Changes | Author |
|------|---------|---------|--------|
| 2025-05-28 | 1.0 | Initial Phase 2 status document | AI Assistant |

---

**Document Status**: 📋 Planning  
**Next Review**: Upon Phase 2 kickoff
