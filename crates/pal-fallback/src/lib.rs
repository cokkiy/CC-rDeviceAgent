use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use pal_core::*;

#[derive(Debug, Clone, Default)]
pub struct Unsupported;

impl Unsupported {
    fn err<T>(&self, capability: &str, operation: &str) -> PalResult<T> {
        Err(PalError::unsupported(capability, operation))
    }
}

impl ProcessManager for Unsupported {
    fn start(&self, _spec: ProcessSpec) -> PalResult<ProcessHandle> {
        self.err("ProcessManager", "start")
    }

    fn terminate(&self, _pid: u32) -> PalResult<()> {
        self.err("ProcessManager", "terminate")
    }

    fn status(&self, _pid: u32) -> PalResult<ProcessStatus> {
        self.err("ProcessManager", "status")
    }
}

impl ServiceManager for Unsupported {
    fn install(&self, _name: &str, _executable: &Path) -> PalResult<()> {
        self.err("ServiceManager", "install")
    }

    fn start(&self, _name: &str) -> PalResult<()> {
        self.err("ServiceManager", "start")
    }

    fn stop(&self, _name: &str) -> PalResult<()> {
        self.err("ServiceManager", "stop")
    }

    fn status(&self, _name: &str) -> PalResult<String> {
        self.err("ServiceManager", "status")
    }
}

impl SignalSource for Unsupported {
    fn wait_for_shutdown(&self) -> PalResult<()> {
        self.err("SignalSource", "wait_for_shutdown")
    }
}

#[derive(Debug, Clone, Default)]
pub struct StdFileSystem;

impl FileSystem for StdFileSystem {
    fn read(&self, path: &Path) -> PalResult<Vec<u8>> {
        fs::read(path).map_err(|err| PalError::io("FileSystem", "read", err))
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> PalResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| PalError::io("FileSystem", "create_dir_all", err))?;
        }
        fs::write(path, bytes).map_err(|err| PalError::io("FileSystem", "write", err))
    }

    fn remove(&self, path: &Path) -> PalResult<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(PalError::io("FileSystem", "remove", err)),
        }
    }

    fn exists(&self, path: &Path) -> PalResult<bool> {
        Ok(path.exists())
    }
}

#[derive(Debug, Clone, Default)]
pub struct ManagedPathResolver;

impl PathResolver for ManagedPathResolver {
    fn resolve_managed_path(&self, root: &Path, requested: &Path) -> PalResult<PathBuf> {
        let mut normalized = PathBuf::from(root);
        for component in requested.components() {
            match component {
                Component::Normal(part) => normalized.push(part),
                Component::CurDir => {}
                Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                    return Err(PalError::new(
                        PalErrorKind::InvalidInput,
                        "resolve_managed_path",
                        "PathResolver",
                        "path must stay inside managed root",
                    ));
                }
            }
        }
        Ok(normalized)
    }
}

impl DiskSpace for Unsupported {
    fn query(&self, _path: &Path) -> PalResult<DiskSpaceInfo> {
        self.err("DiskSpace", "query")
    }
}

impl FileLock for Unsupported {
    fn lock_exclusive(&self, _path: &Path) -> PalResult<()> {
        self.err("FileLock", "lock_exclusive")
    }

    fn unlock(&self, _path: &Path) -> PalResult<()> {
        self.err("FileLock", "unlock")
    }
}

impl NetworkInfo for Unsupported {
    fn interfaces(&self) -> PalResult<Vec<NetworkInterface>> {
        Ok(Vec::new())
    }
}

impl NetworkConfig for Unsupported {
    fn describe(&self) -> PalResult<String> {
        self.err("NetworkConfig", "describe")
    }
}

impl DnsResolver for Unsupported {
    fn resolve(&self, _host: &str) -> PalResult<Vec<String>> {
        self.err("DnsResolver", "resolve")
    }
}

impl NetStat for Unsupported {
    fn collect(&self) -> PalResult<NetworkCounterSnapshot> {
        Ok(NetworkCounterSnapshot::default())
    }
}

impl SystemControl for Unsupported {
    fn reboot(&self, _force: bool) -> PalResult<()> {
        self.err("SystemControl", "reboot")
    }

    fn shutdown(&self) -> PalResult<()> {
        self.err("SystemControl", "shutdown")
    }
}

impl TimeService for Unsupported {
    fn now(&self) -> PalResult<SystemTime> {
        Ok(SystemTime::now())
    }
}

impl EnvVars for Unsupported {
    fn get(&self, key: &str) -> PalResult<Option<String>> {
        Ok(std::env::var(key).ok())
    }
}

impl Bootloader for Unsupported {
    fn active_slot(&self) -> PalResult<String> {
        self.err("Bootloader", "active_slot")
    }

    fn mark_slot_active(&self, _slot: &str) -> PalResult<()> {
        self.err("Bootloader", "mark_slot_active")
    }
}

impl SlotManager for Unsupported {
    fn slots(&self) -> PalResult<Vec<SlotInfo>> {
        self.err("SlotManager", "slots")
    }
}

impl BootEnv for Unsupported {
    fn get(&self, _key: &str) -> PalResult<Option<String>> {
        self.err("BootEnv", "get")
    }

    fn set(&self, _key: &str, _value: &str) -> PalResult<()> {
        self.err("BootEnv", "set")
    }
}

impl TpmProvider for Unsupported {
    fn available(&self) -> PalResult<bool> {
        Ok(false)
    }
}

#[derive(Debug, Clone)]
pub struct FileBackedKeyStore {
    root: PathBuf,
}

