use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use app_sdk::{AppClient, HealthStatus};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    #[cfg(unix)]
    let socket = "/var/run/cc-rdeviceagent/app.sock";
    #[cfg(not(unix))]
    let socket = "http://127.0.0.1:50061"; // placeholder for future TCP fallback

    info!("connecting example payload app");

    #[cfg(unix)]
    let mut client = AppClient::connect_uds(
        socket,
        "payload-hello",
        "0.1.0",
        vec!["metrics".into(), "health".into()],
    )
    .await?;

    #[cfg(not(unix))]
    let mut client = AppClient::connect_tcp(
        socket,
        "payload-hello",
        "0.1.0",
        vec!["metrics".into(), "health".into()],
    )
    .await?;

    info!(app_id = %client.app_id(), "registered payload app");

    // Heartbeat + health + uplink demo loop
    for i in 0..3 {
        let valid = client.heartbeat().await?;
        info!(iteration = i, valid, "heartbeat sent");

        let mut metrics = HashMap::new();
        metrics.insert("loop_count".into(), i.to_string());
        client
            .report_health(HealthStatus::HealthHealthy, "demo app running", metrics)
            .await?;

        let msg_id = client
            .publish("demo/hello", format!("hello-from-payload-{i}"))
            .await?;
        info!(%msg_id, "published demo data");

        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    warn!("unregistering payload app");
    client.unregister().await?;
    Ok(())
}
