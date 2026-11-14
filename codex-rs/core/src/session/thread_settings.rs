// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R32 (2026-11-01); see MODIFICATIONS.md.
//! Handles persistent thread-settings updates and serializes their persistence
//! with checkpoints written directly to storage.

use super::session::Session;
use super::session::SessionSettingsUpdate;
use super::step_settings::StepSettingsUpdate;
use crate::WithTurnExtensionData;
use crate::config::ConstraintResult;
use codex_history::RolloutItem;
use codex_protocol::capabilities::SelectedCapabilityRoot;
use codex_protocol::error::CodexErr;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::ThreadSettingsAppliedEvent;
use codex_protocol::protocol::ThreadSettingsOverrides;
use codex_protocol::protocol::ThreadSettingsSnapshot;
use codex_thread_store::PersistContext;
use codex_thread_store::ThreadStoreError;
use codex_thread_store::ThreadStoreResult;
use std::sync::Arc;
use tokio::sync::SemaphorePermit;

impl Session {
    /// Runtime only: no store writes. Admission and publication share the active-turn lock.
    pub(crate) async fn supplement_runtime_dynamic_tools(
        &self,
        owner: u64,
        disconnected: tokio_util::sync::CancellationToken,
        tools: Vec<codex_protocol::dynamic_tools::DynamicToolSpec>,
        previous: &[codex_protocol::dynamic_tools::DynamicToolSpec],
    ) -> CodexResult<bool> {
        self.supplement_runtime_tools(owner, disconnected, Some(tools), previous, None)
            .await
    }

    /// Atomically admit native and MCP overlays; neither changes if validation fails.
    pub(crate) async fn supplement_runtime_tools(
        &self,
        owner: u64,
        disconnected: tokio_util::sync::CancellationToken,
        tools: Option<Vec<codex_protocol::dynamic_tools::DynamicToolSpec>>,
        previous: &[codex_protocol::dynamic_tools::DynamicToolSpec],
        servers: Option<std::collections::HashMap<String, codex_config::McpServerConfig>>,
    ) -> CodexResult<bool> {
        self.register_runtime_tools(owner, disconnected, tools, previous, servers, None)
            .await
            .map(|(applied, _, _, _)| applied)
    }