impl FileBackedKeyStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_for(&self, name: &str) -> PalResult<PathBuf> {
        if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
            return Err(PalError::new(
                PalErrorKind::InvalidInput,
                "path_for",
                "KeyStore",
                "key name must be a simple file name",
            ));
        }
        Ok(self.root.join(name))
    }
}

impl KeyStore for FileBackedKeyStore {
    fn put_key(&self, name: &str, value: &[u8]) -> PalResult<()> {
        let path = self.path_for(name)?;
        StdFileSystem.write(&path, value)
    }

    fn get_key(&self, name: &str) -> PalResult<Option<Vec<u8>>> {
        let path = self.path_for(name)?;
        if !path.exists() {
            return Ok(None);
        }
        StdFileSystem.read(&path).map(Some)
    }

    fn delete_key(&self, name: &str) -> PalResult<()> {
        let path = self.path_for(name)?;
        StdFileSystem.remove(&path)
    }
}

impl CredentialStore for FileBackedKeyStore {
    fn put_secret(&self, name: &str, value: &[u8]) -> PalResult<()> {
        self.put_key(name, value)
    }

    fn get_secret(&self, name: &str) -> PalResult<Option<Vec<u8>>> {
        self.get_key(name)
    }
}

impl EntropySource for Unsupported {
    fn fill(&self, bytes: &mut [u8]) -> PalResult<()> {
        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|value| value.as_nanos())
            .unwrap_or_default();
        for (idx, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> ((idx % 8) * 8)) & 0xff) as u8;
        }
        Ok(())
    }
}

impl ResourceLimiter for Unsupported {
    fn apply_to_pid(&self, _pid: u32, _limits: &ResourceLimitSpec) -> PalResult<()> {
        self.err("ResourceLimiter", "apply_to_pid")
    }
}

impl Sandbox for Unsupported {
    fn prepare(&self, _profile: &str) -> PalResult<()> {
        self.err("Sandbox", "prepare")
    }
}

impl NamespaceManager for Unsupported {
    fn isolate(&self, _pid: u32) -> PalResult<()> {
        self.err("NamespaceManager", "isolate")
    }
}

impl Capabilities for Unsupported {
    fn drop_to_minimum(&self) -> PalResult<()> {
        self.err("Capabilities", "drop_to_minimum")
    }
}

impl CpuStat for Unsupported {
    fn snapshot(&self) -> PalResult<String> {
        Ok(String::new())
    }
}

impl MemStat for Unsupported {
    fn snapshot(&self) -> PalResult<String> {
        Ok(String::new())
    }
}

impl DiskStat for Unsupported {
    fn snapshot(&self) -> PalResult<String> {
        Ok(String::new())
    }
}

impl SensorReader for Unsupported {
    fn read(&self, _sensor: &str) -> PalResult<Option<String>> {
        Ok(None)
    }
}

impl IpcServer for Unsupported {
    fn bind(&self, _endpoint: &str) -> PalResult<()> {
        self.err("IpcServer", "bind")
    }
}

impl IpcClient for Unsupported {
    fn connect(&self, _endpoint: &str) -> PalResult<()> {
        self.err("IpcClient", "connect")
    }
}

impl SystemLogger for Unsupported {
    fn log(&self, level: &str, message: &str) -> PalResult<()> {
        tracing::event!(target: "pal", tracing::Level::INFO, level, message);
        Ok(())
    }
}

impl DeviceId for Unsupported {
    fn identity(&self) -> PalResult<DeviceIdentity> {
        let fingerprint = machine_id().unwrap_or_else(|| "unknown".to_string());
        Ok(DeviceIdentity {
            device_id: fingerprint.clone(),
            fingerprint,
        })
    }
}

impl MachineFingerprint for Unsupported {
    fn fingerprint(&self) -> PalResult<String> {
        Ok(machine_id().unwrap_or_else(|| "unknown".to_string()))
    }
}

pub fn fallback_context(profile: CapabilityProfile, key_root: PathBuf) -> PlatformContext {
    let unsupported = Arc::new(Unsupported);
    let file_system = Arc::new(StdFileSystem);
    let path_resolver = Arc::new(ManagedPathResolver);
    let key_store = Arc::new(FileBackedKeyStore::new(key_root));

    PlatformContext {
        profile,
        process_manager: unsupported.clone(),
        service_manager: unsupported.clone(),
        signal_source: unsupported.clone(),
        file_system,
        path_resolver,
        disk_space: unsupported.clone(),
        file_lock: unsupported.clone(),
        network_info: unsupported.clone(),
        network_config: unsupported.clone(),
        dns_resolver: unsupported.clone(),
        net_stat: unsupported.clone(),
        system_control: unsupported.clone(),
        time_service: unsupported.clone(),
        env_vars: unsupported.clone(),
        bootloader: unsupported.clone(),
        slot_manager: unsupported.clone(),
        boot_env: unsupported.clone(),
        tpm_provider: unsupported.clone(),
        key_store: key_store.clone(),
        credential_store: key_store,
        entropy_source: unsupported.clone(),
        resource_limiter: unsupported.clone(),
        sandbox: unsupported.clone(),
        namespace_manager: unsupported.clone(),
        capabilities: unsupported.clone(),
        cpu_stat: unsupported.clone(),
        mem_stat: unsupported.clone(),
        disk_stat: unsupported.clone(),
        sensor_reader: unsupported.clone(),
        ipc_server: unsupported.clone(),
        ipc_client: unsupported.clone(),
        system_logger: unsupported.clone(),
        device_id: unsupported.clone(),
        machine_fingerprint: unsupported,
    }
}

fn machine_id() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .find_map(|path| fs::read_to_string(path).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
