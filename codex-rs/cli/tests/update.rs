use anyhow::Result;
use predicates::str::contains;
use std::path::Path;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
}

#[cfg(debug_assertions)]
#[tokio::test]
async fn update_does_not_start_interactive_prompt() -> Result<()> {
    let codex_home = TempDir::new()?;

    let expected_error = if codex_tui::is_opendiy_version(env!("CARGO_PKG_VERSION")) {
        "OpenDIY update requires an installation under"
    } else {
        "`codex update` is not available in debug builds"
    };
    codex_command(codex_home.path())?
        .arg("update")
        .assert()
        .failure()
        .stderr(contains(expected_error));

    Ok(())
}
