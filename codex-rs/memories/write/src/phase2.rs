use crate::MemoryWorkspaceLock;
use crate::lifecycle::ConsolidationPublication;
use crate::metrics::MEMORY_PHASE_TWO_E2E_MS;
use crate::metrics::MEMORY_PHASE_TWO_INPUT;
use crate::metrics::MEMORY_PHASE_TWO_JOBS;
use crate::metrics::MEMORY_PHASE_TWO_TOKEN_USAGE;
use crate::prompts::build_consolidation_prompt_for_version;
use crate::prune_old_extension_resources;
use crate::rebuild_raw_memories_file_from_memories;
use crate::runtime::MemoryStartupContext;
use crate::runtime::SpawnedConsolidationAgent;
use crate::sync_rollout_summaries_from_memories;
use crate::workspace::MemoryStage;
use crate::workspace::memory_workspace_diff;
use crate::workspace::prepare_memory_workspace;
use crate::workspace::remove_memory_symlinks;
use crate::workspace::reset_memory_workspace_baseline;
use crate::workspace::validate_consolidation_artifacts_for_version;
use crate::workspace::write_workspace_diff;
use codex_config::Constrained;
use codex_core::config::Config;
use codex_features::Feature;
use codex_model_provider::ModelProvider;
use codex_protocol::MemoryVersion;
use codex_protocol::ThreadId;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::AgentStatus;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::protocol::TokenUsage;
use codex_protocol::user_input::UserInput;
use codex_state::MemoryStore;
use codex_state::Stage1Output;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
struct Claim {
    token: String,
    watermark: i64,
}

#[derive(Debug, Clone, Default)]
struct Counters {
    input: i64,
}

/// Admission result only; dispatched workers can still fail later.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MemoryConsolidationStartOutcome {
    pub status: String,
    pub reason: Option<String>,
}

impl MemoryConsolidationStartOutcome {
    pub(crate) fn new(status: &str, reason: Option<&str>) -> Self {
        Self {
            status: status.to_owned(),
            reason: reason.map(str::to_owned),
        }
    }
}

/// Runs memory phase 2 (aka consolidation) in strict order. The method represents the linear
/// flow of the consolidation phase.
pub async fn run(
    context: Arc<MemoryStartupContext>,
    config: Arc<Config>,
    parent_permission_profile: PermissionProfile,
) {
    let _ = run_with_retry(context, config, parent_permission_profile, false).await;
}

