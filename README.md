# CC-rDeviceAgent

[![CI](https://github.com/cokkiy/CC-rDeviceAgent/actions/workflows/ci.yml/badge.svg)](https://github.com/cokkiy/CC-rDeviceAgent/actions/workflows/ci.yml)
[![Create Release](https://github.com/cokkiy/CC-rDeviceAgent/actions/workflows/create-release.yml/badge.svg)](https://github.com/cokkiy/CC-rDeviceAgent/actions/workflows/create-release.yml)
[![Publish](https://github.com/cokkiy/CC-rDeviceAgent/actions/workflows/release.yml/badge.svg)](https://github.com/cokkiy/CC-rDeviceAgent/actions/workflows/release.yml)
[![Latest Release](https://img.shields.io/github/v/release/cokkiy/CC-rDeviceAgent?include_prereleases&sort=semver)](https://github.com/cokkiy/CC-rDeviceAgent/releases)
[![Crates.io](https://img.shields.io/crates/v/cc-rdeviceagent-app-sdk.svg?label=app-sdk)](https://crates.io/crates/cc-rdeviceagent-app-sdk)
[![GHCR](https://img.shields.io/badge/GHCR-cc--rdeviceagent-blue?logo=github)](https://github.com/cokkiy/CC-rDeviceAgent/pkgs/container/cc-rdeviceagent)
[![Rust 2024](https://img.shields.io/badge/rust-2024-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/github/license/cokkiy/CC-rDeviceAgent)](LICENSE)

Rust device-side agent for managed workstations, edge devices, and IoT devices.

CC-rDeviceAgent adopts a **dual-sided, three-layer** architecture — facing the management backend northbound and payload applications southbound. The three layers are: Protocol Layer, Core Services Layer, and Platform Abstraction Layer (PAL). The goal is a **lightweight, secure, cross-platform** device agent that provides both device management capabilities (L1) and application platform capabilities (L2).

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                     CC-rDeviceAgent (Process)                           │
│                                                                         │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  Protocol Layer                                                 │    │
│  │  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐          │    │
│  │  │ gRPC     │ │ MQTT     │ │ OTLP     │ │ gRPC     │          │    │
│  │  │ North    │ │ Client   │ │ Receiver │ │ South    │          │    │
│  │  │ (mTLS)   │ │          │ │          │ │ (UDS)    │          │    │
│  │  └──────────┘ └──────────┘ └──────────┘ └──────────┘          │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  Core Services                                                  │    │
│  │                                                                 │    │
│  │  Device Management:                                             │    │
│  │  Control Service · File Transfer · Upgrade Engine ·            │    │
│  │  Config Manager · Telemetry Pipeline                            │    │
│  │                                                                 │    │
│  │  Application Platform:                                          │    │
│  │  App Registry · App Lifecycle · Data Router ·                  │    │
│  │  Config Watcher · Update Notifier                               │    │
│  │                                                                 │    │
│  │  Cross-cutting:                                                 │    │
│  │  Security Center · Audit Chain · Observability Hub ·           │    │
│  │  Scheduler/Quota · State Store (SQLite)                        │    │
│  └────────────────────────────────────────────────────────────────┘    │
│                                                                         │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │  Platform Abstraction Layer (PAL)                              │    │
│  │                                                                 │    │
│  │  ProcessMgr · FileSystem · Network · KeyStore · Bootloader ·   │    │
│  │  Sandbox · SensorReader · PowerMgr · ServiceMgr · TimeSource   │    │
│  │                                                                 │    │
│  │  Implementations: Linux · Windows · macOS · Embedded · Mock    │    │
│  └────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
                                        │
                                        ▼
                         OS / Hardware / External Systems
```

See [`doc/architecture-zh.md`](doc/architecture-zh.md) for detailed architecture design.

---

## Current Status (1.0-beta)

### What the service does

The service has moved past the v0.4 device-control baseline and now tracks the
Phase 2 state in [`doc/action_plan-v2.0-zh.md`](doc/action_plan-v2.0-zh.md):
the application platform and OTA design prototype are wired, but the release is
not yet v1.0 GA.

The northbound service exposes these protobuf services:

- **`DeviceControl`**
  - start, stop, and restart processes
  - reboot or shut down the host
  - return system state, process lists, network interfaces, TCP/UDP listeners, and version info
  - reject raw shell execution by default; control operations go through command policy
  - update watched process names and the in-memory state gathering interval
  - return the telemetry schema and replace MQTT telemetry profiles at runtime
- **`FileTransfer`**
  - upload files as streamed chunks
  - download files as streamed chunks
  - enforce managed path resolution and file size / free-space checks

The southbound application platform exposes **`AppPlatform`** over local IPC:

- register and unregister payload applications
- issue short-lived session tokens and validate app sessions
- accept heartbeats and health reports
- publish app data upstream and subscribe to downlink data
- read and watch device / agent / app scoped configuration

The service listens for northbound control on `control.listen_addr`, which is
`0.0.0.0:50051` in the sample config. When `app_platform.enabled = true`, the
southbound AppPlatform server listens on the configured Unix-domain socket on
Linux/macOS; Windows Named Pipe support is still a stub in this beta.

### Implemented beta capabilities

The 1.0-beta line includes the following Phase 0-2 work:

- PAL workspace split with `pal-core`, Linux / Windows / macOS skeletons,
  fallback implementations, mock adapters, and capability probing
- SQLite State Store with WAL, migrations, app/session/config/audit/upgrade state
  tables, backup/restore, and persisted capability cache
- Security Center primitives: mTLS configuration, X.509 identity extraction,
  RBAC model, replay guard, command whitelist, Ed25519 verification, HKDF helpers,
  and audit hash chain persistence
- App Registry and AppPlatform session lifecycle with registration, heartbeat,
  health report, data publish, config read/watch, and unregister flows
- App Lifecycle prototype with state machine, executable path validation, async
  lifecycle command channel, status query, and list support
- Data Router prototype for app uplink topics and in-process downlink registry
- Config Manager with device / agent / app scopes, versioning, set/get/delete,
  snapshots, watcher support, and tombstone persistence
- Upgrade Engine application-level prototype with OTA state machine, manifest
  model, strategy trait, SHA-256 verification, optional Ed25519 verification,
  state persistence, activation, rollback, and post-check flow
- Rust payload SDK in `crates/app-sdk` and the `examples/payload-hello` example

### MQTT behavior

When `mqtt.enabled = true`, the service can:

- publish device status to `cc/<device_id>/status`
- publish telemetry bundles to `cc/<device_id>/telemetry`
- subscribe to `cc/<device_id>/command`
- publish command acknowledgements to `cc/<device_id>/command/ack`
- publish app uplink data to app-scoped topics

The currently implemented MQTT command handler supports `restart_process` with a
`process_name` parameter. The command is validated through the command policy
path; raw shell execution remains disabled.

Telemetry profiles are validated and normalized from configuration or replaced at
runtime through gRPC. The supported telemetry include keys are:

- `runtime_basic`
- `runtime_system`
- `runtime_apps`
- `runtime_network`
- `runtime_storage`

If no telemetry profiles are configured, the service synthesizes a default profile
that includes all of the sections above and uses the service state interval.

### Beta gaps before v1.0 GA

| Gap                                                             | Severity | Target closeout |
| --------------------------------------------------------------- | -------- | --------------- |
| App lifecycle still uses direct process spawning, not full PAL   | 🔴 High   | Phase 2         |
| AppPlatform RBAC / Audit Chain mapping is not complete          | 🔴 High   | Phase 2         |
| Complete running-agent E2E test with payload app and MQTT mock   | 🔴 High   | Phase 2         |
| Performance baseline is not measured                            | 🔴 High   | Phase 2         |
| Package unpacking, manifest parsing, and install config missing  | 🟡 Medium | Phase 2         |
| Resource isolation / quotas via PAL ResourceLimiter incomplete   | 🟡 Medium | Phase 2         |
| OTA package extraction, health checks, and anti-rollback missing | 🟡 Medium | Phase 2-3       |
| Certificate hot reload, sandboxing, and three-platform security PAL remain partial | 🟡 Medium | Phase 1-2 |

---

## Development Roadmap

See [`doc/action_plan-v2.0-zh.md`](doc/action_plan-v2.0-zh.md) for the full action plan.

```
Phase -1 (v0.4)       Architecture gap analysis & fixes   Complete
Phase 0  (v0.5)       Foundation + PAL contracts          Complete / partial platform adapters
Phase 1  (v0.8)       Security hardening                  Substantially implemented / beta gaps remain
Phase 2  (1.0-beta)   Application platform + OTA design   Current beta track
Phase 2  (v1.0 GA)    App platform GA closeout            E2E, performance, PAL/RBAC/Audit closeout
Phase 3  (v1.5)  Full OTA upgrade                    10 weeks
Phase 4  (v2.0)  Platformization                     10 weeks
Phase 5  (v2.1)  Production readiness                 6 weeks
```

### Key milestones

| Phase | Version    | Focus                     | Status / key deliverables                                      |
| ----- | ---------- | ------------------------- | -------------------------------------------------------------- |
| -1    | v0.4       | Architecture gap analysis | Complete: migration design, CI pipeline, clean baseline        |
| 0     | v0.5       | Foundation + PAL          | Complete core: PAL traits, State Store, CapabilityProfile      |
| 1     | v0.8       | Security                  | Beta: mTLS, RBAC, Audit Chain, command policy; sandbox gaps    |
| 2     | 1.0-beta   | Application platform      | Current: IPC, App Registry, Config Manager, OTA app prototype  |
| 2     | v1.0 GA    | Application platform GA   | Pending: E2E, performance, PAL lifecycle, RBAC/Audit closeout  |
| 3     | v1.5       | OTA upgrade               | Planned: A/B slot, Agent self-update, fault injection tests    |
| 4     | v2.0       | Platformization           | Planned: multi-tenant, canary release, extension points        |
| 5     | v2.1       | Production readiness      | Planned: SLA validation, security audit, documentation         |

---

## Platform Abstraction Layer (PAL)

The PAL is the architectural foundation for cross-platform support. It abstracts all
platform-specific operations behind Rust traits, ensuring business code is completely
platform-independent (no `#[cfg(target_os)]` in business logic).

### Key traits

| Trait             | Linux                 | Windows                    | macOS                |
| ----------------- | --------------------- | -------------------------- | -------------------- |
| ProcessManager    | fork/exec + cgroup v2 | CreateProcess + Job Object | posix_spawn + rlimit |
| ServiceManager    | systemd D-Bus         | SCM API                    | launchd              |
| KeyStore          | TPM 2.0 / pkcs11      | DPAPI / CNG                | Keychain / SEP       |
| BootloaderAdapter | RAUC / U-Boot         | UEFI BCD                   | N/A (limited)        |
| SandboxRunner     | seccomp + namespace   | Job Object + AppContainer  | sandbox-exec         |
| FileSystem        | libc + statvfs        | Win32 File API             | libc + APFS          |
| NetworkProbe      | netlink               | IP Helper API              | SystemConfig         |
| IpcServer         | Unix Socket           | Named Pipe                 | Unix Socket          |
| SystemLogger      | journald              | EventLog                   | unified log          |

### Capability detection

On startup, agent probes platform capabilities and generates a `CapabilityProfile`:

```rust
CapabilityProfile {
  has_tpm: bool,
  has_ab_partition: bool,
  has_cgroup_v2: bool,
  has_secure_boot: bool,
  storage_writable_mb: u64,
  // ...
}
```

Business logic uses this profile to decide which implementation to use at runtime,
with automatic fallback to degraded modes when advanced capabilities are unavailable.

See [`doc/PAL-arch-dd-zh.md`](doc/PAL-arch-dd-zh.md) for the full PAL design.

---

## OTA Upgrade Engine

The OTA upgrade engine implements a 14-state state machine with A/B slot switching,
designed for **zero-brick** reliability.

### State machine overview

```
Idle → Received → Validated → Downloading → Verifying → PreCheck →
Staging → ReadyToActivate → Activating → PostCheck → Committed
                                          ↓
                                    RollingBack → RolledBack → Failed
```

### Key design principles

- **Atomicity**: Upgrade either fully succeeds or fully rolls back
- **Crash recovery**: State persisted to SQLite with WAL mode; resume from breakpoint on restart
- **Health gates**: Each critical state has health checks as gates; auto rollback on failure
- **Trial mode**: Bootloader `boot_count` mechanism; auto-fallback after N failed boots
- **A/B slots**: New version written to inactive slot; bootloader switches on activation
- **Signature verification**: Ed25519 signatures with anti-rollback version protection

### Upgrade types

| Type        | Scope                      | Reboot required       | Rollback mechanism       |
| ----------- | -------------------------- | --------------------- | ------------------------ |
| System      | OS image / kernel / rootfs | Yes (slot switch)     | Bootloader fallback      |
| Agent       | Agent binary self-update   | Yes (process replace) | Updater binary fallback  |
| Application | Business app binaries      | Usually no            | Backup directory restore |
| Config      | Configuration files        | No (hot reload)       | Version history rollback |

See [`doc/OTA-statemachine-detail-design-zh.md`](doc/OTA-statemachine-detail-design-zh.md) for the complete state machine design.

---

## Configuration

By default, the service loads `CC-rDeviceAgent.toml` from the executable directory.
If `service.device_id` is blank, the service resolves it to `<hostname>-<uuid>`.

```toml
[service]
service_name = "CC-rDeviceAgent"
device_id = "device-01"
state_interval_seconds = 5
watched_processes = []
udp_display_target = "127.0.0.1:9008"
launcher_proxy_path = ""

[control]
listen_addr = "0.0.0.0:50051"

[mqtt]
enabled = true
broker_host = "localhost"
broker_port = 1883
telemetry_enabled = true
status_enabled = true
```

The sample config continues with `mqtt.telemetry_profiles`, which define
profile IDs, names, collection intervals, and include sets.

Three-layer configuration model (planned):

```
Device-level (not app-modifiable) → Agent-level → App-level (per app)
```

---

## Binaries and run modes

The main binary is `cc-rdeviceagent`.

Supported CLI arguments:

- `foreground` or `--foreground`
- `daemon` or `--daemon`
- `--config <path>`
- `--console-telemetry` or `--telemetry-console`
- `--version` or `-V`

Platform behavior:

- Windows auto mode first attempts to run as an SCM service
- Unix auto mode daemonizes unless `foreground` is requested
- shutdown is handled through Ctrl+C and OS service signals

Build the workspace:

```bash
cargo build --release
```

Run the service in the foreground:

```bash
./target/release/cc-rdeviceagent foreground --config ./CC-rDeviceAgent.toml
```

For local debugging, you can mirror runtime telemetry to stdout:

```bash
./target/release/cc-rdeviceagent foreground --config ./CC-rDeviceAgent.toml --console-telemetry
```

---

## Repository scope

The crate also contains library modules for scripts, tags, groups, batch execution,
alerts, and plugin abstractions. Those modules are present in the source tree, but
the current service entry point is centered on gRPC device control,
file transfer, telemetry collection, and MQTT publishing.

### Crate structure (planned)

```
agent-core/         Business core (Control, FileXfer, Upgrade, Config, Telemetry)
agent-protocols/    North/South protocol definitions (Protobuf, OTLP)
pal-core/           PAL trait contracts + assembly framework
pal-linux/          Linux adapter implementation
pal-windows/        Windows adapter implementation
pal-macos/          macOS adapter implementation
pal-fallback/       Cross-platform fallback implementations
pal-mock/           Testing mock implementations
agent-telemetry/    Telemetry pipeline (Collector → Processor → Exporter)
agent-store/        State Store (SQLite + WAL)
agent-cli/          CLI entry point
```

---

## Packaging

- Linux install artifacts: `packaging/linux`
- Windows install artifacts: `packaging/windows`
- Docker IoT simulation image: `packaging/docker/Dockerfile.iot-sim`

The install scripts:

- copy the service binary
- install `CC-rDeviceAgent.toml`
- register the service with `systemd` or SCM

---

## IoT simulation container

The simulator launcher is kept in the management-side `CC` repository because it also
manages broker and log paths. With sibling checkouts, run it from `CC`:

```bash
cd ../CC
./scripts/start-iot-sim.sh 10
```

The launcher builds a minimal runtime image for `cc-rdeviceagent`, starts a Mosquitto
broker, and launches headless devices such as `iot-001`, `iot-002`, and `iot-003`.

Useful commands:

```bash
./scripts/start-iot-sim.sh --status
./scripts/start-iot-sim.sh --stop
./scripts/start-iot-sim.sh 10 --dry-run
```

---

## Smoke test

```bash
./scripts/test-smoke.sh
```

---

## Design Documents

| Document                                                                               | Description                                          |
| -------------------------------------------------------------------------------------- | ---------------------------------------------------- |
| [`doc/architecture-zh.md`](doc/architecture-zh.md)                                     | System architecture design (dual-sided, three-layer) |
| [`doc/PAL-arch-dd-zh.md`](doc/PAL-arch-dd-zh.md)                                       | Platform Abstraction Layer detailed design           |
| [`doc/OTA-statemachine-detail-design-zh.md`](doc/OTA-statemachine-detail-design-zh.md) | OTA upgrade state machine detailed design            |
| [`doc/action_plan-v2.0-zh.md`](doc/action_plan-v2.0-zh.md)                             | Action plan v2.0 (revised, with Phase -1)            |
| [`doc/action_plan-zh.md`](doc/action_plan-zh.md)                                       | Action plan v1.0 (original)                          |
| [`doc/requirements_v1.0-zh.md`](doc/requirements_v1.0-zh.md)                           | Requirements specification v1.0                      |
| [`doc/arch-migrate-zh.md`](doc/arch-migrate-zh.md)                                     | Current-vs-target architecture migration analysis    |
| [`doc/review-v0.5-zh.md`](doc/review-v0.5-zh.md)                                       | Code review of v0.5                                  |
| [`doc/review-v1.0-dev-plan-zh.md`](doc/review-v1.0-dev-plan-zh.md)                     | Requirements assessment & dev plan review            |

---

## License

See [`LICENSE`](LICENSE) for details.
