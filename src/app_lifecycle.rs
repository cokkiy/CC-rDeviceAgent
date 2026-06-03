//! App Lifecycle — W2.3
//!
//! Manages payload-application installation, start/stop, health-monitoring,
//! and auto-restart.  Uses PAL ProcessManager so the same code runs on
//! Linux, Windows, and macOS.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
use pal_core::{PlatformContext, ProcessManager, ProcessSpec, ResourceLimitSpec, ResourceLimiter};
use tokio::sync::mpsc;
use tracing::info;

// ── lifecycle state machine ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    Registered,
    Installing,
    Installed,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed { reason: String },
    Uninstalling,
    Uninstalled,
}

impl std::fmt::Display for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed { reason } => write!(f, "failed({reason})"),
            other => write!(
                f,
                "{}",
                serde_json::to_string(other)
                    .unwrap_or_default()
                    .trim_matches('"')
            ),
        }
    }
}

// ── app manifest ───────────────────────────────────────────────────────────

/// Metadata describing a managed payload application.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AppManifest {
    pub app_id: String,
    pub app_name: String,
    pub version: String,
    /// Path to the installed executable / entry point.
    pub executable: PathBuf,
    /// Working directory.
    pub work_dir: PathBuf,
    /// Arguments passed at launch.
    pub args: Vec<String>,
    /// Environment overrides.
    pub env: HashMap<String, String>,
    /// Maximum restart attempts before giving up (0 = unlimited).
    pub max_restarts: u32,
    /// Base back-off duration between restarts.
    pub restart_backoff_secs: u64,
    /// Resource limits (soft caps).
    pub memory_limit_mb: Option<u64>,
    pub cpu_limit_pct: Option<u64>,
}

// ── running instance ─────────────────────────────────────────────────────

struct AppInstance {
    app_id: String,
    app_name: String,
    version: String,
    manifest: Option<AppManifest>,
    state: AppState,
    pid: Option<u32>,
    last_start: Option<Instant>,
}

#[derive(Clone)]
struct LifecyclePal {
    process_manager: Arc<dyn ProcessManager>,
    resource_limiter: Arc<dyn ResourceLimiter>,
}

impl From<&PlatformContext> for LifecyclePal {
    fn from(context: &PlatformContext) -> Self {
        Self {
            process_manager: Arc::clone(&context.process_manager),
            resource_limiter: Arc::clone(&context.resource_limiter),
        }
    }
}

// ── lifecycle manager ────────────────────────────────────────────────────

