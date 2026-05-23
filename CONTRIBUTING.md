# Contributing

## Development Baseline

- Use stable Rust.
- Run `cargo fmt --all -- --check` before submitting changes.
- Run `cargo clippy --workspace --all-targets -- -D warnings` for lint coverage.
- Run `cargo test --workspace --all-targets` for the Phase 0 regression suite.

## Architecture Rules

- Business crates should depend on PAL traits from `pal-core`, not platform APIs.
- Platform-specific code belongs in `pal-*` crates or the binary bootstrap layer.
- New persistent runtime state should go through `agent-store` migrations.
- New observability setup should use `agent-telemetry` and `tracing`.