    /// A retry with an identity can only apply the exact still-current queued request.
    pub(crate) async fn register_runtime_tools(
        &self,
        owner: u64,
        disconnected: tokio_util::sync::CancellationToken,
        tools: Option<Vec<codex_protocol::dynamic_tools::DynamicToolSpec>>,
        previous: &[codex_protocol::dynamic_tools::DynamicToolSpec],
        servers: Option<std::collections::HashMap<String, codex_config::McpServerConfig>>,
        generation: Option<u64>,
    ) -> CodexResult<(
        bool,
        Option<u64>,
        Option<tokio_util::sync::CancellationToken>,
        Option<String>,
    )> {
        // No persistence is touched here. Holding its permit while waiting for an
        // active turn can deadlock a user reply behind a delivered-message record.
        let active = self.active_turn.lock().await;
        let mut state = self.state.lock().await;
        if state.shutting_down || self.cleanup_admission.is_closing() || disconnected.is_cancelled()
        {
            return Err(CodexErr::InvalidRequest(
                "도구를 제공한 연결이 종료됐습니다.".into(),
            ));
        }
        if let Some(generation) = generation {
            let pending = state
                .pending_runtime_tools
                .as_ref()
                .ok_or_else(|| CodexErr::InvalidRequest("대기 중인 MCP 등록이 없습니다.".into()))?;
            if pending.generation != generation
                || pending.owner != owner
                || pending.disconnected.is_cancelled()
                || pending.tools != tools
                || pending.previous != previous
                || Some(&pending.servers) != servers.as_ref()
            {
                return Err(CodexErr::InvalidRequest(
                    "대기 중인 MCP 등록이 변경되었거나 종료됐습니다.".into(),
                ));
            }
        }
        if tools.is_some()
            && state.runtime_dynamic_tools.as_ref().is_some_and(|overlay| {
                !overlay.disconnected.is_cancelled() && overlay.owner != owner
            })
        {
            return Err(CodexErr::InvalidRequest(
                "다른 연결이 이 대화의 도구 보강을 소유하고 있습니다.".into(),
            ));
        }
        let merged = tools
            .as_ref()
            .map(|tools| {
                merge_runtime_tools(&state.session_configuration.dynamic_tools, tools, previous)
                    .map_err(CodexErr::InvalidRequest)
            })
            .transpose()?;
        let pending_servers = servers.clone();
        let overlay = if let Some(servers) = servers {
            let store = &self.services.thread_extension_data;
            if let Some(old) = store.get::<crate::mcp::RuntimeMcpServers>()
                && old.owner != owner
            {
                if !old.disconnected.is_cancelled() {
                    return Err(CodexErr::InvalidRequest(
                        "다른 연결이 이 대화의 MCP 도구를 소유하고 있습니다.".into(),
                    ));
                }
                // A new owner may explicitly register the same live bridge after
                // the old connection ends. The offered definitions are validated
                // as complete values below; nothing is inherited from `old`.
            }
            if self
                .services
                .mcp_thread_init
                .get::<crate::mcp::McpServersDisabled>()
                .is_some()
            {
                return Err(CodexErr::InvalidRequest(
                    "MCP servers are disabled for this thread".into(),
                ));
            }
            let mut overlay =
                crate::mcp::RuntimeMcpServers::new(owner, disconnected.clone(), servers);
            if generation.is_some() {
                let id = state
                    .pending_runtime_tools
                    .as_ref()
                    .expect("generation validated")
                    .registration_id
                    .clone();
                overlay = overlay.with_registration_id(id);
            }
            overlay
                .validate(&state.session_configuration.original_config_do_not_use)
                .map_err(CodexErr::InvalidRequest)?;
            Some(overlay)
        } else {
            None
        };
        if active.is_some() {
            // An exact same-owner MCP registration is already installed. All
            // other validated changes remain queued at the caller until idle.
            if tools.is_none()
                && let Some(ref candidate) = overlay
                && let Some(current) = self
                    .services
                    .thread_extension_data
                    .get::<crate::mcp::RuntimeMcpServers>()
                && current.matches(owner, candidate.servers())
            {
                return Ok((
                    true,
                    generation,
                    None,
                    Some(current.registration_id().to_owned()),
                ));
            }
            if let Some(servers) = pending_servers {
                let id = match generation {
                    Some(id) => id,
                    None => {
                        let id = state
                            .runtime_registration_generation
                            .checked_add(1)
                            .ok_or_else(|| {
                                CodexErr::InvalidRequest("MCP 등록 세대가 소진됐습니다.".into())
                            })?;
                        state.runtime_registration_generation = id;
                        let pending = crate::state::PendingRuntimeTools {
                            generation: id,
                            registration_id: overlay
                                .as_ref()
                                .expect("MCP overlay validated")
                                .registration_id()
                                .to_owned(),
                            owner,
                            disconnected: disconnected.child_token(),
                            tools,
                            previous: previous.to_vec(),
                            servers,
                        };
                        if let Some(previous) = state.pending_runtime_tools.replace(pending) {
                            previous.disconnected.cancel();
                        }
                        id
                    }
                };
                let lifetime = state
                    .pending_runtime_tools
                    .as_ref()
                    .map(|pending| pending.disconnected.clone());
                let registration_id = state
                    .pending_runtime_tools
                    .as_ref()
                    .map(|pending| pending.registration_id.clone());
                return Ok((false, Some(id), lifetime, registration_id));
            }
            return Ok((false, None, None, None));
        }
        if let Some((merged, keys)) = merged {
            state.set_runtime_dynamic_tools(owner, disconnected, Arc::from(merged), keys);
        }
        let registration_id = overlay
            .as_ref()
            .map(|overlay| overlay.registration_id().to_owned());
        if let Some(overlay) = overlay {
            self.services.thread_extension_data.insert(overlay);
            self.mark_mcp_runtime_dirty();
        }
        if pending_servers.is_some() || generation.is_some() {
            if let Some(pending) = state.pending_runtime_tools.take() {
                pending.disconnected.cancel();
            }
        }
        Ok((true, generation, None, registration_id))
    }

