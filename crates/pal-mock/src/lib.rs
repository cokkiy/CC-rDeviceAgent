use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use pal_core::*;

#[derive(Debug, Clone, Default)]
pub struct MockPlatformBuilder {
    pub profile: CapabilityProfile,
}

impl PlatformBuilder for MockPlatformBuilder {
    fn build(&self) -> PalResult<PlatformContext> {
        let mut profile = self.profile.clone();
        if profile.platform.is_empty() {
            profile.platform = "mock".to_string();
        }
        let mut context = pal_fallback::fallback_context(profile, PathBuf::from(".mock-keys"));
        let process = Arc::new(MockProcessManager::default());
        context.process_manager = process;
        context.resource_limiter = Arc::new(MockResourceLimiter);
        Ok(context)
    }
}

#[derive(Debug, Default)]
pub struct MockProcessManager {
    next_pid: Mutex<u32>,
    processes: Mutex<BTreeMap<u32, ProcessSpec>>,
}

impl ProcessManager for MockProcessManager {
    fn start(&self, spec: ProcessSpec) -> PalResult<ProcessHandle> {
        let mut next_pid = self.next_pid.lock().map_err(|_| {
            PalError::new(
                PalErrorKind::Internal,
                "start",
                "MockProcessManager",
                "pid lock poisoned",
            )
        })?;
        *next_pid = next_pid.saturating_add(1).max(1);
        let pid = *next_pid;
        self.processes
            .lock()
            .map_err(|_| {
                PalError::new(
                    PalErrorKind::Internal,
                    "start",
                    "MockProcessManager",
                    "process lock poisoned",
                )
            })?
            .insert(pid, spec);
        Ok(ProcessHandle { pid })
    }

    fn terminate(&self, pid: u32) -> PalResult<()> {
        self.processes
            .lock()
            .map_err(|_| {
                PalError::new(
                    PalErrorKind::Internal,
                    "terminate",
                    "MockProcessManager",
                    "process lock poisoned",
                )
            })?
            .remove(&pid);
        Ok(())
    }

    fn status(&self, pid: u32) -> PalResult<ProcessStatus> {
        let running = self
            .processes
            .lock()
            .map_err(|_| {
                PalError::new(
                    PalErrorKind::Internal,
                    "status",
                    "MockProcessManager",
                    "process lock poisoned",
                )
            })?
            .contains_key(&pid);
        Ok(ProcessStatus {
            pid,
            running,
            exit_code: None,
        })
    }
}

#[derive(Debug, Default)]
pub struct MockResourceLimiter;

impl ResourceLimiter for MockResourceLimiter {
    fn apply_to_pid(&self, _pid: u32, _limits: &ResourceLimitSpec) -> PalResult<()> {
        Ok(())
    }
}
