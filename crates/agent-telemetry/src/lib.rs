use tracing_subscriber::EnvFilter;

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub service_name: String,
    pub otlp_endpoint: Option<String>,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            service_name: "cc-rdeviceagent".to_string(),
            otlp_endpoint: None,
        }
    }
}

pub fn init_tracing(config: &TelemetryConfig) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .try_init();

    if let Some(endpoint) = &config.otlp_endpoint {
        tracing::info!(
            service_name = config.service_name,
            endpoint,
            "OTLP exporter configured for Phase 0 telemetry skeleton"
        );
    }
}
