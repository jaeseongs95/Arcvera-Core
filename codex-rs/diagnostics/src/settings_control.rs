// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
//! Explicit local operator edits. Startup choices and completed log batches are immutable.

use crate::batch_log;
use crate::cache_diagnostics;
use crate::cache_diagnostics::CacheMode;
use serde::Serialize;
use std::fs;
use std::fs::OpenOptions;
use std::fs::TryLockError;
use std::io;
use std::io::Write;
use std::path::Path;

const LOCK_FILE: &str = "cli-diagnostics-settings.lock";

#[derive(Clone, Copy, Debug)]
pub enum Update {
    Logging { enabled: bool },
    Cache { response: bool, wire: bool },
}

#[derive(Debug, Serialize)]
pub struct Status {
    /// A startup selection, not proof of file collection or successful Drive delivery.
    pub current_process_startup_mode: Option<CacheMode>,
    pub saved_logging: batch_log::Settings,
    pub saved_cache: cache_diagnostics::Settings,
    pub next_process_mode_if_environment_unchanged: CacheMode,
    pub restart_required: Option<bool>,
}

/// Read-only: never create a file, start a worker, or initialize a OnceLock.
pub fn status(home: &Path) -> io::Result<Status> {
    let saved_logging = batch_log::read_settings(home)?;
    if saved_logging.enabled {
        saved_logging.validate(&home.join("log").join("cli-batches"))?;
    }
    let saved_cache = cache_diagnostics::read_settings(home)?;
    let current_process_startup_mode = cache_diagnostics::initialized_process_mode();
    let next = CacheMode::resolve(
        saved_logging.enabled,
        saved_cache,
        std::env::var("APP_CACHE_WIRE_DIAGNOSTICS").as_deref() == Ok("1"),
    );
    Ok(Status {
        current_process_startup_mode,
        saved_logging,
        saved_cache,
        next_process_mode_if_environment_unchanged: next,
        restart_required: current_process_startup_mode.map(|current| current != next),
    })
}

/// Only edits the explicitly selected switches, under nonblocking process exclusion.
pub fn update(home: &Path, update: Update) -> io::Result<Status> {
    if matches!(
        update,
        Update::Cache {
            response: false,
            wire: true
        }
    ) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cache wire observation requires response observation",
        ));
    }
    fs::create_dir_all(home)?;
    // This stable lock file is never removed or replaced; the settings files are.
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(home.join(LOCK_FILE))?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "diagnostic settings are busy; query status before retrying",
            ));
        }
        Err(TryLockError::Error(error)) => return Err(error),
    }
    // Reject an existing malformed configuration instead of replacing it with defaults.
    status(home)?;
    let (filename, edits) = match update {
        Update::Logging { enabled } => ("cli-logging.toml", vec![("enabled", enabled)]),
        Update::Cache { response, wire } => (
            "cli-cache-diagnostics.toml",
            vec![("response", response), ("wire", wire)],
        ),
    };
    let target = home.join(filename);
    let original = match fs::read_to_string(&target) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let mut document = original
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    for (key, value) in edits {
        let decor = document
            .get(key)
            .and_then(toml_edit::Item::as_value)
            .map(|value| value.decor().clone());
        document[key] = toml_edit::value(value);
        if let Some(decor) = decor {
            *document[key]
                .as_value_mut()
                .expect("boolean edit")
                .decor_mut() = decor;
        }
    }
    let mut updated = document.to_string();
    if original.contains("\r\n") {
        updated = updated.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    // Validate the full output with the same strict settings type used at startup.
    match update {
        Update::Logging { .. } => {
            let settings: batch_log::Settings = toml::from_str(&updated)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
            if settings.enabled {
                settings.validate(&home.join("log").join("cli-batches"))?;
            }
        }
        Update::Cache { .. } => {
            toml::from_str::<cache_diagnostics::Settings>(&updated)
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        }
    }
    // Use an ordinary file so the published settings do not retain Windows'
    // FILE_ATTRIBUTE_TEMPORARY cache hint.
    let mut temporary = tempfile::Builder::new().make_in(home, |path| {
        OpenOptions::new().write(true).create_new(true).open(path)
    })?;
    temporary.write_all(updated.as_bytes())?;
    match fs::metadata(&target) {
        Ok(metadata) => temporary
            .as_file()
            .set_permissions(metadata.permissions())?,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    temporary.as_file().sync_all()?;
    let temporary = temporary.into_temp_path();
    // Unlike tempfile::persist's MoveFileExW-only implementation, std rename
    // supports replacing a Windows file while readers retain the old handle.
    // TempPath still cleans up the source if publication fails.
    fs::rename(&temporary, &target)?;
    status(home)
}

#[cfg(test)]
#[path = "settings_control_tests.rs"]
mod tests;
