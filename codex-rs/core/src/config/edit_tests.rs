// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R38 (2026-11-06); see MODIFICATIONS.md.
use super::*;
use codex_config::types::AppToolApproval;
use codex_config::types::McpServerOAuthConfig;
use codex_config::types::McpServerToolConfig;
use codex_config::types::McpServerTransportConfig;
use codex_config::types::SessionPickerViewMode;
use codex_protocol::config_types::SERVICE_TIER_DEFAULT_REQUEST_VALUE;
use codex_protocol::config_types::ServiceTier;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;
#[cfg(unix)]
use std::os::unix::fs::symlink;
use tempfile::tempdir;
use toml::Value as TomlValue;

#[cfg(windows)]
#[tokio::test]
async fn existing_config_case_alias_matches_locked_target() {
    let dir = tempdir().unwrap();
    let lower = dir.path().join("config.toml");
    let upper = dir.path().join("CONFIG.TOML");
    std::fs::write(&lower, "model = 'before'\n").unwrap();
    let transaction = ConfigEditTransaction::acquire(&upper).await.unwrap();
    let paths = transaction.checked_write_paths_for(&lower).unwrap();
    assert_eq!(
        std::fs::read_to_string(paths.read_path.unwrap()).unwrap(),
        "model = 'before'\n"
    );
    let expected: TomlValue = toml::from_str("model = 'before'\n").unwrap();
    ConfigEditsBuilder::for_config_path(&upper)
        .with_edits([ConfigEdit::SetPath {
            segments: vec!["model".into()],
            value: value("after"),
        }])
        .apply_with_transaction(transaction, Some(expected))
        .await
        .unwrap();
    let saved: TomlValue = toml::from_str(&std::fs::read_to_string(lower).unwrap()).unwrap();
    assert_eq!(saved["model"].as_str(), Some("after"));
}

#[cfg(unix)]
#[tokio::test]
async fn locked_target_replaced_by_symlink_is_not_equivalent_to_new_target() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.toml");
    let new = dir.path().join("new.toml");
    std::fs::write(&target, "model = 'old'\n").unwrap();
    std::fs::write(&new, "model = 'external'\n").unwrap();
    let guard = ConfigEditTransaction::acquire(&target).await.unwrap();
    std::fs::remove_file(&target).unwrap();
    symlink(&new, &target).unwrap();
    let result = ConfigEditsBuilder::for_config_path(&target)
        .with_edits([ConfigEdit::SetPath {
            segments: vec!["model".into()],
            value: value("wrong"),
        }])
        .apply_with_transaction(guard, None)
        .await;
    assert!(result.is_err());
    assert!(
        std::fs::symlink_metadata(target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_to_string(new).unwrap(),
        "model = 'external'\n"
    );
}

// Child-process-only synchronization for the real lock/CAS regression below.
// Ordinary tests never set these environment variables.
pub(super) fn config_alias_test_event(event: &str, wait_for_release: bool) {
    let (Ok(dir), Ok(role)) = (
        std::env::var("CODEX_CONFIG_ALIAS_TEST_DIR"),
        std::env::var("CODEX_CONFIG_ALIAS_TEST_ROLE"),
    ) else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::write(dir.join(format!("{role}_{event}")), "event").unwrap();
    if wait_for_release {
        wait_for_config_alias_event(&dir.join(format!("release_{role}")));
    }
}

fn wait_for_config_alias_event(path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[cfg(any(unix, windows))]
#[tokio::test]
#[ignore = "child process helper"]
async fn config_alias_process_worker() {
    let role = std::env::var("CODEX_CONFIG_ALIAS_TEST_ROLE").expect("child role is required");
    let dir = PathBuf::from(
        std::env::var("CODEX_CONFIG_ALIAS_TEST_DIR").expect("child fixture directory is required"),
    );
    let path = dir.join(format!("{role}.toml"));
    let expected: TomlValue = toml::from_str("model = 'initial'\n").unwrap();
    let result = ConfigEditsBuilder::for_config_path(&path)
        .with_edits([ConfigEdit::SetPath {
            segments: vec!["model".into()],
            value: value(role.clone()),
        }])
        .apply_checked_with_user_config(expected)
        .await;
    let outcome = match result {
        Ok(_) => "success",
        Err(error) if error.is::<ConfigEditVersionConflict>() => "conflict",
        Err(error) => panic!("unexpected writer error: {error:#}"),
    };
    std::fs::write(dir.join(format!("{role}_done")), outcome).unwrap();
}

#[cfg(any(unix, windows))]
struct AliasTestFixture(Option<tempfile::TempDir>);

#[cfg(any(unix, windows))]
impl AliasTestFixture {
    fn path(&self) -> &Path {
        self.0.as_ref().unwrap().path()
    }
}

#[cfg(any(unix, windows))]
impl Drop for AliasTestFixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            let path = self.0.take().unwrap().keep();
            eprintln!("config alias test fixture retained at {}", path.display());
        }
    }
}

#[cfg(any(unix, windows))]
struct AliasTestChild(Option<std::process::Child>);

#[cfg(any(unix, windows))]
impl AliasTestChild {
    fn finish(&mut self) -> std::process::ExitStatus {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(status) = self.0.as_mut().unwrap().try_wait().unwrap() {
                self.0.take();
                return status;
            }
            assert!(std::time::Instant::now() < deadline, "child did not exit");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

#[cfg(any(unix, windows))]
impl Drop for AliasTestChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            match child.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => {
                    if let Err(err) = child.kill() {
                        eprintln!("failed to kill config alias test child: {err}");
                    }
                }
                Err(err) => {
                    eprintln!("failed to inspect config alias test child: {err}");
                    if let Err(err) = child.kill() {
                        eprintln!("failed to kill config alias test child: {err}");
                    }
                }
            }
            if let Err(err) = child.wait() {
                eprintln!("failed to reap config alias test child: {err}");
            }
        }
    }
}

#[cfg(any(unix, windows))]
#[test]
fn independent_alias_processes_lock_and_check_same_initial_document() {
    use std::process::Command;
    use std::process::Stdio;
    let dir = AliasTestFixture(Some(tempdir().unwrap()));
    let target = dir.path().join("target.toml");
    std::fs::write(&target, "model = 'initial'\n").unwrap();
    config_test_symlink(&target, &dir.path().join("a.toml"));
    config_test_symlink(&target, &dir.path().join("b.toml"));
    let spawn = |role: &str| {
        let stdout = std::fs::File::create(dir.path().join(format!("{role}_stdout.txt"))).unwrap();
        let stderr = std::fs::File::create(dir.path().join(format!("{role}_stderr.txt"))).unwrap();
        AliasTestChild(Some(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--ignored",
                    "--exact",
                    "config::edit::tests::config_alias_process_worker",
                    "--nocapture",
                ])
                .env("CODEX_CONFIG_ALIAS_TEST_DIR", dir.path())
                .env("CODEX_CONFIG_ALIAS_TEST_ROLE", role)
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .spawn()
                .unwrap(),
        ))
    };
    let mut a = spawn("a");
    wait_for_config_alias_event(&dir.path().join("a_cas"));
    let mut b = spawn("b");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !dir.path().join("b_blocked").exists() && !dir.path().join("b_acquired").exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "second writer did not try the OS lock"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let blocked = dir.path().join("b_blocked").exists();
    if !blocked {
        // In the broken candidate both CAS checks complete before either write.
        wait_for_config_alias_event(&dir.path().join("b_cas"));
    }
    std::fs::write(dir.path().join("release_a"), "release").unwrap();
    wait_for_config_alias_event(&dir.path().join("a_done"));
    std::fs::write(dir.path().join("release_b"), "release").unwrap();
    wait_for_config_alias_event(&dir.path().join("b_done"));
    assert!(a.finish().success());
    assert!(b.finish().success());
    assert!(
        blocked,
        "second process acquired a different lock before the first write"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a_done")).unwrap(),
        "success"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("b_done")).unwrap(),
        "conflict"
    );
    let saved: TomlValue = toml::from_str(&std::fs::read_to_string(target).unwrap()).unwrap();
    assert_eq!(saved["model"].as_str(), Some("a"));
}

