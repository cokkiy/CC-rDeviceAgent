# CC-rDeviceAgent App SDK

`cc-rdeviceagent-app-sdk` is the Rust SDK for payload applications that run next to
CC-rDeviceAgent. It wraps the southbound `AppPlatform` gRPC API and handles app
registration, session tokens, heartbeats, health reports, uplink publishing,
configuration watching, downlink subscription, and graceful unregister.

## Install

```toml
[dependencies]
cc-rdeviceagent-app-sdk = "0.1"
```

The Rust crate name is `app_sdk`:

```rust
use app_sdk::AppClient;
```

## Connect And Register

On Unix platforms, connect through the agent Unix domain socket:

```rust,no_run
use app_sdk::AppClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = AppClient::connect_uds(
        "/var/run/cc-rdeviceagent/app.sock",
        "payload-hello",
        env!("CARGO_PKG_VERSION"),
        vec!["metrics".into()],
    )
    .await?;

    client.heartbeat().await?;
    client.unregister().await?;
    Ok(())
}
```

For tests or TCP fallback environments, connect through a gRPC endpoint:

```rust,no_run
use app_sdk::AppClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut client = AppClient::connect_tcp(
        "http://127.0.0.1:50052",
        "payload-hello",
        env!("CARGO_PKG_VERSION"),
        vec!["metrics".into()],
    )
    .await?;

    client.heartbeat().await?;
    Ok(())
}
```

## Publish Uplink Data

```rust,no_run
# use app_sdk::AppClient;
# async fn run(mut client: AppClient) -> anyhow::Result<()> {
let message_id = client
    .publish("sensors/temperature", b"23.5".to_vec())
    .await?;
println!("published {message_id}");
# Ok(())
# }
```

## Watch Configuration

Pass an empty key list to watch all visible app config, or pass specific keys.

```rust,no_run
# use app_sdk::AppClient;
# async fn run(mut client: AppClient) -> anyhow::Result<()> {
let mut updates = client.watch_config(vec!["sample_rate".into()]).await?;

while let Some(update) = updates.recv().await {
    println!(
        "config {} changed to {:?} at version {}",
        update.key, update.value, update.version
    );
}
# Ok(())
# }
```

## Subscribe To Downlink Data

```rust,no_run
# use app_sdk::AppClient;
# async fn run(mut client: AppClient) -> anyhow::Result<()> {
let mut messages = client.subscribe_data(vec!["commands/#".into()]).await?;

while let Some(message) = messages.recv().await {
    println!("downlink {}: {} bytes", message.topic, message.payload.len());
}
# Ok(())
# }
```

## Report Health

```rust,no_run
# use app_sdk::{AppClient, HealthStatus};
# use std::collections::HashMap;
# async fn run(mut client: AppClient) -> anyhow::Result<()> {
let mut metrics = HashMap::new();
metrics.insert("queue_depth".into(), "0".into());

client
    .report_health(HealthStatus::HealthHealthy, "running", metrics)
    .await?;
# Ok(())
# }
```

Call `unregister()` during graceful shutdown so the agent can clean up the app
session immediately.
