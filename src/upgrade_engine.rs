//! Upgrade Engine — W2.7 (Application-level prototype)
//!
//! Implements the OTA state machine for application-level upgrades.
//! System-level upgrades (A/B slot, bootloader) are Phase 3.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{info, warn};

// ── upgrade state machine ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpgradeState {
    Idle,
    Received,
    Validated,
    Downloading,
    Verifying,
    PreCheck,
    Staging,
    ReadyToActivate,
    Activating,
    PostCheck,
    Committed,
    RollingBack,
    RolledBack,
    Failed { reason: String },
}

impl std::fmt::Display for UpgradeState {
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

// ── upgrade package manifest ──────────────────────────────────────────────

/// `manifest.json` embedded in every `tar.zst` upgrade package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeManifest {
    pub app_id: String,
    pub app_name: String,
    pub from_version: String,
    pub to_version: String,
    /// Ed25519 public key (base64) used to verify `signature`.
    pub public_key_b64: String,
    /// Ed25519 signature (base64) over the package SHA-256 hash.
    pub signature_b64: String,
    /// SHA-256 hex digest of the `tar.zst` archive (excluding manifest).
    pub package_sha256: String,
    /// Monotonically increasing build number — prevents rollback to older builds.
    pub build_number: u64,
    /// Optional script run before activation.
    pub pre_activate_script: Option<String>,
    /// Optional script run after activation (health gate).
    pub post_activate_script: Option<String>,
}

// ── upgrade strategy trait ────────────────────────────────────────────────

/// Abstracts upgrade strategy — application, config, or (future) system.
#[async_trait::async_trait]
pub trait UpgradeStrategy: Send + Sync {
    async fn stage(
        &self,
        manifest: &UpgradeManifest,
        package_path: &Path,
        staging_dir: &Path,
    ) -> Result<()>;

    async fn activate(&self, manifest: &UpgradeManifest, staging_dir: &Path) -> Result<()>;

    async fn rollback(&self, manifest: &UpgradeManifest) -> Result<()>;

    async fn post_check(&self, manifest: &UpgradeManifest) -> Result<()> {
        Ok(()) // default: pass
    }
}

// ── application upgrade strategy ─────────────────────────────────────────

/// Upgrades a binary payload application by:
/// 1. Extracting the staging directory
/// 2. Backing up the current install
/// 3. Swapping staging → active
/// 4. Running post-check (process health)
pub struct AppUpgradeStrategy {
    pub install_root: PathBuf,
    pub backup_root: PathBuf,
}

#[async_trait::async_trait]
impl UpgradeStrategy for AppUpgradeStrategy {
    async fn stage(
        &self,
        manifest: &UpgradeManifest,
        package_path: &Path,
        staging_dir: &Path,
    ) -> Result<()> {
        // In real code: decompress tar.zst and verify SHA-256.
        // For prototype: just create the staging directory.
        fs::create_dir_all(staging_dir)
            .await
            .context("create staging dir")?;

        info!(
            app_id = %manifest.app_id,
            to_version = %manifest.to_version,
            staging = %staging_dir.display(),
            "Staged upgrade"
        );
        Ok(())
    }

    async fn activate(&self, manifest: &UpgradeManifest, staging_dir: &Path) -> Result<()> {
        let install_dir = self.install_root.join(&manifest.app_id);
        let backup_dir = self
            .backup_root
            .join(&manifest.app_id)
            .join(&manifest.from_version);

        // Backup existing install if it exists
        if install_dir.exists() {
            fs::create_dir_all(&backup_dir)
                .await
                .context("create backup dir")?;
            copy_dir_recursive(&install_dir, &backup_dir).await?;
            info!(
                app_id = %manifest.app_id,
                backup = %backup_dir.display(),
                "Backed up current install"
            );
        }

        // Move staging → install
        if install_dir.exists() {
            fs::remove_dir_all(&install_dir)
                .await
                .context("remove old install")?;
        }
        fs::rename(staging_dir, &install_dir)
            .await
            .context("move staging to install")?;

        info!(
            app_id = %manifest.app_id,
            to_version = %manifest.to_version,
            "Activated upgrade"
        );
        Ok(())
    }

    async fn rollback(&self, manifest: &UpgradeManifest) -> Result<()> {
        let install_dir = self.install_root.join(&manifest.app_id);
        let backup_dir = self
            .backup_root
            .join(&manifest.app_id)
            .join(&manifest.from_version);

        if !backup_dir.exists() {
            return Err(anyhow!("no backup found at {}", backup_dir.display()));
        }

        if install_dir.exists() {
            fs::remove_dir_all(&install_dir)
                .await
                .context("remove failed install")?;
        }
        copy_dir_recursive(&backup_dir, &install_dir).await?;

        warn!(
            app_id = %manifest.app_id,
            to_version = %manifest.from_version,
            "Rolled back upgrade"
        );
        Ok(())
    }
}

// ── upgrade engine ────────────────────────────────────────────────────────

pub struct UpgradeEngine<S: UpgradeStrategy> {
    strategy: S,
    staging_root: PathBuf,
    state: UpgradeState,
    current_manifest: Option<UpgradeManifest>,
}

impl<S: UpgradeStrategy> UpgradeEngine<S> {
    pub fn new(strategy: S, staging_root: PathBuf) -> Self {
        Self {
            strategy,
            staging_root,
            state: UpgradeState::Idle,
            current_manifest: None,
        }
    }

    pub fn state(&self) -> &UpgradeState {
        &self.state
    }