#[tokio::test]
async fn written_document_result_describes_the_exact_serialized_edit() {
    let home = tempdir().unwrap();
    let path = home.path().join(CONFIG_TOML_FILE);
    std::fs::write(&path, "[mcp_servers.fixture]\ncommand = 'example'\n").unwrap();
    let result = ConfigEditsBuilder::for_config_path(&path)
        .with_edits([ConfigEdit::ClearPath {
            segments: vec!["mcp_servers".into(), "fixture".into()],
        }])
        .apply_with_user_config()
        .await
        .unwrap()
        .unwrap();
    let bytes = std::fs::read_to_string(&path).unwrap();
    assert_eq!(result, toml::from_str::<TomlValue>(&bytes).unwrap());
    assert!(result.get("mcp_servers").is_none());
    std::fs::write(&path, "model = 'another-writer'\n").unwrap();
    assert!(result.get("model").is_none());
}

#[tokio::test]
async fn checked_edit_rejects_changed_disk_document_without_writing() {
    let home = tempdir().unwrap();
    let path = home.path().join(CONFIG_TOML_FILE);
    let expected: TomlValue = toml::from_str("model = 'before'\n").unwrap();
    let current = "# concurrent writer\nmodel = 'after'\n";
    std::fs::write(&path, current).unwrap();
    let error = ConfigEditsBuilder::for_config_path(&path)
        .with_edits([ConfigEdit::SetPath {
            segments: vec!["model".into()],
            value: value("rejected"),
        }])
        .apply_checked_with_user_config(expected)
        .await
        .unwrap_err();
    assert!(error.is::<ConfigEditVersionConflict>());
    assert_eq!(std::fs::read_to_string(path).unwrap(), current);
}

#[tokio::test]
async fn cooperating_editors_serialize_and_keep_both_sibling_keys() {
    let home = tempdir().unwrap();
    let path = home.path().join(CONFIG_TOML_FILE);
    std::fs::write(&path, "# 보존\n[notice]\n").unwrap();
    let first = ConfigEditsBuilder::for_config_path(&path).with_edits([ConfigEdit::SetPath {
        segments: vec!["notice".into(), "hide_full_access_warning".into()],
        value: value(true),
    }]);
    let second = ConfigEditsBuilder::for_config_path(&path).with_edits([ConfigEdit::SetPath {
        segments: vec!["notice".into(), "hide_rate_limit_model_nudge".into()],
        value: value(true),
    }]);
    let (a, b) = tokio::join!(first.apply(), second.apply());
    a.unwrap();
    b.unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("# 보존"));
    let parsed: TomlValue = toml::from_str(&saved).unwrap();
    assert_eq!(
        parsed["notice"]["hide_full_access_warning"].as_bool(),
        Some(true)
    );
    assert_eq!(
        parsed["notice"]["hide_rate_limit_model_nudge"].as_bool(),
        Some(true)
    );
}

#[tokio::test]
async fn started_blocking_edit_keeps_lock_after_waiter_cancellation() {
    let home = tempdir().unwrap();
    let path = home.path().join(CONFIG_TOML_FILE);
    std::fs::write(&path, "model = 'old'\n").unwrap();
    let transaction = ConfigEditTransaction::acquire(&path).await.unwrap();
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let handle = task::spawn_blocking(move || {
        entered_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        apply_blocking_to_resolved_file(
            &transaction,
            &[ConfigEdit::SetPath {
                segments: vec!["model".into()],
                value: value("finished"),
            }],
            None,
        )
        .unwrap();
    });
    entered_rx.await.unwrap();
    handle.abort(); // Started blocking jobs do not stop; they must retain the guard.
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            ConfigEditTransaction::acquire(&path)
        )
        .await
        .is_err()
    );
    release_tx.send(()).unwrap();
    let _ = handle.await;
    let _next = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        ConfigEditTransaction::acquire(&path),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(std::fs::read_to_string(&path).unwrap().contains("finished"));
}

#[cfg(unix)]
#[tokio::test]
async fn config_lock_refuses_symlink() {
    let home = tempdir().unwrap();
    let path = home.path().join(CONFIG_TOML_FILE);
    let target = home.path().join("other");
    std::fs::write(&target, "do not touch").unwrap();
    symlink(&target, home.path().join("config.toml.lock")).unwrap();
    assert!(ConfigEditTransaction::acquire(&path).await.is_err());
    assert_eq!(std::fs::read_to_string(target).unwrap(), "do not touch");
}

#[test]
fn ui_language_persists_across_fresh_reads_without_changing_other_global_settings() {
    use codex_config::ui_language::UiLanguage;
    use codex_config::ui_language::read_user_ui_language;
    let home = tempdir().unwrap();
    let path = home.path().join(CONFIG_TOML_FILE);
    let original = "# keep this comment and formatting\nmodel = 'existing-model'\napproval_policy = 'on-request'\nsandbox_mode = 'read-only'\nui_language = \"en\"\n\n[profiles.work]\nmodel = 'profile-model'\n\n[tui]\nnotifications = false\n";
    std::fs::write(&path, original).unwrap();
    let original_values: TomlValue = toml::from_str(original).unwrap();
    for language in [UiLanguage::Korean, UiLanguage::English] {
        ConfigEditsBuilder::new(home.path())
            .set_ui_language(language)
            .apply_blocking()
            .unwrap();
        let saved = std::fs::read_to_string(&path).unwrap();
        // All bytes outside the edited value, including comments, remain unchanged.
        assert_eq!(
            saved,
            original.replace(
                "ui_language = \"en\"",
                &format!("ui_language = \"{}\"", language.as_str())
            )
        );
        let values: TomlValue = toml::from_str(&saved).unwrap();
        for key in [
            "model",
            "approval_policy",
            "sandbox_mode",
            "profiles",
            "tui",
        ] {
            assert_eq!(values[key], original_values[key]);
        }
        // A new file read is independent of the old UI's OnceLock and survives restart.
        assert_eq!(read_user_ui_language(home.path()).unwrap(), language);
        assert_eq!(read_user_ui_language(home.path()).unwrap(), language);
    }
    let unchanged = std::fs::read(&path).unwrap();
    for invalid in ["", "ko--KR", "kr", "en ko"] {
        assert!(
            UiLanguage::parse(invalid).is_err(),
            "unexpectedly accepted invalid tag: {invalid:?}"
        );
        assert_eq!(std::fs::read(&path).unwrap(), unchanged);
    }
}

#[test]
fn ui_language_saves_to_base_user_file_and_ignores_project_profile_and_named_layers() {
    use codex_config::ui_language::UiLanguage;
    use codex_config::ui_language::read_user_ui_language;
    let home = tempdir().unwrap();
    let project = tempdir().unwrap();
    let project_config = project.path().join("config.toml");
    let named_config = home.path().join("work.config.toml");
    let non_global = "ui_language = 'ko'\nmodel = 'leave-this-alone'\n";
    std::fs::write(&project_config, non_global).unwrap();
    std::fs::write(&named_config, non_global).unwrap();
    std::fs::write(
        home.path().join(CONFIG_TOML_FILE),
        "[profiles.work]\nui_language = 'ko'\n",
    )
    .unwrap();
    ConfigEditsBuilder::new(home.path())
        .set_ui_language(UiLanguage::English)
        .apply_blocking()
        .unwrap();
    assert_eq!(
        read_user_ui_language(home.path()).unwrap(),
        UiLanguage::English
    );
    assert_eq!(
        std::fs::read_to_string(&project_config).unwrap(),
        non_global
    );
    assert_eq!(std::fs::read_to_string(&named_config).unwrap(), non_global);
    let global: TomlValue =
        toml::from_str(&std::fs::read_to_string(home.path().join(CONFIG_TOML_FILE)).unwrap())
            .unwrap();
    assert_eq!(global["ui_language"].as_str(), Some("en"));
    assert_eq!(
        global["profiles"]["work"]["ui_language"].as_str(),
        Some("ko")
    );
}

