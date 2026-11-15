// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Restrict consolidation worker permissions without changing the parent.
use codex_core::config::Config;
use codex_features::Feature;
use codex_protocol::intersect_effective_permission_profiles;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::SandboxPolicy;
use codex_utils_absolute_path::AbsolutePathBuf;

/// Managed pins can normalize a successful disable request back to enabled.
/// Check the resulting policy before either worker isolation path is admitted.
fn restrict_worker_features(config: &mut Config) -> anyhow::Result<()> {
    let forbidden = [
        Feature::Collab,
        Feature::MemoryTool,
        Feature::Apps,
        Feature::Plugins,
        Feature::CodexHooks,
        Feature::SkillMcpDependencyInstall,
    ];
    for feature in forbidden {
        config.features.disable(feature)?;
    }
    for feature in forbidden {
        anyhow::ensure!(
            !config.features.enabled(feature),
            "required worker feature `{}` prevents memory isolation",
            feature.key()
        );
    }
    Ok(())
}

/// Intersect, never replace, the parent's already-materialized authority.
/// External, unsupported path/policy shapes and constraint rejection are errors.
pub(super) fn bind_child_stage(
    mut config: Config,
    parent: &PermissionProfile,
    stage: &AbsolutePathBuf,
) -> anyhow::Result<Config> {
    restrict_worker_features(&mut config)?;
    #[cfg(windows)]
    {
        use codex_core::windows_sandbox::WindowsSandboxLevelExt;
        use codex_protocol::config_types::WindowsSandboxLevel;
        use codex_protocol::sandbox::{effective_windows_sandbox_type, SandboxType};
        let sandbox = effective_windows_sandbox_type(
            config.effective_local_windows_sandbox_type(),
            WindowsSandboxLevel::from_config(&config),
        );
        anyhow::ensure!(sandbox != SandboxType::None, "local Windows sandbox is unavailable");
    }
    #[cfg(target_os = "linux")]
    anyhow::ensure!(config.codex_linux_sandbox_exe.is_some(), "local Linux sandbox is unavailable");
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    anyhow::bail!("memory child sandbox is unsupported on this platform");
    let requested = SandboxPolicy::WorkspaceWrite {
        writable_roots: vec![stage.clone()],
        network_access: false,
        exclude_tmpdir_env_var: true,
        exclude_slash_tmp: true,
    };
    let requested = PermissionProfile::from_legacy_sandbox_policy_for_cwd(&requested, stage.as_path());
    let restricted = intersect_effective_permission_profiles(parent, &requested, stage.as_path())?;
    anyhow::ensure!(matches!(restricted, PermissionProfile::Managed { .. }), "managed child sandbox unavailable");
    anyhow::ensure!(!restricted.network_sandbox_policy().is_enabled(), "child network restriction unavailable");
    anyhow::ensure!(restricted.file_system_sandbox_policy().can_write_local_path_with_cwd(stage.as_path(), stage.as_path()), "parent does not permit stage writes");
    config.cwd = stage.clone();
    config.permissions.set_permission_profile(restricted)?;
    config.permissions.set_workspace_roots(vec![stage.clone()]);
    config.workspace_roots = vec![stage.clone()];
    Ok(config)
}

/// On a Windows host without an OS sandbox, the isolated worker may use only
/// host-mediated stage operations. No unrestricted process tool is registered.
pub(super) fn bind_host_mediated_child(
    mut config: Config,
    parent: &PermissionProfile,
    stage: &AbsolutePathBuf,
) -> anyhow::Result<Config> {
    restrict_worker_features(&mut config)?;
    #[cfg(not(windows))]
    { let _ = (config, parent, stage); anyhow::bail!("host-mediated memory stage is unavailable on this platform"); }
    #[cfg(windows)]
    {
        use std::path::{Component, Prefix};
        use codex_core::windows_sandbox::WindowsSandboxLevelExt;
        use codex_protocol::config_types::WindowsSandboxLevel;
        use codex_protocol::sandbox::{effective_windows_sandbox_type, SandboxType};
        let sandbox = effective_windows_sandbox_type(config.effective_local_windows_sandbox_type(), WindowsSandboxLevel::from_config(&config));
        anyhow::ensure!(sandbox == SandboxType::None, "host-mediated fallback is reserved for an unavailable OS sandbox");
        anyhow::ensure!(matches!(stage.as_path().components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))), "host-mediated memory requires a local disk stage");
        match parent {
            PermissionProfile::External { .. } => anyhow::bail!("external filesystem authority cannot be verified"),
            PermissionProfile::Disabled => {},
            PermissionProfile::Managed { .. } => {
                let fs = parent.file_system_sandbox_policy();
                anyhow::ensure!(fs.can_read_local_path_with_cwd(stage.as_path(), stage.as_path()) && fs.can_write_local_path_with_cwd(stage.as_path(), stage.as_path()), "parent does not grant stage access");
            }
        }
        config.cwd = stage.clone();
        config.permissions.set_workspace_roots(vec![stage.clone()]);
        config.workspace_roots = vec![stage.clone()];
        Ok(config)
    }
}
