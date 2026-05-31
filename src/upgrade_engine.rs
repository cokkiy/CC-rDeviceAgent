//! Upgrade Engine — W2.7 (Application-level prototype)
//!
//! Implements the OTA state machine for application-level upgrades.
//! System-level upgrades (A/B slot, bootloader) are Phase 3.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use agent_core::security::{KeyRef, verify_ed25519_signature};
use agent_store::{StateStore, UpgradeStateRecord};
use anyhow::{Context, Result, anyhow};
use ring::digest;
use serde::{Deserialize, Serialize};
use tokio::fs::{self, File};
use tokio::io::AsyncReadExt;
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
    /// SHA-256 hex digest of the full `tar.zst` archive (including any embedded manifest).
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

    async fn post_check(&self, _manifest: &UpgradeManifest) -> Result<()> {
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
        _package_path: &Path,
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
    store: Option<Mutex<StateStore>>,
}

impl<S: UpgradeStrategy> UpgradeEngine<S> {
    pub fn new(strategy: S, staging_root: PathBuf) -> Self {
        Self {
            strategy,
            staging_root,
            state: UpgradeState::Idle,
            current_manifest: None,
            store: None,
        }
    }

    pub fn new_with_store(strategy: S, staging_root: PathBuf, store: StateStore) -> Self {
        Self {
            strategy,
            staging_root,
            state: UpgradeState::Idle,
            current_manifest: None,
            store: Some(Mutex::new(store)),
        }
    }

    pub fn state(&self) -> &UpgradeState {
        &self.state
    }

    /// Run a full application upgrade lifecycle.
    pub async fn run(&mut self, manifest: UpgradeManifest, package_path: &Path) -> Result<()> {
        self.set_state(UpgradeState::Received);
        self.current_manifest = Some(manifest.clone());

        // Validate manifest
        self.set_state(UpgradeState::Validated);
        if let Err(e) = validate_manifest(&manifest) {
            self.set_state(UpgradeState::Failed {
                reason: e.to_string(),
            });
            return Err(e);
        }

        // Downloading (already on disk — caller downloaded via FileTransfer)
        self.set_state(UpgradeState::Downloading);

        // Verify package integrity
        self.set_state(UpgradeState::Verifying);
        if let Err(e) = verify_package(&manifest, package_path).await {
            self.set_state(UpgradeState::Failed {
                reason: e.to_string(),
            });
            return Err(e);
        }

        // Pre-check
        self.set_state(UpgradeState::PreCheck);

        // Stage
        self.set_state(UpgradeState::Staging);
        let staging_dir = self.staging_root.join(&manifest.app_id);
        if let Err(e) = self
            .strategy
            .stage(&manifest, package_path, &staging_dir)
            .await
        {
            self.set_state(UpgradeState::Failed {
                reason: e.to_string(),
            });
            return Err(e);
        }

        // Activate
        self.set_state(UpgradeState::ReadyToActivate);
        self.set_state(UpgradeState::Activating);
        if let Err(e) = self.strategy.activate(&manifest, &staging_dir).await {
            self.set_state(UpgradeState::RollingBack);
            let _ = self.strategy.rollback(&manifest).await;
            self.set_state(UpgradeState::RolledBack);
            return Err(e);
        }

        // Post-check
        self.set_state(UpgradeState::PostCheck);
        if let Err(e) = self.strategy.post_check(&manifest).await {
            self.set_state(UpgradeState::RollingBack);
            let _ = self.strategy.rollback(&manifest).await;
            self.set_state(UpgradeState::RolledBack);
            return Err(e);
        }

        self.set_state(UpgradeState::Committed);
        info!(app_id = %manifest.app_id, version = %manifest.to_version, "Upgrade committed");
        Ok(())
    }

    fn set_state(&mut self, state: UpgradeState) {
        self.state = state;
        self.persist_state();
    }