pub(crate) async fn run_with_retry(
    context: Arc<MemoryStartupContext>,
    config: Arc<Config>,
    parent_permission_profile: PermissionProfile,
    explicit_retry: bool,
) -> MemoryConsolidationStartOutcome {
    if !context.generation_enabled().await {
        return MemoryConsolidationStartOutcome::new("blocked", Some("generation_disabled"));
    }
    let phase_two_e2e_timer = context.start_timer(MEMORY_PHASE_TWO_E2E_MS);

    let Some(db) = context.memory_store().await else {
        // This should not happen.
        return MemoryConsolidationStartOutcome::new("blocked", Some("state_unavailable"));
    };
    let Some(root) = config.memories.memory_root(&config.codex_home) else {
        return MemoryConsolidationStartOutcome::new("blocked", Some("root_unavailable"));
    };
    let max_raw_memories = config.memories.max_raw_memories_for_consolidation;
    let max_unused_days = config.memories.max_unused_days;

    let workspace_lock = match MemoryWorkspaceLock::try_acquire(&root) {
        Ok(lock) => Arc::new(lock),
        Err(err) => {
            let status = if err.kind() == std::io::ErrorKind::WouldBlock {
                "skipped_workspace_busy"
            } else {
                tracing::error!("failed locking memory workspace: {err}");
                "failed_workspace_lock"
            };
            context.counter(MEMORY_PHASE_TWO_JOBS, /*inc*/ 1, &[("status", status)]);
            return MemoryConsolidationStartOutcome::new("blocked", Some(status));
        }
    };
    // An incomplete reset must never be repaired by extracting/consolidating
    // the old inputs. Check before claiming a job or touching workspace files.
    match codex_memories_read::publication::load_publication(&root) {
        Ok(Some(state))
            if state.status == codex_memories_read::publication::PublicationStatus::Resetting =>
        {
            return MemoryConsolidationStartOutcome::new(
                "blocked",
                Some("reset_recovery_required"),
            );
        }
        Ok(_) => {}
        Err(_) => {
            return MemoryConsolidationStartOutcome::new(
                "blocked",
                Some("publication_unavailable"),
            );
        }
    }

    // 1. Claim the global Phase 2 lock before touching the memory workspace.
    let claim = match job::claim(context.as_ref(), &db, explicit_retry).await {
        Ok(claim) => claim,
        Err(e) => {
            context.counter(MEMORY_PHASE_TWO_JOBS, /*inc*/ 1, &[("status", e)]);
            return MemoryConsolidationStartOutcome::new("blocked", Some(e));
        }
    };

    // 2. Ensure the memories root has a git baseline repository.
    if let Err(err) = prepare_memory_workspace(&root).await {
        tracing::error!("failed preparing memory workspace: {err}");
        job::failed(context.as_ref(), &db, &claim, "failed_prepare_workspace").await;
        return MemoryConsolidationStartOutcome::new("failed", Some("failed_prepare_workspace"));
    }
    if let Err(err) = crate::extensions::seed_extension_instructions(&root).await {
        tracing::error!("failed seeding memory extension instructions: {err}");
        job::failed(context.as_ref(), &db, &claim, "failed_seed_instructions").await;
        return MemoryConsolidationStartOutcome::new("failed", Some("failed_seed_instructions"));
    }

    // 3. Build the locked-down config used by the consolidation agent.
    let Some(agent_config) = agent::get_config(
        config.as_ref(),
        parent_permission_profile,
        context.provider(),
    ) else {
        // If we can't get the config, we can't consolidate.
        tracing::error!("failed to get agent config");
        job::failed(context.as_ref(), &db, &claim, "failed_sandbox_policy").await;
        return MemoryConsolidationStartOutcome::new("failed", Some("failed_sandbox_policy"));
    };

    // 4. Load current DB-backed Phase 2 inputs.
    let raw_memories = match db
        .get_phase2_input_selection(max_raw_memories, max_unused_days)
        .await
    {
        Ok(raw_memories) => raw_memories,
        Err(err) => {
            tracing::error!("failed to list stage1 outputs from global: {err}");
            job::failed(context.as_ref(), &db, &claim, "failed_load_stage1_outputs").await;
            return MemoryConsolidationStartOutcome::new(
                "failed",
                Some("failed_load_stage1_outputs"),
            );
        }
    };
    let raw_memory_count = raw_memories.len();
    let new_watermark = get_watermark(claim.watermark, &raw_memories);

    // 5. Sync the current inputs into the memory workspace.
    if let Err(err) = sync_phase2_workspace_inputs(&root, &raw_memories, &config).await {
        tracing::error!("failed syncing phase2 workspace inputs: {err}");
        if let Err(error) =
            crate::lifecycle::reject_unpublished_inputs(&root, Arc::clone(&workspace_lock)).await
        {
            tracing::error!("failed invalidating partially synced memory inputs: {error}");
        }
        job::failed(
            context.as_ref(),
            &db,
            &claim,
            "failed_sync_workspace_inputs",
        )
        .await;
        return MemoryConsolidationStartOutcome::new(
            "failed",
            Some("failed_sync_workspace_inputs"),
        );
    }

    // 6. Use git to decide whether the synced workspace actually changed.
    let workspace_diff = match memory_workspace_diff(&root).await {
        Ok(diff) => diff,
        Err(err) => {
            tracing::error!("failed checking memory workspace changes: {err}");
            if let Err(error) =
                crate::lifecycle::reject_unpublished_inputs(&root, Arc::clone(&workspace_lock))
                    .await
            {
                tracing::error!("failed invalidating unchecked memory inputs: {error}");
            }
            job::failed(context.as_ref(), &db, &claim, "failed_workspace_status").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_workspace_status"));
        }
    };
    let pending_inputs =
        match crate::lifecycle::has_pending_inputs(&root, config.memories.version).await {
            Ok(pending) => pending,
            Err(err) => {
                tracing::error!("failed reading memory publication state: {err}");
                job::failed(context.as_ref(), &db, &claim, "failed_publication_state").await;
                return MemoryConsolidationStartOutcome::new(
                    "failed",
                    Some("failed_publication_state"),
                );
            }
        };
    if !workspace_diff.has_changes()
        && !pending_inputs
        && validate_consolidation_artifacts_for_version(&root, config.memories.version)
            .await
            .is_ok()
    {
        tracing::error!("Phase 2 no changes");
        // We check only after sync of the file system.
        if job::succeed(
            context.as_ref(),
            &db,
            &claim,
            new_watermark,
            &raw_memories,
            "succeeded_no_workspace_changes",
        )
        .await
        {
            drop(phase_two_e2e_timer);
            context.record_storage_size(&root).await;
            return MemoryConsolidationStartOutcome::new("completed", Some("no_workspace_changes"));
        }
        return MemoryConsolidationStartOutcome::new("failed", Some("failed_record_completion"));
    }

    // 7. Persist the diff for the consolidation agent to inspect.
    if let Err(err) = write_workspace_diff(&root, &workspace_diff).await {
        tracing::error!("failed writing memory workspace diff file: {err}");
        if let Err(error) =
            crate::lifecycle::reject_unpublished_inputs(&root, Arc::clone(&workspace_lock)).await
        {
            tracing::error!("failed invalidating unprocessed memory inputs: {error}");
        }
        job::failed(context.as_ref(), &db, &claim, "failed_workspace_diff_file").await;
        return MemoryConsolidationStartOutcome::new("failed", Some("failed_workspace_diff_file"));
    }

    // Readers must stop consuming the preceding artifact before the agent can
    // edit it. A new note cannot inherit an earlier job's completion.
    let mut publication = match ConsolidationPublication::begin(
        &root,
        &workspace_diff.unified_diff,
        config.memories.version,
        Arc::clone(&workspace_lock),
    )
    .await
    {
        Ok(publication) => publication,
        Err(err) => {
            tracing::error!("failed starting memory publication: {err}");
            job::failed(context.as_ref(), &db, &claim, "failed_begin_publication").await;
            return MemoryConsolidationStartOutcome::new(
                "failed",
                Some("failed_begin_publication"),
            );
        }
    };

    let stage =
        match MemoryStage::prepare(&root, config.memories.version, Arc::clone(&workspace_lock))
            .await
        {
            Ok(stage) => Arc::new(stage),
            Err(error) => {
                tracing::error!("failed preparing bounded memory stage: {error}");
                if let Err(error) = publication.fail(&root).await {
                    tracing::error!("failed marking memory publication unavailable: {error}");
                }
                let detail = format!("failed_prepare_stage: {error}");
                job::failed_with_details(
                    context.as_ref(),
                    &db,
                    &claim,
                    "failed_prepare_stage",
                    &detail,
                )
                .await;
                return MemoryConsolidationStartOutcome::new("failed", Some(&detail));
            }
        };
    let Some(agent_config) = agent::bind_stage(agent_config, stage.root()) else {
        if let Err(error) = publication.fail(&root).await {
            tracing::error!("failed marking memory publication unavailable: {error}");
        }
        job::failed(context.as_ref(), &db, &claim, "failed_stage_permissions").await;
        return MemoryConsolidationStartOutcome::new("failed", Some("failed_stage_permissions"));
    };

    if let Err(error) = publication.bind_stage(&root, stage.root()).await {
        tracing::error!("failed recording memory recovery stage: {error}");
        job::failed(context.as_ref(), &db, &claim, "failed_record_stage").await;
        return MemoryConsolidationStartOutcome::new("failed", Some("failed_record_stage"));
    }

    // 8. Spawn the consolidation agent in a selected input copy.
    let mut prompt = agent::get_prompt(stage.root(), config.memories.version);
    prompt.push(UserInput::Text {
        text: format!(
            "{}\nHost staging root: {}\nYou are working in a temporary input copy. Edit only final consolidated artifacts. Keep memory artifact references relative to the memory root; never retain this temporary stage path as memory. Original source and note files are inputs, not outputs. The host may later select bounded relevant excerpts from current final artifacts in this namespace; do not assume the entire summary or raw source files will be supplied to a future session.\n{}",
            crate::prompts::memory_scope_instructions(config.memories.scope),
            stage.root().display(),
            publication.decision_prompt()
        ),
        text_elements: vec![],
    });
    stage.preserve_outputs();
    let agent = match context
        .spawn_consolidation_agent(
            agent_config,
            prompt,
            Arc::clone(&workspace_lock),
            Arc::clone(&stage),
        )
        .await
    {
        Ok(agent) => agent,
        Err(err) => {
            tracing::error!("failed to spawn global memory consolidation agent: {err}");
            if let Err(error) = publication.fail(&root).await {
                tracing::error!("failed marking memory publication unavailable: {error}");
            }
            job::failed(context.as_ref(), &db, &claim, "failed_spawn_agent").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_spawn_agent"));
        }
    };

    // 9. Hand off completion handling, heartbeats, and baseline reset.
    agent::handle(
        Arc::clone(&context),
        claim,
        new_watermark,
        raw_memories.clone(),
        root,
        config.memories.version,
        publication,
        stage,
        agent,
        workspace_lock,
        phase_two_e2e_timer,
    );

    // 10. Emit dispatch metrics.
    let counters = Counters {
        input: raw_memory_count as i64,
    };
    emit_metrics(context.as_ref(), counters);
    MemoryConsolidationStartOutcome::new("accepted", None)
}

