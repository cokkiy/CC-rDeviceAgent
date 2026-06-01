# ADR-0007: Remove Embedded Desktop Agent

## Status
Accepted

## Context

The Desktop Agent (screen capture service) was originally embedded within CC-rDeviceAgent as a separate gRPC service running on a different port. It provided screen capture functionality via the `DesktopAgent` gRPC service defined in `proto/agent.proto`.

With Phase 2 transforming CC-rDeviceAgent into an application platform that hosts payload applications, this architectural approach no longer aligns with the project's direction. The agent should provide the platform infrastructure, while specific capabilities like screen capture should be implemented as standalone payload applications.

**Original Implementation:**
- `src/agent.rs` - Desktop Agent gRPC service implementation
- `proto/agent.proto` - Protocol definition for screen capture
- `src/app.rs` - Client code proxying screen capture requests
- Configuration: `agent.listen_addr`, `agent.auth_token`, `agent.preferred_display_index`
- Separate loopback address (127.0.0.1:50052) from main control service

**Architectural Concerns:**
1. Violates separation of concerns - agent should be platform, not application
2. Increases agent complexity and attack surface
3. Prevents independent updates of screen capture functionality
4. Does not leverage the southbound IPC channel being built in Phase 2

## Decision

Remove the embedded Desktop Agent implementation from the agent codebase. Screen capture functionality will be reimplemented as a standalone payload application in the future, using the southbound IPC channel to communicate with the agent platform.

**Changes Made:**
1. Deleted `src/agent.rs` (Desktop Agent service implementation)
2. Deleted `proto/agent.proto` (Desktop Agent protocol definition)
3. Removed `AgentConfig` from `src/config.rs`
4. Removed `agent` configuration section from config files
5. Removed Desktop Agent client code from `src/app.rs`
6. Removed `agent_target`, `agent_auth_token()`, and `preferred_display_index()` from `src/state.rs`
7. Updated `build.rs` to remove `proto/agent.proto` from compilation
8. Modified `capture_screen` RPC handler to return `Status::unimplemented` with migration message

**Migration Strategy:**
- The `CaptureScreen` RPC signature remains in the northbound `DeviceControl` service
- Returns `Status::unimplemented` with clear error message directing users to future standalone app
- Provides graceful degradation for existing clients
- RPC can be fully removed in a future major version

## Consequences

### Positive
- **Clear separation of concerns**: Agent provides platform, applications provide capabilities
- **Reduced agent complexity**: Fewer moving parts, smaller attack surface
- **Independent lifecycle**: Desktop Agent can be updated without agent updates
- **Better architecture**: Aligns with Phase 2 application platform vision
- **Demonstrates pattern**: First example of moving functionality to payload apps

### Negative
- **Breaking change**: Existing deployments using screen capture will lose functionality temporarily
- **Feature gap**: Screen capture unavailable until standalone app is developed
- **Migration effort**: Users must deploy standalone Desktop Agent app when available

### Neutral
- **Development timeline**: Standalone Desktop Agent app will be developed in Phase 2 or later
- **Reference implementation**: Desktop Agent will serve as reference for payload app development
- **API compatibility**: Northbound API maintains signature for backward compatibility

## Implementation Notes

**Build Verification:**
```bash
cargo clean
cargo build --release  # Success
cargo test             # 95 passed; 0 failed
```

**No remaining references:**
```bash
rg -i "desktop.?agent" --type rust src/  # No matches
rg "DesktopAgent" --type rust src/       # No matches
```

**Configuration migration:**
- Old `[agent]` section removed from all config files
- No data migration needed (configuration only)

## Future Work

1. **Phase 2 W2.1**: Implement southbound IPC channel (gRPC over UDS/Named Pipe)
2. **Phase 2 W2.8**: Develop standalone Desktop Agent as reference payload application
3. **Phase 2+**: Desktop Agent demonstrates:
   - Application registration and authentication
   - Data routing (screen capture data → backend)
   - Configuration management (display preferences)
   - Health reporting

## References

- [Phase 2 Application Platform Status](../phase2-application-platform-status.md)
- [Action Plan v2.0 - Phase 2](../action_plan-v2.0-zh.md#五phase-2应用基座--ota-启动8-周调整)
- [Requirements v1.0 - Application Platform](../requirements_v1.0-zh.md#22-产品定位)

## Date
2025-05-28
