//! Immutable, process-local cache observation choices. This extension never
//! changes the shared logging settings understood by older installations.
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::OnceLock;

static PROCESS_MODE: OnceLock<CacheMode> = OnceLock::new();

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub response: bool,
    pub wire: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CacheMode {
    pub logging_enabled: bool,
    pub response_requested: bool,
    pub wire_requested: bool,
    pub wire_producer_enabled: bool,
    pub response_enabled: bool,
    pub wire_enabled: bool,
    pub wire_disabled_reason: Option<&'static str>,
}

impl CacheMode {
    pub fn resolve(logging_enabled: bool, settings: Settings, explicit_wire: bool) -> Self {
        let response_enabled = logging_enabled && settings.response;
        let wire_enabled = response_enabled && settings.wire;
        Self {
            logging_enabled,
            response_requested: settings.response,
            wire_requested: settings.wire,
            // Batch configuration is an explicit producer opt-in too. Preserve
            // the legacy environment opt-in for other tracing consumers.
            wire_producer_enabled: explicit_wire || wire_enabled,
            response_enabled,
            wire_enabled,
            wire_disabled_reason: if wire_enabled {
                None
            } else if !logging_enabled {
                Some("logging_disabled")
            } else if !settings.response {
                Some("response_disabled")
            } else {
                Some("wire_not_selected")
            },
        }
    }
}

fn explicit_wire() -> bool {
    std::env::var("APP_CACHE_WIRE_DIAGNOSTICS").as_deref() == Ok("1")
}

/// API-only consumers retain the legacy explicit opt-in. Normal CLI/server
/// startup initializes the same value before installing its tracing layers.
pub fn process_mode() -> CacheMode {
    *PROCESS_MODE.get_or_init(|| CacheMode::resolve(false, Settings::default(), explicit_wire()))
}

/// Observe startup selection without initializing or changing the process mode.
pub fn initialized_process_mode() -> Option<CacheMode> {
    PROCESS_MODE.get().copied()
}

pub fn read_settings(home: &Path) -> io::Result<Settings> {
    match fs::read_to_string(home.join("cli-cache-diagnostics.toml")) {
        Ok(text) => toml::from_str::<Settings>(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(error) => Err(error),
    }
}

pub(crate) fn initialize(home: &Path, logging_enabled: bool) -> io::Result<CacheMode> {
    let settings = read_settings(home)?;
    let mode = CacheMode::resolve(logging_enabled, settings, explicit_wire());
    match PROCESS_MODE.set(mode) {
        Ok(()) => Ok(mode),
        Err(_) if PROCESS_MODE.get() == Some(&mode) => Ok(mode),
        Err(_) => Err(io::Error::other(
            "cache diagnostics were initialized earlier with a different mode",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_wire_opt_in_and_legacy_producer_follow_all_gate_combinations() {
        // Gate bits: logging, response, wire, legacy environment. Outputs are
        // response, batch wire, producer; each expected row is independent of resolve.
        for (gates, expected, reason) in [
            (0b0000, [false, false, false], Some("logging_disabled")),
            (0b0001, [false, false, true], Some("logging_disabled")),
            (0b0010, [false, false, false], Some("logging_disabled")),
            (0b0011, [false, false, true], Some("logging_disabled")),
            (0b0100, [false, false, false], Some("logging_disabled")),
            (0b0101, [false, false, true], Some("logging_disabled")),
            (0b0110, [false, false, false], Some("logging_disabled")),
            (0b0111, [false, false, true], Some("logging_disabled")),
            (0b1000, [false, false, false], Some("response_disabled")),
            (0b1001, [false, false, true], Some("response_disabled")),
            (0b1010, [false, false, false], Some("response_disabled")),
            (0b1011, [false, false, true], Some("response_disabled")),
            (0b1100, [true, false, false], Some("wire_not_selected")),
            (0b1101, [true, false, true], Some("wire_not_selected")),
            (0b1110, [true, true, true], None),
            (0b1111, [true, true, true], None),
        ] {
            let logging = gates & 0b1000 != 0;
            let response = gates & 0b0100 != 0;
            let wire = gates & 0b0010 != 0;
            let explicit = gates & 0b0001 != 0;
            let mode = CacheMode::resolve(logging, Settings { response, wire }, explicit);
            assert_eq!(mode.logging_enabled, logging);
            assert_eq!(mode.response_requested, response);
            assert_eq!(mode.wire_requested, wire);
            assert_eq!(
                [
                    mode.response_enabled,
                    mode.wire_enabled,
                    mode.wire_producer_enabled
                ],
                expected,
                "gates={gates:04b}"
            );
            assert_eq!(mode.wire_disabled_reason, reason, "gates={gates:04b}");
        }
    }

    #[test]
    fn extension_is_strict_and_shared_settings_remain_unchanged() {
        assert_eq!(toml::from_str::<Settings>("").unwrap(), Settings::default());
        assert!(toml::from_str::<Settings>("response = 1").is_err());
        assert!(toml::from_str::<Settings>("prompt_cache = true").is_err());
        assert!(toml::from_str::<crate::batch_log::Settings>("prompt_cache = true").is_err());
        assert!(toml::from_str::<crate::batch_log::Settings>("enabled = true").is_ok());
    }

    #[test]
    fn startup_mode_is_frozen_in_an_isolated_process() {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "cache_diagnostics::tests::frozen_mode_child",
                "--ignored",
                "--test-threads=1",
            ])
            .env_remove("APP_CACHE_WIRE_DIAGNOSTICS");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        assert!(command.status().unwrap().success());
    }

    #[test]
    #[ignore = "isolated process for immutable startup state"]
    fn frozen_mode_child() {
        let root = tempfile::tempdir().unwrap();
        let settings = root.path().join("cli-cache-diagnostics.toml");
        fs::write(&settings, "response = true\nwire = true\n").unwrap();
        let before = initialize(root.path(), true).unwrap();
        assert!(before.response_enabled && before.wire_enabled && before.wire_producer_enabled);
        fs::write(&settings, "response = false\nwire = false\n").unwrap();
        assert_eq!(process_mode(), before);
        assert!(initialize(root.path(), true).is_err());
        assert_eq!(process_mode(), before);
    }
}