    pub(crate) async fn cancel_pending_runtime_tools(&self, owner: u64, generation: u64) -> bool {
        let mut state = self.state.lock().await;
        if state
            .pending_runtime_tools
            .as_ref()
            .is_some_and(|pending| pending.owner == owner && pending.generation == generation)
        {
            if let Some(pending) = state.pending_runtime_tools.take() {
                pending.disconnected.cancel();
            }
            true
        } else {
            false
        }
    }

    pub(crate) fn watch_pending_runtime_tools(
        self: &Arc<Self>,
        owner: u64,
        generation: u64,
        lifetime: tokio_util::sync::CancellationToken,
    ) {
        let session = Arc::downgrade(self);
        tokio::spawn(async move {
            lifetime.cancelled().await;
            if let Some(session) = session.upgrade() {
                session
                    .cancel_pending_runtime_tools(owner, generation)
                    .await;
            }
        });
    }

    pub(crate) async fn runtime_dynamic_tool_owner(
        &self,
        namespace: Option<&str>,
        name: &str,
    ) -> Option<(u64, tokio_util::sync::CancellationToken)> {
        let state = self.state.lock().await;
        let overlay = state.runtime_dynamic_tools.as_ref()?;
        overlay
            .keys
            .iter()
            .any(|(ns, tool)| ns.as_deref() == namespace && tool == name)
            .then(|| (overlay.owner, overlay.disconnected.clone()))
    }

    /// Persist before publishing so a failed write cannot change the running catalog.
    pub(crate) async fn replace_dynamic_tools(
        &self,
        tools: Vec<codex_protocol::dynamic_tools::DynamicToolSpec>,
    ) -> CodexResult<()> {
        let active = self.active_turn.lock().await;
        // User replies hold active_turn while reserving communication order;
        // delivered-message records then acquire persistence. Keep that order.
        let _settings_guard = acquire_persistence_lock(self).await;
        if active.is_some() && self.state.lock().await.runtime_dynamic_tools.is_some() {
            return Err(CodexErr::InvalidRequest(
                "연결 한정 도구를 사용하는 활성 턴의 도구 목록은 교체할 수 없습니다.".into(),
            ));
        }
        if let Some(live_thread) = self.live_thread() {
            let config = self.get_config().await;
            if !config.allow_persistent_dynamic_tool_updates {
                return Err(CodexErr::UnsupportedOperation(
                    "The host has disabled dynamic tool updates for persistent threads.".into(),
                ));
            }
            let persistence_error = |error: ThreadStoreError| match error {
                ThreadStoreError::Unsupported { .. } => {
                    CodexErr::UnsupportedOperation(error.to_string())
                }
                _ => CodexErr::Fatal(format!("failed to replace dynamic tools: {error}")),
            };
            live_thread
                .append_items(&[RolloutItem::DynamicToolsUpdated {
                    thread_id: self.thread_id(),
                    tools: tools.clone(),
                }])
                .await
                .map_err(persistence_error)?;
            live_thread.flush().await.map_err(persistence_error)?;
        }
        let tools = Arc::from(tools);
        let mut state = self.state.lock().await;
        state.persisted_dynamic_tools = Arc::clone(&tools);
        state.session_configuration.dynamic_tools = tools;
        // An explicit official replacement supersedes the connection's old catalog snapshot.
        state.runtime_dynamic_tools = None;
        if let Some(pending) = state.pending_runtime_tools.take() {
            pending.disconnected.cancel();
        }
        Ok(())
    }

