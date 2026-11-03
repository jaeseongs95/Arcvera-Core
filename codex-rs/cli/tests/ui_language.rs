use std::path::Path;

use anyhow::Result;
use codex_config::ui_language::UiLanguage;
use codex_core::config::edit::ConfigEditsBuilder;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

fn base_command(home: &Path, project: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = std::process::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    cmd.env("CODEX_HOME", home)
        .env_remove("CODEX_UI_LANGUAGE")
        .current_dir(project);
    Ok(assert_cmd::Command::from_std(cmd))
}

fn command(home: &Path, project: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = base_command(home, project)?;
    cmd.arg("--help");
    Ok(cmd)
}

#[test]
fn saved_global_language_controls_help_across_new_processes_and_ignores_project_overrides()
-> Result<()> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir(project.path().join(".codex"))?;
    std::fs::write(
        project.path().join(".codex/config.toml"),
        "ui_language = 'ko'\n",
    )?;
    command(home.path(), project.path())?
        .assert()
        .success()
        .stdout(contains("Usage:"));
    for (language, heading) in [
        (UiLanguage::Korean, "사용법:"),
        (UiLanguage::English, "Usage:"),
    ] {
        ConfigEditsBuilder::new(home.path())
            .set_ui_language(language)
            .apply_blocking()?;
        let saved = std::fs::read(home.path().join("config.toml"))?;
        for _ in 0..2 {
            command(home.path(), project.path())?
                .assert()
                .success()
                .stdout(contains(heading));
        }
        // A CLI/session override must not become a display preference either.
        command(home.path(), project.path())?
            .args(["-c", "ui_language='ko'"])
            .assert()
            .success()
            .stdout(contains(heading));
        assert_eq!(std::fs::read(home.path().join("config.toml"))?, saved);
    }
    Ok(())
}

#[test]
fn language_environment_override_is_explicit_temporary_and_validated() -> Result<()> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    ConfigEditsBuilder::new(home.path())
        .set_ui_language(UiLanguage::English)
        .apply_blocking()?;
    let saved = std::fs::read(home.path().join("config.toml"))?;
    command(home.path(), project.path())?
        .env("CODEX_UI_LANGUAGE", "ko")
        .assert()
        .success()
        .stdout(contains("사용법:"));
    command(home.path(), project.path())?
        .env("CODEX_UI_LANGUAGE", "kr")
        .assert()
        .failure()
        .stderr(contains("Unsupported UI language"));
    command(home.path(), project.path())?
        .assert()
        .success()
        .stdout(contains("Usage:"));
    assert_eq!(std::fs::read(home.path().join("config.toml"))?, saved);
    std::fs::write(home.path().join("config.toml"), "ui_language = 'kr'\n")?;
    command(home.path(), project.path())?
        .assert()
        .failure()
        .stderr(contains("ui_language"));
    Ok(())
}

#[test]
fn normalized_environment_tags_select_help_and_parse_error_language_in_new_processes() -> Result<()>
{
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    let saved = "ui_language = 'en'\n";
    std::fs::write(home.path().join("config.toml"), saved)?;
    for (tag, usage, options, invalid) in [
        ("KO", "사용법:", "옵션:", "예상하지 않은 인수"),
        ("ko-KR", "사용법:", "옵션:", "예상하지 않은 인수"),
        ("en-US", "Usage:", "Options:", "unexpected argument"),
    ] {
        for args in [
            vec!["app-server", "--help"],
            vec!["app-server", "daemon", "start", "--help"],
            vec!["plugin", "add", "--help"],
        ] {
            base_command(home.path(), project.path())?
                .env("CODEX_UI_LANGUAGE", tag)
                .args(args)
                .assert()
                .success()
                .stdout(contains(usage).and(contains(options)));
        }
        let input = "--r35-Options:-error:-{value}-한글-🦀";
        base_command(home.path(), project.path())?
            .env("CODEX_UI_LANGUAGE", tag)
            .args(["app-server", input])
            .assert()
            .code(2)
            .stdout("")
            .stderr(
                contains(invalid)
                    .and(contains(input))
                    .and(contains("--help")),
            );
    }
    assert_eq!(
        std::fs::read_to_string(home.path().join("config.toml"))?,
        saved
    );
    Ok(())
}

#[test]
fn english_and_korean_keep_version_output_and_error_exit_status() -> Result<()> {
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    let mut versions = Vec::new();
    for tag in ["en", "ko"] {
        let output = base_command(home.path(), project.path())?
            .env("CODEX_UI_LANGUAGE", tag)
            .arg("--version")
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();
        versions.push(output);
        base_command(home.path(), project.path())?
            .env("CODEX_UI_LANGUAGE", tag)
            .args(["exec", "--color", "not-a-color"])
            .assert()
            .code(2)
            .stdout("")
            .stderr(contains("not-a-color").and(contains("--color")));
    }
    assert_eq!(versions[0], versions[1]);
    Ok(())
}

