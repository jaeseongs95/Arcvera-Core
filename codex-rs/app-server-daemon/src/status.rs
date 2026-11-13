// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md.
//! Passive installation/process observations. This is not an update reservation or an idle gate.

use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use serde::Serialize;
use tokio::fs;
use tokio::time::timeout;

use crate::backend::{ReadOnlyPidStatus, read_only_pid_status};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DaemonStatus {
    Running,
    Stopped,
    Unknown,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusOutput {
    pub status: DaemonStatus,
    pub managed_codex_path: Option<PathBuf>,
    pub managed_codex_version: Option<String>,
    pub app_server_version: Option<String>,
    pub socket_path: Option<PathBuf>,
    pub cli_version: String,
    pub reason: Option<String>,
}

/// Does not load settings, config, authentication, logs, or conversation data.
pub async fn status() -> StatusOutput {
    match codex_utils_home_dir::find_codex_home() {
        Ok(home) => status_for_home(home.as_path()).await,
        Err(error) => StatusOutput {
            status: DaemonStatus::Unknown,
            managed_codex_path: None,
            managed_codex_version: None,
            app_server_version: None,
            socket_path: None,
            cli_version: env!("CARGO_PKG_VERSION").to_string(),
            reason: Some(format!("homeObservationFailed: {error}")),
        },
    }
}

async fn status_for_home(home: &Path) -> StatusOutput {
    let mut output = StatusOutput {
        status: DaemonStatus::Unknown,
        managed_codex_path: None,
        managed_codex_version: None,
        app_server_version: None,
        socket_path: None,
        cli_version: env!("CARGO_PKG_VERSION").to_string(),
        reason: None,
    };
    let mut reasons = Vec::new();
    match selected_binary(home).await {
        Ok(path) => {
            output.managed_codex_path = Some(path.clone());
            match timeout(Duration::from_secs(5), selected_version(&path)).await {
                Ok(Ok(version)) => output.managed_codex_version = version,
                Ok(Err(error)) => {
                    reasons.push(format!("selectedVersionObservationFailed: {error:#}"))
                }
                Err(_) => reasons.push("selectedVersionTimedOut".to_string()),
            }
        }
        Err(error) => reasons.push(format!("selectionObservationFailed: {error:#}")),
    }
    let mut running_count = 0;
    let mut starting = false;
    let state = home.join(crate::STATE_DIR_NAME);
    for name in [crate::LEGACY_PID_FILE_NAME, crate::DAEMON_PID_FILE_NAME] {
        match timeout(
            Duration::from_secs(2),
            read_only_pid_status(&state.join(name)),
        )
        .await
        {
            Ok(Ok(ReadOnlyPidStatus::NotRunning)) => {}
            Ok(Ok(ReadOnlyPidStatus::Running)) => running_count += 1,
            Ok(Ok(ReadOnlyPidStatus::Starting)) => starting = true,
            Ok(Err(error)) => reasons.push(format!("pidObservationFailed({name}): {error:#}")),
            Err(_) => reasons.push(format!("pidObservationTimedOut({name})")),
        }
    }
    let mut socket_exists = true; // Errors must never look like absence.
    match codex_app_server_transport::app_server_control_socket_path(home) {
        Ok(path) => {
            output.socket_path = Some(path.as_path().to_path_buf());
            match exists(path.as_path()).await {
                Ok(present) => {
                    socket_exists = present;
                    if present {
                        match crate::client::probe(path.as_path()).await {
                            Ok(info) => output.app_server_version = Some(info.app_server_version),
                            Err(error) => reasons.push(format!("socketProbeFailed: {error:#}")),
                        }
                    }
                }
                Err(error) => reasons.push(format!("socketObservationFailed: {error:#}")),
            }
        }
        Err(error) => reasons.push(format!("socketPathUnavailable: {error}")),
    }
    if starting {
        reasons.push("processStarting".to_string());
    }
    if running_count > 1 {
        reasons.push("multipleManagedProcesses".to_string());
    }
    if running_count > 0 && !socket_exists {
        reasons.push("liveProcessWithoutSocket".to_string());
    }
    if reasons.is_empty() {
        output.status = if output.app_server_version.is_some() {
            DaemonStatus::Running
        } else if running_count == 0 && !socket_exists {
            DaemonStatus::Stopped
        } else {
            DaemonStatus::Unknown
        };
    } else {
        output.reason = Some(reasons.join("; "));
    }
    output
}

async fn exists(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path).await {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("metadata access failed"),
    }
}

