#[cfg(windows)]
mod windows_impl {
    use std::path::PathBuf;

    use pal_core::*;

    #[derive(Debug, Clone, Default)]
    pub struct WindowsPlatformBuilder;

    impl PlatformBuilder for WindowsPlatformBuilder {
        fn build(&self) -> PalResult<PlatformContext> {
            let mut profile = CapabilityProfile::current_platform();
            profile.has_named_pipe = true;
            profile.has_os_keyring = true;
            Ok(pal_fallback::fallback_context(
                profile,
                PathBuf::from("cc-rdeviceagent-keys"),
            ))
        }
    }

    pub fn build_context() -> PalResult<PlatformContext> {
        WindowsPlatformBuilder.build()
    }
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
mod non_windows {
    use std::path::PathBuf;

    use pal_core::*;

    #[derive(Debug, Clone, Default)]
    pub struct WindowsPlatformBuilder;

    impl PlatformBuilder for WindowsPlatformBuilder {
        fn build(&self) -> PalResult<PlatformContext> {
            let mut profile = CapabilityProfile::current_platform();
            profile
                .details
                .insert("pal_windows".to_string(), "unsupported-target".to_string());
            Ok(pal_fallback::fallback_context(
                profile,
                PathBuf::from(".cc-rdeviceagent-keys"),
            ))
        }
    }

    pub fn build_context() -> PalResult<PlatformContext> {
        WindowsPlatformBuilder.build()
    }
}

#[cfg(not(windows))]
pub use non_windows::*;
