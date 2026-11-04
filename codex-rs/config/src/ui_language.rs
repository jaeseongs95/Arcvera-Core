//! Client display preferences are separate from the shared official CLI configuration.

use std::io;
use std::path::Path;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

pub const CLIENT_PREFERENCES_FILE: &str = "opendiy-client.toml";

#[derive(Serialize, Debug, Default, Copy, Clone, PartialEq, Eq, JsonSchema)]
pub enum UiLanguage {
    #[serde(rename = "ko")]
    Korean,
    #[default]
    #[serde(rename = "en")]
    English,
}

impl UiLanguage {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        // ICU validates and normalizes Unicode BCP 47 locale identifiers. Match
        // the supported base language; region/script/extensions do not require
        // duplicate resources. Do not treat malformed/unsupported input as English.
        if value.contains('_') {
            return Err("Invalid UI language tag; use a BCP 47 tag such as en-US or ko-KR");
        }
        let locale = icu_locale_core::Locale::try_from_str(value)
            .map_err(|_| "Invalid UI language tag; use a BCP 47 tag such as en-US or ko-KR")?;
        match locale.id.language.as_str() {
            "ko" => Ok(Self::Korean),
            "en" => Ok(Self::English),
            _ => Err("Unsupported UI language; supported languages are en and ko"),
        }
    }

    pub fn from_setting(setting: Option<&str>) -> Result<Self, &'static str> {
        setting.map(Self::parse).unwrap_or(Ok(Self::default()))
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Korean => "ko",
            Self::English => "en",
        }
    }
}

impl<'de> Deserialize<'de> for UiLanguage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let tag = String::deserialize(deserializer)?;
        Self::parse(&tag).map_err(serde::de::Error::custom)
    }
}

/// Prefer the client-only file, retaining read compatibility with old base user settings.
/// Reading never migrates or rewrites either file. Profiles and projects are not consulted.
pub fn read_user_ui_language(codex_home: &Path) -> io::Result<UiLanguage> {
    #[derive(Deserialize, Default)]
    struct Preference {
        ui_language: Option<UiLanguage>,
    }
    let read = |file: &str| -> io::Result<Option<UiLanguage>> {
        let text = match std::fs::read_to_string(codex_home.join(file)) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let preference: Preference = toml::from_str(&text)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        Ok(preference.ui_language)
    };
    if let Some(language) = read(CLIENT_PREFERENCES_FILE)? {
        return Ok(language);
    }
    Ok(read(crate::CONFIG_TOML_FILE)?.unwrap_or_default())
}

