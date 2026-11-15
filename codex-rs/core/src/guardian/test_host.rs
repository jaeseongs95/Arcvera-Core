// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Explicit ThreadManager host for core's context-adapter unit tests.
//! Application-level tests install the real Guardian extension instead.

use std::sync::Arc;

use codex_extension_api::SessionIsolation;
use codex_home::CodexHomeUserInstructionsProvider;
use codex_protocol::protocol::InternalSessionSource;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::ThreadSource;

use super::GuardianReviewSessionManager;
use crate::config::Config;
use crate::config::Constrained;
use crate::config::TokenBudgetConfig;
use crate::session::session::Session;

// Compile the extension's actual setup with this test crate's Config type, rather
// than keeping a second settings implementation in the context-adapter test host.
#[path = "../../../ext/guardian-v2/src/sync_reviewer/reviewer_config.rs"]
mod reviewer_config;
pub(super) use reviewer_config::build_reviewer_config;

/// Connect the actual paginated store for tests that start a reviewer thread.
pub(crate) async fn initialize_storage(session: &mut Session, config: &Config) {
    assert!(
        session.services.live_thread.is_none(),
        "initialize before materializing the parent"
    );
    assert!(
        config.codex_home.is_dir(),
        "the owning fixture must retain its home"
    );
    assert!(
        session.services.state_db.is_none(),
        "do not replace an existing state DB"
    );
    let state_db =
        codex_state::StateRuntime::init(config.sqlite.clone(), config.model_provider_id.clone())
            .await
            .expect("initialize Guardian fixture SQLite");
    session.services.state_db = Some(Arc::clone(&state_db));
    session.services.thread_store = Arc::new(codex_thread_store::LocalThreadStore::new(
        codex_thread_store::LocalThreadStoreConfig::from_config(config),
        Some(state_db),
    ));
    install(session, config);
}

pub(crate) fn install(session: &Session, config: &Config) {
    session.services.thread_extension_data.insert(
        codex_guardian_reviewer::ReviewerConfig::<Config>(build_reviewer_config),
    );
    let manager = Arc::new(crate::ThreadManager::new(
        config,
        Arc::clone(&session.services.auth_manager),
        Arc::clone(&session.services.models_manager),
        crate::CodexAppsToolsCache::default(),
        SessionSource::Exec,
        session.services.turn_environments.environment_manager(),
        codex_extension_api::empty_extension_registry(),
        Arc::new(CodexHomeUserInstructionsProvider::new(
            config.codex_home.clone(),
        )),
        /*analytics_events_client*/ None,
        crate::passthrough_image_store(),
        Arc::clone(&session.services.thread_store),
        /*agent_graph_store*/ None,
        session.installation_id.clone(),
        /*attestation_provider*/ None,
        /*external_time_provider*/ None,
    ));
    let runtime = session
        .services
        .thread_extension_data
        .get_or_init(codex_guardian_reviewer::ReviewerTasks::default);
    session
        .services
        .thread_extension_data
        .insert(GuardianReviewSessionManager::new(
            Arc::clone(&runtime),
            move |context, key, kind, snapshot, cancel| {
                let manager = Arc::clone(&manager);
                let runtime = Arc::clone(&runtime);
                Box::pin(async move {
                    let history_reset = context.history_reset.clone();
                    let (mut options, state) = context.thread_options(kind, snapshot).await;
                    if matches!(
                        kind,
                        codex_analytics::GuardianReviewSessionKind::EphemeralForked
                    ) {
                        options.config.ephemeral = true;
                    }
                    options.session_source =
                        Some(SessionSource::Internal(InternalSessionSource::Guardian));
                    options.thread_source = Some(ThreadSource::GuardianReview);
                    options
                        .thread_extension_init
                        .insert(SessionIsolation::Isolated);
                    options
                        .thread_extension_init
                        .insert(codex_guardian_reviewer::reviewer_tool_policy());
                    let session_cancel = cancel.clone();
                    let until = async move {
                        let _cancel_on_exit = cancel.clone().drop_guard();
                        tokio::select! {
                            _ = cancel.cancelled() => {}
                            _ = history_reset.cancelled() => {}
                        }
                    };
                    let spawned = manager
                        .start_thread_until(options, until, &runtime.tasks)
                        .await?;
                    Ok(context
                        .bind_thread(&spawned.thread, key, state, session_cancel)
                        .await)
                })
            },
        ));
}