#[test]
fn blocking_set_model_top_level() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    apply_blocking(
        codex_home,
        &[ConfigEdit::SetModel {
            model: Some("gpt-5.4".to_string()),
            effort: Some(ReasoningEffort::High),
        }],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"model = "gpt-5.4"
model_reasoning_effort = "high"
"#;
    assert_eq!(contents, expected);
}

#[test]
fn set_service_tier_saves_default_as_default() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .set_service_tier(Some(SERVICE_TIER_DEFAULT_REQUEST_VALUE.to_string()))
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    assert_eq!(contents, "service_tier = \"default\"\n");
}

#[test]
fn set_service_tier_saves_priority_as_fast() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .set_service_tier(Some(ServiceTier::Fast.request_value().to_string()))
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    assert_eq!(contents, "service_tier = \"fast\"\n");
}

#[test]
fn set_service_tier_preserves_unknown_service_tier() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .set_service_tier(Some("experimental-tier-id".to_string()))
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    assert_eq!(contents, "service_tier = \"experimental-tier-id\"\n");
}

#[test]
fn builder_with_edits_applies_custom_paths() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .with_edits(vec![ConfigEdit::SetPath {
            segments: vec!["enabled".to_string()],
            value: value(true),
        }])
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    assert_eq!(contents, "enabled = true\n");
}

/// Toggling multi-agent v2 must preserve settings stored in its feature table.
#[test]
fn multi_agent_v2_feature_toggle_preserves_nested_configuration() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    let config_path = codex_home.join(CONFIG_TOML_FILE);
    std::fs::write(
        &config_path,
        "[features.multi_agent_v2]\nenabled = true\nsubagent_usage_hint_text = \"Delegate carefully.\"\n",
    )
    .expect("write config");

    ConfigEditsBuilder::new(codex_home)
        .set_feature_enabled("multi_agent_v2", /*enabled*/ false)
        .apply_blocking()
        .expect("disable feature");
    let disabled: TomlValue =
        toml::from_str(&std::fs::read_to_string(&config_path).expect("read disabled config"))
            .expect("parse disabled config");
    assert_eq!(
        disabled,
        toml::from_str::<TomlValue>(
            "[features.multi_agent_v2]\nenabled = false\nsubagent_usage_hint_text = \"Delegate carefully.\"\n",
        )
        .expect("parse expected config")
    );

    ConfigEditsBuilder::new(codex_home)
        .set_feature_enabled("multi_agent_v2", /*enabled*/ true)
        .apply_blocking()
        .expect("enable feature");
    let enabled: TomlValue =
        toml::from_str(&std::fs::read_to_string(&config_path).expect("read enabled config"))
            .expect("parse enabled config");
    assert_eq!(
        enabled,
        toml::from_str::<TomlValue>(
            "[features.multi_agent_v2]\nenabled = true\nsubagent_usage_hint_text = \"Delegate carefully.\"\n",
        )
        .expect("parse expected config")
    );
}

#[test]
fn sleep_tool_feature_toggle_preserves_mode() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    let config_path = codex_home.join(CONFIG_TOML_FILE);
    std::fs::write(
        &config_path,
        "[features.sleep_tool]\nmode = \"always_on\"\n",
    )
    .expect("write config");

    for enabled in [false, true] {
        ConfigEditsBuilder::new(codex_home)
            .set_feature_enabled("sleep_tool", enabled)
            .apply_blocking()
            .expect("toggle feature");
        let actual: TomlValue =
            toml::from_str(&std::fs::read_to_string(&config_path).expect("read config"))
                .expect("parse config");
        let expected: TomlValue = toml::from_str(&format!(
            "[features.sleep_tool]\nenabled = {enabled}\nmode = \"always_on\"\n",
        ))
        .expect("parse expected config");
        assert_eq!(actual, expected);
    }
}

#[test]
fn network_proxy_feature_toggle_preserves_credential_broker_configuration() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    let config_path = codex_home.join(CONFIG_TOML_FILE);
    for (initial, disabled, enabled_again) in [
        (
            "[features]\nnetwork_proxy = true\n",
            "[features]\n",
            "[features]\nnetwork_proxy = true\n",
        ),
        (
            "[features.network_proxy]\nenabled = true\ncredential_broker = true\ndomains = { \"github.com\" = \"allow\" }\n",
            "[features.network_proxy]\nenabled = false\ncredential_broker = true\ndomains = { \"github.com\" = \"allow\" }\n",
            "[features.network_proxy]\nenabled = true\ncredential_broker = true\ndomains = { \"github.com\" = \"allow\" }\n",
        ),
        (
            "[features.network_proxy]\nenabled = true\ncredentials = { vendor = { env = [\"VENDOR_TOKEN\"] } }\n",
            "[features.network_proxy]\nenabled = false\ncredentials = { vendor = { env = [\"VENDOR_TOKEN\"] } }\n",
            "[features.network_proxy]\nenabled = true\ncredentials = { vendor = { env = [\"VENDOR_TOKEN\"] } }\n",
        ),
    ] {
        std::fs::write(&config_path, initial).expect("write config");
        for (enabled, expected) in [(false, disabled), (true, enabled_again)] {
            ConfigEditsBuilder::new(codex_home)
                .set_feature_enabled("network_proxy", enabled)
                .apply_blocking()
                .expect("toggle feature");
            let updated: TomlValue =
                toml::from_str(&std::fs::read_to_string(&config_path).expect("read config"))
                    .expect("parse config");
            assert_eq!(updated, toml::from_str::<TomlValue>(expected).unwrap());
        }
    }
}

/// Adding nested multi-agent settings must retain an existing legacy boolean toggle.
#[test]
fn multi_agent_v2_nested_edit_preserves_legacy_boolean_toggle() {
    for feature_path in ["features", "profiles.work.features"] {
        let tmp = tempdir().expect("tmpdir");
        let codex_home = tmp.path();
        let config_path = codex_home.join(CONFIG_TOML_FILE);
        std::fs::write(
            &config_path,
            format!("[{feature_path}]\nmulti_agent_v2 = true\n"),
        )
        .expect("write config");
        let mut feature_segments = feature_path
            .split('.')
            .map(str::to_string)
            .collect::<Vec<_>>();
        feature_segments.push("multi_agent_v2".to_string());
        let mut instruction_segments = feature_segments.clone();
        instruction_segments.push("subagent_usage_hint_text".to_string());

        ConfigEditsBuilder::new(codex_home)
            .with_edits([ConfigEdit::SetPath {
                segments: instruction_segments,
                value: value("Delegate carefully."),
            }])
            .apply_blocking()
            .expect("persist nested config");

        let updated: TomlValue =
            toml::from_str(&std::fs::read_to_string(&config_path).expect("read config"))
                .expect("parse config");
        assert_eq!(
            updated,
            toml::from_str::<TomlValue>(&format!(
                "[{feature_path}.multi_agent_v2]\nenabled = true\nsubagent_usage_hint_text = \"Delegate carefully.\"\n",
            ))
            .expect("parse expected config")
        );

        ConfigEditsBuilder::new(codex_home)
            .with_edits([ConfigEdit::SetPath {
                segments: feature_segments.clone(),
                value: value(false),
            }])
            .apply_blocking()
            .expect("disable feature");

        let disabled: TomlValue =
            toml::from_str(&std::fs::read_to_string(&config_path).expect("read config"))
                .expect("parse config");
        assert_eq!(
            disabled,
            toml::from_str::<TomlValue>(&format!(
                "[{feature_path}.multi_agent_v2]\nenabled = false\nsubagent_usage_hint_text = \"Delegate carefully.\"\n",
            ))
            .expect("parse expected config")
        );

        ConfigEditsBuilder::new(codex_home)
            .with_edits([ConfigEdit::ClearPath {
                segments: feature_segments,
            }])
            .apply_blocking()
            .expect("clear feature toggle");

        let cleared: TomlValue =
            toml::from_str(&std::fs::read_to_string(&config_path).expect("read config"))
                .expect("parse config");
        assert_eq!(
            feature_path
                .split('.')
                .try_fold(&cleared, |config, segment| config.get(segment))
                .and_then(|features| features.get("multi_agent_v2")),
            None
        );
    }
}

