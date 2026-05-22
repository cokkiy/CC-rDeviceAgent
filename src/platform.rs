use std::process::Command;

use anyhow::{Context, Result, anyhow};

pub fn reboot(force: bool) -> Result<()> {
    #[cfg(windows)]
    {
        let mut args = vec!["/r", "/t", "0"];
        if force {
            args.insert(1, "/f");
        }
        run_command("shutdown", &args)
    }

    #[cfg(unix)]
    {
        let _ = force;
        run_command("shutdown", &["-r", "now"])
    }
}

pub fn shutdown() -> Result<()> {
    #[cfg(windows)]
    {
        run_command("shutdown", &["/s", "/t", "0"])
    }

    #[cfg(unix)]
    {
        run_command("shutdown", &["now"])
    }
}

#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "solaris",
    target_os = "illumos"
))]
pub fn daemonize() -> Result<()> {
    nix::unistd::daemon(true, false).map_err(|err| anyhow!("daemonize failed: {err}"))
}

#[cfg(target_os = "macos")]
pub fn daemonize() -> Result<()> {
    let result = unsafe { libc::daemon(1, 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error()).context("daemonize failed")
    }
}

#[cfg(not(unix))]
pub fn daemonize() -> Result<()> {
    Ok(())
}

fn run_command(command: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(command)
        .args(args)
        .status()
        .with_context(|| format!("spawn {command}"))?;

    if status.success() {
        return Ok(());
    }

    Err(anyhow!("{command} exited with {status}"))
}
