// Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md.
//! Explicit browsing and user corrections for the loaded thread's memory scope.
use super::*;
use codex_app_server_protocol::MemoryNoteScope;
use codex_app_server_protocol::ThreadMemoryNoteAddParams;
use codex_app_server_protocol::ThreadMemoryNoteAddResponse;
use codex_app_server_protocol::ThreadMemoryReadParams;
use codex_app_server_protocol::ThreadMemoryReadResponse;
use std::io::Read;

use codex_core::config::MEMORY_SUMMARY_MAX_BYTES as MAX_SUMMARY_BYTES;

/// Resolve an explicit write target without rebinding the loaded thread's memory namespace.
async fn memory_note_target(
    config: &Config,
    environments: &[codex_protocol::protocol::TurnEnvironmentSelection],
    requested_scope: Option<MemoryNoteScope>,
) -> Result<(AbsolutePathBuf, MemoryNoteScope), JSONRPCErrorError> {
    use codex_config::types::MemoryScope;
    let scope = requested_scope.unwrap_or(match config.memories.scope {
        MemoryScope::Project => MemoryNoteScope::Project,
        MemoryScope::Global => MemoryNoteScope::Global,
    });
    let mut target = config.clone();
    target.memories.scope = match scope {
        MemoryNoteScope::Project => MemoryScope::Project,
        MemoryNoteScope::Global => MemoryScope::Global,
    };
    if !target.memory_environment_supported(environments) {
        return Err(invalid_params(
            "project memory note requires a single local environment",
        ));
    }
    if scope == MemoryNoteScope::Project {
        if config.active_project.is_untrusted() {
            return Err(invalid_params(
                "project memory notes are unavailable in an untrusted project",
            ));
        }
        if config.memories.scope != MemoryScope::Project {
            // Global threads have no pinned project identity. Resolve only this explicit target,
            // using the retained thread cwd and marker configuration, never a model-supplied path.
            #[derive(serde::Deserialize)]
            struct Markers {
                project_root_markers: Option<Vec<String>>,
            }
            let markers: Markers = config
                .config_layer_stack
                .effective_config()
                .try_into()
                .map_err(|error| {
                    invalid_params(format!("project memory markers are unavailable: {error}"))
                })?;
            let markers = markers
                .project_root_markers
                .unwrap_or_else(codex_config::default_project_root_markers);
            let cwd = config.cwd.clone();
            let environment_cwd = environments
                .first()
                .map(|environment| environment.cwd.to_abs_path())
                .transpose()
                .map_err(|error| {
                    invalid_params(format!("project memory local cwd is unavailable: {error}"))
                })?;
            target.memories.project = Some(
                tokio::task::spawn_blocking(move || {
                    if let Some(environment_cwd) = environment_cwd
                        && environment_cwd.canonicalize()? != cwd.canonicalize()?
                    {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "local environment cwd does not match the loaded thread project",
                        ));
                    }
                    codex_config::memory_scope::MemoryProject::resolve(cwd.as_path(), &markers)
                })
                .await
                .map_err(|error| {
                    internal_error(format!(
                        "project memory identity resolution failed: {error}"
                    ))
                })?
                .map_err(|error| {
                    invalid_params(format!("project memory identity is unavailable: {error}"))
                })?,
            );
        }
    }
    let root = target
        .memories
        .memory_root(&config.codex_home)
        .ok_or_else(|| invalid_params("project memory identity is unavailable"))?;
    codex_core::config::validate_scoped_memory_root(root.as_path())
        .map_err(|error| invalid_params(format!("memory note target is unavailable: {error}")))?;
    Ok((root, scope))
}