#[test]
fn session_picker_view_edit_writes_root_tui_setting() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .with_edits([session_picker_view_edit(SessionPickerViewMode::Dense)])
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[tui]
session_picker_view = "dense"
"#;
    assert_eq!(contents, expected);
}

#[test]
fn keymap_binding_edit_writes_root_action_binding() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .with_edits([keymap_binding_edit("composer", "submit", "ctrl-enter")])
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[tui.keymap.composer]
submit = "ctrl-enter"
"#;
    assert_eq!(contents, expected);
}

#[test]
fn keymap_bindings_edit_writes_single_binding_as_string() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .with_edits([keymap_bindings_edit(
            "composer",
            "submit",
            &["ctrl-enter".to_string()],
        )])
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[tui.keymap.composer]
submit = "ctrl-enter"
"#;
    assert_eq!(contents, expected);
}

#[test]
fn keymap_bindings_edit_writes_multiple_bindings_as_array() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .with_edits([keymap_bindings_edit(
            "composer",
            "submit",
            &["enter".to_string(), "ctrl-enter".to_string()],
        )])
        .apply_blocking()
        .expect("persist");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let value: TomlValue = toml::from_str(&raw).expect("parse config");

    assert_eq!(
        value
            .get("tui")
            .and_then(|value| value.get("keymap"))
            .and_then(|value| value.get("composer"))
            .and_then(|value| value.get("submit"))
            .and_then(TomlValue::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(TomlValue::as_str)
                    .collect::<Vec<_>>()
            }),
        Some(vec!["enter", "ctrl-enter"])
    );
}

#[test]
fn keymap_binding_edit_replaces_existing_binding_without_touching_profile() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"profile = "team"

[tui.keymap.composer]
submit = "enter"

[profiles.team.tui.keymap.composer]
submit = "shift-enter"
"#,
    )
    .expect("seed config");

    ConfigEditsBuilder::new(codex_home)
        .with_edits([keymap_binding_edit("composer", "submit", "ctrl-enter")])
        .apply_blocking()
        .expect("persist");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let value: TomlValue = toml::from_str(&raw).expect("parse config");

    assert_eq!(
        value
            .get("tui")
            .and_then(|value| value.get("keymap"))
            .and_then(|value| value.get("composer"))
            .and_then(|value| value.get("submit"))
            .and_then(TomlValue::as_str),
        Some("ctrl-enter")
    );
    assert_eq!(
        value
            .get("profiles")
            .and_then(|value| value.get("team"))
            .and_then(|value| value.get("tui"))
            .and_then(|value| value.get("keymap"))
            .and_then(|value| value.get("composer"))
            .and_then(|value| value.get("submit"))
            .and_then(TomlValue::as_str),
        Some("shift-enter")
    );
}

#[test]
fn keymap_binding_clear_edit_removes_root_action_binding_without_touching_profile() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"profile = "team"

[tui.keymap.composer]
submit = "enter"

[profiles.team.tui.keymap.composer]
submit = "shift-enter"
"#,
    )
    .expect("seed config");

    ConfigEditsBuilder::new(codex_home)
        .with_edits([keymap_binding_clear_edit("composer", "submit")])
        .apply_blocking()
        .expect("persist");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let value: TomlValue = toml::from_str(&raw).expect("parse config");

    assert_eq!(
        value
            .get("tui")
            .and_then(|value| value.get("keymap"))
            .and_then(|value| value.get("composer"))
            .and_then(|value| value.get("submit")),
        None
    );
    assert_eq!(
        value
            .get("profiles")
            .and_then(|value| value.get("team"))
            .and_then(|value| value.get("tui"))
            .and_then(|value| value.get("keymap"))
            .and_then(|value| value.get("composer"))
            .and_then(|value| value.get("submit"))
            .and_then(TomlValue::as_str),
        Some("shift-enter")
    );
}

#[test]
fn set_model_availability_nux_count_writes_shown_count() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    let shown_count = HashMap::from([("gpt-foo".to_string(), 4)]);

    ConfigEditsBuilder::new(codex_home)
        .set_model_availability_nux_count(&shown_count)
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[tui.model_availability_nux]
gpt-foo = 4
"#;
    assert_eq!(contents, expected);
}

#[test]
fn set_skill_config_writes_disabled_entry() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .with_edits([ConfigEdit::SetSkillConfig {
            path: PathBuf::from("/tmp/skills/demo/SKILL.md"),
            enabled: false,
        }])
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[[skills.config]]
path = "/tmp/skills/demo/SKILL.md"
enabled = false
"#;
    assert_eq!(contents, expected);
}

#[test]
fn set_skill_config_removes_entry_when_enabled() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[[skills.config]]
path = "/tmp/skills/demo/SKILL.md"
enabled = false
"#,
    )
    .expect("seed config");

    ConfigEditsBuilder::new(codex_home)
        .with_edits([ConfigEdit::SetSkillConfig {
            path: PathBuf::from("/tmp/skills/demo/SKILL.md"),
            enabled: true,
        }])
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    assert_eq!(contents, "");
}

#[test]
fn set_skill_config_writes_name_selector_entry() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .with_edits([ConfigEdit::SetSkillConfigByName {
            name: "github:yeet".to_string(),
            enabled: false,
        }])
        .apply_blocking()
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[[skills.config]]
name = "github:yeet"
enabled = false
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_model_ignores_inline_legacy_profile_contents() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    // Seed with inline tables for profiles to simulate common user config.
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"profile = "fast"

profiles = { fast = { model = "gpt-4o", sandbox_mode = "strict" } }
"#,
    )
    .expect("seed");

    apply_blocking(
        codex_home,
        &[ConfigEdit::SetModel {
            model: Some("o4-mini".to_string()),
            effort: None,
        }],
    )
    .expect("persist");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let value: TomlValue = toml::from_str(&raw).expect("parse config");

    assert_eq!(
        value.get("model").and_then(TomlValue::as_str),
        Some("o4-mini")
    );

    // Legacy profile values stay untouched when root settings are updated.
    let profiles_tbl = value
        .get("profiles")
        .and_then(|v| v.as_table())
        .expect("profiles table");
    let fast_tbl = profiles_tbl
        .get("fast")
        .and_then(|v| v.as_table())
        .expect("fast table");
    assert_eq!(
        fast_tbl.get("sandbox_mode").and_then(|v| v.as_str()),
        Some("strict")
    );
    assert_eq!(
        fast_tbl.get("model").and_then(|v| v.as_str()),
        Some("gpt-4o")
    );
}

