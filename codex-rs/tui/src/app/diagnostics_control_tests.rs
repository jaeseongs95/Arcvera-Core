// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
use super::*;

#[tokio::test]
async fn diagnostic_actions_report_saved_settings_without_changing_runtime_or_other_settings() {
    for locale in [Locale::English, Locale::Korean] {
        let home = tempfile::tempdir().unwrap();
        let current = codex_diagnostics::cache_diagnostics::initialized_process_mode();
        let initial = control_at_home(Action::LoggingStatus, home.path().to_path_buf(), locale)
            .await
            .unwrap();
        assert_eq!(initial["settings"]["saved_logging"]["enabled"], false);
        assert_eq!(std::fs::read_dir(home.path()).unwrap().count(), 0);
        let enabled = control_at_home(
            Action::LoggingSet { enabled: true },
            home.path().to_path_buf(),
            locale,
        )
        .await
        .unwrap();
        assert_eq!(enabled["settings"]["saved_logging"]["enabled"], true);
        assert_eq!(enabled["settings"]["saved_cache"]["response"], false);
        assert_eq!(
            enabled["settings"]["next_process_mode_if_environment_unchanged"]["response_enabled"],
            false
        );
        let cache = control_at_home(
            Action::CacheDiagnosticsSet {
                response: true,
                wire: true,
            },
            home.path().to_path_buf(),
            locale,
        )
        .await
        .unwrap();
        assert_eq!(cache["settings"]["saved_cache"]["wire"], true);
        assert_eq!(
            cache["settings"]["next_process_mode_if_environment_unchanged"]["wire_enabled"],
            true
        );
        assert_eq!(
            codex_diagnostics::cache_diagnostics::initialized_process_mode(),
            current
        );
        assert!(cache["message"].as_str().unwrap().contains(match locale {
            Locale::English => "next launch",
            Locale::Korean => "다음 실행",
        }));
        let before = std::fs::read(home.path().join("cli-cache-diagnostics.toml")).unwrap();
        let error = control_at_home(
            Action::CacheDiagnosticsSet {
                response: false,
                wire: true,
            },
            home.path().to_path_buf(),
            locale,
        )
        .await
        .unwrap_err();
        assert!(error.contains(match locale {
            Locale::English => "requires response",
            Locale::Korean => "응답 관측",
        }));
        assert_eq!(
            std::fs::read(home.path().join("cli-cache-diagnostics.toml")).unwrap(),
            before
        );
        let readback = control_at_home(
            Action::CacheDiagnosticsStatus,
            home.path().to_path_buf(),
            locale,
        )
        .await
        .unwrap();
        assert_eq!(readback["settings"]["saved_cache"]["wire"], true);
    }
}