impl ThreadRequestProcessor {
    pub(crate) async fn thread_memory_read(
        &self,
        params: ThreadMemoryReadParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        let (thread_id, thread) = self.load_thread(&params.thread_id).await?;
        let config = thread.config().await;
        let snapshot = thread.config_snapshot().await;
        let root = config
            .memories
            .memory_root(&config.codex_home)
            .filter(|_| config.memory_environment_supported(snapshot.environment_selections()));
        let scope = if config.memories.scope == codex_config::types::MemoryScope::Project {
            "project"
        } else {
            "global"
        };
        let enabled = config.features.enabled(Feature::MemoryTool);
        let (summary, status) = if root.is_none() {
            (None, "unavailable")
        } else if !enabled || !config.memories.use_memories && params.include_disabled != Some(true)
        {
            (None, "disabled")
        } else if root.as_ref().is_some_and(|root| {
            codex_core::config::validate_scoped_memory_root(root.as_path()).is_err()
        }) {
            (None, "unreadable")
        } else {
            let path = root
                .as_ref()
                .expect("checked root")
                .join("memory_summary.md");
            tokio::task::spawn_blocking(move || {
                let file = match std::fs::File::open(path) {
                    Ok(file) => file,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                        return (None, "missing");
                    }
                    Err(_) => return (None, "unreadable"),
                };
                let mut bytes = Vec::new();
                if file
                    .take(MAX_SUMMARY_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .is_err()
                {
                    return (None, "unreadable");
                }
                if bytes.len() as u64 > MAX_SUMMARY_BYTES {
                    return (None, "oversized");
                }
                match String::from_utf8(bytes) {
                    Ok(summary) if !summary.trim().is_empty() => (Some(summary), "ready"),
                    Ok(_) => (None, "missing"),
                    Err(_) => (None, "unreadable"),
                }
            })
            .await
            .map_err(|error| internal_error(format!("memory read failed: {error}")))?
        };
        let generate = if let Some(db) = thread.state_db() {
            db.get_thread_memory_mode(thread_id)
                .await
                .map_err(|error| {
                    internal_error(format!("memory generation state unavailable: {error}"))
                })?
                .map_or(config.memories.generate_memories, |mode| mode == "enabled")
        } else {
            false
        };
        let consolidation = if enabled && root.is_some() {
            match codex_memories_write::memory_consolidation_status(&thread, &config).await {
                Ok(state) => {
                    state.map(
                        |state| codex_app_server_protocol::MemoryConsolidationState {
                            status: state.status,
                            started_at: state.started_at,
                            finished_at: state.finished_at,
                            lease_until: state.lease_until,
                            retry_at: state.retry_at,
                            last_error: state.last_error,
                            input_watermark: state.input_watermark,
                            last_success_watermark: state.last_success_watermark,
                        },
                    )
                }
                Err(error) => Some(codex_app_server_protocol::MemoryConsolidationState {
                    status: "unavailable".into(),
                    started_at: None,
                    finished_at: None,
                    lease_until: None,
                    retry_at: None,
                    last_error: Some(error.to_string()),
                    input_watermark: None,
                    last_success_watermark: None,
                }),
            }
        } else {
            None
        };
        Ok(Some(
            ThreadMemoryReadResponse {
                summary,
                scope: Some(scope.to_string()),
                root: root.as_ref().map(|root| root.display().to_string()),
                status: Some(status.to_string()),
                use_memories: Some(enabled && config.memories.use_memories),
                generate_memories: Some(enabled && generate),
                consolidation,
            }
            .into(),
        ))
    }

    pub(crate) async fn thread_memory_consolidate(
        &self,
        params: codex_app_server_protocol::ThreadMemoryConsolidateParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        let (thread_id, thread) = self.load_thread(&params.thread_id).await?;
        let config = thread.config().await;
        let snapshot = thread.config_snapshot().await;
        // This RPC starts a model worker without the ordinary turn admission path.
        // Preserve the same managed provider/network-policy check as thread/sample.
        self.config_manager
            .check_thread_model_provider(config.as_ref())
            .await
            .map_err(|error| config_load_error(&error))?;
        let outcome = Box::pin(codex_memories_write::consolidate_memories(
            Arc::clone(&self.thread_manager),
            Arc::clone(&self.auth_manager),
            thread_id,
            thread,
            config,
            snapshot.permission_profile,
            &snapshot.session_source,
            params.retry,
        ))
        .await
        .map_err(|error| {
            internal_error(format!("memory consolidation admission failed: {error}"))
        })?;
        Ok(Some(
            codex_app_server_protocol::ThreadMemoryConsolidateResponse {
                status: outcome.status,
                reason: outcome.reason,
            }
            .into(),
        ))
    }

    pub(crate) async fn thread_memory_note_add(
        &self,
        params: ThreadMemoryNoteAddParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        let (_, thread) = self.load_thread(&params.thread_id).await?;
        let config = thread.config().await;
        let snapshot = thread.config_snapshot().await;
        if !config.features.enabled(Feature::MemoryTool) {
            return Err(invalid_params(
                "memory is disabled or unavailable for this environment",
            ));
        }
        let (root, scope) =
            memory_note_target(&config, snapshot.environment_selections(), params.scope).await?;
        codex_memories_write::add_user_memory_note(root.as_path(), &params.note)
            .await
            .map_err(|error| invalid_params(format!("memory note was not saved: {error}")))?;
        Ok(Some(
            ThreadMemoryNoteAddResponse {
                pending_consolidation: true,
                scope: Some(scope),
                root: Some(root.display().to_string()),
            }
            .into(),
        ))
    }
}