#[cfg(unix)]
#[test]
fn blocking_set_model_writes_through_symlink_chain() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    let target_dir = tempdir().expect("target dir");
    let target_path = target_dir.path().join(CONFIG_TOML_FILE);
    let link_path = codex_home.join("config-link.toml");
    let config_path = codex_home.join(CONFIG_TOML_FILE);

    symlink(&target_path, &link_path).expect("symlink link");
    symlink("config-link.toml", &config_path).expect("symlink config");

    apply_blocking(
        codex_home,
        &[ConfigEdit::SetModel {
            model: Some("gpt-5.4".to_string()),
            effort: Some(ReasoningEffort::High),
        }],
    )
    .expect("persist");

    let meta = std::fs::symlink_metadata(&config_path).expect("config metadata");
    assert!(meta.file_type().is_symlink());

    let contents = std::fs::read_to_string(&target_path).expect("read target");
    let expected = r#"model = "gpt-5.4"
model_reasoning_effort = "high"
"#;
    assert_eq!(contents, expected);
}

#[cfg(unix)]
#[test]
fn blocking_set_model_replaces_symlink_on_cycle() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    let link_a = codex_home.join("a.toml");
    let link_b = codex_home.join("b.toml");
    let config_path = codex_home.join(CONFIG_TOML_FILE);

    symlink("b.toml", &link_a).expect("symlink a");
    symlink("a.toml", &link_b).expect("symlink b");
    symlink("a.toml", &config_path).expect("symlink config");

    apply_blocking(
        codex_home,
        &[ConfigEdit::SetModel {
            model: Some("gpt-5.4".to_string()),
            effort: None,
        }],
    )
    .expect("persist");

    let meta = std::fs::symlink_metadata(&config_path).expect("config metadata");
    assert!(!meta.file_type().is_symlink());

    let contents = std::fs::read_to_string(&config_path).expect("read config");
    let expected = r#"model = "gpt-5.4"
"#;
    assert_eq!(contents, expected);
}

#[test]
fn batch_write_table_upsert_preserves_inline_comments() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    let original = r#"approval_policy = "never"

[mcp_servers.linear]
name = "linear"
# ok
url = "https://linear.example"

[mcp_servers.linear.http_headers]
foo = "bar"

[sandbox_workspace_write]
# ok 3
network_access = false
"#;
    std::fs::write(codex_home.join(CONFIG_TOML_FILE), original).expect("seed config");

    apply_blocking(
        codex_home,
        &[
            ConfigEdit::SetPath {
                segments: vec![
                    "mcp_servers".to_string(),
                    "linear".to_string(),
                    "url".to_string(),
                ],
                value: value("https://linear.example/v2"),
            },
            ConfigEdit::SetPath {
                segments: vec![
                    "sandbox_workspace_write".to_string(),
                    "network_access".to_string(),
                ],
                value: value(true),
            },
        ],
    )
    .expect("apply");

    let updated = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"approval_policy = "never"

[mcp_servers.linear]
name = "linear"
# ok
url = "https://linear.example/v2"

[mcp_servers.linear.http_headers]
foo = "bar"

[sandbox_workspace_write]
# ok 3
network_access = true
"#;
    assert_eq!(updated, expected);
}

#[test]
fn blocking_clear_model_does_not_follow_legacy_active_profile() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"profile = "fast"