    /// Captures and flushes current settings under the shared persistence permit.
    pub(crate) async fn checkpoint_thread_settings(&self) -> ThreadStoreResult<()> {
        let _settings_guard = acquire_persistence_lock(self).await;
        if let Some(live_thread) = self.live_thread() {
            live_thread
                .append_items(&[RolloutItem::EventMsg(applied_event(self).await)])
                .await?;
            live_thread.flush().await?;
        }
        Ok(())
    }
}

/// Applies standalone thread settings. The caller holds the persistence permit through notification.
#[expect(
    clippy::await_holding_invalid_type,
    reason = "the active-turn guard makes continuation invalidation atomic with turn admission"
)]
pub(super) async fn update(
    session: &Session,
    overrides: impl Into<WithTurnExtensionData<ThreadSettingsOverrides>>,
) -> ConstraintResult<ThreadSettingsSnapshot> {
    let updates = prepare_update(overrides, &session.services.selected_capability_roots);
    let commit = session.update_settings(updates).await?;
    let active_turn = session.active_turn.lock().await;
    // A registered running task retains its lineage so it can be recovered after a daemon
    // restart. Reservations and finishing turns have no interrupted work to continue.
    if active_turn
        .as_ref()
        .is_none_or(|active_turn| active_turn.task.is_none())
    {
        session.state.lock().await.last_started_turn_id = None;
    }
    Ok(commit.snapshot)
}

/// Converts protocol overrides into the internal settings update shape.
pub(super) fn prepare_update(
    overrides: impl Into<WithTurnExtensionData<ThreadSettingsOverrides>>,
    roots: &[SelectedCapabilityRoot],
) -> SessionSettingsUpdate {
    let WithTurnExtensionData {
        request: overrides,
        turn_extension_init,
    } = overrides.into();
    let ThreadSettingsOverrides {
        model_context_window,
        environments: environment_requests,
        runtime_workspace_roots,
        profile_workspace_roots,
        approval_policy,
        approvals_reviewer,
        sandbox_policy,
        permission_profile,
        active_permission_profile,
        windows_sandbox_level,
        model,
        effort,
        summary,
        service_tier,
        collaboration_mode,
        personality,
        disabled_plugin_ids,
    } = overrides;
    SessionSettingsUpdate {
        model_context_window,
        turn_extension_init,
        step_settings: StepSettingsUpdate {
            model,
            effort,
            collaboration_mode,
            reasoning_summary: summary,
            service_tier,
            personality,
            approval_policy,
            approvals_reviewer,
        },
        environments: environment_requests.map(|requests| requests.select(roots)),
        runtime_workspace_roots,
        profile_workspace_roots,
        sandbox_policy,
        permission_profile,
        active_permission_profile,
        windows_sandbox_level,
        disabled_plugin_ids,
        ..Default::default()
    }
}

/// Acquires the shared permit before capturing or changing persistent settings.
pub(super) async fn acquire_persistence_lock(session: &Session) -> SemaphorePermit<'_> {
    session
        .thread_settings_persistence
        .acquire()
        .await
        .unwrap_or_else(|_| unreachable!("thread settings persistence semaphore is never closed"))
}

/// Applies persistent settings and emits the resulting thread-owned snapshot.
pub(super) async fn apply_update(
    session: &Session,
    submission_id: String,
    updates: SessionSettingsUpdate,
) -> ConstraintResult<()> {
    let _settings_guard = acquire_persistence_lock(session).await;
    let commit = session.update_settings(updates).await?;
    emit_applied(session, submission_id, commit.snapshot, true).await;
    Ok(())
}