async fn sync_phase2_workspace_inputs(
    root: &Path,
    raw_memories: &[Stage1Output],
    config: &Config,
) -> std::io::Result<()> {
    let raw_memory_count = raw_memories.len();
    sync_rollout_summaries_from_memories(root, raw_memories, raw_memory_count).await?;
    match config.memories.version {
        MemoryVersion::V1 => {
            rebuild_raw_memories_file_from_memories(root, raw_memories, raw_memory_count).await?
        }
        MemoryVersion::V2 => {}
    }
    prune_old_extension_resources(root).await;
    Ok(())
}

mod job {
    use super::*;

    pub(super) async fn claim(
        context: &MemoryStartupContext,
        db: &MemoryStore,
        explicit_retry: bool,
    ) -> Result<Claim, &'static str> {
        if !context.generation_enabled().await {
            return Err("skipped_generation_disabled");
        }
        let claim = if explicit_retry {
            db.try_claim_global_phase2_job_for_retry(
                context.thread_id(),
                crate::stage_two::JOB_LEASE_SECONDS,
            )
            .await
        } else {
            db.try_claim_global_phase2_job(context.thread_id(), crate::stage_two::JOB_LEASE_SECONDS)
                .await
        }
        .map_err(|e| {
            tracing::error!("failed to claim job: {e}");
            "failed_claim"
        })?;
        let (token, watermark) = match claim {
            codex_state::Phase2JobClaimOutcome::Claimed {
                ownership_token,
                input_watermark,
            } => {
                context.counter(
                    MEMORY_PHASE_TWO_JOBS,
                    /*inc*/ 1,
                    &[("status", "claimed")],
                );
                (ownership_token, input_watermark)
            }
            codex_state::Phase2JobClaimOutcome::SkippedRetryUnavailable => {
                return Err("skipped_retry_unavailable");
            }
            codex_state::Phase2JobClaimOutcome::SkippedCooldown => {
                return Err("skipped_cooldown");
            }
            codex_state::Phase2JobClaimOutcome::SkippedRunning => return Err("skipped_running"),
        };