profiles = { fast = { model = "gpt-4o", sandbox_mode = "strict" } }
"#,
    )
    .expect("seed");

    apply_blocking(
        codex_home,
        &[ConfigEdit::SetModel {
            model: None,
            effort: Some(ReasoningEffort::High),
        }],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"profile = "fast"

profiles = { fast = { model = "gpt-4o", sandbox_mode = "strict" } }
model_reasoning_effort = "high"
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_model_does_not_follow_legacy_active_profile() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"profile = "team"

[profiles.team]
model_reasoning_effort = "low"
"#,
    )
    .expect("seed");

    apply_blocking(
        codex_home,
        &[ConfigEdit::SetModel {
            model: Some("o5-preview".to_string()),
            effort: Some(ReasoningEffort::Minimal),
        }],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"profile = "team"
model = "o5-preview"
model_reasoning_effort = "minimal"

[profiles.team]
model_reasoning_effort = "low"
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_hide_full_access_warning_preserves_table() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"# Global comment

[notice]
# keep me
existing = "value"
"#,
    )
    .expect("seed");

    apply_blocking(
        codex_home,
        &[ConfigEdit::SetNoticeHideFullAccessWarning(true)],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"# Global comment

[notice]
# keep me
existing = "value"
hide_full_access_warning = true
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_hide_rate_limit_model_nudge_preserves_table() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[notice]
existing = "value"
"#,
    )
    .expect("seed");

    apply_blocking(
        codex_home,
        &[ConfigEdit::SetNoticeHideRateLimitModelNudge(true)],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[notice]
existing = "value"
hide_rate_limit_model_nudge = true
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_hide_gpt5_1_migration_prompt_preserves_table() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[notice]
existing = "value"
"#,
    )
    .expect("seed");
    apply_blocking(
        codex_home,
        &[ConfigEdit::SetNoticeHideModelMigrationPrompt(
            "hide_gpt5_1_migration_prompt".to_string(),
            true,
        )],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[notice]
existing = "value"
hide_gpt5_1_migration_prompt = true
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_hide_gpt_5_1_codex_max_migration_prompt_preserves_table() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[notice]
existing = "value"
"#,
    )
    .expect("seed");
    apply_blocking(
        codex_home,
        &[ConfigEdit::SetNoticeHideModelMigrationPrompt(
            "hide_gpt-5.1-codex-max_migration_prompt".to_string(),
            true,
        )],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[notice]
existing = "value"
"hide_gpt-5.1-codex-max_migration_prompt" = true
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_record_model_migration_seen_preserves_table() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[notice]
existing = "value"
"#,
    )
    .expect("seed");
    apply_blocking(
        codex_home,
        &[ConfigEdit::RecordModelMigrationSeen {
            from: "gpt-5.2".to_string(),
            to: "gpt-5.4".to_string(),
        }],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[notice]
existing = "value"

[notice.model_migrations]
"gpt-5.2" = "gpt-5.4"
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_hide_external_config_migration_prompt_home_preserves_table() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[notice]
existing = "value"
"#,
    )
    .expect("seed");
    apply_blocking(
        codex_home,
        &[ConfigEdit::SetNoticeHideExternalConfigMigrationPromptHome(
            true,
        )],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[notice]
existing = "value"

[notice.external_config_migration_prompts]
home = true
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_hide_external_config_migration_prompt_project_preserves_table() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[notice]
existing = "value"
"#,
    )
    .expect("seed");
    apply_blocking(
        codex_home,
        &[
            ConfigEdit::SetNoticeHideExternalConfigMigrationPromptProject(
                "/Users/alexsong/code/skills".to_string(),
                true,
            ),
        ],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[notice]
existing = "value"

[notice.external_config_migration_prompts.projects]
"/Users/alexsong/code/skills" = true
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_external_config_migration_prompt_home_last_prompted_at_preserves_table() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[notice]
existing = "value"
"#,
    )
    .expect("seed");
    apply_blocking(
        codex_home,
        &[ConfigEdit::SetNoticeExternalConfigMigrationPromptHomeLastPromptedAt(1_760_000_000)],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[notice]
existing = "value"

[notice.external_config_migration_prompts]
home_last_prompted_at = 1760000000
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_set_external_config_migration_prompt_project_last_prompted_at_preserves_table() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[notice]
existing = "value"
"#,
    )
    .expect("seed");
    apply_blocking(
        codex_home,
        &[
            ConfigEdit::SetNoticeExternalConfigMigrationPromptProjectLastPromptedAt(
                "/Users/alexsong/code/skills".to_string(),
                1_760_000_000,
            ),
        ],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[notice]
existing = "value"

[notice.external_config_migration_prompts.project_last_prompted_at]
"/Users/alexsong/code/skills" = 1760000000
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_replace_mcp_servers_round_trips() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    let mut servers = BTreeMap::new();
    servers.insert(
        "ema".to_string(),
        serde_json::from_value(serde_json::json!({
            "url": "https://ema.example/mcp", "auth": "ema_auth", "scopes": ["tools"],
            "oauth_resource": "https://ema.example",
            "oauth": {
                "client_id": "resource-client",
                "client_secret": "test-ema-secret",
                "token_endpoint_auth_method": "client_secret_post",
                "authorization_server_issuer": "https://as.example"
            }
        }))
        .expect("EMA server"),
    );
    servers.insert(
        "stdio".to_string(),
        McpServerConfig {
            auth: Default::default(),
            transport: McpServerTransportConfig::Stdio {
                command: "cmd".to_string(),
                args: vec!["--flag".to_string()],
                env: Some(
                    [
                        ("B".to_string(), "2".to_string()),
                        ("A".to_string(), "1".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                env_vars: vec!["FOO".into()],
                cwd: None,
            },
            environment_id: codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID.to_string(),
            enabled: true,
            required: false,
            startup_readiness: Default::default(),
            supports_parallel_tool_calls: true,
            tool_input_schema_max_bytes: None,
            omit_tools_from: None,
            disabled_reason: None,
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            default_tools_approval_mode: None,
            enabled_tools: Some(vec!["one".to_string(), "two".to_string()]),
            disabled_tools: None,
            scopes: None,
            oauth: None,
            oauth_resource: None,
            tools: HashMap::new(),
        },
    );

    servers.insert(
        "http".to_string(),
        McpServerConfig {
            auth: Default::default(),
            transport: McpServerTransportConfig::StreamableHttp {
                url: "https://example.com".to_string(),
                bearer_token_env_var: Some("TOKEN".to_string()),
                http_headers: Some(
                    [("Z-Header".to_string(), "z".to_string())]
                        .into_iter()
                        .collect(),
                ),
                env_http_headers: None,
                http_headers_helper: Some("auth-cli headers".to_string()),
            },
            environment_id: codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID.to_string(),
            enabled: false,
            required: false,
            startup_readiness: Default::default(),
            supports_parallel_tool_calls: false,
            tool_input_schema_max_bytes: None,
            omit_tools_from: None,
            disabled_reason: None,
            startup_timeout_sec: Some(std::time::Duration::from_secs(5)),
            tool_timeout_sec: None,
            default_tools_approval_mode: None,
            enabled_tools: None,
            disabled_tools: Some(vec!["forbidden".to_string()]),
            scopes: None,
            oauth: Some(McpServerOAuthConfig {
                client_id: Some("eci-prd-pub-codex-123".to_string()),
                client_secret: Some("test-client-secret".into()),
                callback_url: Some("http://127.0.0.1/callback/example".to_string()),
                callback_port: Some(9876),
                ..Default::default()
            }),
            oauth_resource: Some("https://resource.example.com".to_string()),
            tools: HashMap::new(),
        },
    );

    apply_blocking(
        codex_home,
        &[ConfigEdit::ReplaceMcpServers(servers.clone())],
    )
    .expect("persist");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = "\
[mcp_servers.ema]
url = \"https://ema.example/mcp\"
auth = \"ema_auth\"
scopes = [\"tools\"]
oauth_resource = \"https://ema.example\"

[mcp_servers.ema.oauth]
client_id = \"resource-client\"
client_secret = \"test-ema-secret\"
token_endpoint_auth_method = \"client_secret_post\"
authorization_server_issuer = \"https://as.example\"

[mcp_servers.http]
url = \"https://example.com\"
bearer_token_env_var = \"TOKEN\"
http_headers_helper = \"auth-cli headers\"
enabled = false
startup_timeout_sec = 5.0
disabled_tools = [\"forbidden\"]
oauth_resource = \"https://resource.example.com\"

[mcp_servers.http.http_headers]
Z-Header = \"z\"

[mcp_servers.http.oauth]
client_id = \"eci-prd-pub-codex-123\"
client_secret = \"test-client-secret\"
callback_url = \"http://127.0.0.1/callback/example\"
callback_port = 9876

[mcp_servers.stdio]
command = \"cmd\"
args = [\"--flag\"]
env_vars = [\"FOO\"]
supports_parallel_tool_calls = true
enabled_tools = [\"one\", \"two\"]

[mcp_servers.stdio.env]
A = \"1\"
B = \"2\"
";
    assert_eq!(raw, expected);
}

#[test_case::test_case(30_000; "positive limit")]
#[test_case::test_case(usize::MAX; "largest native limit")]
fn blocking_replace_mcp_servers_serializes_tool_approval_overrides(output_token_limit: usize) {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    let mut servers = BTreeMap::new();
    servers.insert(
        "docs".to_string(),
        McpServerConfig {
            auth: Default::default(),
            transport: McpServerTransportConfig::Stdio {
                command: "docs-server".to_string(),
                args: Vec::new(),
                env: None,
                env_vars: Vec::new(),
                cwd: None,
            },
            environment_id: codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID.to_string(),
            enabled: true,
            required: false,
            startup_readiness: Default::default(),
            supports_parallel_tool_calls: false,
            tool_input_schema_max_bytes: None,
            omit_tools_from: None,
            disabled_reason: None,
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            default_tools_approval_mode: Some(AppToolApproval::Prompt),
            enabled_tools: None,
            disabled_tools: None,
            scopes: None,
            oauth: None,
            oauth_resource: None,
            tools: HashMap::from([(
                "search".to_string(),
                McpServerToolConfig {
                    approval_mode: Some(AppToolApproval::Approve),
                    output_token_limit: std::num::NonZeroUsize::new(output_token_limit),
                },
            )]),
        },
    );

    let result = apply_blocking(codex_home, &[ConfigEdit::ReplaceMcpServers(servers)]);
    if i64::try_from(output_token_limit).is_err() {
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("output_token_limit exceeds the TOML integer range")
        );
        return;
    }
    result.expect("persist");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = format!(
        "\
[mcp_servers.docs]
command = \"docs-server\"
default_tools_approval_mode = \"prompt\"

[mcp_servers.docs.tools.search]
approval_mode = \"approve\"
output_token_limit = {output_token_limit}
"
    );
    assert_eq!(raw, expected);
}

#[test]
fn blocking_replace_mcp_servers_preserves_inline_comments() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[mcp_servers]
# keep me
foo = { command = "cmd" }
"#,
    )
    .expect("seed");

    let mut servers = BTreeMap::new();
    servers.insert(
        "foo".to_string(),
        McpServerConfig {
            auth: Default::default(),
            transport: McpServerTransportConfig::Stdio {
                command: "cmd".to_string(),
                args: Vec::new(),
                env: None,
                env_vars: Vec::new(),
                cwd: None,
            },
            environment_id: codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID.to_string(),
            enabled: true,
            required: false,
            startup_readiness: Default::default(),
            supports_parallel_tool_calls: false,
            tool_input_schema_max_bytes: None,
            omit_tools_from: None,
            disabled_reason: None,
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            default_tools_approval_mode: None,
            enabled_tools: None,
            disabled_tools: None,
            scopes: None,
            oauth: None,
            oauth_resource: None,
            tools: HashMap::new(),
        },
    );

    apply_blocking(codex_home, &[ConfigEdit::ReplaceMcpServers(servers)]).expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[mcp_servers]
# keep me
foo = { command = "cmd" }
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_replace_mcp_servers_preserves_inline_comment_suffix() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[mcp_servers]
foo = { command = "cmd" } # keep me
"#,
    )
    .expect("seed");

    let mut servers = BTreeMap::new();
    servers.insert(
        "foo".to_string(),
        McpServerConfig {
            auth: Default::default(),
            transport: McpServerTransportConfig::Stdio {
                command: "cmd".to_string(),
                args: Vec::new(),
                env: None,
                env_vars: Vec::new(),
                cwd: None,
            },
            environment_id: codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID.to_string(),
            enabled: false,
            required: false,
            startup_readiness: Default::default(),
            supports_parallel_tool_calls: false,
            tool_input_schema_max_bytes: None,
            omit_tools_from: None,
            disabled_reason: None,
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            default_tools_approval_mode: None,
            enabled_tools: None,
            disabled_tools: None,
            scopes: None,
            oauth: None,
            oauth_resource: None,
            tools: HashMap::new(),
        },
    );

    apply_blocking(codex_home, &[ConfigEdit::ReplaceMcpServers(servers)]).expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[mcp_servers]
foo = { command = "cmd" , enabled = false } # keep me
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_replace_mcp_servers_preserves_inline_comment_after_removing_keys() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[mcp_servers]
foo = { command = "cmd", args = ["--flag"] } # keep me
"#,
    )
    .expect("seed");

    let mut servers = BTreeMap::new();
    servers.insert(
        "foo".to_string(),
        McpServerConfig {
            auth: Default::default(),
            transport: McpServerTransportConfig::Stdio {
                command: "cmd".to_string(),
                args: Vec::new(),
                env: None,
                env_vars: Vec::new(),
                cwd: None,
            },
            environment_id: codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID.to_string(),
            enabled: true,
            required: false,
            startup_readiness: Default::default(),
            supports_parallel_tool_calls: false,
            tool_input_schema_max_bytes: None,
            omit_tools_from: None,
            disabled_reason: None,
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            default_tools_approval_mode: None,
            enabled_tools: None,
            disabled_tools: None,
            scopes: None,
            oauth: None,
            oauth_resource: None,
            tools: HashMap::new(),
        },
    );

    apply_blocking(codex_home, &[ConfigEdit::ReplaceMcpServers(servers)]).expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[mcp_servers]
foo = { command = "cmd"} # keep me
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_replace_mcp_servers_preserves_inline_comment_prefix_on_update() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        r#"[mcp_servers]
# keep me
foo = { command = "cmd" }
"#,
    )
    .expect("seed");

    let mut servers = BTreeMap::new();
    servers.insert(
        "foo".to_string(),
        McpServerConfig {
            auth: Default::default(),
            transport: McpServerTransportConfig::Stdio {
                command: "cmd".to_string(),
                args: Vec::new(),
                env: None,
                env_vars: Vec::new(),
                cwd: None,
            },
            environment_id: codex_config::DEFAULT_MCP_SERVER_ENVIRONMENT_ID.to_string(),
            enabled: false,
            required: false,
            startup_readiness: Default::default(),
            supports_parallel_tool_calls: false,
            tool_input_schema_max_bytes: None,
            omit_tools_from: None,
            disabled_reason: None,
            startup_timeout_sec: None,
            tool_timeout_sec: None,
            default_tools_approval_mode: None,
            enabled_tools: None,
            disabled_tools: None,
            scopes: None,
            oauth: None,
            oauth_resource: None,
            tools: HashMap::new(),
        },
    );

    apply_blocking(codex_home, &[ConfigEdit::ReplaceMcpServers(servers)]).expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"[mcp_servers]
# keep me
foo = { command = "cmd" , enabled = false }
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_clear_path_noop_when_missing() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    apply_blocking(
        codex_home,
        &[ConfigEdit::ClearPath {
            segments: vec!["missing".to_string()],
        }],
    )
    .expect("apply");

    assert!(
        !codex_home.join(CONFIG_TOML_FILE).exists(),
        "config.toml should not be created on noop"
    );
}

#[test]
fn blocking_notification_toggle_updates_boolean_and_preserves_custom_filters() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    let config_path = codex_home.join(CONFIG_TOML_FILE);

    apply_blocking(
        codex_home,
        &[ConfigEdit::SetTuiNotificationsEnabled { enabled: false }],
    )
    .expect("apply");
    assert_eq!(
        std::fs::read_to_string(&config_path).expect("read config"),
        "[tui]\nnotifications = false\n"
    );

    let custom_filters = "[tui]\nnotifications = [\"approval-requested\"]\n";
    std::fs::write(&config_path, custom_filters).expect("write config");
    let error = apply_blocking(
        codex_home,
        &[ConfigEdit::SetTuiNotificationsEnabled { enabled: false }],
    )
    .expect_err("custom filters should reject a boolean toggle");
    assert_eq!(
        error.to_string(),
        "Event-specific notifications are configured. Edit those filters in the config file."
    );
    assert_eq!(
        std::fs::read_to_string(config_path).expect("read config"),
        custom_filters
    );
}

#[tokio::test]
async fn async_builder_set_model_persists() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path().to_path_buf();

    ConfigEditsBuilder::new(&codex_home)
        .set_model(Some("gpt-5.4"), Some(ReasoningEffort::High))
        .apply()
        .await
        .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let expected = r#"model = "gpt-5.4"
model_reasoning_effort = "high"
"#;
    assert_eq!(contents, expected);
}