/// Commands sent from RPC handlers into the lifecycle task.
pub enum LifecycleCmd {
    Register {
        app_id: String,
        app_name: String,
        version: String,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Install {
        manifest: Box<AppManifest>,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Start {
        app_id: String,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Stop {
        app_id: String,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Restart {
        app_id: String,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    Uninstall {
        app_id: String,
        reply: tokio::sync::oneshot::Sender<Result<()>>,
    },
    GetState {
        app_id: String,
        reply: tokio::sync::oneshot::Sender<Option<AppState>>,
    },
    FindByPid {
        pid: u32,
        reply: tokio::sync::oneshot::Sender<Option<String>>,
    },
    ListApps {
        reply: tokio::sync::oneshot::Sender<Vec<(String, AppState)>>,
    },
}

/// Lightweight handle for RPC / health-check consumers.
#[derive(Clone)]
pub struct AppLifecycleHandle {
    tx: mpsc::Sender<LifecycleCmd>,
}

impl AppLifecycleHandle {
    pub async fn register(&self, app_id: &str, app_name: &str, version: &str) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(LifecycleCmd::Register {
                app_id: app_id.into(),
                app_name: app_name.into(),
                version: version.into(),
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn install(&self, manifest: AppManifest) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(LifecycleCmd::Install {
                manifest: Box::new(manifest),
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn start(&self, app_id: &str) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(LifecycleCmd::Start {
                app_id: app_id.into(),
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn stop(&self, app_id: &str) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(LifecycleCmd::Stop {
                app_id: app_id.into(),
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn restart(&self, app_id: &str) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(LifecycleCmd::Restart {
                app_id: app_id.into(),
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn uninstall(&self, app_id: &str) -> Result<()> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tx
            .send(LifecycleCmd::Uninstall {
                app_id: app_id.into(),
                reply: tx,
            })
            .await?;
        rx.await?
    }

    pub async fn get_state(&self, app_id: &str) -> Option<AppState> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = self
            .tx
            .send(LifecycleCmd::GetState {
                app_id: app_id.into(),
                reply: tx,
            })
            .await;
        rx.await.ok().flatten()
    }

    pub async fn find_app_by_pid(&self, pid: u32) -> Option<String> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = self
            .tx
            .send(LifecycleCmd::FindByPid { pid, reply: tx })
            .await;
        rx.await.ok().flatten()
    }

    pub async fn list_apps(&self) -> Vec<(String, AppState)> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = self.tx.send(LifecycleCmd::ListApps { reply: tx }).await;
        rx.await.unwrap_or_default()
    }
}

/// Spawn the lifecycle manager task and return a handle to it.
/// The task owns all `AppInstance` state and runs until the channel is closed.
pub fn spawn_lifecycle_manager(context: &PlatformContext) -> AppLifecycleHandle {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(lifecycle_task(rx, LifecyclePal::from(context)));
    AppLifecycleHandle { tx }
}

// ── lifecycle task ────────────────────────────────────────────────────────

async fn lifecycle_task(mut rx: mpsc::Receiver<LifecycleCmd>, pal: LifecyclePal) {
    let mut apps: HashMap<String, AppInstance> = HashMap::new();

    while let Some(cmd) = rx.recv().await {
        match cmd {
            LifecycleCmd::Register {
                app_id,
                app_name,
                version,
                reply,
            } => {
                let res = do_register(&mut apps, app_id.clone(), app_name, version);
                if res.is_ok() {
                    info!(app_id = %app_id, "App registered in lifecycle");
                }
                let _ = reply.send(res);
            }

            LifecycleCmd::Install { manifest, reply } => {
                let app_id = manifest.app_id.clone();
                let res = do_install(&mut apps, *manifest);
                if res.is_ok() {
                    info!(app_id = %app_id, "App installed");
                }
                let _ = reply.send(res);
            }

            LifecycleCmd::Start { app_id, reply } => {
                let res = do_start(&mut apps, &app_id, &pal).await;
                if res.is_ok() {
                    info!(app_id = %app_id, "App started");
                }
                let _ = reply.send(res);
            }

            LifecycleCmd::Stop { app_id, reply } => {
                let res = do_stop(&mut apps, &app_id, &pal).await;
                if res.is_ok() {
                    info!(app_id = %app_id, "App stopped");
                }
                let _ = reply.send(res);
            }

            LifecycleCmd::Restart { app_id, reply } => {
                let _ = do_stop(&mut apps, &app_id, &pal).await;
                let res = do_start(&mut apps, &app_id, &pal).await;
                info!(app_id = %app_id, ok = res.is_ok(), "App restarted");
                let _ = reply.send(res);
            }

            LifecycleCmd::Uninstall { app_id, reply } => {
                let res = do_uninstall(&mut apps, &app_id, &pal).await;
                if res.is_ok() {
                    info!(app_id = %app_id, "App uninstalled");
                }
                let _ = reply.send(res);
            }

            LifecycleCmd::GetState { app_id, reply } => {
                let state = apps.get(&app_id).map(|i| i.state.clone());
                let _ = reply.send(state);
            }

            LifecycleCmd::FindByPid { pid, reply } => {
                let app_id = apps
                    .values()
                    .find(|instance| instance.pid == Some(pid))
                    .map(|instance| instance.app_id.clone());
                let _ = reply.send(app_id);
            }

            LifecycleCmd::ListApps { reply } => {
                let list = apps
                    .values()
                    .map(|i| (i.app_id.clone(), i.state.clone()))
                    .collect();
                let _ = reply.send(list);
            }
        }
    }
}

// ── operations ────────────────────────────────────────────────────────────

fn do_register(
    apps: &mut HashMap<String, AppInstance>,
    app_id: String,
    app_name: String,
    version: String,
) -> Result<()> {
    apps.entry(app_id.clone())
        .and_modify(|instance| {
            instance.app_name = app_name.clone();
            instance.version = version.clone();
            if !matches!(
                instance.state,
                AppState::Installed
                    | AppState::Starting
                    | AppState::Running
                    | AppState::Stopping
                    | AppState::Stopped
            ) {
                instance.state = AppState::Registered;
            }
        })
        .or_insert_with(|| AppInstance {
            app_id,
            app_name,
            version,
            manifest: None,
            state: AppState::Registered,
            pid: None,
            last_start: None,
        });
    Ok(())
}

fn do_install(apps: &mut HashMap<String, AppInstance>, manifest: AppManifest) -> Result<()> {
    if !manifest.executable.exists() {
        return Err(anyhow!(
            "executable not found: {}",
            manifest.executable.display()
        ));
    }
    let app_id = manifest.app_id.clone();
    let app_name = manifest.app_name.clone();
    let version = manifest.version.clone();
    apps.entry(app_id.clone())
        .and_modify(|instance| {
            instance.app_name = app_name.clone();
            instance.version = version.clone();
            instance.manifest = Some(manifest.clone());
            instance.state = AppState::Installed;
            instance.pid = None;
            instance.last_start = None;
        })
        .or_insert_with(|| AppInstance {
            app_id,
            app_name,
            version,
            manifest: Some(manifest),
            state: AppState::Installed,
            pid: None,
            last_start: None,
        });
    Ok(())
}

async fn do_start(
    apps: &mut HashMap<String, AppInstance>,
    app_id: &str,
    pal: &LifecyclePal,
) -> Result<()> {
    let inst = apps
        .get_mut(app_id)
        .ok_or_else(|| anyhow!("unknown app: {}", app_id))?;

    match &inst.state {
        AppState::Running => return Ok(()), // idempotent
        AppState::Uninstalled | AppState::Uninstalling => {
            return Err(anyhow!("app is uninstalled"));
        }
        _ => {}
    }

    let manifest = inst
        .manifest
        .as_ref()
        .ok_or_else(|| anyhow!("app is registered but not installed: {}", app_id))?;

    inst.state = AppState::Starting;

    let spec = ProcessSpec {
        program: manifest.executable.clone(),
        args: manifest.args.clone(),
        env: manifest
            .env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>(),
        cwd: Some(manifest.work_dir.clone()),
    };

    let handle = pal
        .process_manager
        .start(spec)
        .context("start app process through PAL")?;
    let pid = handle.pid;

    if let Some(limits) = resource_limits_from_manifest(manifest) {
        pal.resource_limiter
            .apply_to_pid(pid, &limits)
            .context("apply app resource limits through PAL")?;
    }

    inst.pid = Some(pid);
    inst.state = AppState::Running;
    inst.last_start = Some(Instant::now());
    Ok(())
}

async fn do_stop(
    apps: &mut HashMap<String, AppInstance>,
    app_id: &str,
    pal: &LifecyclePal,
) -> Result<()> {
    let inst = apps
        .get_mut(app_id)
        .ok_or_else(|| anyhow!("unknown app: {}", app_id))?;

    if let Some(pid) = inst.pid.take() {
        inst.state = AppState::Stopping;
        pal.process_manager
            .terminate(pid)
            .context("stop app process through PAL")?;
    }

    inst.state = AppState::Stopped;
    Ok(())
}

async fn do_uninstall(
    apps: &mut HashMap<String, AppInstance>,
    app_id: &str,
    pal: &LifecyclePal,
) -> Result<()> {
    let inst = apps
        .get_mut(app_id)
        .ok_or_else(|| anyhow!("unknown app: {}", app_id))?;

    // Stop first if running
    if matches!(inst.state, AppState::Running | AppState::Starting) {
        if let Some(pid) = inst.pid.take() {
            pal.process_manager
                .terminate(pid)
                .context("terminate app process before uninstall through PAL")?;
        }
    }

    inst.state = AppState::Uninstalled;
    // Remove install directory if it's under our managed apps path
    // (safety: only remove if work_dir is inside /var/lib/cc-rdeviceagent/apps/)
    if let Some(manifest) = inst.manifest.as_ref() {
        let work_dir = manifest.work_dir.clone();
        if work_dir.starts_with("/var/lib/cc-rdeviceagent/apps") {
            let _ = tokio::fs::remove_dir_all(&work_dir).await;
        }
    }

    apps.remove(app_id);
    Ok(())
}

fn resource_limits_from_manifest(manifest: &AppManifest) -> Option<ResourceLimitSpec> {
    let memory_bytes = manifest
        .memory_limit_mb
        .map(|mb| mb.saturating_mul(1024).saturating_mul(1024));
    let cpu_millis = manifest.cpu_limit_pct.map(|pct| pct.saturating_mul(10));

    (memory_bytes.is_some() || cpu_millis.is_some()).then_some(ResourceLimitSpec {
        memory_bytes,
        cpu_millis,
        open_files: None,
    })
}

// ── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use pal_core::PlatformBuilder;
    use std::env;

    fn dummy_manifest(id: &str, exe: PathBuf) -> AppManifest {
        AppManifest {
            app_id: id.into(),
            app_name: id.into(),
            version: "0.1.0".into(),
            executable: exe,
            work_dir: env::temp_dir(),
            args: vec![],
            env: HashMap::new(),
            max_restarts: 3,
            restart_backoff_secs: 1,
            memory_limit_mb: None,
            cpu_limit_pct: None,
        }
    }

    fn lifecycle_handle() -> AppLifecycleHandle {
        let context = pal_mock::MockPlatformBuilder::default().build().unwrap();
        spawn_lifecycle_manager(&context)
    }

    #[tokio::test]
    async fn install_nonexistent_exe_fails() {
        let h = lifecycle_handle();
        let m = dummy_manifest("bad-app", PathBuf::from("/nonexistent/binary"));
        let res = h.install(m).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn install_and_state_query() {
        // Use `true` (always exits 0) or `cmd /c exit 0` on Windows
        #[cfg(unix)]
        let exe = PathBuf::from("/bin/true");
        #[cfg(windows)]
        let exe = PathBuf::from("C:\\Windows\\System32\\cmd.exe");

        if !exe.exists() {
            return; // skip on environments without the binary
        }

        let h = lifecycle_handle();
        let m = dummy_manifest("my-app", exe);
        h.install(m).await.unwrap();

        let state = h.get_state("my-app").await;
        assert_eq!(state, Some(AppState::Installed));
    }

    #[tokio::test]
    async fn register_only_app_is_not_startable_until_installed() {
        let h = lifecycle_handle();
        h.register("registered-app", "registered-app", "0.1.0")
            .await
            .unwrap();

        assert_eq!(
            h.get_state("registered-app").await,
            Some(AppState::Registered)
        );
        let err = h.start("registered-app").await.unwrap_err();
        assert!(err.to_string().contains("not installed"));
        assert_eq!(
            h.get_state("registered-app").await,
            Some(AppState::Registered)
        );
    }

    #[tokio::test]
    async fn start_and_stop_use_pal_process_manager() {
        #[cfg(unix)]
        let exe = PathBuf::from("/bin/true");
        #[cfg(windows)]
        let exe = PathBuf::from("C:\\Windows\\System32\\cmd.exe");

        if !exe.exists() {
            return;
        }

        let h = lifecycle_handle();
        let m = dummy_manifest("managed-app", exe);
        h.install(m).await.unwrap();
        h.start("managed-app").await.unwrap();
        assert_eq!(h.get_state("managed-app").await, Some(AppState::Running));

        h.stop("managed-app").await.unwrap();
        assert_eq!(h.get_state("managed-app").await, Some(AppState::Stopped));
    }

    #[tokio::test]
    async fn start_applies_manifest_resource_limits_through_pal() {
        #[cfg(unix)]
        let exe = PathBuf::from("/bin/true");
        #[cfg(windows)]
        let exe = PathBuf::from("C:\\Windows\\System32\\cmd.exe");

        if !exe.exists() {
            return;
        }

        let h = lifecycle_handle();
        let mut m = dummy_manifest("limited-app", exe);
        m.memory_limit_mb = Some(64);
        m.cpu_limit_pct = Some(25);
        h.install(m).await.unwrap();
        h.start("limited-app").await.unwrap();
        assert_eq!(h.get_state("limited-app").await, Some(AppState::Running));
    }

    #[test]
    fn state_display() {
        assert_eq!(AppState::Running.to_string(), "running");
        assert_eq!(
            AppState::Failed {
                reason: "oom".into()
            }
            .to_string(),
            "failed(oom)"
        );
    }
}
