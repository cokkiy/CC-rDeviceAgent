use std::path::PathBuf;

use pal_core::*;

#[derive(Debug, Clone, Default)]
pub struct MacosPlatformBuilder;

impl PlatformBuilder for MacosPlatformBuilder {
    fn build(&self) -> PalResult<PlatformContext> {
        let mut profile = CapabilityProfile::current_platform();
        profile.has_unix_socket = true;
        profile.has_os_keyring = true;
        Ok(pal_fallback::fallback_context(
            profile,
            PathBuf::from(".cc-rdeviceagent-keys"),
        ))
    }
}

pub fn build_context() -> PalResult<PlatformContext> {
    MacosPlatformBuilder.build()
}