#[test]
fn blocking_builder_set_model_round_trips_back_and_forth() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    let initial_expected = r#"model = "o4-mini"
model_reasoning_effort = "low"
"#;
    ConfigEditsBuilder::new(codex_home)
        .set_model(Some("o4-mini"), Some(ReasoningEffort::Low))
        .apply_blocking()
        .expect("persist initial");
    let mut contents =
        std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    assert_eq!(contents, initial_expected);

    let updated_expected = r#"model = "gpt-5.4"
model_reasoning_effort = "high"
"#;
    ConfigEditsBuilder::new(codex_home)
        .set_model(Some("gpt-5.4"), Some(ReasoningEffort::High))
        .apply_blocking()
        .expect("persist update");
    contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    assert_eq!(contents, updated_expected);

    ConfigEditsBuilder::new(codex_home)
        .set_model(Some("o4-mini"), Some(ReasoningEffort::Low))
        .apply_blocking()
        .expect("persist revert");
    contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    assert_eq!(contents, initial_expected);
}

#[tokio::test]
async fn blocking_set_asynchronous_helpers_available() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path().to_path_buf();

    ConfigEditsBuilder::new(&codex_home)
        .set_hide_full_access_warning(/*acknowledged*/ true)
        .apply()
        .await
        .expect("persist");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let notice = toml::from_str::<TomlValue>(&raw)
        .expect("parse config")
        .get("notice")
        .and_then(|item| item.as_table())
        .and_then(|tbl| tbl.get("hide_full_access_warning"))
        .and_then(toml::Value::as_bool);
    assert_eq!(notice, Some(true));
}

#[test]
fn blocking_builder_set_realtime_audio_persists_and_clears() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .set_realtime_microphone(Some("USB Mic"))
        .set_realtime_speaker(Some("Desk Speakers"))
        .apply_blocking()
        .expect("persist realtime audio");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let config: TomlValue = toml::from_str(&raw).expect("parse config");
    let realtime_audio = config
        .get("audio")
        .and_then(TomlValue::as_table)
        .expect("audio table should exist");
    assert_eq!(
        realtime_audio.get("microphone").and_then(TomlValue::as_str),
        Some("USB Mic")
    );
    assert_eq!(
        realtime_audio.get("speaker").and_then(TomlValue::as_str),
        Some("Desk Speakers")
    );

    ConfigEditsBuilder::new(codex_home)
        .set_realtime_microphone(/*microphone*/ None)
        .apply_blocking()
        .expect("clear realtime microphone");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let config: TomlValue = toml::from_str(&raw).expect("parse config");
    let realtime_audio = config
        .get("audio")
        .and_then(TomlValue::as_table)
        .expect("audio table should exist");
    assert_eq!(realtime_audio.get("microphone"), None);
    assert_eq!(
        realtime_audio.get("speaker").and_then(TomlValue::as_str),
        Some("Desk Speakers")
    );
}