/// Match lifecycle selection precedence, preserving errors that its best-effort helpers hide.
async fn selected_binary(home: &Path) -> Result<PathBuf> {
    let dedicated = home.join("packages/app-server-daemon");
    let mut root = dedicated.clone();
    if !exists(&dedicated.join("current")).await? {
        for (package, names) in [
            (
                "app-server-daemon",
                [
                    "daemon.pid",
                    "daemon.stderr.log",
                    "daemon-updater.pid",
                    "daemon-updater.stderr.log",
                ],
            ),
            (
                "standalone",
                [
                    "app-server.pid",
                    "app-server.stderr.log",
                    "app-server-updater.pid",
                    "app-server-updater.stderr.log",
                ],
            ),
        ] {
            let mut found = false;
            for name in names {
                found |= exists(&home.join(crate::STATE_DIR_NAME).join(name)).await?;
            }
            if found {
                root = home.join("packages").join(package);
                break;
            }
        }
    }
    let current = root.join("current");
    if exists(&current).await? {
        anyhow::ensure!(
            fs::metadata(&current).await?.is_dir(),
            "selected installation is not a directory"
        );
    }
    let name = if cfg!(windows) {
        "arcvera.exe"
    } else {
        "arcvera"
    };
    let packaged = current.join("bin").join(name);
    let legacy = current.join(name);
    if exists(&packaged).await?
        || (!exists(&legacy).await? && (cfg!(windows) || root.ends_with("app-server-daemon")))
    {
        Ok(packaged)
    } else {
        Ok(legacy)
    }
}

async fn selected_version(path: &Path) -> Result<Option<String>> {
    if !exists(path).await? {
        return Ok(None);
    }
    let before = fs::canonicalize(path).await?;
    anyhow::ensure!(
        fs::metadata(&before).await?.is_file(),
        "selected executable is not a file"
    );
    let version = crate::managed_install::managed_codex_version(&before).await?;
    anyhow::ensure!(
        fs::canonicalize(path).await? == before,
        "selected installation changed during observation"
    );
    Ok(Some(version))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_installation_is_stopped_without_creating_home() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("absent");
        let output = status_for_home(&home).await;
        assert_eq!(output.status, DaemonStatus::Stopped);
        assert!(output.managed_codex_version.is_none());
        assert!(!home.exists());
    }

    #[tokio::test]
    async fn malformed_and_starting_records_are_unknown_and_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let state = temp.path().join(crate::STATE_DIR_NAME);
        fs::create_dir(&state).await.unwrap();
        for name in [crate::LEGACY_PID_FILE_NAME, crate::DAEMON_PID_FILE_NAME] {
            for bytes in [b"invalid PID JSON".as_slice(), b"".as_slice()] {
                let path = state.join(name);
                fs::write(&path, bytes).await.unwrap();
                let output = status_for_home(temp.path()).await;
                assert_eq!(output.status, DaemonStatus::Unknown);
                assert!(output.reason.is_some());
                assert_eq!(fs::read(&path).await.unwrap(), bytes);
                assert!(!path.with_extension("pid.lock").exists());
                fs::remove_file(path).await.unwrap();
            }
        }
    }

    #[tokio::test]
    async fn invalid_selected_installation_is_unknown_not_absent() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("packages/app-server-daemon");
        fs::create_dir_all(&package).await.unwrap();
        fs::write(package.join("current"), b"not a directory")
            .await
            .unwrap();
        let output = status_for_home(temp.path()).await;
        assert_eq!(output.status, DaemonStatus::Unknown);
        assert!(
            output
                .reason
                .unwrap()
                .contains("selectionObservationFailed")
        );
        assert_eq!(
            fs::read(package.join("current")).await.unwrap(),
            b"not a directory"
        );
    }
}