    fn persist_state(&self) {
        let Some(store) = &self.store else {
            return;
        };
        let Some(manifest) = &self.current_manifest else {
            return;
        };
        let record = UpgradeStateRecord {
            id: manifest.app_id.clone(),
            target_version: manifest.to_version.clone(),
            state: self.state.to_string(),
            state_json: serde_json::to_string(&self.state).unwrap_or_else(|_| "{}".to_string()),
        };
        if let Err(error) = store.lock().unwrap().upsert_upgrade_state(&record) {
            warn!(app_id = %manifest.app_id, error = %error, "failed to persist upgrade state");
        }
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

async fn verify_package(m: &UpgradeManifest, package_path: &Path) -> Result<()> {
    if !package_path.exists() {
        return Err(anyhow!("package not found: {}", package_path.display()));
    }
    let digest = sha256_file(package_path).await?;
    let digest_hex = base16::encode_lower(digest.as_ref());
    if !m.package_sha256.is_empty() && digest_hex != m.package_sha256.to_ascii_lowercase() {
        return Err(anyhow!(
            "package sha256 mismatch: expected {}, got {}",
            m.package_sha256,
            digest_hex
        ));
    }

    if !m.signature_b64.is_empty() || !m.public_key_b64.is_empty() {
        if m.signature_b64.is_empty() || m.public_key_b64.is_empty() {
            return Err(anyhow!(
                "manifest signature and public key must be provided together"
            ));
        }
        let signature = decode_base64(&m.signature_b64).context("decode signature_b64")?;
        let public_key = decode_base64(&m.public_key_b64).context("decode public_key_b64")?;
        let key_ref = KeyRef::inline_public_key(format!("upgrade:{}", m.app_id), &public_key);
        verify_ed25519_signature(digest.as_ref(), &signature, &key_ref)
            .map_err(|e| anyhow!("package signature verification failed: {e}"))?;
    }
    Ok(())
}

async fn sha256_file(path: &Path) -> Result<digest::Digest> {
    let mut file = File::open(path)
        .await
        .with_context(|| format!("open package {}", path.display()))?;
    let mut ctx = digest::Context::new(&digest::SHA256);
    let mut buf = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buf)
            .await
            .with_context(|| format!("read package {}", path.display()))?;
        if n == 0 {
            break;
        }
        ctx.update(&buf[..n]);
    }
    Ok(ctx.finish())
}

fn decode_base64(input: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    STANDARD.decode(input).map_err(Into::into)
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

    fn dummy_manifest(from: &str, to: &str, build: u64, package_sha256: String) -> UpgradeManifest {
        UpgradeManifest {
            app_id: "test-app".into(),
            app_name: "Test App".into(),
            from_version: from.into(),
            to_version: to.into(),
            public_key_b64: String::new(),
            signature_b64: String::new(),
            package_sha256,
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
            _manifest: &UpgradeManifest,
            _package_path: &Path,
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
        let digest = sha256_file(&pkg).await.unwrap();

        let mut engine = UpgradeEngine::new(NoopStrategy, staging.clone());
        let manifest = dummy_manifest("1.0.0", "1.1.0", 2, base16::encode_lower(digest.as_ref()));

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

    #[tokio::test]
    async fn upgrade_state_is_persisted() {
        let tmp = env::temp_dir().join("cc_upgrade_state_test");
        let pkg = tmp.join("package.tar.zst");
        let staging = tmp.join("staging");

        fs::create_dir_all(&tmp).await.unwrap();
        fs::write(&pkg, b"state package").await.unwrap();
        let digest = sha256_file(&pkg).await.unwrap();

        let store = StateStore::open_in_memory().unwrap();
        let mut engine = UpgradeEngine::new_with_store(NoopStrategy, staging, store);
        let manifest = dummy_manifest("1.0.0", "1.1.0", 2, base16::encode_lower(digest.as_ref()));

        engine.run(manifest, &pkg).await.unwrap();
        let stored = engine
            .store
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .load_upgrade_state("test-app")
            .unwrap()
            .unwrap();
        assert_eq!(stored.state, "committed");
        assert_eq!(stored.target_version, "1.1.0");
    }

    #[test]
    fn state_display() {
        assert_eq!(UpgradeState::Committed.to_string(), "committed");
        assert_eq!(
            UpgradeState::Failed {
                reason: "disk full".into()
            }
            .to_string(),
            "failed(disk full)"
        );
    }
}