#[test]
fn blocking_builder_set_realtime_voice_persists_and_clears() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();

    ConfigEditsBuilder::new(codex_home)
        .set_realtime_voice(Some("cedar"))
        .apply_blocking()
        .expect("persist realtime voice");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let config: TomlValue = toml::from_str(&raw).expect("parse config");
    let realtime = config
        .get("realtime")
        .and_then(TomlValue::as_table)
        .expect("realtime table should exist");
    assert_eq!(
        realtime.get("voice").and_then(TomlValue::as_str),
        Some("cedar")
    );

    ConfigEditsBuilder::new(codex_home)
        .set_realtime_voice(/*voice*/ None)
        .apply_blocking()
        .expect("clear realtime voice");

    let raw = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    let config: TomlValue = toml::from_str(&raw).expect("parse config");
    let realtime = config
        .get("realtime")
        .and_then(TomlValue::as_table)
        .expect("realtime table should exist");
    assert_eq!(realtime.get("voice"), None);
}

#[test]
fn replace_mcp_servers_blocking_clears_table_when_empty() {
    let tmp = tempdir().expect("tmpdir");
    let codex_home = tmp.path();
    std::fs::write(
        codex_home.join(CONFIG_TOML_FILE),
        "[mcp_servers]\nfoo = { command = \"cmd\" }\n",
    )
    .expect("seed");

    apply_blocking(
        codex_home,
        &[ConfigEdit::ReplaceMcpServers(BTreeMap::new())],
    )
    .expect("persist");

    let contents = std::fs::read_to_string(codex_home.join(CONFIG_TOML_FILE)).expect("read config");
    assert!(!contents.contains("mcp_servers"));
}

#[cfg(any(unix, windows))]
fn config_test_symlink(source: &std::path::Path, link: &std::path::Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(source, link).unwrap();
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn cycle_replaced_by_external_document_checks_current_bytes() {
    let dir = tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let other = dir.path().join("other.toml");
    config_test_symlink(&other, &config);
    config_test_symlink(&config, &other);
    let guard = ConfigEditTransaction::acquire(&config).await.unwrap();
    std::fs::remove_file(&config).unwrap();
    let current = "model = 'external'\n";
    std::fs::write(&config, current).unwrap();
    let stale: TomlValue = toml::from_str("").unwrap();
    let result = ConfigEditsBuilder::for_config_path(&config)
        .with_edits([ConfigEdit::SetPath {
            segments: vec!["model".into()],
            value: value("wrong"),
        }])
        .apply_with_transaction(guard, Some(stale))
        .await;
    assert!(result.unwrap_err().is::<ConfigEditVersionConflict>());
    assert_eq!(std::fs::read_to_string(config).unwrap(), current);
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn alias_transactions_serialize_and_reject_stale_checked_write() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("target.toml");
    let first = dir.path().join("first.toml");
    let second = dir.path().join("second.toml");
    std::fs::write(&target, "model = 'old'\n").unwrap();
    config_test_symlink(&target, &first);
    config_test_symlink(&target, &second);
    let expected: TomlValue = toml::from_str("model = 'old'\n").unwrap();
    let guard = ConfigEditTransaction::acquire(&first).await.unwrap();
    let (attempted_tx, attempted_rx) = tokio::sync::oneshot::channel();
    let (acquired_tx, mut acquired_rx) = tokio::sync::oneshot::channel();
    let expected_second = expected.clone();
    let writer = tokio::spawn(async move {
        attempted_tx.send(()).unwrap();
        let guard = ConfigEditTransaction::acquire(&second).await.unwrap();
        acquired_tx.send(()).unwrap();
        ConfigEditsBuilder::for_config_path(&second)
            .with_edits([ConfigEdit::SetPath {
                segments: vec!["model".into()],
                value: value("second"),
            }])
            .apply_with_transaction(guard, Some(expected_second))
            .await
    });
    attempted_rx.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(250), &mut acquired_rx)
            .await
            .is_err(),
        "alias writer acquired while the target lock was held"
    );
    ConfigEditsBuilder::for_config_path(&first)
        .with_edits([ConfigEdit::SetPath {
            segments: vec!["model".into()],
            value: value("first"),
        }])
        .apply_with_transaction(guard, Some(expected))
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), acquired_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(
        writer
            .await
            .unwrap()
            .unwrap_err()
            .is::<ConfigEditVersionConflict>()
    );
    let saved: TomlValue = toml::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
    assert_eq!(saved["model"].as_str(), Some("first"));
    assert!(
        std::fs::symlink_metadata(first)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn retargeted_alias_is_rejected_before_write() {
    let dir = tempdir().unwrap();
    let old = dir.path().join("old.toml");
    let new = dir.path().join("new.toml");
    let link = dir.path().join("link.toml");
    std::fs::write(&old, "model = 'old'\n").unwrap();
    std::fs::write(&new, "model = 'new'\n").unwrap();
    config_test_symlink(&old, &link);
    let guard = ConfigEditTransaction::acquire(&link).await.unwrap();
    std::fs::remove_file(&link).unwrap();
    config_test_symlink(&new, &link);
    let result = ConfigEditsBuilder::for_config_path(&link)
        .with_edits([ConfigEdit::SetPath {
            segments: vec!["model".into()],
            value: value("bad"),
        }])
        .apply_with_transaction(guard, None)
        .await;
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(old).unwrap(), "model = 'old'\n");
    assert_eq!(std::fs::read_to_string(new).unwrap(), "model = 'new'\n");
}

#[cfg(any(unix, windows))]
#[tokio::test]
async fn dangling_aliases_share_lock_for_missing_target_parent() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("missing").join("config.toml");
    let first = dir.path().join("first.toml");
    let second = dir.path().join("second.toml");
    config_test_symlink(&target, &first);
    config_test_symlink(&target, &second);
    let guard = ConfigEditTransaction::acquire(&first).await.unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(250),
            ConfigEditTransaction::acquire(&second)
        )
        .await
        .is_err()
    );
    ConfigEditsBuilder::for_config_path(&first)
        .with_edits([ConfigEdit::SetPath {
            segments: vec!["model".into()],
            value: value("created"),
        }])
        .apply_with_transaction(guard, None)
        .await
        .unwrap();
    let read: TomlValue = toml::from_str(&std::fs::read_to_string(target).unwrap()).unwrap();
    assert_eq!(read["model"].as_str(), Some("created"));
    assert!(
        std::fs::symlink_metadata(first)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        std::fs::symlink_metadata(second)
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn directory_alias_and_direct_path_share_lock() {
    let dir = tempdir().unwrap();
    let real = dir.path().join("real");
    let alias = dir.path().join("alias");
    std::fs::create_dir(&real).unwrap();
    symlink(&real, &alias).unwrap();
    let first = real.join("config.toml");
    let second = alias.join("config.toml");
    let guard = ConfigEditTransaction::acquire(&first).await.unwrap();
    assert!(
        tokio::time::timeout(
            std::time::Duration::from_millis(250),
            ConfigEditTransaction::acquire(&second)
        )
        .await
        .is_err()
    );
    drop(guard);
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        ConfigEditTransaction::acquire(&second),
    )
    .await
    .unwrap()
    .unwrap();
}
