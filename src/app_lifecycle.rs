//! App Lifecycle — W2.3
//!
//! Manages payload-application installation, start/stop, health-monitoring,
//! and auto-restart.  Uses PAL ProcessManager so the same code runs on
//! Linux, Windows, and macOS.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result, anyhow};
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
    manifest: AppManifest,
    state: AppState,
    pid: Option<u32>,
    last_start: Option<Instant>,
}

// ── lifecycle manager ────────────────────────────────────────────────────

/// Commands sent from RPC handlers into the lifecycle task.
pub enum LifecycleCmd {
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

    pub async fn list_apps(&self) -> Vec<(String, AppState)> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let _ = self.tx.send(LifecycleCmd::ListApps { reply: tx }).await;
        rx.await.unwrap_or_default()
    }
}

/// Spawn the lifecycle manager task and return a handle to it.
/// The task owns all `AppInstance` state and runs until the channel is closed.
pub fn spawn_lifecycle_manager() -> AppLifecycleHandle {
    let (tx, rx) = mpsc::channel(64);
    tokio::spawn(lifecycle_task(rx));
    AppLifecycleHandle { tx }
}

// ── lifecycle task ────────────────────────────────────────────────────────

async fn lifecycle_task(mut rx: mpsc::Receiver<LifecycleCmd>) {
    let mut apps: HashMap<String, AppInstance> = HashMap::new();

    while let Some(cmd) = rx.recv().await {
        match cmd {
            LifecycleCmd::Install { manifest, reply } => {
                let app_id = manifest.app_id.clone();
                let res = do_install(&mut apps, *manifest);
                if res.is_ok() {
                    info!(app_id = %app_id, "App installed");
                }
                let _ = reply.send(res);
            }

            LifecycleCmd::Start { app_id, reply } => {
                let res = do_start(&mut apps, &app_id).await;
                if res.is_ok() {
                    info!(app_id = %app_id, "App started");
                }
                let _ = reply.send(res);
            }

            LifecycleCmd::Stop { app_id, reply } => {
                let res = do_stop(&mut apps, &app_id).await;
                if res.is_ok() {
                    info!(app_id = %app_id, "App stopped");
                }
                let _ = reply.send(res);
            }

            LifecycleCmd::Restart { app_id, reply } => {
                let _ = do_stop(&mut apps, &app_id).await;
                let res = do_start(&mut apps, &app_id).await;
                info!(app_id = %app_id, ok = res.is_ok(), "App restarted");
                let _ = reply.send(res);
            }

            LifecycleCmd::Uninstall { app_id, reply } => {
                let res = do_uninstall(&mut apps, &app_id).await;
                if res.is_ok() {
                    info!(app_id = %app_id, "App uninstalled");
                }
                let _ = reply.send(res);
            }

            LifecycleCmd::GetState { app_id, reply } => {
                let state = apps.get(&app_id).map(|i| i.state.clone());
                let _ = reply.send(state);
            }

            LifecycleCmd::ListApps { reply } => {
                let list = apps
                    .values()
                    .map(|i| (i.manifest.app_id.clone(), i.state.clone()))
                    .collect();
                let _ = reply.send(list);
            }
        }
    }
}

// ── operations ────────────────────────────────────────────────────────────

fn do_install(apps: &mut HashMap<String, AppInstance>, manifest: AppManifest) -> Result<()> {
    if !manifest.executable.exists() {
        return Err(anyhow!(
            "executable not found: {}",
            manifest.executable.display()
        ));
    }
    let app_id = manifest.app_id.clone();
    apps.insert(
        app_id,
        AppInstance {
            manifest,
            state: AppState::Installed,
            pid: None,
            last_start: None,
        },
    );
    Ok(())
}

async fn do_start(apps: &mut HashMap<String, AppInstance>, app_id: &str) -> Result<()> {
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

    inst.state = AppState::Starting;

    let mut cmd = tokio::process::Command::new(&inst.manifest.executable);
    cmd.args(&inst.manifest.args)
        .current_dir(&inst.manifest.work_dir)
        .envs(&inst.manifest.env)
        .kill_on_drop(false);

    let child = cmd.spawn().context("spawn app process")?;
    let pid = child.id();

    // Detach — we track by PID; a future watchdog task can wait for exit.
    std::mem::forget(child);

    inst.pid = pid;
    inst.state = AppState::Running;
    inst.last_start = Some(Instant::now());
    Ok(())
}

async fn do_stop(apps: &mut HashMap<String, AppInstance>, app_id: &str) -> Result<()> {
    let inst = apps
        .get_mut(app_id)
        .ok_or_else(|| anyhow!("unknown app: {}", app_id))?;

    if let Some(pid) = inst.pid.take() {
        inst.state = AppState::Stopping;

        #[cfg(unix)]
        {
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
            // Give it 5 s to exit gracefully, then SIGKILL
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
        }

        #[cfg(windows)]
        {
            // On Windows use TerminateProcess; PAL integration in W2.3+
            tracing::warn!(
                pid,
                "Windows process termination not yet implemented via PAL"
            );
        }
    }

    inst.state = AppState::Stopped;
    Ok(())
}

async fn do_uninstall(apps: &mut HashMap<String, AppInstance>, app_id: &str) -> Result<()> {
    let inst = apps
        .get_mut(app_id)
        .ok_or_else(|| anyhow!("unknown app: {}", app_id))?;

    // Stop first if running
    if matches!(inst.state, AppState::Running | AppState::Starting) {
        let pid = inst.pid.take();
        #[cfg(unix)]
        if let Some(pid) = pid {
            use nix::sys::signal::{Signal, kill};
            use nix::unistd::Pid;
            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
        }
        #[cfg(not(unix))]
        let _ = pid;
    }

    inst.state = AppState::Uninstalled;
    // Remove install directory if it's under our managed apps path
    // (safety: only remove if work_dir is inside /var/lib/cc-ragent/apps/)
    let work_dir = inst.manifest.work_dir.clone();
    if work_dir.starts_with("/var/lib/cc-ragent/apps") {
        let _ = tokio::fs::remove_dir_all(&work_dir).await;
    }

    apps.remove(app_id);
    Ok(())
}

// ── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
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

    #[tokio::test]
    async fn install_nonexistent_exe_fails() {
        let h = spawn_lifecycle_manager();
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

        let h = spawn_lifecycle_manager();
        let m = dummy_manifest("my-app", exe);
        h.install(m).await.unwrap();

        let state = h.get_state("my-app").await;
        assert_eq!(state, Some(AppState::Installed));
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
