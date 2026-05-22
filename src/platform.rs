use anyhow::{Context, Result, anyhow};
#[cfg(any(target_os = "linux", windows))]
use pal_core::PlatformBuilder;
use pal_core::PlatformContext;
use std::sync::OnceLock;

static PLATFORM_CONTEXT: OnceLock<PlatformContext> = OnceLock::new();

pub fn context() -> Result<&'static PlatformContext> {
    if let Some(context) = PLATFORM_CONTEXT.get() {
        return Ok(context);
    }

    let context = build_context()?;
    PLATFORM_CONTEXT
        .set(context)
        .map_err(|_| anyhow!("platform context already initialized"))?;
    PLATFORM_CONTEXT
        .get()
        .ok_or_else(|| anyhow!("platform context initialization failed"))
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