/// The caller owns the settings persistence permit. Use the existing store's durability fence
/// before acknowledging a context edit, without waiting for its event consumer.
pub(super) async fn persist_context_window_settings(
    session: &Session,
    snapshot: &ThreadSettingsSnapshot,
) -> std::io::Result<()> {
    if let Some(live_thread) = session.live_thread() {
        live_thread
            .append_items(&[RolloutItem::EventMsg(EventMsg::ThreadSettingsApplied(
                ThreadSettingsAppliedEvent {
                    thread_id: Some(session.thread_id()),
                    thread_settings: snapshot.clone(),
                },
            ))])
            .await
            .map_err(std::io::Error::other)?;
        live_thread
            .persist(PersistContext::Standard)
            .await
            .map_err(std::io::Error::other)?;
        live_thread.flush().await.map_err(std::io::Error::other)?;
    }
    Ok(())
}

/// Emits the snapshot published by one successful settings update.
pub(super) async fn emit_applied(
    session: &Session,
    submission_id: String,
    snapshot: ThreadSettingsSnapshot,
    persist: bool,
) {
    let msg = EventMsg::ThreadSettingsApplied(ThreadSettingsAppliedEvent {
        thread_id: Some(session.thread_id()),
        thread_settings: snapshot,
    });
    let event = Event {
        id: submission_id,
        msg,
    };
    if persist {
        session
            .send_event_raw_without_materializing_rollout(event)
            .await;
    } else {
        session.send_event_raw_with_persistence(event, false).await;
    }
}

/// Builds a current thread-owned snapshot for storage checkpoints.
pub(super) async fn applied_event(session: &Session) -> EventMsg {
    EventMsg::ThreadSettingsApplied(ThreadSettingsAppliedEvent {
        thread_id: Some(session.thread_id()),
        thread_settings: session.thread_settings_snapshot().await,
    })
}

type RuntimeToolKey = (Option<String>, String);

#[cfg(test)]
mod runtime_merge_tests {
    use super::*;
    use codex_protocol::dynamic_tools::{
        DynamicToolFunctionSpec, DynamicToolNamespaceSpec, DynamicToolNamespaceTool,
        DynamicToolSpec,
    };

    fn namespace(description: &str, tools: &[(&str, &str)]) -> DynamicToolSpec {
        DynamicToolSpec::Namespace(DynamicToolNamespaceSpec {
            name: "local_tools".into(),
            description: description.into(),
            tools: tools
                .iter()
                .map(|(name, description)| {
                    DynamicToolNamespaceTool::Function(DynamicToolFunctionSpec {
                        name: (*name).into(),
                        description: (*description).into(),
                        input_schema: serde_json::json!({"type":"object", "properties":{}}),
                        defer_loading: false,
                    })
                })
                .collect(),
        })
    }

    #[test]
    fn namespace_merge_preserves_custom_members_and_rejects_unknown_definitions() {
        let function = DynamicToolSpec::Function(DynamicToolFunctionSpec {
            name: "local_tools".into(),
            description: "custom".into(),
            input_schema: serde_json::json!({"type":"object", "properties":{}}),
            defer_loading: false,
        });
        let namespaced = namespace("host", &[("cron", "new")]);
        assert!(merge_runtime_tools(&[function.clone()], &[namespaced.clone()], &[]).is_err());
        assert!(merge_runtime_tools(&[namespaced], &[function], &[]).is_err());
        let base = namespace("user namespace", &[("custom", "keep"), ("cron", "old")]);
        let incoming = namespace("host namespace", &[("cron", "new"), ("control", "new")]);
        assert!(merge_runtime_tools(&[base.clone()], &[incoming.clone()], &[]).is_err());
        let (merged, keys) = merge_runtime_tools(
            &[base],
            &[incoming],
            &[namespace("old namespace", &[("cron", "old")])],
        )
        .unwrap();
        assert_eq!(
            merged,
            vec![namespace(
                "user namespace",
                &[("custom", "keep"), ("cron", "new"), ("control", "new")]
            )]
        );
        assert_eq!(
            keys,
            vec![
                (Some("local_tools".into()), "cron".into()),
                (Some("local_tools".into()), "control".into())
            ]
        );
    }
}

