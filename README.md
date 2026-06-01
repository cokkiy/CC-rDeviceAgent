# CC-rDeviceAgent

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

## Current Status (v0.4)

### What the service does

The main service exposes two protobuf services:

- **`DeviceControl`**
  - start, stop, and restart processes
  - reboot or shut down the host
  - return system state, process lists, network interfaces, TCP/UDP listeners, and version info
  - execute shell commands with a timeout
  - update watched process names and the in-memory state gathering interval
  - return the telemetry schema and replace MQTT telemetry profiles at runtime
- **`FileTransfer`**
  - upload files as streamed chunks
  - download files as streamed chunks

The service listens on `control.listen_addr`, which is `0.0.0.0:50051` in the sample config.

### MQTT behavior

When `mqtt.enabled = true`, the service can:

- publish device status to `cc/<device_id>/status`
- publish telemetry bundles to `cc/<device_id>/telemetry`
- subscribe to `cc/<device_id>/command`
- publish command acknowledgements to `cc/<device_id>/command/ack`

The currently implemented MQTT command handler supports `restart_process` with a
`process_name` parameter.

Telemetry profiles are validated and normalized from configuration or replaced at
runtime through gRPC. The supported telemetry include keys are:

- `runtime_basic`
- `runtime_system`
- `runtime_apps`
- `runtime_network`
- `runtime_storage`

If no telemetry profiles are configured, the service synthesizes a default profile
that includes all of the sections above and uses the service state interval.

### Known gaps (v0.4 → target architecture)

| Gap                                 | Severity | Target    |
| ----------------------------------- | -------- | --------- |
| No platform abstraction layer (PAL) | 🔴 High   | Phase 0   |
| No Upgrade Engine (OTA)             | 🔴 High   | Phase 2-3 |
| No Security Center / mTLS           | 🔴 High   | Phase 1   |
| No Audit Chain                      | 🔴 High   | Phase 1   |
| No App Registry / Lifecycle         | 🔴 High   | Phase 2   |
| No Config Manager                   | 🟡 Medium | Phase 2   |
| No unit tests or CI                 | 🟡 Medium | Phase 0   |

---

## Development Roadmap

See [`doc/action_plan-v2.0-zh.md`](doc/action_plan-v2.0-zh.md) for the full action plan.

```
Phase -1 (v0.4)  Architecture gap analysis & fixes   2 weeks  ← Current
Phase 0  (v0.5)  Foundation + PAL complete contracts  5 weeks
Phase 1  (v0.8)  Security hardening                   6 weeks
Phase 2  (v1.0)  Application platform + OTA design    8 weeks
Phase 3  (v1.5)  Full OTA upgrade                    10 weeks
Phase 4  (v2.0)  Platformization                     10 weeks
Phase 5  (v2.1)  Production readiness                 6 weeks
```

### Key milestones

| Phase | Version | Focus                     | Key deliverables                                               |
| ----- | ------- | ------------------------- | -------------------------------------------------------------- |
| -1    | v0.4    | Architecture gap analysis | Migration design, CI pipeline, clean baseline                  |
| 0     | v0.5    | Foundation + PAL          | Full PAL traits, Linux adapter, State Store, CapabilityProfile |
| 1     | v0.8    | Security                  | mTLS, KeyStore, RBAC, Audit Chain, Sandbox, command whitelist  |
| 2     | v1.0    | Application platform      | IPC, App Registry, Lifecycle, Config Manager, OTA design       |
| 3     | v1.5    | OTA upgrade               | A/B slot, Agent self-update, fault injection tests             |
| 4     | v2.0    | Platformization           | Multi-tenant, canary release, extension points                 |
| 5     | v2.1    | Production readiness      | SLA validation, security audit, documentation                  |

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
