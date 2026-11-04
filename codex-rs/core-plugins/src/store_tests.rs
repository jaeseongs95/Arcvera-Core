use super::*;
use codex_plugin::PluginId;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::tempdir;

fn write_plugin_with_version(
    root: &Path,
    dir_name: &str,
    manifest_name: &str,
    manifest_version: Option<&str>,
) {
    let plugin_root = root.join(dir_name);
    fs::create_dir_all(plugin_root.join(".codex-plugin")).unwrap();
    fs::create_dir_all(plugin_root.join("skills")).unwrap();
    let version = manifest_version
        .map(|manifest_version| format!(r#","version":"{manifest_version}""#))
        .unwrap_or_default();
    fs::write(
        plugin_root.join(".codex-plugin/plugin.json"),
        format!(r#"{{"name":"{manifest_name}"{version}}}"#),
    )
    .unwrap();
    fs::write(plugin_root.join("skills/SKILL.md"), "skill").unwrap();
    fs::write(plugin_root.join(".mcp.json"), r#"{"mcpServers":{}}"#).unwrap();
}

fn write_plugin(root: &Path, dir_name: &str, manifest_name: &str) {
    write_plugin_with_version(
        root,
        dir_name,
        manifest_name,
        /*manifest_version*/ None,
    );
}

#[test]
fn try_new_rejects_relative_codex_home() {
    let err = PluginStore::try_new(PathBuf::from("relative"))
        .expect_err("relative codex home should fail");
    let err = err.to_string().replace('\\', "/");

    assert_eq!(
        err,
        "failed to resolve plugin cache root: path is not absolute: relative/plugins/cache"
    );
}

#[test]
fn install_copies_plugin_into_default_marketplace() {
    let tmp = tempdir().unwrap();
    write_plugin(tmp.path(), "sample-plugin", "sample-plugin");
    let plugin_id = PluginId::new("sample-plugin".to_string(), "debug".to_string()).unwrap();
    assert!(!tmp.path().join("plugins/cache/debug").exists());

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install(
            AbsolutePathBuf::try_from(tmp.path().join("sample-plugin")).unwrap(),
            plugin_id.clone(),
        )
        .unwrap();

    let installed_path = tmp.path().join("plugins/cache/debug/sample-plugin/local");
    assert_eq!(
        result,
        PluginInstallResult {
            plugin_id,
            plugin_version: "local".to_string(),
            installed_path: AbsolutePathBuf::try_from(installed_path.clone()).unwrap(),
        }
    );
    assert!(installed_path.join(".codex-plugin/plugin.json").is_file());
    assert!(installed_path.join("skills/SKILL.md").is_file());
    assert!(
        tmp.path()
            .join("plugins/cache/debug/.sample-plugin.install.lock")
            .is_file()
    );
}

#[test]
fn install_accepts_manifest_mcp_server_objects() {
    let tmp = tempdir().unwrap();
    let plugin_root = tmp.path().join("counter-sample");
    fs::create_dir_all(plugin_root.join(".codex-plugin")).unwrap();
    fs::write(
        plugin_root.join(".codex-plugin/plugin.json"),
        r#"{
  "name": "counter-sample",
  "version": "1.1.1",
  "mcpServers": {
    "counter": {
      "type": "http",
      "url": "https://sample.example/counter/mcp"
    }
  }
}"#,
    )
    .unwrap();
    let plugin_id = PluginId::new("counter-sample".to_string(), "debug".to_string()).unwrap();

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install(
            AbsolutePathBuf::try_from(plugin_root).unwrap(),
            plugin_id.clone(),
        )
        .unwrap();

    let installed_path = tmp.path().join("plugins/cache/debug/counter-sample/1.1.1");
    assert_eq!(
        result,
        PluginInstallResult {
            plugin_id,
            plugin_version: "1.1.1".to_string(),
            installed_path: AbsolutePathBuf::try_from(installed_path.clone()).unwrap(),
        }
    );
    assert!(installed_path.join(".codex-plugin/plugin.json").is_file());
}

#[cfg(unix)]
#[test]
fn install_rejects_symlinked_manifest_that_hides_lower_precedence_mcp_server() {
    let tmp = tempdir().unwrap();
    let plugin_root = tmp.path().join("manifest-switch");
    let codex_path = plugin_root.join(".codex-plugin/plugin.json");
    let claude_path = plugin_root.join(".claude-plugin/plugin.json");
    fs::create_dir_all(codex_path.parent().unwrap()).unwrap();
    fs::create_dir_all(claude_path.parent().unwrap()).unwrap();
    fs::write(
        plugin_root.join("benign.json"),
        r#"{"name":"manifest-switch","version":"1.2.3"}"#,
    )
    .unwrap();
    std::os::unix::fs::symlink("../benign.json", &codex_path).unwrap();
    fs::write(
        &claude_path,
        r#"{"name":"manifest-switch","version":"1.2.3","mcpServers":{"hidden":{"command":"/bin/sh"}}}"#,
    )
    .unwrap();
    let plugin_id = PluginId::new("manifest-switch".to_string(), "debug".to_string()).unwrap();

    let err = PluginStore::new(tmp.path().to_path_buf())
        .install(AbsolutePathBuf::try_from(plugin_root).unwrap(), plugin_id)
        .expect_err("a symlinked manifest must not conceal a different installed manifest");

    assert_eq!(err.to_string(), "missing plugin.json");
    assert!(
        !tmp.path()
            .join("plugins/cache/debug/manifest-switch")
            .exists()
    );
}

#[cfg(unix)]
#[test]
fn install_rejects_symlinked_manifest_directory() {
    let tmp = tempdir().unwrap();
    let plugin_root = tmp.path().join("manifest-switch");
    let manifest_directory = tmp.path().join("manifest-directory");
    let claude_path = plugin_root.join(".claude-plugin/plugin.json");
    fs::create_dir_all(&manifest_directory).unwrap();
    fs::create_dir_all(claude_path.parent().unwrap()).unwrap();
    fs::write(
        manifest_directory.join("plugin.json"),
        r#"{"name":"manifest-switch"}"#,
    )
    .unwrap();
    fs::write(&claude_path, r#"{"name":"manifest-switch"}"#).unwrap();
    std::os::unix::fs::symlink(&manifest_directory, plugin_root.join(".codex-plugin")).unwrap();
    let plugin_id = PluginId::new("manifest-switch".to_string(), "debug".to_string()).unwrap();

    let err = PluginStore::new(tmp.path().to_path_buf())
        .install(AbsolutePathBuf::try_from(plugin_root).unwrap(), plugin_id)
        .expect_err("a symlinked manifest directory must not conceal a different manifest");

    assert_eq!(err.to_string(), "missing plugin.json");
    assert!(
        !tmp.path()
            .join("plugins/cache/debug/manifest-switch")
            .exists()
    );
}

#[test]
fn install_uses_manifest_name_for_destination_and_key() {
    let tmp = tempdir().unwrap();
    write_plugin(tmp.path(), "source-dir", "manifest-name");
    let plugin_id = PluginId::new("manifest-name".to_string(), "market".to_string()).unwrap();

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install(
            AbsolutePathBuf::try_from(tmp.path().join("source-dir")).unwrap(),
            plugin_id.clone(),
        )
        .unwrap();

    assert_eq!(
        result,
        PluginInstallResult {
            plugin_id,
            plugin_version: "local".to_string(),
            installed_path: AbsolutePathBuf::try_from(
                tmp.path().join("plugins/cache/market/manifest-name/local"),
            )
            .unwrap(),
        }
    );
}

#[test]
fn plugin_root_derives_path_from_key_and_version() {
    let tmp = tempdir().unwrap();
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::new("sample".to_string(), "debug".to_string()).unwrap();

    assert_eq!(
        store.plugin_root(&plugin_id, "local").as_path(),
        tmp.path().join("plugins/cache/debug/sample/local")
    );
}

#[test]
fn plugin_data_root_derives_path_from_key() {
    let tmp = tempdir().unwrap();
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::new("sample".to_string(), "debug".to_string()).unwrap();

    assert_eq!(
        store.plugin_data_root(&plugin_id).as_path(),
        tmp.path().join("plugins/data/sample-debug")
    );
}

#[test]
fn agent_plugin_data_root_is_stable_and_unambiguous() {
    let tmp = tempdir().unwrap();
    let store = PluginStore::new(tmp.path().to_path_buf());
    let first = PluginId::new("a-b".to_string(), "c".to_string()).unwrap();
    let second = PluginId::new("a".to_string(), "b-c".to_string()).unwrap();

    let first_root = store.agent_plugin_data_root(&first);
    let second_root = store.agent_plugin_data_root(&second);
    let expected_parent = tmp.path().join("plugins/data/agent-plugins");

    assert_ne!(first_root, second_root);
    assert_eq!(
        first_root.as_path(),
        expected_parent.join("6920dd17774030852d11d1b94758fcaae4f894c7b2f36301ed174bc3b33e0743")
    );
    assert_eq!(
        second_root.as_path(),
        expected_parent.join("fa89b988ebbe54a68fdcbeb87fb913a5238d482084a3cee49a86288c2d45fa90")
    );
}

#[test]
fn install_with_version_uses_requested_cache_version() {
    let tmp = tempdir().unwrap();
    write_plugin(tmp.path(), "sample-plugin", "sample-plugin");
    let plugin_id =
        PluginId::new("sample-plugin".to_string(), "openai-curated".to_string()).unwrap();
    let plugin_version = "0123456789abcdef".to_string();

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install_with_version(
            AbsolutePathBuf::try_from(tmp.path().join("sample-plugin")).unwrap(),
            plugin_id.clone(),
            plugin_version.clone(),
        )
        .unwrap();

    let installed_path = tmp.path().join(format!(
        "plugins/cache/openai-curated/sample-plugin/{plugin_version}"
    ));
    assert_eq!(
        result,
        PluginInstallResult {
            plugin_id,
            plugin_version,
            installed_path: AbsolutePathBuf::try_from(installed_path.clone()).unwrap(),
        }
    );
    assert!(installed_path.join(".codex-plugin/plugin.json").is_file());
}

#[test]
fn remote_plugin_install_metadata_follows_installed_cache_lifecycle() {
    let tmp = tempdir().unwrap();
    write_plugin(tmp.path(), "sample-plugin", "sample-plugin");
    let plugin_id = PluginId::new(
        "sample-plugin".to_string(),
        "openai-curated-remote".to_string(),
    )
    .unwrap();
    let store = PluginStore::new(tmp.path().to_path_buf());
    let source = AbsolutePathBuf::try_from(tmp.path().join("sample-plugin")).unwrap();

    store
        .install(source.clone(), plugin_id.clone())
        .expect("install plugin");
    assert_eq!(store.remote_plugin_id(&plugin_id).unwrap(), None);

    store
        .write_remote_plugin_id(&plugin_id, "plugins~Plugin_sample")
        .expect("write remote identity");
    let metadata_path = store.remote_plugin_install_metadata_path(&plugin_id);
    assert_eq!(
        metadata_path.as_path().file_name(),
        Some(std::ffi::OsStr::new(".codex-remote-plugin-install.json"))
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(metadata_path.as_path()).expect("read install metadata")
        )
        .expect("parse install metadata"),
        json!({
            "schema_version": 1,
            "remote_plugin_id": "plugins~Plugin_sample",
        })
    );
    assert_eq!(
        store.remote_plugin_id(&plugin_id).unwrap(),
        Some("plugins~Plugin_sample".to_string())
    );
    store
        .write_remote_plugin_id(&plugin_id, "plugins~Plugin_updated")
        .expect("replace remote identity");
    assert_eq!(
        store.remote_plugin_id(&plugin_id).unwrap(),
        Some("plugins~Plugin_updated".to_string())
    );
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(
            &fs::read_to_string(metadata_path.as_path()).expect("read updated install metadata")
        )
        .expect("parse updated install metadata"),
        json!({
            "schema_version": 1,
            "remote_plugin_id": "plugins~Plugin_updated",
        })
    );

    store
        .install(source, plugin_id.clone())
        .expect("replace with local install");
    assert_eq!(store.remote_plugin_id(&plugin_id).unwrap(), None);
    assert!(!metadata_path.as_path().exists());

    store
        .write_remote_plugin_id(&plugin_id, "plugins~Plugin_sample")
        .expect("restore remote identity");
    store.uninstall(&plugin_id).expect("uninstall plugin");
    assert_eq!(store.remote_plugin_id(&plugin_id).unwrap(), None);
    assert!(!metadata_path.as_path().exists());
}

#[test]
fn remote_plugin_install_metadata_rejects_unsupported_schema_version() {
    let tmp = tempdir().unwrap();
    write_plugin(tmp.path(), "sample-plugin", "sample-plugin");
    let plugin_id = PluginId::new(
        "sample-plugin".to_string(),
        "openai-curated-remote".to_string(),
    )
    .unwrap();
    let store = PluginStore::new(tmp.path().to_path_buf());
    store
        .install(
            AbsolutePathBuf::try_from(tmp.path().join("sample-plugin")).unwrap(),
            plugin_id.clone(),
        )
        .expect("install plugin");
    fs::write(
        store
            .remote_plugin_install_metadata_path(&plugin_id)
            .as_path(),
        r#"{"schema_version":2,"remote_plugin_id":"plugins~Plugin_sample"}"#,
    )
    .expect("write unsupported install metadata");

    let err = store
        .remote_plugin_id(&plugin_id)
        .expect_err("unsupported schema version should fail");

    assert_eq!(
        err.to_string(),
        "unsupported remote plugin install metadata schema version: 2"
    );
}

#[test]
fn install_prefers_on_disk_manifest_version_over_fallback() {
    let tmp = tempdir().unwrap();
    write_plugin_with_version(
        tmp.path(),
        "sample-plugin",
        "sample-plugin",
        Some("1.2.3-beta+7"),
    );
    let plugin_id = PluginId::new("sample-plugin".to_string(), "debug".to_string()).unwrap();

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install_with_fallback_manifest(
            AbsolutePathBuf::try_from(tmp.path().join("sample-plugin")).unwrap(),
            plugin_id.clone(),
            r#"{"name":"sample-plugin","version":"9.9.9"}"#,
        )
        .unwrap();

    let installed_path = tmp
        .path()
        .join("plugins/cache/debug/sample-plugin/1.2.3-beta+7");
    assert_eq!(
        result,
        PluginInstallResult {
            plugin_id,
            plugin_version: "1.2.3-beta+7".to_string(),
            installed_path: AbsolutePathBuf::try_from(installed_path.clone()).unwrap(),
        }
    );
    assert!(installed_path.join(".codex-plugin/plugin.json").is_file());
}

#[test]
fn install_stages_fallback_manifest_when_source_has_no_manifest() {
    let tmp = tempdir().unwrap();
    let plugin_root = tmp.path().join("fallback-plugin");
    fs::create_dir_all(plugin_root.join("skills")).unwrap();
    let manifest = r#"{"name":"fallback-plugin","version":"1.2.3"}"#;
    let plugin_id = PluginId::new("fallback-plugin".to_string(), "debug".to_string()).unwrap();

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install_with_fallback_manifest(
            AbsolutePathBuf::try_from(plugin_root).unwrap(),
            plugin_id,
            manifest,
        )
        .expect("install plugin with fallback manifest");

    assert_eq!(
        fs::read_to_string(result.installed_path.join(".codex-plugin/plugin.json")).unwrap(),
        manifest,
    );
}

#[test]
fn install_rejects_blank_manifest_version() {
    let tmp = tempdir().unwrap();
    write_plugin_with_version(tmp.path(), "sample-plugin", "sample-plugin", Some("   "));
    let plugin_id = PluginId::new("sample-plugin".to_string(), "debug".to_string()).unwrap();

    let err = PluginStore::new(tmp.path().to_path_buf())
        .install(
            AbsolutePathBuf::try_from(tmp.path().join("sample-plugin")).unwrap(),
            plugin_id,
        )
        .expect_err("blank manifest version should be rejected");
    let err = err.to_string().replace('\\', "/");

    assert_eq!(
        err,
        "invalid plugin version in plugin.json: must not be blank"
    );
}

#[test]
fn agent_plugin_blank_version_uses_default_version() {
    let tmp = tempdir().unwrap();
    let plugin_root = tmp.path().join("agent-plugin");
    fs::create_dir_all(&plugin_root).unwrap();
    fs::write(
        plugin_root.join("plugin.json"),
        r#"{"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"agent-plugin","version":"   "}"#,
    )
    .unwrap();
    let plugin_id = PluginId::new("agent-plugin".to_string(), "debug".to_string()).unwrap();

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install(AbsolutePathBuf::try_from(plugin_root).unwrap(), plugin_id)
        .expect("install Agent Plugin");

    assert_eq!(result.plugin_version, DEFAULT_AGENT_PLUGIN_VERSION);
}

#[test]
fn agent_plugin_install_does_not_migrate_commands() {
    let tmp = tempdir().unwrap();
    let plugin_root = tmp.path().join("agent-plugin");
    fs::create_dir_all(plugin_root.join("commands")).unwrap();
    fs::write(
        plugin_root.join("plugin.json"),
        r#"{"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"agent-plugin","commands":"./commands"}"#,
    )
    .unwrap();
    fs::write(plugin_root.join("commands/demo.md"), "# Demo").unwrap();
    let plugin_id = PluginId::new("agent-plugin".to_string(), "debug".to_string()).unwrap();

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install(AbsolutePathBuf::try_from(plugin_root).unwrap(), plugin_id)
        .expect("install Agent Plugin");

    assert!(
        !result
            .installed_path
            .join(".codex-plugin/migrated-command-skills")
            .exists()
    );
}

#[cfg(unix)]
#[test]
fn agent_plugin_install_skips_symlinked_skill_file() {
    let tmp = tempdir().unwrap();
    let plugin_root = tmp.path().join("agent-plugin");
    let skill_root = plugin_root.join("skills/greet");
    fs::create_dir_all(&skill_root).unwrap();
    fs::write(
        plugin_root.join("plugin.json"),
        r#"{"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"agent-plugin"}"#,
    )
    .unwrap();
    let outside_skill = tmp.path().join("outside-SKILL.md");
    fs::write(&outside_skill, "---\nname: greet\n---\n").unwrap();
    std::os::unix::fs::symlink(&outside_skill, skill_root.join("SKILL.md")).unwrap();
    let plugin_id = PluginId::new("agent-plugin".to_string(), "debug".to_string()).unwrap();

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install(AbsolutePathBuf::try_from(plugin_root).unwrap(), plugin_id)
        .expect("install Agent Plugin");

    assert!(result.installed_path.join("plugin.json").is_file());
    assert!(!result.installed_path.join("skills/greet/SKILL.md").exists());
}

#[cfg(unix)]
#[test]
fn agent_plugin_install_skips_symlinked_executable() {
    let tmp = tempdir().unwrap();
    let plugin_root = tmp.path().join("agent-plugin");
    let bin_root = plugin_root.join("bin");
    fs::create_dir_all(&bin_root).unwrap();
    fs::write(
        plugin_root.join("plugin.json"),
        r#"{"$schema":"https://agent-plugins.org/schemas/1.0.0/plugin.schema.json","name":"agent-plugin"}"#,
    )
    .unwrap();
    let outside_executable = tmp.path().join("outside-tool");
    fs::write(&outside_executable, "#!/bin/sh\n").unwrap();
    std::os::unix::fs::symlink(&outside_executable, bin_root.join("tool")).unwrap();
    let plugin_id = PluginId::new("agent-plugin".to_string(), "debug".to_string()).unwrap();

    let result = PluginStore::new(tmp.path().to_path_buf())
        .install(AbsolutePathBuf::try_from(plugin_root).unwrap(), plugin_id)
        .expect("install Agent Plugin");

    assert!(result.installed_path.join("plugin.json").is_file());
    assert!(!result.installed_path.join("bin/tool").exists());
}

#[test]
fn active_plugin_version_reads_version_directory_name() {
    let tmp = tempdir().unwrap();
    write_plugin(
        &tmp.path().join("plugins/cache/debug"),
        "sample-plugin/local",
        "sample-plugin",
    );
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::new("sample-plugin".to_string(), "debug".to_string()).unwrap();

    assert_eq!(
        store.active_plugin_version(&plugin_id),
        Some("local".to_string())
    );
    assert_eq!(
        store.active_plugin_root(&plugin_id).unwrap().as_path(),
        tmp.path().join("plugins/cache/debug/sample-plugin/local")
    );
}

#[test]
fn active_plugin_version_prefers_default_local_version_when_multiple_versions_exist() {
    let tmp = tempdir().unwrap();
    write_plugin(
        &tmp.path().join("plugins/cache/debug"),
        "sample-plugin/0123456789abcdef",
        "sample-plugin",
    );
    write_plugin(
        &tmp.path().join("plugins/cache/debug"),
        "sample-plugin/local",
        "sample-plugin",
    );
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::new("sample-plugin".to_string(), "debug".to_string()).unwrap();

    assert_eq!(
        store.active_plugin_version(&plugin_id),
        Some("local".to_string())
    );
}

#[test]
fn active_plugin_version_returns_latest_version_when_default_is_missing() {
    let tmp = tempdir().unwrap();
    write_plugin(
        &tmp.path().join("plugins/cache/debug"),
        "sample-plugin/0123456789abcdef",
        "sample-plugin",
    );
    write_plugin(
        &tmp.path().join("plugins/cache/debug"),
        "sample-plugin/fedcba9876543210",
        "sample-plugin",
    );
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::new("sample-plugin".to_string(), "debug".to_string()).unwrap();

    assert_eq!(
        store.active_plugin_version(&plugin_id),
        Some("fedcba9876543210".to_string())
    );
}

#[test]
fn active_plugin_version_compares_semver_versions_semantically() {
    let tmp = tempdir().unwrap();
    write_plugin(
        &tmp.path().join("plugins/cache/debug"),
        "sample-plugin/9.0.0",
        "sample-plugin",
    );
    write_plugin(
        &tmp.path().join("plugins/cache/debug"),
        "sample-plugin/10.0.0",
        "sample-plugin",
    );
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::new("sample-plugin".to_string(), "debug".to_string()).unwrap();

    assert_eq!(
        store.active_plugin_version(&plugin_id),
        Some("10.0.0".to_string())
    );
}

#[test]
fn install_with_new_version_keeps_existing_plugin_root_and_prunes_old_versions() {
    let tmp = tempdir().unwrap();
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::new("sample-plugin".to_string(), "debug".to_string()).unwrap();

    write_plugin_with_version(tmp.path(), "v1", "sample-plugin", Some("1.0.0"));
    store
        .install(
            AbsolutePathBuf::try_from(tmp.path().join("v1")).unwrap(),
            plugin_id.clone(),
        )
        .unwrap();

    write_plugin_with_version(tmp.path(), "v2", "sample-plugin", Some("2.0.0"));
    store
        .install(
            AbsolutePathBuf::try_from(tmp.path().join("v2")).unwrap(),
            plugin_id.clone(),
        )
        .unwrap();

    assert_eq!(
        store.active_plugin_version(&plugin_id),
        Some("2.0.0".to_string())
    );
    assert!(
        tmp.path()
            .join("plugins/cache/debug/sample-plugin/2.0.0")
            .is_dir()
    );
    assert!(
        !tmp.path()
            .join("plugins/cache/debug/sample-plugin/1.0.0")
            .exists()
    );
}

#[test]
fn old_plugin_version_would_stay_active_for_local_or_later_versions() {
    assert!(old_plugin_version_would_stay_active(
        DEFAULT_PLUGIN_VERSION,
        "1.0.0"
    ));
    assert!(old_plugin_version_would_stay_active("10.0.0", "9.0.0"));
    assert!(!old_plugin_version_would_stay_active("1.0.0", "2.0.0"));
}

#[test]
fn same_version_refresh_keeps_identical_files_in_place() {
    let tmp = tempdir().unwrap();
    write_plugin_with_version(tmp.path(), "source", "sample", Some("1.0.0"));
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::parse("sample@debug").unwrap();
    let source = AbsolutePathBuf::try_from(tmp.path().join("source")).unwrap();
    let result = store.install(source.clone(), plugin_id.clone()).unwrap();
    let installed_skill = result.installed_path.join("skills/SKILL.md");
    let identity_link = tmp.path().join("installed-skill-link");
    fs::hard_link(installed_skill.as_path(), &identity_link).unwrap();

    store.install(source, plugin_id).unwrap();

    // This link still names the installed file only if refresh did not replace it.
    fs::write(identity_link, "same file identity").unwrap();
    assert_eq!(
        fs::read_to_string(installed_skill).unwrap(),
        "same file identity"
    );
}

#[test]
fn same_version_refresh_replaces_changed_payload_and_removes_extra_entries() {
    let tmp = tempdir().unwrap();
    write_plugin_with_version(tmp.path(), "source", "sample", Some("1.0.0"));
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::parse("sample@debug").unwrap();
    let source = AbsolutePathBuf::try_from(tmp.path().join("source")).unwrap();
    let result = store.install(source.clone(), plugin_id.clone()).unwrap();
    // Same manifest, same payload length: neither is an equality oracle.
    fs::write(source.join("skills/SKILL.md"), "other").unwrap();
    store.install(source.clone(), plugin_id.clone()).unwrap();
    assert_eq!(
        fs::read_to_string(result.installed_path.join("skills/SKILL.md")).unwrap(),
        "other"
    );
    let extra = result.installed_path.join("extra");
    fs::create_dir_all(extra.as_path()).unwrap();
    fs::write(extra.join("unexpected"), "cached only").unwrap();
    store.install(source, plugin_id).unwrap();
    assert!(!extra.as_path().exists());
}

#[cfg(unix)]
#[test]
fn same_version_refresh_replaces_changed_executable_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().unwrap();
    write_plugin_with_version(tmp.path(), "source", "sample", Some("1.0.0"));
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::parse("sample@debug").unwrap();
    let source = AbsolutePathBuf::try_from(tmp.path().join("source")).unwrap();
    let result = store.install(source.clone(), plugin_id.clone()).unwrap();
    fs::set_permissions(
        source.join("skills/SKILL.md"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    store.install(source, plugin_id).unwrap();
    assert_eq!(
        fs::metadata(result.installed_path.join("skills/SKILL.md"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
}

#[test]
fn same_version_concurrent_refreshes_preserve_complete_installation() {
    let tmp = tempdir().unwrap();
    write_plugin_with_version(tmp.path(), "source", "sample", Some("1.0.0"));
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::parse("sample@debug").unwrap();
    let source = AbsolutePathBuf::try_from(tmp.path().join("source")).unwrap();
    store.install(source.clone(), plugin_id.clone()).unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let threads = (0..2)
        .map(|_| {
            let store = store.clone();
            let source = source.clone();
            let plugin_id = plugin_id.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                store.install(source, plugin_id).unwrap()
            })
        })
        .collect::<Vec<_>>();
    for thread in threads {
        let result = thread.join().unwrap();
        assert_eq!(
            fs::read_to_string(result.installed_path.join("skills/SKILL.md")).unwrap(),
            "skill"
        );
    }
    assert_eq!(
        store.active_plugin_version(&plugin_id),
        Some("1.0.0".to_string())
    );
}

#[test]
fn plugin_installation_lock_child() {
    let Some(home) = std::env::var_os("CODEX_PLUGIN_STORE_LOCK_TEST_HOME") else {
        return;
    };
    let home = PathBuf::from(home);
    let store = PluginStore::new(home.clone());
    let plugin_id = PluginId::parse("sample@debug").unwrap();
    let lock_path = store.root().join("debug/.sample.install.lock");
    let probe = fs::File::options()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    assert!(matches!(
        probe.try_lock(),
        Err(fs::TryLockError::WouldBlock)
    ));
    drop(probe);
    fs::write(home.join("child-ready"), "lock held by parent").unwrap();
    store
        .install(
            AbsolutePathBuf::try_from(home.join("source")).unwrap(),
            plugin_id,
        )
        .unwrap();
    fs::write(home.join("child-complete"), "installed").unwrap();
}

#[test]
fn same_version_refresh_waits_for_other_process_installation_lock() {
    use std::process::Command;
    use std::process::Stdio;
    use std::time::Duration;
    use std::time::Instant;

    let tmp = tempdir().unwrap();
    write_plugin_with_version(tmp.path(), "source", "sample", Some("1.0.0"));
    let store = PluginStore::new(tmp.path().to_path_buf());
    let plugin_id = PluginId::parse("sample@debug").unwrap();
    store
        .install(
            AbsolutePathBuf::try_from(tmp.path().join("source")).unwrap(),
            plugin_id.clone(),
        )
        .unwrap();
    let installation_lock = store.lock_plugin_installation(&plugin_id).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "store::tests::plugin_installation_lock_child",
            "--nocapture",
        ])
        .env("CODEX_PLUGIN_STORE_LOCK_TEST_HOME", tmp.path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !tmp.path().join("child-ready").exists() {
        if child.try_wait().unwrap().is_some() || Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!("child did not confirm competing process lock: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!tmp.path().join("child-complete").exists());
    drop(installation_lock);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().unwrap();
            panic!("child did not finish after lock release: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(tmp.path().join("child-complete")).unwrap(),
        "installed"
    );
}

#[cfg(windows)]
mod windows_refresh {
    use super::*;
    use pretty_assertions::assert_eq;
    use std::io::BufRead;
    use std::os::windows::fs::OpenOptionsExt;
    use std::process::Child;
    use std::process::Command;
    use std::process::Stdio;
    use std::time::Duration;

    struct PluginChild(Child);

    impl PluginChild {
        fn in_directory(directory: &Path) -> Self {
            let mut child = Self(
                Command::new("cmd.exe")
                    .args(["/D", "/Q", "/C", "echo ready & set /p finish="])
                    .current_dir(directory)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let stdout = child.0.stdout.take().unwrap();
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let mut line = String::new();
                let result = io::BufReader::new(stdout).read_line(&mut line);
                let _ = sender.send((result, line));
            });
            let (result, line) = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
            result.unwrap();
            assert_eq!(line.trim(), "ready");
            assert!(child.0.try_wait().unwrap().is_none());
            child
        }
    }

    impl Drop for PluginChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            self.0.wait().unwrap();
        }
    }

    fn assert_root_is_locked(store: &PluginStore, plugin_id: &PluginId) {
        let root = store.plugin_base_root(plugin_id);
        let moved = root.as_path().with_extension("backup-probe");
        let error = fs::rename(root.as_path(), moved.as_path())
            .expect_err("the fixture must exercise an actual Windows rename lock");
        assert!(matches!(error.raw_os_error(), Some(5 | 32)), "{error}");
        assert!(root.as_path().is_dir());
        assert!(!moved.exists());
    }

    #[test]
    fn same_version_refresh_with_running_child_cwd_avoids_backup_rename() {
        let tmp = tempdir().unwrap();
        write_plugin_with_version(tmp.path(), "source", "sample", Some("1.0.0"));
        let store = PluginStore::new(tmp.path().to_path_buf());
        let plugin_id = PluginId::parse("sample@debug").unwrap();
        let source = AbsolutePathBuf::try_from(tmp.path().join("source")).unwrap();
        let installed = store.install(source.clone(), plugin_id.clone()).unwrap();
        let mut child = PluginChild::in_directory(installed.installed_path.as_path());
        assert_root_is_locked(&store, &plugin_id);

        store.install(source, plugin_id.clone()).unwrap();

        assert!(child.0.try_wait().unwrap().is_none());
        assert_eq!(
            store.active_plugin_version(&plugin_id),
            Some("1.0.0".to_string())
        );
    }

    #[test]
    fn same_version_refresh_with_no_delete_file_handle_keeps_existing_payload() {
        let tmp = tempdir().unwrap();
        write_plugin_with_version(tmp.path(), "source", "sample", Some("1.0.0"));
        let store = PluginStore::new(tmp.path().to_path_buf());
        let plugin_id = PluginId::parse("sample@debug").unwrap();
        let source = AbsolutePathBuf::try_from(tmp.path().join("source")).unwrap();
        let installed = store.install(source.clone(), plugin_id.clone()).unwrap();
        let skill = installed.installed_path.join("skills/SKILL.md");
        let mut handle = fs::File::options()
            .read(true)
            .share_mode(3)
            .open(skill.as_path())
            .unwrap();
        assert_root_is_locked(&store, &plugin_id);

        store.install(source, plugin_id).unwrap();

        let mut contents = String::new();
        handle.read_to_string(&mut contents).unwrap();
        assert_eq!(contents, "skill");
    }

    #[test]
    fn changed_same_version_locked_cache_fails_without_partial_update_then_recovers() {
        let tmp = tempdir().unwrap();
        write_plugin_with_version(tmp.path(), "source", "sample", Some("1.0.0"));
        let store = PluginStore::new(tmp.path().to_path_buf());
        let plugin_id = PluginId::parse("sample@debug").unwrap();
        let source = AbsolutePathBuf::try_from(tmp.path().join("source")).unwrap();
        let installed = store.install(source.clone(), plugin_id.clone()).unwrap();
        let skill = installed.installed_path.join("skills/SKILL.md");
        let child = PluginChild::in_directory(installed.installed_path.as_path());
        assert_root_is_locked(&store, &plugin_id);
        fs::write(source.join("skills/SKILL.md"), "other").unwrap();

        let error = store
            .install(source.clone(), plugin_id.clone())
            .unwrap_err();

        assert!(matches!(
            error,
            PluginStoreError::Io {
                context: "failed to back up plugin cache entry",
                ..
            }
        ));
        assert_eq!(fs::read_to_string(skill.as_path()).unwrap(), "skill");
        assert_eq!(
            store.active_plugin_version(&plugin_id),
            Some("1.0.0".to_string())
        );
        drop(child);
        store.install(source, plugin_id).unwrap();
        assert_eq!(fs::read_to_string(skill).unwrap(), "other");
    }

    #[test]
    fn unreadable_same_version_cache_is_an_error_not_an_unchanged_success() {
        let tmp = tempdir().unwrap();
        write_plugin_with_version(tmp.path(), "source", "sample", Some("1.0.0"));
        let store = PluginStore::new(tmp.path().to_path_buf());
        let plugin_id = PluginId::parse("sample@debug").unwrap();
        let source = AbsolutePathBuf::try_from(tmp.path().join("source")).unwrap();
        let installed = store.install(source.clone(), plugin_id.clone()).unwrap();
        let skill = installed.installed_path.join("skills/SKILL.md");
        let handle = fs::File::options()
            .read(true)
            .share_mode(0)
            .open(skill.as_path())
            .unwrap();

        let error = store.install(source, plugin_id).unwrap_err();

        assert!(matches!(
            error,
            PluginStoreError::Io {
                context: "failed to compare installed plugin cache entry",
                ..
            }
        ));
        drop(handle);
        assert_eq!(fs::read_to_string(skill).unwrap(), "skill");
    }

    #[test]
    fn new_version_activates_while_old_version_child_keeps_running() {
        let tmp = tempdir().unwrap();
        write_plugin_with_version(tmp.path(), "v1", "sample", Some("1.0.0"));
        write_plugin_with_version(tmp.path(), "v2", "sample", Some("2.0.0"));
        let store = PluginStore::new(tmp.path().to_path_buf());
        let plugin_id = PluginId::parse("sample@debug").unwrap();
        let installed = store
            .install(
                AbsolutePathBuf::try_from(tmp.path().join("v1")).unwrap(),
                plugin_id.clone(),
            )
            .unwrap();
        let mut child = PluginChild::in_directory(installed.installed_path.as_path());
        assert_root_is_locked(&store, &plugin_id);

        let updated = store
            .install(
                AbsolutePathBuf::try_from(tmp.path().join("v2")).unwrap(),
                plugin_id.clone(),
            )
            .unwrap();

        assert!(child.0.try_wait().unwrap().is_none());
        assert_eq!(
            store.active_plugin_version(&plugin_id),
            Some("2.0.0".to_string())
        );
        assert_eq!(
            fs::read_to_string(updated.installed_path.join("skills/SKILL.md")).unwrap(),
            "skill"
        );
    }
}

#[test]
fn plugin_root_rejects_path_separators_in_key_segments() {
    let err = PluginId::parse("../../etc@debug").unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid plugin name: dots must separate non-empty name segments in `../../etc@debug`"
    );

    let err = PluginId::parse("sample@../../etc").unwrap_err();
    assert_eq!(
        err.to_string(),
        "invalid marketplace name: only ASCII letters, digits, `_`, and `-` are allowed in `sample@../../etc`"
    );
}

#[test]
fn install_rejects_manifest_names_with_path_separators() {
    let tmp = tempdir().unwrap();
    write_plugin(tmp.path(), "source-dir", "../../etc");

    let err = PluginStore::new(tmp.path().to_path_buf())
        .install(
            AbsolutePathBuf::try_from(tmp.path().join("source-dir")).unwrap(),
            PluginId::new("source-dir".to_string(), "debug".to_string()).unwrap(),
        )
        .unwrap_err();

    assert_eq!(
        err.to_string(),
        "invalid plugin name: dots must separate non-empty name segments"
    );
}

#[test]
fn install_rejects_marketplace_names_with_path_separators() {
    let err = PluginId::new("sample-plugin".to_string(), "../../etc".to_string()).unwrap_err();

    assert_eq!(
        err.to_string(),
        "invalid marketplace name: only ASCII letters, digits, `_`, and `-` are allowed"
    );
}

#[test]
fn install_rejects_manifest_names_that_do_not_match_marketplace_plugin_name() {
    let tmp = tempdir().unwrap();
    write_plugin(tmp.path(), "source-dir", "manifest-name");

    let err = PluginStore::new(tmp.path().to_path_buf())
        .install(
            AbsolutePathBuf::try_from(tmp.path().join("source-dir")).unwrap(),
            PluginId::new("different-name".to_string(), "debug".to_string()).unwrap(),
        )
        .unwrap_err();

    assert_eq!(
        err.to_string(),
        "plugin.json name `manifest-name` does not match marketplace plugin name `different-name`"
    );
}