fn merge_runtime_tools(
    base: &[codex_protocol::dynamic_tools::DynamicToolSpec],
    incoming: &[codex_protocol::dynamic_tools::DynamicToolSpec],
    previous: &[codex_protocol::dynamic_tools::DynamicToolSpec],
) -> Result<
    (
        Vec<codex_protocol::dynamic_tools::DynamicToolSpec>,
        Vec<RuntimeToolKey>,
    ),
    String,
> {
    use codex_protocol::dynamic_tools::{DynamicToolNamespaceTool, DynamicToolSpec};
    fn expected(
        previous: &[DynamicToolSpec],
        namespace: Option<&str>,
        current: &codex_protocol::dynamic_tools::DynamicToolFunctionSpec,
    ) -> bool {
        previous.iter().any(|spec| match spec {
            DynamicToolSpec::Function(tool) => namespace.is_none() && tool == current,
            DynamicToolSpec::Namespace(ns) => {
                namespace == Some(ns.name.as_str())
                    && ns.tools.iter().any(|tool| {
                        let DynamicToolNamespaceTool::Function(tool) = tool;
                        tool == current
                    })
            }
        })
    }
    let mut merged = base.to_vec();
    let mut keys = Vec::new();
    for spec in incoming {
        if merged.iter().any(|existing| match (existing, spec) {
            (DynamicToolSpec::Function(function), DynamicToolSpec::Namespace(namespace))
            | (DynamicToolSpec::Namespace(namespace), DynamicToolSpec::Function(function)) => {
                function.name == namespace.name
            }
            _ => false,
        }) {
            return Err("기존 사용자 정의 도구와 같은 이름의 다른 종류가 충돌합니다.".into());
        }
        match spec {
            DynamicToolSpec::Function(tool) => {
                if let Some(existing) = merged.iter_mut().find_map(|spec| match spec {
                    DynamicToolSpec::Function(existing) if existing.name == tool.name => {
                        Some(existing)
                    }
                    _ => None,
                }) {
                    if existing != tool && !expected(previous, None, existing) {
                        return Err(format!("기존 사용자 정의 도구와 충돌합니다: {}", tool.name));
                    }
                    *existing = tool.clone();
                } else {
                    merged.push(spec.clone());
                }
                keys.push((None, tool.name.clone()));
            }
            DynamicToolSpec::Namespace(ns) => {
                if let Some(existing) = merged.iter_mut().find_map(|spec| match spec {
                    DynamicToolSpec::Namespace(existing) if existing.name == ns.name => {
                        Some(existing)
                    }
                    _ => None,
                }) {
                    for tool in &ns.tools {
                        let DynamicToolNamespaceTool::Function(tool) = tool;
                        if let Some(current) = existing.tools.iter_mut().find_map(|entry| {
                            let DynamicToolNamespaceTool::Function(current) = entry;
                            (current.name == tool.name).then_some(current)
                        }) {
                            if current != tool && !expected(previous, Some(&ns.name), current) {
                                return Err(format!(
                                    "기존 사용자 정의 도구와 충돌합니다: {}.{}",
                                    ns.name, tool.name
                                ));
                            }
                            *current = tool.clone();
                        } else {
                            existing
                                .tools
                                .push(DynamicToolNamespaceTool::Function(tool.clone()));
                        }
                    }
                } else {
                    merged.push(spec.clone());
                }
                for tool in &ns.tools {
                    let DynamicToolNamespaceTool::Function(tool) = tool;
                    keys.push((Some(ns.name.clone()), tool.name.clone()));
                }
            }
        }
    }
    Ok((merged, keys))
}
