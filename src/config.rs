use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::telemetry::{TelemetryProfileConfig, validate_profiles};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct AppConfig {
    pub service: ServiceConfig,
    pub control: ControlConfig,
    pub agent: AgentConfig,
    pub mqtt: MqttConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ServiceConfig {
    pub service_name: String,
    #[serde(alias = "station_id")]
    pub device_id: String,
    pub state_interval_seconds: u64,
    pub watched_processes: Vec<String>,
    pub udp_display_target: String,
    pub launcher_proxy_path: String,
    pub file_transfer: FileTransferConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct FileTransferConfig {
    pub max_file_bytes: u64,
    pub min_free_bytes: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ControlConfig {
    pub listen_addr: String,
    pub tls: TlsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AgentConfig {
    pub listen_addr: String,
    pub auth_token: String,
    pub preferred_display_index: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct MqttConfig {
    pub enabled: bool,
    pub broker_host: String,
    pub broker_port: u16,
    pub telemetry_enabled: bool,
    pub status_enabled: bool,
    pub telemetry_profiles: Vec<TelemetryProfileConfig>,
    pub tls: TlsConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct TlsConfig {
    pub enabled: bool,
    pub ca_cert_path: Option<PathBuf>,
    pub cert_path: Option<PathBuf>,
    pub key_path: Option<PathBuf>,
    pub require_client_auth: bool,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            service_name: "CC-rDeviceAgent".to_string(),
            device_id: String::new(),
            state_interval_seconds: 5,
            watched_processes: Vec::new(),
            udp_display_target: "127.0.0.1:9008".to_string(),
            launcher_proxy_path: String::new(),
            file_transfer: FileTransferConfig::default(),
        }
    }
}

impl Default for FileTransferConfig {
    fn default() -> Self {
        Self {
            max_file_bytes: 512 * 1024 * 1024,
            min_free_bytes: 64 * 1024 * 1024,
        }
    }
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            listen_addr: "0.0.0.0:50051".to_string(),
            tls: TlsConfig::default(),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            listen_addr: "127.0.0.1:50052".to_string(),
            auth_token: "local-change-me".to_string(),
            preferred_display_index: 0,
        }
    }
}

impl Default for MqttConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            broker_host: "localhost".to_string(),
            broker_port: 4222,
            telemetry_enabled: true,
            status_enabled: true,
            telemetry_profiles: Vec::new(),
            tls: TlsConfig::default(),
        }
    }
}

impl AppConfig {
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let (config, _) = Self::load_with_path(path)?;
        Ok(config)
    }

    pub fn load_with_path(path: Option<&Path>) -> Result<(Self, PathBuf)> {
        let path = path
            .map(Path::to_path_buf)
            .unwrap_or_else(default_config_path);

        if !path.exists() {
            return Ok((Self::default(), path));
        }

        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("read config {}", path.display()))?;
        let config = toml::from_str::<Self>(&content)
            .with_context(|| format!("parse config {}", path.display()))?;
        config.validate()?;
        Ok((config, path))
    }

    pub fn validate(&self) -> Result<()> {
        validate_profiles(&self.mqtt.telemetry_profiles)
            .map_err(|error| anyhow::anyhow!("invalid mqtt.telemetry_profiles: {error}"))?;
        validate_tls_config("control.tls", &self.control.tls)?;
        validate_tls_config("mqtt.tls", &self.mqtt.tls)?;
        Ok(())
    }

    pub fn persist(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let content = toml::to_string_pretty(self).context("serialize config to TOML")?;
        std::fs::write(path, content).with_context(|| format!("write config {}", path.display()))
    }

    pub fn resolved_device_id(&self) -> String {
        if !self.service.device_id.trim().is_empty() {
            return self.service.device_id.trim().to_string();
        }

        let host = hostname::get()
            .ok()
            .and_then(|value| value.into_string().ok())
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "device".to_string());

        format!("{host}-{}", Uuid::new_v4())
    }
}

fn validate_tls_config(name: &str, tls: &TlsConfig) -> Result<()> {
    if !tls.enabled {
        return Ok(());
    }

    if tls.ca_cert_path.is_none() {
        anyhow::bail!("{name}.ca_cert_path is required when TLS is enabled");
    }
    if tls.cert_path.is_none() {
        anyhow::bail!("{name}.cert_path is required when TLS is enabled");
    }
    if tls.key_path.is_none() {
        anyhow::bail!("{name}.key_path is required when TLS is enabled");
    }

    Ok(())
}

pub fn default_config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("CC-rDeviceAgent.toml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{TelemetryInclude, TelemetryProfileConfig};

    #[test]
    fn config_round_trip_preserves_telemetry_profiles() {
        let config = AppConfig {
            mqtt: MqttConfig {
                enabled: true,
                telemetry_profiles: vec![TelemetryProfileConfig {
                    id: "fast".to_string(),
                    name: "Fast".to_string(),
                    enabled: true,
                    collection_interval_ms: 1000,
                    includes: vec![
                        TelemetryInclude::RuntimeBasic,
                        TelemetryInclude::RuntimeApps,
                    ],
                }],
                ..MqttConfig::default()
            },
            ..AppConfig::default()
        };

        let toml = toml::to_string_pretty(&config).unwrap();
        let parsed: AppConfig = toml::from_str(&toml).unwrap();

        assert_eq!(
            parsed.mqtt.telemetry_profiles,
            config.mqtt.telemetry_profiles
        );
    }

    #[test]
    fn config_validation_rejects_duplicate_ids() {
        let config = AppConfig {
            mqtt: MqttConfig {
                telemetry_profiles: vec![
                    TelemetryProfileConfig::default_full(1000),
                    TelemetryProfileConfig::default_full(2000),
                ],
                ..MqttConfig::default()
            },
            ..AppConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn config_validation_requires_grpc_mtls_material_when_enabled() {
        let config = AppConfig {
            control: ControlConfig {
                tls: TlsConfig {
                    enabled: true,
                    ..TlsConfig::default()
                },
                ..ControlConfig::default()
            },
            ..AppConfig::default()
        };

        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("control.tls.ca_cert_path"));
    }

    #[test]
    fn config_validation_requires_mqtt_client_cert_when_tls_enabled() {
        let config = AppConfig {
            mqtt: MqttConfig {
                tls: TlsConfig {
                    enabled: true,
                    ca_cert_path: Some(PathBuf::from("ca.pem")),
                    cert_path: Some(PathBuf::from("client.pem")),
                    key_path: None,
                    require_client_auth: true,
                },
                ..MqttConfig::default()
            },
            ..AppConfig::default()
        };

        let error = config.validate().unwrap_err().to_string();
        assert!(error.contains("mqtt.tls.key_path"));
    }
}
