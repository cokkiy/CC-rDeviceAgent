use anyhow::{Context, Result};
#[cfg(any(target_os = "linux", windows))]
use pal_core::PlatformBuilder;
use pal_core::PlatformContext;
use std::sync::OnceLock;

static PLATFORM_CONTEXT: OnceLock<Result<PlatformContext, String>> = OnceLock::new();

pub fn context() -> Result<&'static PlatformContext> {
    PLATFORM_CONTEXT
        .get_or_init(|| build_context().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| anyhow::anyhow!("{}", e))
}

pub fn reboot(force: bool) -> Result<()> {
    context()?
        .system_control
        .reboot(force)
        .context("reboot through PAL")
}

pub fn shutdown() -> Result<()> {
    context()?
        .system_control
        .shutdown()
        .context("shutdown through PAL")
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
    nix::unistd::daemon(true, false).map_err(|err| anyhow::anyhow!("daemonize failed: {err}"))
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

#[cfg(target_os = "linux")]
fn build_context() -> Result<PlatformContext> {
    pal_linux::LinuxPlatformBuilder
        .build()
        .context("build linux PAL context")
}

#[cfg(windows)]
fn build_context() -> Result<PlatformContext> {
    pal_windows::WindowsPlatformBuilder
        .build()
        .context("build windows PAL context")
}

#[cfg(all(unix, not(target_os = "linux")))]
fn build_context() -> Result<PlatformContext> {
    let mut profile = pal_core::CapabilityProfile::current_platform();
    profile.has_unix_socket = true;
    Ok(pal_fallback::fallback_context(
        profile,
        std::path::PathBuf::from(".cc-rdeviceagent-keys"),
    ))
}
