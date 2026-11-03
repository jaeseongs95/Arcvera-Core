//! Explicit local diagnostic controls; filesystem work never runs on the UI task.

use crate::i18n::Locale;
use codex_diagnostics::settings_control;
use serde_json::Value;
use serde_json::json;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug)]
pub(super) enum Action {
    LoggingStatus,
    LoggingSet { enabled: bool },
    CacheDiagnosticsStatus,
    CacheDiagnosticsSet { response: bool, wire: bool },
}

pub(super) async fn handle(action: Action) -> Result<Value, String> {
    let locale = crate::i18n::locale();
    // The local client owns these diagnostics, including when its backend is remote.
    let home = codex_utils_home_dir::find_codex_home().map_err(|error| {
        crate::i18n::message_for(
            locale,
            "diagnostics-home-failed",
            &[("error", &error.to_string())],
        )
    })?;
    control_at_home(action, home.to_path_buf(), locale).await
}

async fn control_at_home(action: Action, home: PathBuf, locale: Locale) -> Result<Value, String> {
    if matches!(
        action,
        Action::CacheDiagnosticsSet {
            response: false,
            wire: true
        }
    ) {
        return Err(crate::i18n::message_for(
            locale,
            "diagnostics-cache-wire-requires-response",
            &[],
        ));
    }
    let mutation = matches!(
        action,
        Action::LoggingSet { .. } | Action::CacheDiagnosticsSet { .. }
    );
    let status = tokio::task::spawn_blocking(move || match action {
        Action::LoggingStatus | Action::CacheDiagnosticsStatus => settings_control::status(&home),
        Action::LoggingSet { enabled } => {
            settings_control::update(&home, settings_control::Update::Logging { enabled })
        }
        Action::CacheDiagnosticsSet { response, wire } => {
            settings_control::update(&home, settings_control::Update::Cache { response, wire })
        }
    })
    .await
    .map_err(|error| {
        crate::i18n::message_for(
            locale,
            "diagnostics-operation-unconfirmed",
            &[("error", &error.to_string())],
        )
    })?
    .map_err(|error| {
        if error.kind() == std::io::ErrorKind::WouldBlock {
            crate::i18n::message_for(locale, "diagnostics-settings-busy", &[])
        } else {
            crate::i18n::message_for(
                locale,
                "diagnostics-operation-failed",
                &[("error", &error.to_string())],
            )
        }
    })?;
    Ok(json!({
        "scope": "local_client",
        "settings": status,
        "message": crate::i18n::message_for(
            locale,
            if mutation { "diagnostics-saved-next-launch" } else { "diagnostics-status" },
            &[],
        ),
    }))
}

#[cfg(test)]
#[path = "diagnostics_control_tests.rs"]
mod tests;