        Ok(Claim { token, watermark })
    }

    pub(super) async fn failed(
        context: &MemoryStartupContext,
        db: &MemoryStore,
        claim: &Claim,
        reason: &'static str,
    ) {
        failed_with_details(context, db, claim, reason, reason).await;
    }

    pub(super) async fn failed_with_details(
        context: &MemoryStartupContext,
        db: &MemoryStore,
        claim: &Claim,
        reason: &'static str,
        details: &str,
    ) {
        context.counter(MEMORY_PHASE_TWO_JOBS, /*inc*/ 1, &[("status", reason)]);
        if matches!(
            db.mark_global_phase2_job_failed(
                &claim.token,
                details,
                crate::stage_two::JOB_RETRY_DELAY_SECONDS,
            )
            .await,
            Ok(false)
        ) {
            let _ = db
                .mark_global_phase2_job_failed_if_unowned(
                    &claim.token,
                    details,
                    crate::stage_two::JOB_RETRY_DELAY_SECONDS,
                )
                .await;
        }
    }

    pub(super) async fn succeed(
        context: &MemoryStartupContext,
        db: &MemoryStore,
        claim: &Claim,
        completion_watermark: i64,
        selected_outputs: &[codex_state::Stage1Output],
        reason: &'static str,
    ) -> bool {
        context.counter(MEMORY_PHASE_TWO_JOBS, /*inc*/ 1, &[("status", reason)]);
        db.mark_global_phase2_job_succeeded(&claim.token, completion_watermark, selected_outputs)
            .await
            .unwrap_or(false)
    }
}

