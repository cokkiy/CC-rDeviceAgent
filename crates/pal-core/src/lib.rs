use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

pub type PalResult<T> = Result<T, PalError>;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum PalErrorKind {
    Unsupported,
    PermissionDenied,
    NotFound,
    InvalidInput,
    Io,
    Timeout,
    Unavailable,
    Security,
    Internal,
}

#[derive(Debug, Clone, thiserror::Error, Serialize, Deserialize)]
#[error("{kind:?} during {operation} for {capability}: {message}")]
pub struct PalError {
    pub kind: PalErrorKind,
    pub operation: String,
    pub capability: String,
    pub message: String,
}

impl PalError {
    pub fn new(
        kind: PalErrorKind,
        operation: impl Into<String>,
        capability: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            operation: operation.into(),
            capability: capability.into(),
            message: message.into(),
        }
    }

    pub fn unsupported(capability: impl Into<String>, operation: impl Into<String>) -> Self {
        let capability = capability.into();
        Self::new(
            PalErrorKind::Unsupported,
            operation,
            capability.clone(),
            format!("{capability} is not supported on this platform"),
        )
    }

    pub fn io(
        capability: impl Into<String>,
        operation: impl Into<String>,
        error: impl fmt::Display,
    ) -> Self {
        Self::new(PalErrorKind::Io, operation, capability, error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CapabilityProfile {
    pub platform: String,
    pub arch: String,
    pub has_tpm: bool,
    pub has_os_keyring: bool,
    pub has_ab_partition: bool,
    pub has_cgroup_v2: bool,
    pub has_cgroup_v1: bool,
    pub has_secure_boot: bool,
    pub has_systemd: bool,
    pub has_journald: bool,
    pub has_unix_socket: bool,
    pub has_named_pipe: bool,
    pub has_screen_capture: bool,
    pub storage_writable_mb: u64,
    pub network_interfaces: Vec<String>,
    pub detected_at_unix_ms: u64,
    pub details: BTreeMap<String, String>,
}

impl CapabilityProfile {
    pub fn current_platform() -> Self {
        Self {
            platform: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            detected_at_unix_ms: unix_ms(SystemTime::now()),
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProcessSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessHandle {
    pub pid: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStatus {
    pub pid: u32,
    pub running: bool,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSpaceInfo {
    pub path: PathBuf,
    pub total_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkInterface {
    pub name: String,
    pub addresses: Vec<String>,
    pub is_loopback: bool,
    pub is_up: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InterfaceCounterSnapshot {
    pub if_name: String,
    pub bytes_received: u64,
    pub bytes_sented: u64,
    pub unicast_packet_received: u64,
    pub unicast_packet_sented: u64,
    pub multicast_packet_received: u64,
    pub multicast_packet_sented: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkCounterSnapshot {
    pub datagrams_received: u64,
    pub datagrams_sent: u64,
    pub datagrams_discarded: u64,
    pub datagrams_with_errors: u64,
    pub segments_received: u64,
    pub segments_sent: u64,
    pub errors_received: u64,
    pub current_connections: u64,
    pub reset_connections: u64,
    pub interface_counters: Vec<InterfaceCounterSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimitSpec {
    pub memory_bytes: Option<u64>,
    pub cpu_millis: Option<u64>,
    pub open_files: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotInfo {
    pub name: String,
    pub active: bool,
    pub bootable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub device_id: String,
    pub fingerprint: String,
}

pub trait ProcessManager: Send + Sync {
    fn start(&self, spec: ProcessSpec) -> PalResult<ProcessHandle>;
    fn terminate(&self, pid: u32) -> PalResult<()>;
    fn status(&self, pid: u32) -> PalResult<ProcessStatus>;
}

pub trait ServiceManager: Send + Sync {
    fn install(&self, name: &str, executable: &Path) -> PalResult<()>;
    fn start(&self, name: &str) -> PalResult<()>;
    fn stop(&self, name: &str) -> PalResult<()>;
    fn status(&self, name: &str) -> PalResult<String>;
}

pub trait SignalSource: Send + Sync {
    fn wait_for_shutdown(&self) -> PalResult<()>;
}

pub trait FileSystem: Send + Sync {
    fn read(&self, path: &Path) -> PalResult<Vec<u8>>;
    fn write(&self, path: &Path, bytes: &[u8]) -> PalResult<()>;
    fn remove(&self, path: &Path) -> PalResult<()>;
    fn exists(&self, path: &Path) -> PalResult<bool>;
}

pub trait PathResolver: Send + Sync {
    fn resolve_managed_path(&self, root: &Path, requested: &Path) -> PalResult<PathBuf>;
}

pub trait DiskSpace: Send + Sync {
    fn query(&self, path: &Path) -> PalResult<DiskSpaceInfo>;
}

pub trait FileLock: Send + Sync {
    fn lock_exclusive(&self, path: &Path) -> PalResult<()>;
    fn unlock(&self, path: &Path) -> PalResult<()>;
}

pub trait NetworkInfo: Send + Sync {
    fn interfaces(&self) -> PalResult<Vec<NetworkInterface>>;
}

pub trait NetworkConfig: Send + Sync {
    fn describe(&self) -> PalResult<String>;
}

pub trait DnsResolver: Send + Sync {
    fn resolve(&self, host: &str) -> PalResult<Vec<String>>;
}

pub trait NetStat: Send + Sync {
    fn collect(&self) -> PalResult<NetworkCounterSnapshot>;
}

pub trait SystemControl: Send + Sync {
    fn reboot(&self, force: bool) -> PalResult<()>;
    fn shutdown(&self) -> PalResult<()>;
}

pub trait TimeService: Send + Sync {
    fn now(&self) -> PalResult<SystemTime>;
}

pub trait EnvVars: Send + Sync {
    fn get(&self, key: &str) -> PalResult<Option<String>>;
}

pub trait Bootloader: Send + Sync {
    fn active_slot(&self) -> PalResult<String>;
    fn mark_slot_active(&self, slot: &str) -> PalResult<()>;
}

pub trait SlotManager: Send + Sync {
    fn slots(&self) -> PalResult<Vec<SlotInfo>>;
}

pub trait BootEnv: Send + Sync {
    fn get(&self, key: &str) -> PalResult<Option<String>>;
    fn set(&self, key: &str, value: &str) -> PalResult<()>;
}

pub trait TpmProvider: Send + Sync {
    fn available(&self) -> PalResult<bool>;
}

pub trait KeyStore: Send + Sync {
    fn put_key(&self, name: &str, value: &[u8]) -> PalResult<()>;
    fn get_key(&self, name: &str) -> PalResult<Option<Vec<u8>>>;
    fn delete_key(&self, name: &str) -> PalResult<()>;
}

pub trait CredentialStore: Send + Sync {
    fn put_secret(&self, name: &str, value: &[u8]) -> PalResult<()>;
    fn get_secret(&self, name: &str) -> PalResult<Option<Vec<u8>>>;
}

pub trait EntropySource: Send + Sync {
    fn fill(&self, bytes: &mut [u8]) -> PalResult<()>;
}

pub trait ResourceLimiter: Send + Sync {
    fn apply_to_pid(&self, pid: u32, limits: &ResourceLimitSpec) -> PalResult<()>;
}

pub trait Sandbox: Send + Sync {
    fn prepare(&self, profile: &str) -> PalResult<()>;
}

pub trait NamespaceManager: Send + Sync {
    fn isolate(&self, pid: u32) -> PalResult<()>;
}

pub trait Capabilities: Send + Sync {
    fn drop_to_minimum(&self) -> PalResult<()>;
}

pub trait CpuStat: Send + Sync {
    fn snapshot(&self) -> PalResult<String>;
}

pub trait MemStat: Send + Sync {
    fn snapshot(&self) -> PalResult<String>;
}

pub trait DiskStat: Send + Sync {
    fn snapshot(&self) -> PalResult<String>;
}

pub trait SensorReader: Send + Sync {
    fn read(&self, sensor: &str) -> PalResult<Option<String>>;
}

pub trait IpcServer: Send + Sync {
    fn bind(&self, endpoint: &str) -> PalResult<()>;
}

pub trait IpcClient: Send + Sync {
    fn connect(&self, endpoint: &str) -> PalResult<()>;
}

pub trait SystemLogger: Send + Sync {
    fn log(&self, level: &str, message: &str) -> PalResult<()>;
}

pub trait DeviceId: Send + Sync {
    fn identity(&self) -> PalResult<DeviceIdentity>;
}

pub trait MachineFingerprint: Send + Sync {
    fn fingerprint(&self) -> PalResult<String>;
}

pub trait CapabilityProbe: Send + Sync {
    fn probe(&self) -> PalResult<CapabilityProfile>;
}

#[derive(Clone)]
pub struct PlatformContext {
    pub profile: CapabilityProfile,
    pub process_manager: Arc<dyn ProcessManager>,
    pub service_manager: Arc<dyn ServiceManager>,
    pub signal_source: Arc<dyn SignalSource>,
    pub file_system: Arc<dyn FileSystem>,
    pub path_resolver: Arc<dyn PathResolver>,
    pub disk_space: Arc<dyn DiskSpace>,
    pub file_lock: Arc<dyn FileLock>,
    pub network_info: Arc<dyn NetworkInfo>,
    pub network_config: Arc<dyn NetworkConfig>,
    pub dns_resolver: Arc<dyn DnsResolver>,
    pub net_stat: Arc<dyn NetStat>,
    pub system_control: Arc<dyn SystemControl>,
    pub time_service: Arc<dyn TimeService>,
    pub env_vars: Arc<dyn EnvVars>,
    pub bootloader: Arc<dyn Bootloader>,
    pub slot_manager: Arc<dyn SlotManager>,
    pub boot_env: Arc<dyn BootEnv>,
    pub tpm_provider: Arc<dyn TpmProvider>,
    pub key_store: Arc<dyn KeyStore>,
    pub credential_store: Arc<dyn CredentialStore>,
    pub entropy_source: Arc<dyn EntropySource>,
    pub resource_limiter: Arc<dyn ResourceLimiter>,
    pub sandbox: Arc<dyn Sandbox>,
    pub namespace_manager: Arc<dyn NamespaceManager>,
    pub capabilities: Arc<dyn Capabilities>,
    pub cpu_stat: Arc<dyn CpuStat>,
    pub mem_stat: Arc<dyn MemStat>,
    pub disk_stat: Arc<dyn DiskStat>,
    pub sensor_reader: Arc<dyn SensorReader>,
    pub ipc_server: Arc<dyn IpcServer>,
    pub ipc_client: Arc<dyn IpcClient>,
    pub system_logger: Arc<dyn SystemLogger>,
    pub device_id: Arc<dyn DeviceId>,
    pub machine_fingerprint: Arc<dyn MachineFingerprint>,
}

pub trait PlatformBuilder {
    fn build(&self) -> PalResult<PlatformContext>;
}

pub struct CapabilityRouter {
    profile: CapabilityProfile,
}

impl CapabilityRouter {
    pub fn new(profile: CapabilityProfile) -> Self {
        Self { profile }
    }

    pub fn profile(&self) -> &CapabilityProfile {
        &self.profile
    }

    pub fn prefer_cgroup_v2(&self) -> bool {
        self.profile.has_cgroup_v2
    }

    pub fn prefer_os_keyring(&self) -> bool {
        self.profile.has_os_keyring
    }
}

fn unix_ms(time: SystemTime) -> u64 {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}
