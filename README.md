# CC-rDeviceAgent

Rust device-side agent for managed workstations and IoT devices.

## Components

- `cc-rdeviceagent`
  - privileged background service
  - Windows: runs as an SCM service
  - Linux: runs as a daemon by default, or foreground under `systemd`
- `cc-rdeviceagent-agent`
  - user-session desktop helper
  - handles screen capture for the logged-in desktop
  - binds only to loopback and requires a shared token header from the service

## Build

```bash
cargo build --release
```

## Run

Service in foreground:

```bash
./target/release/cc-rdeviceagent foreground --config ./CC-rDeviceAgent.toml
```

Desktop agent:

```bash
./target/release/cc-rdeviceagent-agent --config ./CC-rDeviceAgent.toml
```

## Capture notes

- `CaptureScreen` is proxied through the desktop agent, not performed inside the service.
- `agent.preferred_display_index` selects the monitor used for capture.
- On Linux, the `screenshots` crate already chooses Wayland vs X11; if Wayland capture fails, the agent retries through `grim` when available.
- `agent.auth_token` must match between the service and the desktop agent. The install scripts generate and stamp a shared random token automatically.

## Packaging

- Linux install artifacts: `packaging/linux`
- Windows install artifacts: `packaging/windows`
- Docker IoT simulation image: `packaging/docker/Dockerfile.iot-sim`

## IoT simulation container

The simulator launcher is kept in the management-side `CC` repository because it
also manages broker and log paths. With sibling checkouts, run it from `CC`:

```bash
cd ../CC
./scripts/start-iot-sim.sh 10
```

The launcher builds a minimal runtime image for `cc-rdeviceagent`, starts a Mosquitto
broker, and launches headless stations such as `iot-001`, `iot-002`, and `iot-003`.
It packages a host-built `cc-rdeviceagent` binary into the image, so Docker does not
recompile the Rust project on every simulator startup.
If `1883` is already taken, the script automatically reuses the existing host broker.

Useful commands:

```bash
./scripts/start-iot-sim.sh --status
./scripts/start-iot-sim.sh --stop
./scripts/start-iot-sim.sh 10 --dry-run
```

## Smoke test

```bash
./scripts/test-smoke.sh
```

Optional strict desktop-capture validation:

```bash
REQUIRE_CAPTURE=1 ./scripts/test-smoke.sh
```