mod agent {
    use super::*;
    use tracing::warn;

    pub(super) fn get_config(
        config: &Config,
        parent_permission_profile: PermissionProfile,
        provider: &dyn ModelProvider,
    ) -> Option<Config> {
        let root = config.memories.memory_root(&config.codex_home)?;
        let mut agent_config = config.clone();

        agent_config.cwd = root.clone();
        // Consolidation threads must never feed back into phase-1 memory generation.
        agent_config.ephemeral = true;
        agent_config.memories.generate_memories = false;
        agent_config.memories.use_memories = false;
        // Background memory work must not send user-facing completion notifications.
        agent_config.notify = None;
        agent_config.include_apps_instructions = false;
        agent_config.mcp_servers = Constrained::allow_only(HashMap::new());
        // Approval policy
        agent_config.permissions.approval_policy = Constrained::allow_only(AskForApproval::Never);
        // Consolidation runs as an internal worker and must not recursively delegate.
        let _ = agent_config.features.disable(Feature::Collab);
        let _ = agent_config.features.disable(Feature::MemoryTool);
        let _ = agent_config.features.disable(Feature::Apps);
        let _ = agent_config.features.disable(Feature::Plugins);
        let _ = agent_config
            .features
            .disable(Feature::SkillMcpDependencyInstall);

        // Preserve the parent's explicit choice to skip Codex-managed sandboxing.
        match parent_permission_profile {
            PermissionProfile::Disabled => agent_config
                .permissions
                .set_permission_profile(PermissionProfile::Disabled),
            PermissionProfile::External { network } => agent_config
                .permissions
                .set_permission_profile(PermissionProfile::External { network }),
            PermissionProfile::Managed { .. } => {
                // The consolidation agent only needs local memory-root write access and no network.
                agent_config.set_legacy_sandbox_policy(SandboxPolicy::WorkspaceWrite {
                    writable_roots: vec![root],
                    network_access: false,
                    exclude_tmpdir_env_var: true,
                    exclude_slash_tmp: true,
                })
            }
        }
        .ok()?;

        agent_config.model = Some(
            config
                .memories
                .consolidation_model
                .clone()
                .unwrap_or_else(|| provider.memory_consolidation_preferred_model().to_string()),
        );
        agent_config.model_reasoning_effort = Some(crate::stage_two::REASONING_EFFORT);
        agent_config.service_tier = Some(config.memories.consolidation_service_tier.clone());

        Some(agent_config)
    }

