//! UI-only language selection. Never apply this dictionary to prompts, tool arguments,
//! slash command tokens, paths, or text supplied by the user or a server.
//!
//! `CODEX_UI_LANGUAGE=ko|en` overrides the saved base user `ui_language`; default is English.
//! Before startup initialization, library callers retain the original English strings.

use std::io;
use std::path::Path;
use std::sync::OnceLock;

pub use codex_config::ui_language::UiLanguage as Locale;

mod catalog;
pub use catalog::message;
pub use catalog::message_for;
pub use catalog::message_with_count;
pub use catalog::message_with_count_for;
pub use catalog::static_message;

static LOCALE: OnceLock<Locale> = OnceLock::new();

/// Call before constructing CLI help or TUI views. This never changes configuration files.
pub fn initialize() -> io::Result<()> {
    // CLI and TUI startup can both call this in one process. A later global file
    // edit is for the next process and must not invalidate this launch's locale.
    if LOCALE.get().is_some() {
        return Ok(());
    }
    let setting = match std::env::var("CODEX_UI_LANGUAGE") {
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(std::env::VarError::NotUnicode(_)) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "CODEX_UI_LANGUAGE must contain a Unicode language tag such as ko or en",
            ));
        }
    };
    let home = codex_utils_home_dir::find_codex_home()?;
    let locale = locale_from_user_settings(home.as_path(), setting.as_deref())?;
    // A concurrent initializer keeps the first successful selection for this process.
    let _ = LOCALE.set(locale);
    Ok(())
}

fn locale_from_user_settings(
    home: &Path,
    environment_override: Option<&str>,
) -> io::Result<Locale> {
    if environment_override.is_some() {
        return Locale::from_setting(environment_override)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error));
    }
    codex_config::ui_language::read_user_ui_language(home)
}

pub fn locale() -> Locale {
    #[cfg(test)]
    if let Some(locale) = TEST_LOCALE.with(std::cell::Cell::get) {
        return locale;
    }
    LOCALE.get().copied().unwrap_or(Locale::English)
}

/// Translate an explicitly selected UI literal, falling back to the supplied original.
pub fn text(original: &str) -> &str {
    text_for(locale(), original)
}

pub fn text_for(locale: Locale, original: &str) -> &str {
    if locale == Locale::English {
        return original;
    }
    catalog::legacy_text(original).unwrap_or(original)
}

#[cfg(test)]
thread_local! {
    static TEST_LOCALE: std::cell::Cell<Option<Locale>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_locale<T>(locale: Locale, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Locale>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_LOCALE.with(|value| value.set(self.0));
        }
    }
    let _restore = Restore(TEST_LOCALE.with(|value| value.replace(Some(locale))));
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_language_controls_ui_and_environment_override_never_writes_configuration() {
        use crate::legacy_core::config::edit::ConfigEditsBuilder;
        use codex_config::ui_language::UiLanguage;
        let home = tempfile::tempdir().unwrap();
        assert_eq!(
            locale_from_user_settings(home.path(), None).unwrap(),
            Locale::English
        );
        for (saved, expected, rendered) in [
            (UiLanguage::Korean, Locale::Korean, "작업 중"),
            (UiLanguage::English, Locale::English, "Working"),
        ] {
            ConfigEditsBuilder::new(home.path())
                .set_ui_language(saved)
                .apply_blocking()
                .unwrap();
            assert_eq!(
                locale_from_user_settings(home.path(), None).unwrap(),
                expected
            );
            assert_eq!(
                text_for(
                    locale_from_user_settings(home.path(), None).unwrap(),
                    "Working"
                ),
                rendered
            );
            let path = home.path().join("config.toml");
            let bytes = std::fs::read(&path).unwrap();
            assert_eq!(
                locale_from_user_settings(home.path(), Some("en")).unwrap(),
                Locale::English
            );
            assert_eq!(
                locale_from_user_settings(home.path(), Some("ko")).unwrap(),
                Locale::Korean
            );
            assert!(locale_from_user_settings(home.path(), Some("kr")).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn language_selection_defaults_to_english_and_rejects_unknown_values() {
        assert_eq!(Locale::from_setting(None), Ok(Locale::English));
        assert_eq!(Locale::from_setting(Some("ko")), Ok(Locale::Korean));
        assert_eq!(Locale::from_setting(Some("en")), Ok(Locale::English));
        assert_eq!(Locale::from_setting(Some("KO")), Ok(Locale::Korean));
        assert_eq!(Locale::from_setting(Some("ko-KR")), Ok(Locale::Korean));
        assert_eq!(Locale::from_setting(Some("en-US")), Ok(Locale::English));
        for invalid in ["", "kr", "ja", "ko_kr"] {
            assert!(Locale::from_setting(Some(invalid)).is_err());
        }
    }

    #[test]
    fn english_and_unknown_text_preserve_the_original() {
        assert_eq!(text_for(Locale::Korean, "Working"), "작업 중");
        assert_eq!(text_for(Locale::English, "Working"), "Working");
        for token in [
            "/model",
            "read-only",
            "model",
            "request_id",
            "사용자 원문",
            "untranslated UI",
        ] {
            assert_eq!(text_for(Locale::Korean, token), token);
        }
    }

    #[test]
    fn memory_scope_labels_translate_without_changing_commands_or_paths() {
        assert_eq!(text_for(Locale::Korean, "Memory scope:"), "메모리 범위:");
        assert_eq!(text_for(Locale::Korean, "project"), "프로젝트");
        assert_eq!(text_for(Locale::English, "project"), "project");
        for opaque in [
            "/memory view",
            "thread/memory/note/add",
            "project_memories/key/memories_v2",
        ] {
            assert_eq!(text_for(Locale::Korean, opaque), opaque);
        }
    }

    #[test]
    fn scoped_language_restores_after_rendering() {
        with_locale(Locale::English, || {
            with_locale(Locale::Korean, || assert_eq!(text("Working"), "작업 중"));
            assert_eq!(text("Working"), "Working");
        });
    }
}
