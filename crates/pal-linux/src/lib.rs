use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;
use std::time::SystemTime;

use pal_core::*;

#[derive(Debug, Clone, Default)]
pub struct LinuxPlatformBuilder;

impl PlatformBuilder for LinuxPlatformBuilder {
    fn build(&self) -> PalResult<PlatformContext> {
        let probe = LinuxCapabilityProbe;
        let profile = probe.probe()?;
        let mut context = pal_fallback::fallback_context(profile, default_key_root());
        let linux = Arc::new(LinuxAdapter);

        context.process_manager = linux.clone();
        context.service_manager = linux.clone();
        context.disk_space = linux.clone();
        context.network_info = linux.clone();
        context.dns_resolver = linux.clone();
        context.net_stat = linux.clone();
        context.system_control = linux.clone();
        context.time_service = linux.clone();
        context.env_vars = linux.clone();
        context.ipc_server = linux.clone();
        context.ipc_client = linux.clone();
        context.system_logger = linux.clone();
        context.device_id = linux.clone();
        context.resource_limiter = linux.clone();
        context.machine_fingerprint = linux;
        Ok(context)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LinuxCapabilityProbe;

impl CapabilityProbe for LinuxCapabilityProbe {
    fn probe(&self) -> PalResult<CapabilityProfile> {
        let mut profile = CapabilityProfile::current_platform();
        profile.has_systemd = Path::new("/run/systemd/system").exists();
        profile.has_journald = Path::new("/run/systemd/journal/socket").exists();
        profile.has_cgroup_v2 = Path::new("/sys/fs/cgroup/cgroup.controllers").exists();
        profile.has_cgroup_v1 = Path::new("/sys/fs/cgroup").exists() && !profile.has_cgroup_v2;
        profile.has_unix_socket = true;
        profile.has_tpm = Path::new("/dev/tpm0").exists() || Path::new("/dev/tpmrm0").exists();
        profile.has_os_keyring = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some();
        profile.has_secure_boot = Path::new("/sys/firmware/efi/efivars").exists();
        profile.has_screen_capture = command_in_path("grim");
        profile.storage_writable_mb = statvfs_available_mb(Path::new(".")).unwrap_or_default();
        profile.network_interfaces = interface_names().unwrap_or_default();
        Ok(profile)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LinuxAdapter;

impl ProcessManager for LinuxAdapter {
    #[tracing::instrument(skip(self, spec), fields(program = %spec.program.display()))]
    fn start(&self, spec: ProcessSpec) -> PalResult<ProcessHandle> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        command.envs(&spec.env);
        if let Some(cwd) = spec.cwd {
            command.current_dir(cwd);
        }
        let child = command
            .spawn()
            .map_err(|err| PalError::io("ProcessManager", "start", err))?;
        Ok(ProcessHandle { pid: child.id() })
    }

    fn terminate(&self, pid: u32) -> PalResult<()> {
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(pid as i32),
            nix::sys::signal::Signal::SIGTERM,
        )
        .map_err(|err| PalError::io("ProcessManager", "terminate", err))
    }

    fn status(&self, pid: u32) -> PalResult<ProcessStatus> {
        let proc_path = format!("/proc/{pid}");
        Ok(ProcessStatus {
            pid,
            running: Path::new(&proc_path).exists(),
            exit_code: None,
        })
    }
}

impl ServiceManager for LinuxAdapter {
    fn install(&self, _name: &str, _executable: &Path) -> PalResult<()> {
        Err(PalError::unsupported("ServiceManager", "install"))
    }

    fn start(&self, name: &str) -> PalResult<()> {
        run_command("systemctl", &["start", name])
    }

    fn stop(&self, name: &str) -> PalResult<()> {
        run_command("systemctl", &["stop", name])
    }

    fn status(&self, name: &str) -> PalResult<String> {
        let output = Command::new("systemctl")
            .args(["is-active", name])
            .output()
            .map_err(|err| PalError::io("ServiceManager", "status", err))?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

impl DiskSpace for LinuxAdapter {
    fn query(&self, path: &Path) -> PalResult<DiskSpaceInfo> {
        let mut stat = std::mem::MaybeUninit::<libc::statvfs>::uninit();
        let c_path =
            std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).map_err(|err| {
                PalError::new(
                    PalErrorKind::InvalidInput,
                    "query",
                    "DiskSpace",
                    err.to_string(),
                )
            })?;
        let rc = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
        if rc != 0 {
            return Err(PalError::io(
                "DiskSpace",
                "query",
                std::io::Error::last_os_error(),
            ));
        }
        let stat = unsafe { stat.assume_init() };
        Ok(DiskSpaceInfo {
            path: path.to_path_buf(),
            total_bytes: stat.f_blocks.saturating_mul(stat.f_frsize),
            available_bytes: stat.f_bavail.saturating_mul(stat.f_frsize),
        })
    }
}

impl NetworkInfo for LinuxAdapter {
    fn interfaces(&self) -> PalResult<Vec<NetworkInterface>> {
        interface_names().map(|names| {
            names
                .into_iter()
                .map(|name| NetworkInterface {
                    is_loopback: name == "lo",
                    is_up: true,
                    name,
                    addresses: Vec::new(),
                })
                .collect()
        })
    }
}

impl DnsResolver for LinuxAdapter {
    fn resolve(&self, host: &str) -> PalResult<Vec<String>> {
        (host, 0)
            .to_socket_addrs()
            .map_err(|err| PalError::io("DnsResolver", "resolve", err))
            .map(|addrs| addrs.map(|addr| addr.ip().to_string()).collect())
    }
}

impl NetStat for LinuxAdapter {
    fn collect(&self) -> PalResult<NetworkCounterSnapshot> {
        let snmp = std::fs::read_to_string("/proc/net/snmp")
            .map_err(|err| PalError::io("NetStat", "read_snmp", err))?;
        let dev = std::fs::read_to_string("/proc/net/dev")
            .map_err(|err| PalError::io("NetStat", "read_dev", err))?;

        let udp = parse_protocol_section(&snmp, "Udp");
        let tcp = parse_protocol_section(&snmp, "Tcp");
        let datagrams_discarded = udp.get("NoPorts").copied().unwrap_or_default()
            + udp.get("RcvbufErrors").copied().unwrap_or_default()
            + udp.get("SndbufErrors").copied().unwrap_or_default();

        Ok(NetworkCounterSnapshot {
            datagrams_received: udp.get("InDatagrams").copied().unwrap_or_default(),
            datagrams_sent: udp.get("OutDatagrams").copied().unwrap_or_default(),
            datagrams_discarded,
            datagrams_with_errors: udp.get("InErrors").copied().unwrap_or_default(),
            segments_received: tcp.get("InSegs").copied().unwrap_or_default(),
            segments_sent: tcp.get("OutSegs").copied().unwrap_or_default(),
            errors_received: tcp.get("InErrs").copied().unwrap_or_default(),
            current_connections: tcp.get("CurrEstab").copied().unwrap_or_default(),
            reset_connections: tcp.get("EstabResets").copied().unwrap_or_default(),
            interface_counters: parse_dev(&dev),
        })
    }
}

impl SystemControl for LinuxAdapter {
    fn reboot(&self, force: bool) -> PalResult<()> {
        let mut args = vec!["-r", "now"];
        if force {
            args.insert(0, "--force");
        }
        run_command("shutdown", &args)
    }

    fn shutdown(&self) -> PalResult<()> {
        run_command("shutdown", &["now"])
    }
}

impl TimeService for LinuxAdapter {
    fn now(&self) -> PalResult<SystemTime> {
        Ok(SystemTime::now())
    }
}

impl EnvVars for LinuxAdapter {
    fn get(&self, key: &str) -> PalResult<Option<String>> {
        Ok(std::env::var(key).ok())
    }
}

impl IpcServer for LinuxAdapter {
    fn bind(&self, endpoint: &str) -> PalResult<()> {
        let path = Path::new(endpoint);
        if path.exists() {
            std::fs::remove_file(path).map_err(|err| PalError::io("IpcServer", "remove", err))?;
        }
        let _listener = std::os::unix::net::UnixListener::bind(path)
            .map_err(|err| PalError::io("IpcServer", "bind", err))?;
        Ok(())
    }
}

impl IpcClient for LinuxAdapter {
    fn connect(&self, endpoint: &str) -> PalResult<()> {
        let _stream = std::os::unix::net::UnixStream::connect(endpoint)
            .map_err(|err| PalError::io("IpcClient", "connect", err))?;
        Ok(())
    }
}

impl SystemLogger for LinuxAdapter {
    fn log(&self, level: &str, message: &str) -> PalResult<()> {
        tracing::info!(target: "pal.linux.system_logger", level, message);
        Ok(())
    }
}

impl ResourceLimiter for LinuxAdapter {
    fn apply_to_pid(&self, pid: u32, limits: &ResourceLimitSpec) -> PalResult<()> {
        if let Some(open_files) = limits.open_files {
            let limit = libc::rlimit {
                rlim_cur: open_files as libc::rlim_t,
                rlim_max: open_files as libc::rlim_t,
            };
            let rc = unsafe {
                libc::prlimit(
                    pid as libc::pid_t,
                    libc::RLIMIT_NOFILE,
                    &limit,
                    std::ptr::null_mut(),
                )
            };
            if rc != 0 {
                return Err(PalError::io(
                    "ResourceLimiter",
                    "apply_open_files",
                    std::io::Error::last_os_error(),
                ));
            }
        }
        Ok(())
    }
}

impl DeviceId for LinuxAdapter {
    fn identity(&self) -> PalResult<DeviceIdentity> {
        let fingerprint = machine_fingerprint();
        Ok(DeviceIdentity {
            device_id: fingerprint.clone(),
            fingerprint,
        })
    }
}

impl MachineFingerprint for LinuxAdapter {
    fn fingerprint(&self) -> PalResult<String> {
        Ok(machine_fingerprint())
    }
}

pub fn build_context() -> PalResult<PlatformContext> {
    LinuxPlatformBuilder.build()
}

fn run_command(command: &str, args: &[&str]) -> PalResult<()> {
    let status = Command::new(command)
        .args(args)
        .status()
        .map_err(|err| PalError::io("SystemControl", "spawn", err))?;
    if status.success() {
        Ok(())
    } else {
        Err(PalError::new(
            PalErrorKind::Unavailable,
            "run_command",
            "SystemControl",
            format!("{command} exited with {status}"),
        ))
    }
}

fn parse_protocol_section(content: &str, section: &str) -> HashMap<String, u64> {
    let mut map = HashMap::new();
    let mut lines = content.lines().peekable();

    while let Some(header_line) = lines.next() {
        let Some(value_line) = lines.next() else {
            break;
        };

        if !header_line.starts_with(section) || !value_line.starts_with(section) {
            continue;
        }

        for (key, value) in header_line
            .split_whitespace()
            .skip(1)
            .zip(value_line.split_whitespace().skip(1))
        {
            if let Ok(number) = value.parse::<u64>() {
                map.insert(key.to_string(), number);
            }
        }
    }

    map
}

fn parse_dev(content: &str) -> Vec<InterfaceCounterSnapshot> {
    content
        .lines()
        .skip(2)
        .filter_map(|line| {
            let (name, raw_values) = line.split_once(':')?;
            let values = raw_values
                .split_whitespace()
                .filter_map(|value| value.parse::<u64>().ok())
                .collect::<Vec<_>>();
            if values.len() < 16 {
                return None;
            }
            let multicast_received = values[7];
            let received_packets = values[1];
            let sent_packets = values[9];
            Some(InterfaceCounterSnapshot {
                if_name: name.trim().to_string(),
                bytes_received: values[0],
                bytes_sented: values[8],
                unicast_packet_received: received_packets.saturating_sub(multicast_received),
                unicast_packet_sented: sent_packets,
                multicast_packet_received: multicast_received,
                multicast_packet_sented: 0,
            })
        })
        .collect()
}

fn interface_names() -> PalResult<Vec<String>> {
    let entries = std::fs::read_dir("/sys/class/net")
        .map_err(|err| PalError::io("NetworkInfo", "read_dir", err))?;
    Ok(entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect())
}

fn command_in_path(command: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| {
        std::env::split_paths(&paths).any(|dir| {
            let path = dir.join(command);
            path.exists()
                && path
                    .metadata()
                    .map(|meta| meta.mode() & 0o111 != 0)
                    .unwrap_or(false)
        })
    })
}

fn statvfs_available_mb(path: &Path) -> PalResult<u64> {
    let info = LinuxAdapter.query(path)?;
    Ok(info.available_bytes / 1024 / 1024)
}

fn machine_fingerprint() -> String {
    [
        "/sys/class/dmi/id/product_uuid",
        "/etc/machine-id",
        "/var/lib/dbus/machine-id",
    ]
    .iter()
    .find_map(|path| std::fs::read_to_string(path).ok())
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
    .unwrap_or_else(|| "unknown-linux-device".to_string())
}

fn default_key_root() -> std::path::PathBuf {
    std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("cc-rdeviceagent")
        .join("keys")
}
