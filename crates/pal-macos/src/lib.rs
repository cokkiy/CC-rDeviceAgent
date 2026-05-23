#[cfg(target_os = "macos")]
mod macos {
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
}

#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(not(target_os = "macos"))]
mod non_macos {
    use std::path::PathBuf;

    use pal_core::*;

    #[derive(Debug, Clone, Default)]
    pub struct MacosPlatformBuilder;

    impl PlatformBuilder for MacosPlatformBuilder {
        fn build(&self) -> PalResult<PlatformContext> {
            let mut profile = CapabilityProfile::current_platform();
            profile
                .details
                .insert("pal_macos".to_string(), "unsupported-target".to_string());
            Ok(pal_fallback::fallback_context(
                profile,
                PathBuf::from(".cc-rdeviceagent-keys"),
            ))
        }
    }

    pub fn build_context() -> PalResult<PlatformContext> {
        MacosPlatformBuilder.build()
    }
}

#[cfg(not(target_os = "macos"))]
pub use non_macos::*;