/// Save only a client preference, preserving the official CLI's config byte-for-byte.
pub fn write_user_ui_language(codex_home: &Path, language: UiLanguage) -> io::Result<()> {
    let paths =
        codex_utils_path::resolve_symlink_write_paths(&codex_home.join(CLIENT_PREFERENCES_FILE))?;
    let text = match paths.read_path.as_ref().map(std::fs::read_to_string) {
        Some(Ok(text)) => text,
        Some(Err(error)) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => String::new(),
    };
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    document["ui_language"] = toml_edit::value(language.as_str());
    codex_utils_path::write_atomically(&paths.write_path, &document.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedicated_preference_preserves_shared_config_and_other_client_settings() {
        let home = tempfile::tempdir().unwrap();
        let shared = "# Shared with the official CLI\nmodel = 'fixture'\nui_language = 'ko'\n";
        std::fs::write(home.path().join(crate::CONFIG_TOML_FILE), shared).unwrap();
        assert_eq!(
            read_user_ui_language(home.path()).unwrap(),
            UiLanguage::Korean
        );
        std::fs::write(
            home.path().join(CLIENT_PREFERENCES_FILE),
            "# Keep client settings\nfuture_preference = true\n",
        )
        .unwrap();
        assert_eq!(
            read_user_ui_language(home.path()).unwrap(),
            UiLanguage::Korean
        );
        write_user_ui_language(home.path(), UiLanguage::English).unwrap();
        assert_eq!(
            read_user_ui_language(home.path()).unwrap(),
            UiLanguage::English
        );
        assert_eq!(
            std::fs::read_to_string(home.path().join(crate::CONFIG_TOML_FILE)).unwrap(),
            shared
        );
        let saved = std::fs::read_to_string(home.path().join(CLIENT_PREFERENCES_FILE)).unwrap();
        assert!(saved.contains("# Keep client settings"));
        assert!(saved.contains("future_preference = true"));
        assert!(saved.contains("ui_language = \"en\""));
    }

    #[test]
    fn fresh_preference_does_not_create_shared_config_and_invalid_sidecar_is_an_error() {
        let home = tempfile::tempdir().unwrap();
        write_user_ui_language(home.path(), UiLanguage::Korean).unwrap();
        assert!(!home.path().join(crate::CONFIG_TOML_FILE).exists());
        assert_eq!(
            read_user_ui_language(home.path()).unwrap(),
            UiLanguage::Korean
        );
        std::fs::write(
            home.path().join(crate::CONFIG_TOML_FILE),
            "ui_language = 'en'\n",
        )
        .unwrap();
        for contents in ["ui_language = 'kr'\n", "ui_language = 7\n", "[invalid"] {
            std::fs::write(home.path().join(CLIENT_PREFERENCES_FILE), contents).unwrap();
            assert_eq!(
                read_user_ui_language(home.path()).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn missing_user_preference_is_english_and_invalid_values_are_rejected() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            read_user_ui_language(home.path()).unwrap(),
            UiLanguage::English
        );
        let path = home.path().join(crate::CONFIG_TOML_FILE);
        std::fs::write(&path, "model = 'untouched'\n").unwrap();
        assert_eq!(
            read_user_ui_language(home.path()).unwrap(),
            UiLanguage::English
        );
        for invalid in ["'kr'", "'en_US'", "'ko--KR'", "''", "7", "true"] {
            std::fs::write(&path, format!("ui_language = {invalid}\n")).unwrap();
            assert_eq!(
                read_user_ui_language(home.path()).unwrap_err().kind(),
                io::ErrorKind::InvalidData
            );
        }
        for invalid in ["kr", "ja", "en_US", "ko--KR", "", "ko en"] {
            assert!(UiLanguage::parse(invalid).is_err());
        }
    }

    #[test]
    fn locale_tags_share_one_normalization_for_arguments_and_saved_preferences() {
        for (tag, expected) in [
            ("KO", UiLanguage::Korean),
            ("ko-KR", UiLanguage::Korean),
            ("ko-Kore-KR", UiLanguage::Korean),
            ("EN-us", UiLanguage::English),
            ("en-US-u-nu-latn", UiLanguage::English),
        ] {
            assert_eq!(UiLanguage::parse(tag), Ok(expected));
            assert_eq!(UiLanguage::from_setting(Some(tag)), Ok(expected));
            let saved: toml::Value = toml::from_str(&format!("ui_language = '{tag}'")).unwrap();
            let language: UiLanguage = saved["ui_language"].clone().try_into().unwrap();
            assert_eq!(language, expected);
            assert_eq!(serde_json::to_value(language).unwrap(), expected.as_str());
        }
        assert_eq!(UiLanguage::from_setting(None), Ok(UiLanguage::English));
        assert_ne!(
            UiLanguage::parse("ja").unwrap_err(),
            UiLanguage::parse("ko--KR").unwrap_err()
        );
    }

    #[test]
    fn only_base_user_file_controls_language_even_with_project_and_profile_values() {
        let home = tempfile::tempdir().unwrap();
        let project = tempfile::tempdir().unwrap();
        std::fs::create_dir(project.path().join(".codex")).unwrap();
        std::fs::write(
            project.path().join(".codex/config.toml"),
            "ui_language = 'ko'\n",
        )
        .unwrap();
        std::fs::write(
            home.path().join(crate::CONFIG_TOML_FILE),
            "ui_language = 'en'\nprofile = 'korean'\n[profiles.korean]\nui_language = 'ko'\n",
        )
        .unwrap();
        assert_eq!(
            read_user_ui_language(home.path()).unwrap(),
            UiLanguage::English
        );
        // Even an invalid project value must not enter this global-only reader.
        std::fs::write(
            project.path().join(".codex/config.toml"),
            "ui_language = 7\n",
        )
        .unwrap();
        assert_eq!(
            read_user_ui_language(home.path()).unwrap(),
            UiLanguage::English
        );
    }
}
