//! UI-only language selection. Never apply this dictionary to prompts, tool arguments,
//! slash command tokens, paths, or text supplied by the user or a server.
//!
//! `CODEX_UI_LANGUAGE=ko|en` overrides the client preference (with legacy base fallback).
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

/// Explain an automatic fallback without discarding either executable path.
pub fn daemon_selection_mismatch_warning(
    mismatch: &codex_app_server_daemon::CliDaemonMismatch,
) -> String {
    message(
        "daemon-auto-selected-mismatch",
        &[
            ("cli_path", &mismatch.cli_path.display().to_string()),
            ("managed_path", &mismatch.managed_path.display().to_string()),
        ],
    )
}

/// The socket identifies the contacted runtime, not the selected package path.
pub(crate) fn daemon_connection_mismatch_warning(
    cli_version: &str,
    server_version: Option<&str>,
    endpoint: &str,
) -> String {
    let unknown = message("status-unknown-version", &[]);
    let cli_path = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| unknown.clone());
    message(
        "daemon-implicit-version-mismatch",
        &[
            ("server", server_version.unwrap_or(&unknown)),
            ("client", cli_version),
            ("cli_path", &cli_path),
            ("endpoint", endpoint),
        ],
    )
}

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
    fn daemon_connection_mismatch_preserves_runtime_source_and_explicit_guidance() {
        let endpoint = r"unix://D:\공유 서버\control.sock";
        let cli = env!("CARGO_PKG_VERSION");
        let server = "0.166.0+native-r36.20261104";
        for (locale, refused, unknown_path) in [
            (
                Locale::English,
                "The connection was refused",
                "does not identify the running server executable path",
            ),
            (
                Locale::Korean,
                "연결을 거부했습니다",
                "실행 중인 서버 실행 파일 경로는 확인할 수 없습니다",
            ),
        ] {
            for version in [Some(server), None] {
                let warning = with_locale(locale, || {
                    daemon_connection_mismatch_warning(cli, version, endpoint)
                });
                for expected in [
                    endpoint,
                    cli,
                    refused,
                    unknown_path,
                    "--no-daemon",
                    "codex app-server daemon update --from-cli --yes --no-force",
                ] {
                    assert!(warning.contains(expected), "{warning}");
                }
                let unknown = with_locale(locale, || message("status-unknown-version", &[]));
                assert!(warning.contains(version.unwrap_or(&unknown)), "{warning}");
                assert!(warning.contains(&std::env::current_exe().unwrap().display().to_string()));
            }
        }
    }

    #[test]
    fn daemon_selection_mismatch_warning_preserves_paths_and_guidance_in_both_languages() {
        let mismatch = codex_app_server_daemon::CliDaemonMismatch {
            cli_path: r"D:\CLI 실행\R37 build\codex.exe".into(),
            managed_path: r"D:\공유 서버\old package\bin\codex.exe".into(),
        };
        for (locale, selected, client, preserved) in [
            (
                Locale::English,
                r"selected shared server executable D:\공유 서버\old package\bin\codex.exe",
                r"differs from this CLI D:\CLI 실행\R37 build\codex.exe",
                "the shared installation was preserved",
            ),
            (
                Locale::Korean,
                r"선택된 공유 서버 실행 파일 D:\공유 서버\old package\bin\codex.exe",
                r"현재 CLI D:\CLI 실행\R37 build\codex.exe와 다릅니다",
                "공유 설치본은 보존했습니다",
            ),
        ] {
            let warning = with_locale(locale, || daemon_selection_mismatch_warning(&mismatch));
            assert!(warning.contains(selected), "{warning}");
            assert!(warning.contains(client), "{warning}");
            assert!(warning.contains(preserved), "{warning}");
            assert!(warning.contains("codex app-server daemon update --from-cli --yes --no-force"));
        }
    }

    #[test]
    fn saved_language_controls_ui_and_environment_override_never_writes_configuration() {
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
            codex_config::ui_language::write_user_ui_language(home.path(), saved).unwrap();
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
            let path = home
                .path()
                .join(codex_config::ui_language::CLIENT_PREFERENCES_FILE);
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
    fn official_0167_ui_literals_translate_without_changing_whitespace_or_opaque_values() {
        for (english, korean) in [
            ("  Sign in another way", "  다른 방법으로 로그인"),
            ("Sign in another way", "다른 방법으로 로그인"),
            (
                "Use an API key or another provider",
                "API 키 또는 다른 제공자 사용",
            ),
            ("Use Google Cloud Vertex AI", "Google Cloud Vertex AI 사용"),
            (
                "Connect using your Google Cloud credentials",
                "Google Cloud 자격 증명으로 연결",
            ),
            (" to go back", " 돌아가기"),
            (" to review requests", " 요청 검토"),
            (
                "> Set up Google Cloud Vertex AI",
                "> Google Cloud Vertex AI 설정",
            ),
            (
                "Enter the Google Cloud project ID to use with Vertex AI.",
                "Vertex AI에서 사용할 Google Cloud 프로젝트 ID를 입력하세요.",
            ),
            ("  Project ID: ", "  프로젝트 ID: "),
            (
                "Press Enter to use global, or enter a Vertex AI location.",
                "global을 사용하려면 Enter를 누르거나 Vertex AI 위치를 입력하세요.",
            ),
            ("  Location (optional): ", "  위치(선택): "),
            (
                "Enter an absolute file path on the host running Codex, or press Enter to use Application Default Credentials (ADC).",
                "Codex가 실행 중인 호스트의 절대 파일 경로를 입력하거나 Enter를 눌러 애플리케이션 기본 사용자 인증 정보(ADC)를 사용하세요.",
            ),
            ("  Credential file (optional): ", "  자격 증명 파일(선택): "),
            (
                "Validating credentials and saving configuration…",
                "자격 증명 확인 및 설정 저장 중…",
            ),
            (
                "  To set up ADC with the Google Cloud CLI, run",
                "  Google Cloud CLI로 ADC를 설정하려면 다음 명령을 실행하세요",
            ),
            (" set up", " 설정"),
            (" continue", " 계속"),
            (" back", " 뒤로"),
            (
                "Enter a Google Cloud project ID.",
                "Google Cloud 프로젝트 ID를 입력하세요.",
            ),
            (
                "✓ Google Cloud Vertex AI configured",
                "✓ Google Cloud Vertex AI 설정 완료",
            ),
            (" continue in new chat · ", " 새 대화에서 계속 · "),
            (" start fresh · ", " 새로 시작 · "),
            (" other chats", " 다른 대화"),
            ("Continue in a new chat", "새 대화에서 계속"),
            ("Model provider mismatch", "모델 제공자 불일치"),
            ("Model provider unavailable", "모델 제공자 사용 불가"),
            (
                "Model provider blocked by policy",
                "정책에 의해 차단된 모델 제공자",
            ),
        ] {
            assert_eq!(text_for(Locale::Korean, english), korean, "{english:?}");
            assert_eq!(text_for(Locale::English, english), english, "{english:?}");
        }
        // "global" is an existing UI scope label; provider data bypasses this API.
        assert_eq!(text_for(Locale::Korean, "global"), "전역");
        assert_eq!(text_for(Locale::English, "global"), "global");
        for opaque in [
            "my-project",
            "  gcloud auth application-default login",
            r"D:\자격 증명\provider.json",
            "server error { $provider }",
        ] {
            assert_eq!(text_for(Locale::Korean, opaque), opaque);
            assert_eq!(text_for(Locale::English, opaque), opaque);
        }
    }

    #[test]
    fn official_0167_provider_messages_preserve_named_arguments_in_both_languages() {
        let provider = "제공자 { $current } 🦀";
        let current = "current \"provider\"";
        let required = "required { $provider }";
        let address = "host.example:1234/{ $address }";
        for (id, args, english, korean) in [
            (
                "dialogs-provider-mismatch",
                vec![("provider", provider), ("current", current)],
                format!(
                    "This chat uses {provider}, but Codex now uses {current}, so this chat is read-only."
                ),
                format!(
                    "이 대화는 {provider}을(를) 사용하지만 현재 Codex는 {current}을(를) 사용하므로 이 대화는 읽기 전용입니다."
                ),
            ),
            (
                "dialogs-provider-not-configured",
                vec![("provider", provider)],
                format!(
                    "This chat uses model provider “{provider}”, which is no longer configured. Restore this provider’s configuration to continue."
                ),
                format!(
                    "이 대화에서 사용하는 모델 제공자 “{provider}”가 더 이상 구성되어 있지 않습니다. 계속하려면 이 제공자의 설정을 복원하세요."
                ),
            ),
            (
                "dialogs-provider-policy-conflict",
                vec![("provider", provider), ("required", required)],
                format!(
                    "This chat uses model provider “{provider}”, but your organization requires “{required}”. Contact your administrator or start a new chat using the required provider."
                ),
                format!(
                    "이 대화는 모델 제공자 “{provider}”을(를) 사용하지만 조직에서는 “{required}”을(를) 요구합니다. 관리자에게 문의하거나 필요한 제공자로 새 대화를 시작하세요."
                ),
            ),
            (
                "dialogs-provider-configuration-location",
                vec![("address", address)],
                format!("Configuration on {address}"),
                format!("{address}의 설정"),
            ),
        ] {
            assert_eq!(message_for(Locale::English, id, &args), english, "{id}");
            assert_eq!(message_for(Locale::Korean, id, &args), korean, "{id}");
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
