# CC-rDeviceAgent

Rust device-side agent for managed workstations, edge devices, and IoT stations.

The current codebase centers on a gRPC control service with optional MQTT telemetry,
status publishing, command acknowledgements, and a loopback-only desktop capture
helper used for screen capture.

## What the service does

The main service exposes two protobuf services:

- `StationControl`
  - start, stop, and restart processes
  - reboot or shut down the host
  - return system state, process lists, network interfaces, TCP/UDP listeners, and version info
  - execute shell commands with a timeout
  - update watched process names and the in-memory state gathering interval
  - return the telemetry schema and replace MQTT telemetry profiles at runtime
  - proxy `CaptureScreen` requests to the desktop agent
- `FileTransfer`
  - upload files as streamed chunks
  - download files as streamed chunks

The service listens on `control.listen_addr`, which is `0.0.0.0:50051` in the sample config.

## Desktop capture helper

The desktop capture path is implemented in the desktop agent gRPC service:

- binds only to loopback using `agent.listen_addr`
- requires the `x-cc-agent-token` header to match `agent.auth_token`
- captures the configured display index and streams PNG chunks back to the service
- caches the latest capture so interrupted downloads can resume by byte offset
- on Linux, retries via `grim` when the primary screenshot path fails

`CaptureScreen` on the main service is only a proxy. The privileged service does not
capture the desktop directly.

## MQTT behavior

When `mqtt.enabled = true`, the service can:

- publish station status to `cc/<station_id>/status`
- publish telemetry bundles to `cc/<station_id>/telemetry`
- subscribe to `cc/<station_id>/command`
- publish command acknowledgements to `cc/<station_id>/command/ack`

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

## Configuration

By default, the service loads `CC-rDeviceAgent.toml` from the executable directory.
If `service.station_id` is blank, the service resolves it to `<hostname>-<uuid>`.

The configuration surface used by the running service is:

```toml
[service]
service_name = "CC-rDeviceAgent"
station_id = "station-01"
state_interval_seconds = 5
watched_processes = []
udp_display_target = "127.0.0.1:9008"
launcher_proxy_path = ""

[control]
listen_addr = "0.0.0.0:50051"

[agent]
listen_addr = "127.0.0.1:50052"
auth_token = "local-change-me"
preferred_display_index = 0

[mqtt]
enabled = true
broker_host = "localhost"
broker_port = 1883
telemetry_enabled = true
status_enabled = true
```

The sample repository config continues with `mqtt.telemetry_profiles`, which define
profile IDs, names, collection intervals, and include sets.

## Binaries and run modes

The main binary is `cc-rdeviceagent`.

Supported CLI arguments:

- `foreground` or `--foreground`
- `daemon` or `--daemon`
- `--config <path>`
- `--console-telemetry` or `--telemetry-console`

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

The repository also contains a desktop agent implementation used by the Linux and
Windows packaging scripts and by the smoke test. Those scripts expect a companion
binary named `cc-rdeviceagent-agent` built alongside the service.

For local debugging, you can mirror runtime telemetry to stdout:

```bash
./target/release/cc-rdeviceagent foreground --config ./CC-rDeviceAgent.toml --console-telemetry
```

## Repository scope

The crate also contains library modules for scripts, tags, groups, batch execution,
alerts, and plugin abstractions. Those modules are present in the source tree, but
the current service entry point documented above is centered on gRPC station control,
file transfer, telemetry collection, MQTT publishing, and desktop capture proxying.

## Packaging

- Linux install artifacts: `packaging/linux`
- Windows install artifacts: `packaging/windows`
- Docker IoT simulation image: `packaging/docker/Dockerfile.iot-sim`

The install scripts:

- copy the service and desktop-agent binaries
- install `CC-rDeviceAgent.toml`
- generate and stamp a shared `agent.auth_token`
- register the service with `systemd` or SCM
- register the desktop agent as a user service or scheduled task

## IoT simulation container

The simulator launcher is kept in the management-side `CC` repository because it also
manages broker and log paths. With sibling checkouts, run it from `CC`:

```bash
cd ../CC
./scripts/start-iot-sim.sh 10
```

The launcher builds a minimal runtime image for `cc-rdeviceagent`, starts a Mosquitto
broker, and launches headless stations such as `iot-001`, `iot-002`, and `iot-003`.
It packages a host-built `cc-rdeviceagent` binary into the image, so Docker does not
recompile the Rust project on every simulator startup. If port `1883` is already in
use, the script automatically reuses the existing host broker.

Useful commands:

```bash
./scripts/start-iot-sim.sh --status
./scripts/start-iot-sim.sh --stop
./scripts/start-iot-sim.sh 10 --dry-run
```

## Smoke test

The smoke test builds the project, starts the desktop agent and service with the
selected config, then runs the dedicated smoketest binary against the live endpoints.

```bash
./scripts/test-smoke.sh
```

Optional strict desktop-capture validation:

```bash
REQUIRE_CAPTURE=1 ./scripts/test-smoke.sh
```
