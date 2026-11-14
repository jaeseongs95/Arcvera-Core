// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md.
use crate::MemoryWorkspaceLock;
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

    // 1. Claim the global Phase 2 lock before touching the memory workspace.
    let claim = match job::claim(context.as_ref(), &db, explicit_retry).await {
        Ok(claim) => claim,
        Err(e) => {
            context.counter(MEMORY_PHASE_TWO_JOBS, /*inc*/ 1, &[("status", e)]);
            return MemoryConsolidationStartOutcome::new("blocked", Some(e));
        }
    };

    // Capture the selected inputs and bind this claim to the exact namespace.
    let raw_memories = match db.get_phase2_input_selection(max_raw_memories, max_unused_days).await {
        Ok(inputs) => Arc::new(inputs),
        Err(error) => {
            tracing::error!(%error, "failed selecting memory inputs");
            job::failed(context.as_ref(), &db, &claim, "failed_load_stage1_outputs").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_load_stage1_outputs"));
        }
    };
    let raw_memory_count = raw_memories.len();
    let new_watermark = get_watermark(claim.watermark, &raw_memories);
    let identity = match codex_memories_read::publication_snapshot::root_identity(&root) {
        Ok(identity) => identity,
        Err(error) => {
            tracing::error!(%error, "invalid memory publication root");
            job::failed(context.as_ref(), &db, &claim, "failed_publication_root").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_publication_root"));
        }
    };
    match db.bind_phase2_publication_root(&claim.token, &identity).await {
        Ok(true) => {},
        Ok(false) => {
            job::failed(context.as_ref(), &db, &claim, "failed_publication_owner").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_publication_owner"));
        }
        Err(error) => {
            tracing::error!(%error, "failed binding memory publication owner");
            job::failed(context.as_ref(), &db, &claim, "failed_publication_owner").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_publication_owner"));
        }
    }
    let Some(agent_config) = agent::get_config(config.as_ref(), parent_permission_profile.clone(), context.provider()) else {
        job::failed(context.as_ref(), &db, &claim, "failed_sandbox_policy").await;
        return MemoryConsolidationStartOutcome::new("blocked", Some("failed_sandbox_policy"));
    };
    let prepared = match crate::publication_lifecycle::prepare(
        &root, config.memories.version, &db, Arc::clone(&raw_memories), config.as_ref(), Arc::clone(&workspace_lock)
    ).await {
        Ok(prepared) => prepared,
        Err(error) => {
            tracing::error!(%error, "failed preparing isolated memory publication");
            let detail = format!("failed_prepare_publication: {error:#}");
            job::failed_with_details(context.as_ref(), &db, &claim, "failed_prepare_publication", &detail).await;
            let status = if error.downcast_ref::<std::io::Error>().is_some_and(|cause| cause.kind() == std::io::ErrorKind::WouldBlock) { "blocked" } else { "failed" };
            return MemoryConsolidationStartOutcome::new(status, Some(&detail));
        }
    };
    if prepared.unchanged {
        return match crate::publication_lifecycle::confirm_unchanged(
            context.as_ref(), &root, config.memories.version, &db, &claim.token,
            claim.watermark, new_watermark, &raw_memories, prepared, Arc::clone(&workspace_lock),
        ).await {
            Ok(true) => MemoryConsolidationStartOutcome::new("completed", Some("no_workspace_changes")),
            Ok(false) => {
                job::failed(context.as_ref(), &db, &claim, "failed_publication_fence").await;
                MemoryConsolidationStartOutcome::new("failed", Some("failed_publication_fence"))
            }
            Err(error) => {
                tracing::error!(%error, "failed confirming unchanged memory publication");
                job::failed(context.as_ref(), &db, &claim, "failed_publication_fence").await;
                MemoryConsolidationStartOutcome::new("failed", Some("failed_publication_fence"))
            }
        };
    }
    let stage = match codex_utils_absolute_path::AbsolutePathBuf::from_absolute_path(&prepared.stage) {
        Ok(stage) => stage,
        Err(error) => {
            tracing::error!(%error, "invalid memory stage path");
            job::failed(context.as_ref(), &db, &claim, "failed_stage_path").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_stage_path"));
        }
    };
    let (agent_config, host_mediated) = match crate::publication_permissions::bind_child_stage(
        agent_config.clone(), &parent_permission_profile, &stage
    ) {
        Ok(config) => (config, false),
        Err(sandbox_error) => match crate::publication_permissions::bind_host_mediated_child(agent_config, &parent_permission_profile, &stage) {
            Ok(config) => (config, true),
            Err(error) => {
                tracing::warn!(%sandbox_error, %error, "memory consolidation isolation unavailable");
                job::failed(context.as_ref(), &db, &claim, "unsupported_stage_permissions").await;
                return MemoryConsolidationStartOutcome::new("blocked", Some("unsupported_stage_permissions"));
            }
        },
    };
    match db.heartbeat_global_phase2_job(&claim.token, crate::stage_two::JOB_LEASE_SECONDS).await {
        Ok(true) => {},
        Ok(false) => {
            job::failed(context.as_ref(), &db, &claim, "failed_publication_owner").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_publication_owner"));
        }
        Err(error) => {
            tracing::error!(%error, "failed renewing memory publication lease");
            job::failed(context.as_ref(), &db, &claim, "failed_publication_owner").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_publication_owner"));
        }
    }
    // 8. Spawn the consolidation agent.
    let mut prompt = agent::get_prompt(stage.as_path(), config.memories.version, host_mediated);
    prompt.push(UserInput::Text {
        text: "Work only in the provided staging workspace. Original notes and selected input files are immutable. Do not put staging paths into generated memory. The host verifies these inputs and publishes outputs only after successful completion.".to_string(),
        text_elements: vec![],
    });
    let stage_tools = host_mediated.then(|| crate::publication_tools::StageTools::new(
        prepared.stage.clone(), config.memories.version, Arc::clone(&prepared.gate), Arc::clone(&workspace_lock), Arc::clone(&prepared.tool_failed)
    ));
    if host_mediated {
        prompt.push(UserInput::Text {
            text: "Use only the memory_stage tool to list and read relative paths in the provided stage, and to write or delete generated output paths. Shell, network, external tools, and direct filesystem access are unavailable. Read phase2_workspace_diff.md first; preserve the selected source inputs and notes. A successful tool call applies only to the stage, not to the published memory.".to_string(),
            text_elements: vec![],
        });
    }
    let agent = match context
        .spawn_consolidation_agent(agent_config, prompt, Arc::clone(&workspace_lock), stage_tools)
        .await
    {
        Ok(agent) => agent,
        Err(err) => {
            tracing::error!("failed to spawn global memory consolidation agent: {err}");
            job::failed(context.as_ref(), &db, &claim, "failed_spawn_agent").await;
            return MemoryConsolidationStartOutcome::new("failed", Some("failed_spawn_agent"));
        }
    };

    // 9. Hand off completion handling, heartbeats, and baseline reset.
    agent::handle(
        Arc::clone(&context),
        claim,
        new_watermark,
        Arc::clone(&raw_memories),
        root,
        config.memories.version,
        prepared,
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

pub(crate) async fn sync_phase2_workspace_inputs(
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
        for feature in [Feature::Collab, Feature::MemoryTool, Feature::Apps, Feature::Plugins, Feature::CodexHooks, Feature::SkillMcpDependencyInstall] {
            agent_config.features.disable(feature).ok()?;
        }

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

    pub(super) fn get_prompt(root: &Path, version: MemoryVersion, host_mediated: bool) -> Vec<UserInput> {
        let prompt = if host_mediated { crate::prompts::build_stage_tool_prompt(root, version) } else { build_consolidation_prompt_for_version(root, version) };
        vec![UserInput::Text {
            text: prompt,
            text_elements: vec![],
        }]
    }

    /// Handle the agent while it is running.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn handle(
        context: Arc<MemoryStartupContext>,
        claim: Claim,
        new_watermark: i64,
        selected_outputs: Arc<Vec<codex_state::Stage1Output>>,
        memory_root: codex_utils_absolute_path::AbsolutePathBuf,
        version: MemoryVersion,
        prepared: crate::publication_lifecycle::PreparedPublication,
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
                // Keep the existing lease until it expires so another worker cannot race a
                // consolidation agent whose shutdown has not completed.
                return;
            }

            if !agent_completed {
                job::failed(context.as_ref(), &db, &claim, "failed_agent").await;
                return;
            }
            match crate::publication_lifecycle::finish(
                context.as_ref(), &memory_root, version, &db, &claim.token, claim.watermark,
                new_watermark, &selected_outputs, prepared, Arc::clone(&_workspace_lock),
            ).await {
                Ok(true) => {
                    drop(phase_two_e2e_timer);
                    context.record_storage_size(&memory_root).await;
                }
                Ok(false) => {
                    tracing::warn!("memory publication lost its lease or input binding");
                    job::failed(context.as_ref(), &db, &claim, "failed_publication_fence").await;
                }
                Err(error) => {
                    tracing::error!(%error, "memory publication validation or commit failed");
                    let detail = format!("failed_publication: {error:#}");
                    job::failed_with_details(context.as_ref(), &db, &claim, "failed_publication", &detail).await;
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