    pub(super) fn get_prompt(root: &Path, version: MemoryVersion) -> Vec<UserInput> {
        let prompt = build_consolidation_prompt_for_version(root, version);
        vec![UserInput::Text {
            text: prompt,
            text_elements: vec![],
        }]
    }

    /// Keep parent enforcement while directing ordinary worker writes to its
    /// own copy. Disabled/External remains outside Codex-managed OS isolation.
    pub(super) fn bind_stage(
        mut config: Config,
        root: &codex_utils_absolute_path::AbsolutePathBuf,
    ) -> Option<Config> {
        config.cwd = root.clone();
        if matches!(
            config.permissions.effective_permission_profile(),
            PermissionProfile::Managed { .. }
        ) {
            config
                .set_legacy_sandbox_policy(SandboxPolicy::WorkspaceWrite {
                    writable_roots: vec![root.clone()],
                    network_access: false,
                    exclude_tmpdir_env_var: true,
                    exclude_slash_tmp: true,
                })
                .ok()?;
        }
        Some(config)
    }

    /// Handle the agent while it is running.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle(
        context: Arc<MemoryStartupContext>,
        claim: Claim,
        new_watermark: i64,
        selected_outputs: Vec<codex_state::Stage1Output>,
        memory_root: codex_utils_absolute_path::AbsolutePathBuf,
        version: MemoryVersion,
        publication: ConsolidationPublication,
        stage: Arc<MemoryStage>,
        agent: SpawnedConsolidationAgent,
        workspace_lock: Arc<MemoryWorkspaceLock>,
        phase_two_e2e_timer: Option<codex_otel::Timer>,
    ) {
        tokio::spawn(async move {
            let _workspace_lock = workspace_lock;
            let Some(db) = context.memory_store().await else {
                return;
            };
            let SpawnedConsolidationAgent { thread_id, thread } = agent;

            // Loop the agent until we have the final status.
            let final_status =
                loop_agent(db.clone(), claim.token.clone(), thread_id, &thread).await;

            let agent_completed = matches!(final_status, AgentStatus::Completed(_));
            if agent_completed
                && let Some(token_usage) = thread
                    .token_usage_info()
                    .await
                    .map(|info| info.total_token_usage)
            {
                emit_token_usage_metrics(context.as_ref(), &token_usage);
            }

            if let Err(err) = context
                .shutdown_consolidation_agent(SpawnedConsolidationAgent { thread_id, thread })
                .await
            {
                warn!("failed to auto-close global memory consolidation agent {thread_id}: {err}");
                if let Err(error) = publication.fail(&memory_root).await {
                    tracing::error!("failed marking memory publication unavailable: {error}");
                }
                // Keep the existing lease until it expires so another worker cannot race a
                // consolidation agent whose shutdown has not completed.
                return;
            }

            let artifacts_valid = if agent_completed {
                let validation = async {
                    validate_consolidation_artifacts_for_version(stage.root(), version).await?;
                    publication
                        .validate_decision(&memory_root, stage.root())
                        .await?;
                    Ok::<(), anyhow::Error>(())
                }
                .await;
                match validation {
                    Ok(()) => true,
                    Err(err) => {
                        tracing::error!("memory consolidation artifacts are invalid: {err}");
                        if let Err(error) = publication.fail(&memory_root).await {
                            tracing::error!(
                                "failed marking memory publication unavailable: {error}"
                            );
                        }
                        job::failed(context.as_ref(), &db, &claim, "failed_invalid_artifacts")
                            .await;
                        false
                    }
                }
            } else {
                false
            };

            if agent_completed && artifacts_valid {
                // Do not reset the workspace baseline if we lost the lock.
                let still_owns_lock = match db
                    .heartbeat_global_phase2_job(
                        &claim.token,
                        crate::stage_two::JOB_LEASE_SECONDS,
                    )
                    .await
                    .inspect_err(|err| {
                        tracing::error!(
                            "failed confirming global memory consolidation ownership before resetting workspace baseline: {err}"
                        );
                    }) {
                    Ok(true) => true,
                    Ok(false) => {
                        tracing::error!(
                            "lost global memory consolidation ownership before resetting workspace baseline"
                        );
                        if let Err(error) = publication.fail(&memory_root).await {
                            tracing::error!("failed marking memory publication unavailable: {error}");
                        }
                        false
                    }
                    Err(_) => {
                        job::failed(context.as_ref(), &db, &claim, "failed_confirm_ownership")
                            .await;
                        if let Err(error) = publication.fail(&memory_root).await {
                            tracing::error!("failed marking memory publication unavailable: {error}");
                        }
                        false
                    }
                };
                if still_owns_lock {
                    if let Err(err) = stage.publish_outputs(&memory_root, version).await {
                        tracing::error!("failed copying final memory artifacts from stage: {err}");
                        job::failed(context.as_ref(), &db, &claim, "failed_publish_stage").await;
                        if let Err(error) = publication.fail(&memory_root).await {
                            tracing::error!(
                                "failed marking memory publication unavailable: {error}"
                            );
                        }
                    } else if let Err(err) = reset_memory_workspace_baseline(&memory_root).await {
                        tracing::error!("failed resetting memory workspace baseline: {err}");
                        job::failed(context.as_ref(), &db, &claim, "failed_workspace_commit").await;
                        if let Err(error) = publication.fail(&memory_root).await {
                            tracing::error!(
                                "failed marking memory publication unavailable: {error}"
                            );
                        }
                    } else if !job::succeed(
                        context.as_ref(),
                        &db,
                        &claim,
                        new_watermark,
                        &selected_outputs,
                        "succeeded",
                    )
                    .await
                    {
                        tracing::error!(
                            "failed marking global memory consolidation job succeeded after resetting workspace baseline"
                        );
                        if let Err(error) = publication.fail(&memory_root).await {
                            tracing::error!(
                                "failed marking memory publication unavailable: {error}"
                            );
                        }
                    } else {
                        if let Err(err) = publication.publish(&memory_root, version).await {
                            // The DB job is done, but this generation is not
                            // available to readers. Do not turn it into ready.
                            tracing::error!("failed publishing completed memory artifacts: {err}");
                            if let Err(error) = publication.fail(&memory_root).await {
                                tracing::error!(
                                    "failed marking memory publication unavailable: {error}"
                                );
                            }
                        } else {
                            stage.allow_cleanup();
                            drop(phase_two_e2e_timer);
                            context.record_storage_size(&memory_root).await;
                        }
                    }
                }
            } else if !agent_completed {
                if let Err(err) = remove_memory_symlinks(stage.root()).await {
                    tracing::error!("failed removing memory workspace symbolic links: {err}");
                }
                job::failed(context.as_ref(), &db, &claim, "failed_agent").await;
                if let Err(error) = publication.fail(&memory_root).await {
                    tracing::error!("failed marking memory publication unavailable: {error}");
                }
            }
        });
    }

    async fn loop_agent(
        db: MemoryStore,
        token: String,
        thread_id: ThreadId,
        thread: &codex_core::CodexThread,
    ) -> AgentStatus {
        let mut heartbeat_interval =
            tokio::time::interval(Duration::from_secs(crate::stage_two::JOB_HEARTBEAT_SECONDS));
        heartbeat_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut status_poll_interval = tokio::time::interval(Duration::from_secs(1));
        status_poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let session_termination = thread.wait_until_terminated();
        tokio::pin!(session_termination);

        loop {
            let status = thread.agent_status().await;
            if is_final_agent_status(&status) {
                break status;
            }

            tokio::select! {
                _ = &mut session_termination => {
                    let status = thread.agent_status().await;
                    if is_final_agent_status(&status) {
                        break status;
                    }
                    tracing::warn!(
                        "memory consolidation agent {thread_id} exited before final status; last status was {status:?}"
                    );
                    break AgentStatus::Errored(format!(
                        "memory consolidation agent exited before final status: {status:?}"
                    ));
                }
                _ = status_poll_interval.tick() => {
                }
                _ = heartbeat_interval.tick() => {
                    match db
                        .heartbeat_global_phase2_job(
                            &token,
                            crate::stage_two::JOB_LEASE_SECONDS,
                        )
                        .await
                    {
                        Ok(true) => {}
                        Ok(false) => {
                            tracing::warn!(
                                "lost global phase-2 ownership during heartbeat for memory consolidation agent {thread_id}"
                            );
                            break AgentStatus::Errored(
                                "lost global phase-2 ownership during heartbeat".to_string(),
                            );
                        }
                        Err(err) => {
                            tracing::warn!(
                                "phase-2 heartbeat update failed for memory consolidation agent {thread_id}: {err}"
                            );
                            break AgentStatus::Errored(format!(
                                "phase-2 heartbeat update failed: {err}"
                            ));
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "phase2_sandbox_tests.rs"]
mod sandbox_tests;
#[cfg(test)]
#[path = "phase2_workspace_roots_tests.rs"]
mod workspace_roots_tests;

pub(super) fn get_watermark(
    claimed_watermark: i64,
    latest_memories: &[codex_state::Stage1Output],
) -> i64 {
    latest_memories
        .iter()
        .map(|memory| memory.source_updated_at.timestamp())
        .max()
        .unwrap_or(claimed_watermark)
        .max(claimed_watermark)
}

fn is_final_agent_status(status: &AgentStatus) -> bool {
    !matches!(
        status,
        AgentStatus::PendingInit | AgentStatus::Running | AgentStatus::Interrupted
    )
}

fn emit_metrics(context: &MemoryStartupContext, counters: Counters) {
    if counters.input > 0 {
        context.counter(MEMORY_PHASE_TWO_INPUT, counters.input, &[]);
    }

    context.counter(
        MEMORY_PHASE_TWO_JOBS,
        /*inc*/ 1,
        &[("status", "agent_spawned")],
    );
}

fn emit_token_usage_metrics(context: &MemoryStartupContext, token_usage: &TokenUsage) {
    context.histogram(
        MEMORY_PHASE_TWO_TOKEN_USAGE,
        token_usage.total_tokens.max(0),
        &[("token_type", "total")],
    );
    context.histogram(
        MEMORY_PHASE_TWO_TOKEN_USAGE,
        token_usage.input_tokens.max(0),
        &[("token_type", "input")],
    );
    context.histogram(
        MEMORY_PHASE_TWO_TOKEN_USAGE,
        token_usage.cached_input(),
        &[("token_type", "cached_input")],
    );
    context.histogram(
        MEMORY_PHASE_TWO_TOKEN_USAGE,
        token_usage.cache_write_input_tokens.max(0),
        &[("token_type", "cache_write_input")],
    );
    context.histogram(
        MEMORY_PHASE_TWO_TOKEN_USAGE,
        token_usage.output_tokens.max(0),
        &[("token_type", "output")],
    );
    context.histogram(
        MEMORY_PHASE_TWO_TOKEN_USAGE,
        token_usage.reasoning_output_tokens.max(0),
        &[("token_type", "reasoning_output")],
    );
}