    /// Run a full application upgrade lifecycle.
    pub async fn run(&mut self, manifest: UpgradeManifest, package_path: &Path) -> Result<()> {
        self.state = UpgradeState::Received;
        self.current_manifest = Some(manifest.clone());

        // Validate manifest
        self.state = UpgradeState::Validated;
        validate_manifest(&manifest)?;

        // Downloading (already on disk — caller downloaded via FileTransfer)
        self.state = UpgradeState::Downloading;

        // Verify package integrity
        self.state = UpgradeState::Verifying;
        verify_package(&manifest, package_path)?;

        // Pre-check
        self.state = UpgradeState::PreCheck;

        // Stage
        self.state = UpgradeState::Staging;
        let staging_dir = self.staging_root.join(&manifest.app_id);
        if let Err(e) = self.strategy.stage(&manifest, package_path, &staging_dir).await {
            self.state = UpgradeState::Failed { reason: e.to_string() };
            return Err(e);
        }

        // Activate
        self.state = UpgradeState::ReadyToActivate;
        self.state = UpgradeState::Activating;
        if let Err(e) = self.strategy.activate(&manifest, &staging_dir).await {
            self.state = UpgradeState::RollingBack;
            let _ = self.strategy.rollback(&manifest).await;
            self.state = UpgradeState::RolledBack;
            return Err(e);
        }

        // Post-check
        self.state = UpgradeState::PostCheck;
        if let Err(e) = self.strategy.post_check(&manifest).await {
            self.state = UpgradeState::RollingBack;
            let _ = self.strategy.rollback(&manifest).await;
            self.state = UpgradeState::RolledBack;
            return Err(e);
        }

        self.state = UpgradeState::Committed;
        info!(app_id = %manifest.app_id, version = %manifest.to_version, "Upgrade committed");
        Ok(())
    }
}

// ── validation helpers ────────────────────────────────────────────────────

fn validate_manifest(m: &UpgradeManifest) -> Result<()> {
    if m.app_id.is_empty() {
        return Err(anyhow!("manifest: app_id is empty"));
    }
    if m.to_version.is_empty() {
        return Err(anyhow!("manifest: to_version is empty"));
    }
    if m.build_number == 0 {
        return Err(anyhow!("manifest: build_number must be > 0"));
    }
    Ok(())
}

fn verify_package(m: &UpgradeManifest, package_path: &Path) -> Result<()> {
    if !package_path.exists() {
        return Err(anyhow!("package not found: {}", package_path.display()));
    }
    // TODO: verify SHA-256 and Ed25519 signature via Security Center
    // Prototype: accept any existing file
    Ok(())
}

// ── recursive copy helper ─────────────────────────────────────────────────

async fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).await?;
    let mut entries = fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dst_path)).await?;
        } else {
            fs::copy(&src_path, &dst_path).await?;
        }
    }
    Ok(())
}

// ── tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn dummy_manifest(from: &str, to: &str, build: u64) -> UpgradeManifest {
        UpgradeManifest {
            app_id: "test-app".into(),
            app_name: "Test App".into(),
            from_version: from.into(),
            to_version: to.into(),
            public_key_b64: String::new(),
            signature_b64: String::new(),
            package_sha256: String::new(),
            build_number: build,
            pre_activate_script: None,
            post_activate_script: None,
        }
    }

    struct NoopStrategy;

    #[async_trait::async_trait]
    impl UpgradeStrategy for NoopStrategy {
    async fn stage(
        &self,
        manifest: &UpgradeManifest,
        package_path: &Path,
        staging: &Path,
    ) -> Result<()> {
        fs::create_dir_all(staging).await?;
        Ok(())
    }
    async fn activate(&self, _m: &UpgradeManifest, _staging: &Path) -> Result<()> {
        Ok(())
    }
    async fn rollback(&self, _m: &UpgradeManifest) -> Result<()> {
        Ok(())
    }
    }

    #[tokio::test]
    async fn full_upgrade_lifecycle() {
        let tmp = env::temp_dir().join("cc_upgrade_test");
        let pkg = tmp.join("package.tar.zst");
        let staging = tmp.join("staging");

        fs::create_dir_all(&tmp).await.unwrap();
        fs::write(&pkg, b"dummy package").await.unwrap();

        let mut engine = UpgradeEngine::new(NoopStrategy, staging.clone());
        let manifest = dummy_manifest("1.0.0", "1.1.0", 2);

        engine.run(manifest, &pkg).await.unwrap();
        assert_eq!(*engine.state(), UpgradeState::Committed);
    }

    #[tokio::test]
    async fn invalid_manifest_fails() {
        let mut engine = UpgradeEngine::new(NoopStrategy, PathBuf::from("/tmp"));
        let bad = UpgradeManifest {
            app_id: String::new(), // invalid
            app_name: String::new(),
            from_version: "1.0".into(),
            to_version: "1.1".into(),
            public_key_b64: String::new(),
            signature_b64: String::new(),
            package_sha256: String::new(),
            build_number: 1,
            pre_activate_script: None,
            post_activate_script: None,
        };
        let res = engine.run(bad, Path::new("/nonexistent")).await;
        assert!(res.is_err());
    }

    #[test]
    fn state_display() {
        assert_eq!(UpgradeState::Committed.to_string(), "committed");
        assert_eq!(
            UpgradeState::Failed { reason: "disk full".into() }.to_string(),
            "failed(disk full)"
        );
    }
}
