// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md.
//! Invocation overrides apply on fresh starts or explicitly confirmed managed restarts.
//! The lifecycle lock serializes persistence against other clients and updates.

use crate::Daemon;
use crate::LifecycleOutput;
use crate::ensure_supported_platform;
use anyhow::Context;
use anyhow::Result;
use anyhow::ensure;
use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

/// Automatic CLI startup must not run a different, stopped managed binary.
/// Callers can continue with their embedded/foreground backend and show this reason.
#[derive(Debug)]
pub struct CliDaemonMismatch {
    pub cli_path: PathBuf,
    pub managed_path: PathBuf,
}

impl std::fmt::Display for CliDaemonMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The selected managed app-server executable {} differs from this CLI {}. No daemon was started and the installed selection was preserved. To explicitly select this CLI package, run `arcvera app-server daemon update --from-cli --yes --no-force`.",
            self.managed_path.display(),
            self.cli_path.display()
        )
    }
}

impl std::error::Error for CliDaemonMismatch {}

pub(super) fn should_manage_updater(cli_start: bool, server_version: Option<&str>) -> bool {
    // Automatic fallback must leave an old/unverified server's updater alone.
    // Explicit lifecycle operations retain their existing updater policy.
    !cli_start || server_version == Some(env!("CARGO_PKG_VERSION"))
}

/// Start a missing daemon with these features, preserving other saved overrides,
/// or leave a running daemon unchanged.
/// Callers must check the running server's configuration before using it.
/// Startup diagnostics go to tracing, so a caller with a live TUI can keep rendering.
pub async fn start_with_features(features: &BTreeMap<String, bool>) -> Result<LifecycleOutput> {
    ensure_supported_platform()?;
    #[cfg(windows)]
    crate::backend::windows::ensure_not_elevated()?;
    let mut daemon = Daemon::from_environment()?;
    daemon.log_diagnostics = true;
    let _operation_lock = daemon.acquire_operation_lock().await?;
    let selected = daemon.current_installation()?;
    Box::pin(selected.start_for_cli(features, &std::env::current_exe()?)).await
}

/// Apply confirmed feature settings to an existing managed daemon and restart it.
/// Callers must obtain consent for changes to shared services and interrupted work,
/// then verify effective server compatibility after this operation completes.
pub async fn restart_with_features(features: &BTreeMap<String, bool>) -> Result<LifecycleOutput> {
    ensure_supported_platform()?;
    #[cfg(windows)]
    crate::backend::windows::ensure_not_elevated()?;
    let daemon = Daemon::from_environment()?;
    let _operation_lock = daemon.acquire_operation_lock().await?;
    let selected = daemon.current_installation()?;
    Box::pin(selected.restart_with_features_locked(features)).await
}

impl Daemon {
    // Keep the CLI guard and real start in one boundary, exercised by regression tests.
    async fn start_for_cli(
        &self,
        features: &BTreeMap<String, bool>,
        cli_path: &Path,
    ) -> Result<LifecycleOutput> {
        let settings = self.load_settings().await?;
        let mut overrides = settings.feature_overrides;
        overrides.extend(features.clone());
        self.start_with_overrides(Some(&overrides), Some(cli_path))
            .await
    }

    /// The caller holds the operation lock across this check and `start`.
    /// Existing/starting servers retain the normal compatibility and recovery flow.
    pub(super) async fn ensure_cli_matches_stopped_installation(
        &self,
        settings: &crate::settings::DaemonSettings,
        cli_path: &Path,
    ) -> Result<()> {
        if crate::client::probe(&self.socket_path).await.is_ok()
            || self.running_backend_instance(settings).await?.is_some()
        {
            return Ok(());
        }
        match tokio::fs::metadata(&self.managed_codex_bin).await {
            Ok(metadata) if metadata.is_file() => {}
            // Missing/incomplete installations keep the existing preparation policy.
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to inspect selected daemon executable {}",
                        self.managed_codex_bin.display()
                    )
                });
            }
        }
        let cli = crate::managed_install::executable_identity(cli_path).await?;
        let managed = crate::managed_install::executable_identity(&self.managed_codex_bin).await?;
        if !cli.same_contents(&managed) {
            return Err(CliDaemonMismatch {
                cli_path: cli_path.to_path_buf(),
                managed_path: self.managed_codex_bin.clone(),
            }
            .into());
        }
        Ok(())
    }

    pub(super) async fn restart(
        &self,
        features: &BTreeMap<String, bool>,
    ) -> Result<LifecycleOutput> {
        let mut settings = self.load_settings().await?;
        // Explicit requests replace saved aliases as well as canonical keys.
        settings.feature_overrides.retain(|name, _| {
            let canonical = codex_features::feature_for_key(name)
                .map(codex_features::Feature::key)
                .unwrap_or(name);
            !features.contains_key(canonical)
        });
        settings.feature_overrides.extend(features.clone());
        self.restart_with_settings(settings).await
    }

    pub(super) async fn restart_with_features_locked(
        &self,
        features: &BTreeMap<String, bool>,
    ) -> Result<LifecycleOutput> {
        let mut settings = self.load_settings().await?;
        ensure!(
            self.running_backend_instance(&settings).await?.is_some(),
            "no running managed daemon; rerun the command to check the current server"
        );
        let previous = settings.feature_overrides.clone();
        settings.feature_overrides.extend(features.clone());
        if settings.feature_overrides == previous {
            return self.start(&previous).await;
        }
        self.restart_with_settings(settings).await
    }
}

#[cfg(test)]
#[path = "launch_tests.rs"]
mod tests;