#[cfg(test)]
mod note_scope_tests {
    use super::*;
    use codex_core::config::ConfigBuilder;
    use codex_core::config::ConfigOverrides;
    use codex_protocol::protocol::EnvironmentConfigState;
    use codex_protocol::protocol::TurnEnvironmentRequest;
    use codex_protocol::protocol::TurnEnvironmentSelection;
    use codex_utils_path_uri::PathUri;

    #[tokio::test]
    async fn memory_note_scope_target_rejects_untrusted_remote_and_missing_identity()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        let config = ConfigBuilder::default()
            .codex_home(home.path().to_path_buf())
            .loader_overrides(codex_config::LoaderOverrides::without_managed_config_for_tests())
            .harness_overrides(ConfigOverrides {
                cwd: Some(project.path().to_path_buf()),
                ..Default::default()
            })
            .build()
            .await?;
        let mut untrusted = config.clone();
        untrusted.active_project.trust_level =
            Some(codex_protocol::config_types::TrustLevel::Untrusted);
        let error = memory_note_target(&untrusted, &[], Some(MemoryNoteScope::Project))
            .await
            .unwrap_err();
        assert!(error.message.contains("untrusted"));
        let environment = |id: &str| {
            TurnEnvironmentSelection::new(
                TurnEnvironmentRequest {
                    environment_id: id.into(),
                    cwd: PathUri::from_abs_path(&config.cwd),
                    workspace_roots: vec![],
                    config: EnvironmentConfigState::FromThread,
                },
                &[],
            )
        };
        let error = memory_note_target(
            &config,
            &[environment("remote")],
            Some(MemoryNoteScope::Project),
        )
        .await
        .unwrap_err();
        assert!(error.message.contains("single local environment"));
        let error = memory_note_target(
            &config,
            &[environment("local"), environment("local")],
            Some(MemoryNoteScope::Project),
        )
        .await
        .unwrap_err();
        assert!(error.message.contains("single local environment"));
        let mut different = environment("local");
        different.cwd = PathUri::from_abs_path(&config.codex_home);
        let error = memory_note_target(&config, &[different], Some(MemoryNoteScope::Project))
            .await
            .unwrap_err();
        assert!(error.message.contains("does not match"));
        let mut missing = config.clone();
        missing.cwd = config.cwd.join("missing-project-directory");
        let error = memory_note_target(&missing, &[], Some(MemoryNoteScope::Project))
            .await
            .unwrap_err();
        assert!(error.message.contains("identity is unavailable"));
        let mut unavailable = config.clone();
        unavailable.memories.scope = codex_config::types::MemoryScope::Project;
        unavailable.memories.project = None;
        let error = memory_note_target(&unavailable, &[], None)
            .await
            .unwrap_err();
        assert!(error.message.contains("identity is unavailable"));
        assert!(!home.path().join("project_memories").exists());
        assert!(
            !home
                .path()
                .join("memories/extensions/ad_hoc/notes")
                .exists()
        );
        Ok(())
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn memory_note_scope_target_rejects_redirected_namespace_before_creating_notes()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        let mut config = ConfigBuilder::default()
            .codex_home(home.path().to_path_buf())
            .loader_overrides(codex_config::LoaderOverrides::without_managed_config_for_tests())
            .harness_overrides(ConfigOverrides {
                cwd: Some(project.path().to_path_buf()),
                ..Default::default()
            })
            .build()
            .await?;
        config.memories.scope = codex_config::types::MemoryScope::Project;
        config.memories.project = Some(codex_config::memory_scope::MemoryProject::resolve(
            project.path(),
            &[],
        )?);
        let alias = home.path().join("project_memories");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), &alias)?;
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            let result = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&alias)
                .arg(outside.path())
                .creation_flags(0x08000000) // CREATE_NO_WINDOW
                .output()?;
            anyhow::ensure!(
                result.status.success(),
                "fixture junction failed: {}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
        let error = memory_note_target(&config, &[], Some(MemoryNoteScope::Project))
            .await
            .unwrap_err();
        assert!(error.message.contains("redirected"));
        assert!(std::fs::read_dir(outside.path())?.next().is_none());
        Ok(())
    }
}
