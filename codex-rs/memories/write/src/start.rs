// Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md.
use crate::ensure_layout;
use crate::extensions::seed_extension_instructions;
use crate::guard;
use crate::metrics::MEMORY_STARTUP;
use crate::phase1;
use crate::phase2;
use crate::runtime::MemoryStartupContext;
use codex_core::CodexThread;
use codex_core::ThreadManager;
use codex_core::config::Config;
use codex_features::Feature;
use codex_login::AuthManager;
use codex_protocol::MemoryVersion;
use codex_protocol::ThreadId;
use codex_protocol::models::PermissionProfile;
use codex_protocol::protocol::SessionSource;
use std::sync::Arc;
use tracing::warn;

/// Observe the loaded thread's selected job; this is not a per-note application receipt.
pub async fn memory_consolidation_status(
    thread: &CodexThread,
    config: &Config,
) -> anyhow::Result<Option<codex_state::MemoryConsolidationStatus>> {
    let snapshot = thread.config_snapshot().await;
    anyhow::ensure!(
        config.memory_environment_supported(snapshot.environment_selections()),
        "memory consolidation status is unavailable for this environment"
    );
    let root = config
        .memories
        .memory_root(&config.codex_home)
        .ok_or_else(|| anyhow::anyhow!("project memory identity is unavailable"))?;
    codex_config::memory_scope::validate_scoped_memory_root(root.as_path())?;
    let db = thread
        .state_db()
        .ok_or_else(|| anyhow::anyhow!("memory state database is unavailable"))?;
    let store = match config.memories.scope {
        codex_config::types::MemoryScope::Project => {
            let project = config
                .memories
                .project
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("project memory identity is unavailable"))?;
            db.memories_for_project(config.memories.version, &project.key)
                .await?
        }
        codex_config::types::MemoryScope::Global => {
            db.memories_for_version(config.memories.version).await?
        }
    };
    store.consolidation_status().await
}

/// Admit an explicitly requested consolidation using only the selected phase-2 workspace.
/// This does not extract other conversations or enable memory generation.
#[allow(clippy::too_many_arguments)]
pub async fn consolidate_memories(
    thread_manager: Arc<ThreadManager>,
    auth_manager: Arc<AuthManager>,
    thread_id: ThreadId,
    thread: Arc<CodexThread>,
    config: Arc<Config>,
    parent_permission_profile: PermissionProfile,
    source: &SessionSource,
    retry: bool,
) -> anyhow::Result<crate::MemoryConsolidationStartOutcome> {
    use crate::MemoryConsolidationStartOutcome as Outcome;
    if config.ephemeral || source.is_non_root_agent() {
        return Ok(Outcome::new("blocked", Some("ineligible_session")));
    }
    if !config.features.enabled(Feature::MemoryTool) {
        return Ok(Outcome::new("blocked", Some("memory_disabled")));
    }
    let Some(root) = config.memories.memory_root(&config.codex_home) else {
        return Ok(Outcome::new("blocked", Some("root_unavailable")));
    };
    let snapshot = thread.config_snapshot().await;
    if !config.memory_environment_supported(snapshot.environment_selections()) {
        return Ok(Outcome::new("blocked", Some("unsupported_environment")));
    }
    codex_config::memory_scope::validate_scoped_memory_root(root.as_path())?;
    let context = Arc::new(MemoryStartupContext::new(
        thread_manager,
        Arc::clone(&auth_manager),
        thread_id,
        thread,
        config.as_ref(),
        source.clone(),
    ));
    if !context.generation_enabled().await {
        return Ok(Outcome::new("blocked", Some("generation_disabled")));
    }
    if context.memory_store().await.is_none() {
        return Ok(Outcome::new("blocked", Some("state_unavailable")));
    }
    if !guard::rate_limits_ok(&auth_manager, &config).await {
        return Ok(Outcome::new("blocked", Some("rate_limit")));
    }
    // The lock is a sibling of the root, including on the first scoped run.
    if let Some(parent) = root.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    codex_config::memory_scope::validate_scoped_memory_root(root.as_path())?;
    Ok(phase2::run_with_retry(context, config, parent_permission_profile, retry).await)
}

/// Starts the asynchronous startup memory pipeline for an eligible root session.
///
/// The pipeline is skipped for ephemeral sessions, disabled feature flags, and
/// subagent sessions.
pub fn start_memories_startup_task(
    thread_manager: Arc<ThreadManager>,
    auth_manager: Arc<AuthManager>,
    thread_id: ThreadId,
    thread: Arc<CodexThread>,
    config: Arc<Config>,
    parent_permission_profile: PermissionProfile,
    source: &SessionSource,
) {
    if config.ephemeral
        || !config.features.enabled(Feature::MemoryTool)
        || config.memories.memory_root(&config.codex_home).is_none()
        || source.is_non_root_agent()
    {
        return;
    }

    // Dual-write warms v2 while v1 is selected; it is no longer needed after cutover.
    let versions = if config.memories.dual_write && config.memories.version == MemoryVersion::V1 {
        vec![MemoryVersion::V1, MemoryVersion::V2]
    } else {
        vec![config.memories.version]
    };
    for version in versions {
        let mut pipeline_config = config.as_ref().clone();
        pipeline_config.memories.version = version;
        let config = Arc::new(pipeline_config);
        let auth_manager = Arc::clone(&auth_manager);
        let parent_permission_profile = parent_permission_profile.clone();
        let context = Arc::new(MemoryStartupContext::new(
            Arc::clone(&thread_manager),
            Arc::clone(&auth_manager),
            thread_id,
            Arc::clone(&thread),
            config.as_ref(),
            source.clone(),
        ));
        let thread = Arc::clone(&thread);
        tokio::spawn(async move {
            let snapshot = thread.config_snapshot().await;
            if !config.memory_environment_supported(snapshot.environment_selections()) {
                return;
            }
            if !context.generation_enabled().await {
                return;
            }
            if context.memory_store().await.is_none() {
                warn!("state db unavailable for memories startup pipeline; skipping");
                return;
            }
            let Some(root) = config.memories.memory_root(&config.codex_home) else {
                return;
            };
            if let Err(err) =
                codex_config::memory_scope::validate_scoped_memory_root(root.as_path())
            {
                warn!("project memory root is unavailable: {err}");
                return;
            }
            if let Err(err) = ensure_layout(&root).await {
                warn!("failed preparing memories root: {err}");
                return;
            }
            if let Err(err) = seed_extension_instructions(&root).await {
                warn!("failed seeding memory extension instructions: {err}");
            }

            // Clean memories to make preserve DB size. This does not consume tokens so can be
            // done before the quota check.
            phase1::prune(context.as_ref(), &config).await;

            if !guard::rate_limits_ok(&auth_manager, &config).await {
                context.counter(
                    MEMORY_STARTUP,
                    /*inc*/ 1,
                    &[("status", "skipped_rate_limit")],
                );
                return;
            }

            // Run phase 1.
            phase1::run(Arc::clone(&context), Arc::clone(&config)).await;
            // Run phase 2.
            phase2::run(context, config, parent_permission_profile).await;
        });
    }
}
