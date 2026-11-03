// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
//! Client display preference read only from the base user config, never merged layers.

use std::io;
use std::path::Path;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

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

/// Read the local display preference before CLI help or any merged/server config is loaded.
/// Profiles, projects, named user layers and remote thread settings are intentionally absent.
pub fn read_user_ui_language(codex_home: &Path) -> io::Result<UiLanguage> {
    #[derive(Deserialize, Default)]
    struct Preference {
        ui_language: Option<UiLanguage>,
    }
    let text = match std::fs::read_to_string(codex_home.join(crate::CONFIG_TOML_FILE)) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(UiLanguage::default()),
        Err(error) => return Err(error),
    };
    let preference: Preference =
        toml::from_str(&text).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(preference.ui_language.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

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