#[test]
fn feature_updates_localize_success_without_changing_identifiers_or_other_settings() -> Result<()> {
    for (language, enabled_message, disabled_message) in [
        (
            "en",
            "Enabled feature `unified_exec` in config.toml.\n",
            "Disabled feature `unified_exec` in config.toml.\n",
        ),
        (
            "ko",
            "config.toml에서 기능 `unified_exec`을 활성화했습니다.\n",
            "config.toml에서 기능 `unified_exec`을 비활성화했습니다.\n",
        ),
    ] {
        let codex_home = TempDir::new()?;
        let user_home = TempDir::new()?;
        let project = TempDir::new()?;
        let config_path = codex_home.path().join("config.toml");
        std::fs::write(
            &config_path,
            format!("ui_language = '{language}'\nmodel = 'r35-original-model-id'\n"),
        )?;
        for (action, message, enabled) in [
            ("enable", enabled_message, true),
            ("disable", disabled_message, false),
        ] {
            base_command(codex_home.path(), project.path())?
                .env("HOME", user_home.path())
                .args(["features", action, "unified_exec"])
                .assert()
                .success()
                .stdout(message)
                .stderr("");
            let saved: toml::Value = toml::from_str(&std::fs::read_to_string(&config_path)?)?;
            assert_eq!(saved["features"]["unified_exec"].as_bool(), Some(enabled));
            assert_eq!(saved["ui_language"].as_str(), Some(language));
            assert_eq!(saved["model"].as_str(), Some("r35-original-model-id"));
            assert!(!user_home.path().join(".codex/config.toml").exists());
            assert!(!project.path().join(".codex/config.toml").exists());
        }
    }
    Ok(())
}

#[test]
fn feature_errors_and_development_warnings_preserve_conditions_and_opaque_values() -> Result<()> {
    for (language, unknown_message, warning_message) in [
        (
            "en",
            "Unknown feature flag:",
            "Under-development features enabled:",
        ),
        (
            "ko",
            "알 수 없는 기능 플래그:",
            "개발 중인 기능을 활성화했습니다:",
        ),
    ] {
        let codex_home = TempDir::new()?;
        let user_home = TempDir::new()?;
        let project = TempDir::new()?;
        let config_path = codex_home.path().join("config.toml");
        let initial = format!("ui_language = '{language}'\nmodel = 'r35-original-model-id'\n");
        std::fs::write(&config_path, &initial)?;
        let unknown = "r35-unknown-{ $feature }-한글-🦀";
        for action in ["enable", "disable"] {
            base_command(codex_home.path(), project.path())?
                .env("HOME", user_home.path())
                .args(["features", action, unknown])
                .assert()
                .code(1)
                .stdout("")
                .stderr(contains(unknown_message).and(contains(unknown)));
            assert_eq!(std::fs::read_to_string(&config_path)?, initial);
        }
        // This R35 feature is under development. Only enabling it emits the
        // warning; disabling it and enabling a stable feature stay quiet.
        let feature = "tui_multi_backend";
        assert!(codex_features::FEATURES.iter().any(|spec| {
            spec.key == feature && matches!(spec.stage, codex_features::Stage::UnderDevelopment)
        }));
        base_command(codex_home.path(), project.path())?
            .env("HOME", user_home.path())
            .args(["features", "enable", feature])
            .assert()
            .success()
            .stdout(contains(feature).and(contains("config.toml")))
            .stderr(
                contains(warning_message)
                    .and(contains(feature))
                    .and(contains("suppress_unstable_features_warning = true"))
                    .and(contains(config_path.display().to_string())),
            );
        base_command(codex_home.path(), project.path())?
            .env("HOME", user_home.path())
            .args(["features", "disable", feature])
            .assert()
            .success()
            .stdout(contains(feature).and(contains("config.toml")))
            .stderr("");
        let saved: toml::Value = toml::from_str(&std::fs::read_to_string(&config_path)?)?;
        assert_eq!(saved["features"][feature].as_bool(), Some(false));
        assert_eq!(saved["ui_language"].as_str(), Some(language));
        assert_eq!(saved["model"].as_str(), Some("r35-original-model-id"));
        assert!(!user_home.path().join(".codex/config.toml").exists());
        assert!(!project.path().join(".codex/config.toml").exists());
    }
    Ok(())
}
