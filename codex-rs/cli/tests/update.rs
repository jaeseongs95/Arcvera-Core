// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md.
use anyhow::Result;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("arcvera")?);
    cmd.env("ARCVERA_HOME", codex_home);
    Ok(cmd)
}

#[tokio::test]
async fn update_does_not_start_interactive_prompt() -> Result<()> {
    let codex_home = TempDir::new()?;

    let expected_error = "Automatic updates are disabled in Arcvera Core";
    let missing_home = codex_home.path().join("does-not-exist");
    for args in [
        vec!["update"],
        vec!["update", "--check"],
        vec!["update", "--status"],
        vec!["update", "--enable-updates"],
        vec!["--no-alt-screen", "update"],
        vec!["-c", "example=true", "update"],
        vec!["app-server", "daemon", "update"],
        vec!["app-server", "daemon", "pid-update-loop"],
    ] {
        codex_command(&missing_home)?
            .args(args)
            .assert()
            .failure()
            .stderr(contains(expected_error));
    }

    let output = codex_command(&missing_home)?
        .args(["update", "--unknown-option"])
        .output()?;
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains(expected_error));

    Ok(())
}

#[cfg(windows)]
#[test]
fn disabled_update_works_with_case_variant_executable_name() -> Result<()> {
    let temp = TempDir::new()?;
    let executable = temp.path().join("ARCVERA.EXE");
    std::fs::copy(codex_utils_cargo_bin::cargo_bin("arcvera")?, &executable)?;
    assert_cmd::Command::new(executable)
        .env("ARCVERA_HOME", temp.path().join("missing"))
        .arg("update")
        .assert()
        .failure()
        .stderr(contains("Automatic updates are disabled in Arcvera Core"));
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[tokio::test]
async fn desktop_app_never_launches_or_installs() -> Result<()> {
    let temp = TempDir::new()?;
    let missing_home = temp.path().join("does-not-exist");
    for args in [
        vec!["app"],
        vec![
            "app",
            "missing-workspace",
            "--download-url",
            "http://127.0.0.1:1/must-not-fetch",
        ],
    ] {
        codex_command(&missing_home)?
            .args(args)
            .assert()
            .failure()
            .stderr(contains("Desktop app launch and installation are disabled"))
            .stderr(contains(
                "https://github.com/jaeseongs95/Arcvera-Core/releases",
            ));
        assert!(
            !missing_home.exists(),
            "disabled command must not create a home"
        );
    }
    Ok(())
}
