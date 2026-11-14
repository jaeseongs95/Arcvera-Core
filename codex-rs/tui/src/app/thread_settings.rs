// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R32 (2026-11-01); see MODIFICATIONS.md.
//! Thread settings sync between TUI-local state and app-server thread state.

use super::App;
use crate::app_command::AppCommand;
use crate::app_event::AppEvent;
use crate::app_event::PermissionProfileSelection;
use crate::app_server_session::AppServerSession;
use crate::chatwidget::cyber_model_approval_reviewer;
use crate::session_state::ThreadSessionState;
use codex_app_server_protocol::ApprovalsReviewer as AppServerApprovalsReviewer;
use codex_app_server_protocol::AskForApproval as AppServerAskForApproval;
use codex_app_server_protocol::ThreadSettings;
use codex_app_server_protocol::ThreadSettingsUpdateParams;
use codex_config::types::ApprovalsReviewer;
use codex_protocol::ThreadId;
use codex_protocol::config_types::ModeKind;
use codex_protocol::models::BUILT_IN_PERMISSION_PROFILE_WORKSPACE;
use codex_protocol::models::PermissionProfile;
use codex_protocol::openai_models::MODEL_SPECIALTY_CYBER;

impl App {
    pub(super) async fn handle_context_command(
        &mut self,
        app_server: &mut AppServerSession,
        thread_id: Option<ThreadId>,
        args: String,
    ) {
        let Some(thread_id) = thread_id.filter(|id| Some(*id) == self.active_thread_id) else {
            self.chat_widget.add_error_message(
                crate::i18n::text("Use /context after the current chat is connected.").to_string(),
            );
            return;
        };
        let view = self.chat_widget.connector_scope_generation();
        match args.trim() {
            "inspect" => {
                if !app_server.start_context_inspection(thread_id, view, self.app_event_tx.clone())
                {
                    self.chat_widget.add_error_message(
                        crate::i18n::text(
                            "A context inspection is already pending for this connection.",
                        )
                        .to_string(),
                    );
                } else {
                    self.chat_widget.add_info_message(
                        crate::i18n::text("Inspecting context metadata; you can keep working.")
                            .to_string(),
                        None,
                    );
                }
                return;
            }
            "inspect next" => {
                if !app_server.has_context_inspection_next(thread_id, view) {
                    self.chat_widget.add_info_message(
                        crate::i18n::text("No next context page is available for this view.")
                            .to_string(),
                        None,
                    );
                } else if !app_server.start_context_page(
                    thread_id,
                    view,
                    self.app_event_tx.clone(),
                    true,
                    25,
                ) {
                    self.chat_widget.add_error_message(
                        crate::i18n::text(
                            "A context inspection is already pending for this connection.",
                        )
                        .to_string(),
                    );
                } else {
                    self.chat_widget.add_info_message(
                        crate::i18n::text("Inspecting context metadata; you can keep working.")
                            .to_string(),
                        None,
                    );
                }
                return;
            }
            "inspect small" => {
                if !app_server.start_context_page(
                    thread_id,
                    view,
                    self.app_event_tx.clone(),
                    false,
                    1,
                ) {
                    self.chat_widget.add_error_message(
                        crate::i18n::text(
                            "A context inspection is already pending for this connection.",
                        )
                        .to_string(),
                    );
                } else {
                    self.chat_widget.add_info_message(
                        crate::i18n::text("Inspecting context metadata; you can keep working.")
                            .to_string(),
                        None,
                    );
                }
                return;
            }
            "cancel" => {
                let message = if app_server.cancel_context_inspection(thread_id, view) {
                    "Context inspection display cancelled; the sent request is still draining."
                } else {
                    "No context inspection is pending for this view."
                };
                self.chat_widget
                    .add_info_message(crate::i18n::text(message).to_string(), None);
                return;
            }
            _ => {}
        }
        let model_context_window = match parse_context_window_argument(&args) {
            Ok(value) => value,
            Err(message) => {
                self.chat_widget
                    .add_error_message(crate::i18n::text(message).to_string());
                return;
            }
        };
        let params = ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            model_context_window,
            ..Default::default()
        };
        match app_server.request_thread_settings_update(params).await {
            Ok(Some(response)) if response.settings_applied == Some(true) => {
                if let Some(settings) = response.thread_settings {
                    self.chat_widget
                        .update_context_window_display(settings.effective_context_window);
                    let selection = settings.model_context_window.map_or_else(
                        || crate::i18n::text("default").to_string(),
                        |tokens| format!("{tokens} {}", crate::i18n::text("tokens")),
                    );
                    let effective = settings.effective_context_window.map_or_else(
                        || crate::i18n::text("unknown").to_string(),
                        |tokens| format!("{tokens} {}", crate::i18n::text("tokens")),
                    );
                    self.chat_widget.add_info_message(
                        format!("{} {selection}\n{} {effective}\n{}",
                            crate::i18n::text("This chat's context window:"),
                            crate::i18n::text("Usable window after model limits and headroom:"),
                            crate::i18n::text("Changes apply to the next turn and do not increase the model's actual limit.")),
                        Some("/context inspect · /context <tokens> · /context reset · /context".to_string()),
                    );
                } else {
                    self.chat_widget.add_error_message(
                        crate::i18n::text(
                            "This server does not support chat context window queries.",
                        )
                        .to_string(),
                    );
                }
            }
            Ok(_) => self.chat_widget.add_error_message(
                crate::i18n::text("The server did not confirm the context window setting.")
                    .to_string(),
            ),
            Err(error) => self.chat_widget.add_error_message(format!(
                "{} {error:#}",
                crate::i18n::text("Failed to set context window:")
            )),
        }
    }

    pub(super) fn finish_context_inspection(
        &mut self,
        app_server: &mut AppServerSession,
        key: crate::app_server_session::ContextInspectionKey,
        outcome: crate::app_server_session::ContextInspectionOutcome,
    ) {
        use crate::app_server_session::{ContextInspectionFailure, ContextInspectionOutcome};
        if !app_server.accept_context_inspection(key, &outcome)
            || !key.matches_view(
                self.active_thread_id,
                self.chat_widget.connector_scope_generation(),
            )
        {
            return;
        }
        match outcome {
            ContextInspectionOutcome::Completed(Ok(response))
                if response.thread_id == key.thread_id.to_string() =>
            {
                self.chat_widget.add_info_message(
                    render_context_composition(&response),
                    Some("/context inspect".to_string()),
                );
            }
            ContextInspectionOutcome::Completed(Err(error)) => self.chat_widget.add_error_message(
                crate::i18n::text(match error {
                    ContextInspectionFailure::Unsupported => {
                        "This server does not support context inspection."
                    }
                    ContextInspectionFailure::PageTooLarge => {
                        "Context page is too large; restart with /context inspect small."
                    }
                    ContextInspectionFailure::SingleFragmentTooLarge => {
                        "A single fragment exceeds the 64 KiB metadata page limit; it cannot be displayed."
                    }
                    ContextInspectionFailure::Stale => {
                        "The context snapshot changed; restart with /context inspect."
                    }
                    ContextInspectionFailure::Failed => {
                        "Context inspection failed; retry with /context inspect."
                    }
                })
                .to_string(),
            ),
            ContextInspectionOutcome::TimedOut => self.chat_widget.add_error_message(
                crate::i18n::text(
                    "Context inspection timed out; the sent request is still draining.",
                )
                .to_string(),
            ),
            ContextInspectionOutcome::Completed(Ok(_)) | ContextInspectionOutcome::Drained => {}
        }
    }

    pub(super) async fn sync_active_thread_model_setting(
        &mut self,
        app_server: &mut AppServerSession,
        model: String,
        effort: Option<codex_protocol::openai_models::ReasoningEffort>,
    ) {
        let Some(mut params) = self.active_thread_model_setting_update_params(model) else {
            return;
        };
        params.effort = effort;
        self.send_model_settings_update(app_server, params).await;
    }

    pub(super) async fn send_model_settings_update(
        &mut self,
        app_server: &mut AppServerSession,
        params: ThreadSettingsUpdateParams,
    ) -> bool {
        let defaulted_to_auto_review = params.approvals_reviewer
            == Some(AppServerApprovalsReviewer::AutoReview)
            && (self.chat_widget.config_ref().approvals_reviewer != ApprovalsReviewer::AutoReview
                || AppServerAskForApproval::from(
                    self.chat_widget
                        .config_ref()
                        .permissions
                        .approval_policy
                        .value(),
                ) != AppServerAskForApproval::OnRequest);
        let pending_cyber_profile =
            params
                .permissions
                .as_ref()
                .map(|profile_id| PermissionProfileSelection {
                    profile_id: profile_id.clone(),
                    approval_policy: params.approval_policy,
                    approvals_reviewer: params
                        .approvals_reviewer
                        .map(codex_app_server_protocol::ApprovalsReviewer::to_core),
                    display_label: profile_id.clone(),
                });
        let settings_updated = self.send_thread_settings_update(app_server, params).await;
        if settings_updated
            && let (Some(thread_id), Some(selection)) =
                (self.active_thread_id, pending_cyber_profile)
        {
            self.pending_server_profiles.insert(thread_id, selection);
        }
        if defaulted_to_auto_review && settings_updated {
            self.app_event_tx.send(AppEvent::CyberModelAutoReviewNotice);
        }
        settings_updated
    }

    pub(super) fn active_thread_model_setting_update_params(
        &self,
        model: String,
    ) -> Option<ThreadSettingsUpdateParams> {
        let thread_id = self.active_thread_id?;
        let is_cyber_model = self.model_catalog.try_list_models().is_ok_and(|models| {
            models.iter().any(|preset| {
                preset.model == model
                    && preset.model_specialty.as_deref() == Some(MODEL_SPECIALTY_CYBER)
            })
        });

        let mut params = ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            model: Some(model),
            collaboration_mode: Some(self.chat_widget.effective_collaboration_mode()),
            ..ThreadSettingsUpdateParams::default()
        };

        if is_cyber_model {
            let workspace_profile = PermissionProfile::workspace_write();
            let workspace_allowed = self
                .config
                .permissions
                .can_set_permission_profile(&workspace_profile)
                .is_ok()
                && self.config.is_permission_profile_allowed(
                    BUILT_IN_PERMISSION_PROFILE_WORKSPACE,
                    &workspace_profile,
                );

            if workspace_allowed && let Some(reviewer) = cyber_model_approval_reviewer(&self.config)
            {
                params.permissions = Some(BUILT_IN_PERMISSION_PROFILE_WORKSPACE.to_string());
                params.approval_policy = Some(AppServerAskForApproval::OnRequest);
                params.approvals_reviewer = Some(reviewer.into());
            }
        }

        Some(params)
    }

    pub(super) async fn sync_active_thread_reasoning_setting(
        &mut self,
        app_server: &mut AppServerSession,
        effort: Option<codex_protocol::openai_models::ReasoningEffort>,
    ) {
        let Some(params) = self.active_thread_reasoning_setting_update_params(effort) else {
            return;
        };
        self.send_thread_settings_update(app_server, params).await;
    }

    pub(super) fn active_thread_reasoning_setting_update_params(
        &self,
        effort: Option<codex_protocol::openai_models::ReasoningEffort>,
    ) -> Option<ThreadSettingsUpdateParams> {
        let thread_id = self.active_thread_id?;
        Some(ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            effort,
            collaboration_mode: Some(self.chat_widget.current_collaboration_mode().clone()),
            ..ThreadSettingsUpdateParams::default()
        })
    }

    pub(super) async fn sync_active_thread_plan_mode_reasoning_setting(
        &mut self,
        app_server: &mut AppServerSession,
    ) {
        let Some(thread_id) = self.active_thread_id else {
            return;
        };
        let params = ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            collaboration_mode: Some(self.chat_widget.effective_collaboration_mode()),
            ..ThreadSettingsUpdateParams::default()
        };
        self.send_thread_settings_update(app_server, params).await;
    }

    pub(super) async fn sync_override_turn_context_settings(
        &mut self,
        app_server: &mut AppServerSession,
        thread_id: ThreadId,
        op: &AppCommand,
    ) {
        let AppCommand::OverrideTurnContext {
            cwd,
            approval_policy,
            approvals_reviewer,
            permission_profile: _,
            active_permission_profile,
            model,
            effort,
            summary,
            service_tier,
            collaboration_mode,
        } = op
        else {
            return;
        };

        let params = ThreadSettingsUpdateParams {
            thread_id: thread_id.to_string(),
            cwd: cwd.clone(),
            approval_policy: *approval_policy,
            approvals_reviewer: approvals_reviewer.map(AppServerApprovalsReviewer::from),
            permissions: active_permission_profile
                .as_ref()
                .map(|profile| profile.id.clone()),
            model: model.clone(),
            effort: effort.clone().unwrap_or_default(),
            summary: *summary,
            service_tier: service_tier.clone(),
            collaboration_mode: collaboration_mode.clone(),
            ..ThreadSettingsUpdateParams::default()
        };
        self.send_thread_settings_update(app_server, params).await;
    }

    pub(super) async fn apply_thread_settings_to_cached_session(
        &mut self,
        thread_id: ThreadId,
        settings: &ThreadSettings,
    ) {
        if let Some(blank) = self.agents_overview.blank_sessions.get_mut(&thread_id) {
            apply_thread_settings_to_session(&mut blank.session, settings);
        }
        if self.primary_thread_id == Some(thread_id)
            && let Some(session) = self.primary_session_configured.as_mut()
        {
            apply_thread_settings_to_session(session, settings);
        }

        if let Some(channel) = self.thread_event_channels.get(&thread_id) {
            let mut store = channel.store.lock().await;
            if let Some(session) = store.session.as_mut() {
                apply_thread_settings_to_session(session, settings);
            }
        }
    }

    pub(super) async fn send_thread_settings_update(
        &mut self,
        app_server: &mut AppServerSession,
        params: ThreadSettingsUpdateParams,
    ) -> bool {
        if !thread_settings_update_has_changes(&params) {
            return false;
        }
        let thread_id = ThreadId::from_string(&params.thread_id).ok();
        let changes_permissions = params.cwd.is_some()
            || params.approval_policy.is_some()
            || params.approvals_reviewer.is_some()
            || params.sandbox_policy.is_some()
            || params.permissions.is_some();
        if changes_permissions
            && thread_id.is_some_and(|id| self.pending_server_profiles.contains_key(&id))
        {
            return false;
        }
        match app_server.thread_settings_update(params).await {
            Ok(settings_updated) => {
                if settings_updated
                    && changes_permissions
                    && let Some(thread_id) = thread_id
                {
                    self.agents_overview
                        .requested_permission_profiles
                        .remove(&thread_id);
                }
                settings_updated
            }
            Err(err) => {
                tracing::warn!("failed to update app-server thread settings from TUI: {err}");
                self.chat_widget
                    .add_error_message(format!("Failed to update thread settings: {err}"));
                false
            }
        }
    }
}

fn render_context_composition(
    response: &codex_app_server_protocol::ThreadContextReadResponse,
) -> String {
    let text = crate::i18n::text;
    let number = |value: Option<u64>| {
        value.map_or_else(|| text("unknown").to_string(), |value| value.to_string())
    };
    let freshness =
        |value: bool| crate::i18n::static_message(if value { "d26-current" } else { "d26-stale" });
    let sizes = &response.sizes;
    let cache = &response.cache;
    let minimum_validity = cache
        .minimum_valid_until
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.to_rfc3339())
        .unwrap_or_else(|| text("unknown").to_string());
    let cache_revision = match cache.revision_status.as_str() {
        "current" => crate::i18n::static_message("d26-current"),
        "stale" => crate::i18n::static_message("d26-stale"),
        _ => text("unknown"),
    };
    let cache_status = match cache.status.as_str() {
        "hit" => text("cache hit"),
        "miss" => text("cache miss"),
        "invalid" => text("invalid provider usage"),
        _ => text("unknown"),
    };
    let ttl_support = match cache.ttl_support.as_str() {
        "configured_provider_policy" => text("configured provider policy"),
        _ => text("unknown"),
    };
    let ttl_application = match cache.ttl_application.as_str() {
        "requested_unconfirmed" => text("requested, unconfirmed"),
        "request_cache_activity_observed" => text("response cache activity observed"),
        _ => text("unknown"),
    };
    let mut lines = vec![
        text("Current chat context composition").to_string(),
        text("Body-free metadata; one page at a time, not an exact next request.").to_string(),
        format!(
            "{}: {} / {}",
            text("Index revision / history version"),
            response.context_index_revision,
            response.history_version
        ),
        format!(
            "{}: {} / {} / {}",
            text("Resident / selected / excluded bytes"),
            sizes.resident_serialized_bytes,
            sizes.selected_resident_serialized_bytes,
            sizes.excluded_resident_serialized_bytes
        ),
        format!(
            "{}: {}",
            text("Archive bytes"),
            number(sizes.archive_serialized_bytes)
        ),
        format!(
            "{}: {} / {}",
            text("Selected / navigation estimated tokens"),
            sizes.selected_body_estimated_tokens,
            sizes.navigation_estimated_tokens
        ),
        format!(
            "{}: {} / {} / {}",
            text("Base / tools / total estimated tokens"),
            number(sizes.base_estimated_tokens),
            number(sizes.tool_estimated_tokens),
            number(sizes.total_estimated_tokens)
        ),
        format!(
            "{}: {} / {} / {} / {} ({cache_revision})",
            text("Provider response read / write / uncached / total tokens"),
            number(cache.cached_input_tokens),
            number(cache.write_input_tokens),
            number(cache.uncached_input_tokens),
            number(cache.total_input_tokens)
        ),
        format!("{}: {cache_status}", text("Provider response cache status")),
        format!(
            "{}: {} / {}",
            text("Requested / effective TTL seconds"),
            number(cache.requested_ttl_seconds),
            number(cache.effective_ttl_seconds)
        ),
        format!(
            "{}: {ttl_support} / {ttl_application}",
            text("TTL policy / application")
        ),
        format!(
            "{}: {}",
            text("MCP / hook component split"),
            text("unknown")
        ),
    ];
    if response.fragments.is_empty() {
        lines.push(text("No fragment on this page.").to_string());
    } else {
        use codex_app_server_protocol::{
            ThreadContextResidency as R, ThreadContextSourceKind as K,
        };
        let label = crate::i18n::static_message;
        lines.push(
            text(
                "Page fragments: kind / selection / reason / residency / bytes / estimated tokens",
            )
            .to_string(),
        );
        for (index, fragment) in response.fragments.iter().take(25).enumerate() {
            let kind = label(match fragment.kind.as_ref() {
                Some(K::User) => "d26-kind-user",
                Some(K::Assistant) => "d26-kind-assistant",
                Some(K::Control) => "d26-kind-control",
                Some(K::Opaque) => "d26-kind-opaque",
                Some(K::ToolCall) => "d26-kind-call",
                Some(K::ToolResult) => "d26-kind-result",
                Some(K::Unknown) | None => "d26-unknown",
            });
            let selected = label(if fragment.selected {
                "d26-selected"
            } else {
                "d26-excluded"
            });
            let reason = label(match fragment.selection_reason.as_deref() {
                Some("user_instruction") => "d26-reason-instruction",
                Some("persistent_control") => "d26-reason-control",
                Some("opaque_context") => "d26-reason-opaque",
                Some("legacy_identity") => "d26-reason-legacy",
                Some("unpaired_tool_source") => "d26-reason-unpaired",
                Some("pending_tool_group") => "d26-reason-pending",
                Some("unclassified_tool_group") => "d26-reason-group",
                Some("unclassified") => "d26-reason-unclassified",
                Some("reachable_task") => "d26-reason-reachable",
                _ => "d26-unknown",
            });
            let residency = label(match &fragment.residency {
                R::Resident => "d26-resident-data",
                R::NotResident => "d26-not-resident",
                R::Unknown => "d26-unknown",
            });
            lines.push(format!(
                "  {} {}: {kind} / {selected} / {reason} / {residency} / {} / {}",
                label("d26-fragment-label"),
                index + 1,
                number(fragment.resident_serialized_bytes),
                number(fragment.estimated_tokens)
            ));
        }
    }
    if let Some(request) = &response.request_sizes {
        lines.push(format!(
            "{}: {} / {} / {} / {} / {} ({})",
            text("Prepared input / base / tools / output schema / sum bytes"),
            request.input_serialized_bytes,
            request.base_instructions_serialized_bytes,
            request.tool_definitions_serialized_bytes,
            request.output_schema_serialized_bytes,
            request.component_payload_bytes,
            freshness(
                request.history_version == response.history_version
                    && request.context_index_revision == response.context_index_revision
            )
        ));
        lines.push(text("Prepared component bytes exclude the transport envelope and are not provider token usage.").to_string());
    } else {
        lines.push(format!(
            "{}: {}",
            text("Prepared request component bytes"),
            text("unknown")
        ));
    }
    lines.push(format!(
        "{}: {}",
        text("Cache minimum validity for reported writes"),
        minimum_validity
    ));
    lines.push(
        text("Cache validity is not server-confirmed expiry; individual fragment TTL is unknown.")
            .to_string(),
    );
    lines.push(
        if response.next_cursor.is_some() {
            text("More fragments exist; use /context inspect next.")
        } else {
            text("No additional fragment page.")
        }
        .to_string(),
    );
    lines.join("\n")
}

fn parse_context_window_argument(args: &str) -> Result<Option<Option<i64>>, &'static str> {
    match args.trim() {
        "" => Ok(None),
        "reset" | "default" => Ok(Some(None)),
        value => value
            .parse::<i64>()
            .ok()
            .filter(|tokens| *tokens > 0)
            .map(|tokens| Some(Some(tokens)))
            .ok_or("Usage: /context [inspect|inspect next|inspect small|cancel|positive integer tokens|reset]"),
    }
}

fn apply_thread_settings_to_session(session: &mut ThreadSessionState, settings: &ThreadSettings) {
    if settings.collaboration_mode.mode == ModeKind::Default {
        session.model = settings.model.clone();
        session.reasoning_effort = settings.effort.clone();
    }
    session.model_provider_id = settings.model_provider.clone();
    session.service_tier = settings.service_tier.clone();
    session.approval_policy = settings.approval_policy;
    session.approvals_reviewer = settings.approvals_reviewer.to_core();
    session.permission_profile = PermissionProfile::from_legacy_sandbox_policy_for_cwd(
        &settings.sandbox_policy.to_core(),
        settings.cwd.as_path(),
    );
    session.active_permission_profile = settings.active_permission_profile.clone().map(Into::into);
    session.set_cwd_retargeting_implicit_runtime_workspace_root(settings.cwd.clone());
    let mut collaboration_mode = settings.collaboration_mode.clone();
    collaboration_mode
        .settings
        .model
        .clone_from(&settings.model);
    collaboration_mode.settings.reasoning_effort = settings.effort.clone();
    session.collaboration_mode = Some(Box::new(collaboration_mode));
}

fn thread_settings_update_has_changes(params: &ThreadSettingsUpdateParams) -> bool {
    params.model_context_window.is_some()
        || params.cwd.is_some()
        || params.approval_policy.is_some()
        || params.approvals_reviewer.is_some()
        || params.sandbox_policy.is_some()
        || params.permissions.is_some()
        || params.model.is_some()
        || params.service_tier.is_some()
        || params.effort.is_some()
        || params.summary.is_some()
        || params.collaboration_mode.is_some()
}

#[cfg(test)]
mod context_window_tests {
    use super::parse_context_window_argument;

    #[test]
    fn inspection_render_uses_body_free_units_and_preserves_unknowns() {
        let mut response: codex_app_server_protocol::ThreadContextReadResponse = serde_json::from_value(serde_json::json!({
            "threadId":"synthetic", "snapshotId":"snapshot", "contextIndexRevision":3,
            "historyVersion":5, "asOf":"2026-01-01T00:00:00Z",
            "sizes":{"residentSerializedBytes":20,"selectedResidentSerializedBytes":12,"excludedResidentSerializedBytes":8,
                "archiveSerializedBytes":null,"selectedBodyEstimatedTokens":4,"estimateProvenance":"heuristic",
                "navigationEstimatedTokens":2,"baseEstimatedTokens":null,"toolEstimatedTokens":null,"totalEstimatedTokens":null},
            "limits":{"requestedContextWindowOverride":null,"effectiveContextWindow":null,"provenance":"unknown"},
            "fragments":[{"sourceId":"never-display-source-id", "sourceOrdinal":null,"residentOccurrence":0,"taskId":null,"kind":null,
                "selected":true,"required":null,"selectionReason":null,"residency":"resident","residentSerializedBytes":12,"estimatedTokens":4}],
            "cache":{"scope":"response","provenance":"not_reported","status":"unknown","cachedInputTokens":null,
                "writeInputTokens":null,"uncachedInputTokens":null,"totalInputTokens":null,"observedAt":null,"observationId":null,
                "requestContextIndexRevision":null,"revisionStatus":"unknown","requestedTtlSeconds":null,"effectiveTtlSeconds":null,
                "ttlSupport":"unknown","ttlApplication":"unknown","requestStartedAt":null,"minimumValidUntil":null,
                "minimumValidUntilProvenance":"unknown","minimumValidUntilScope":"unknown","estimatedExpiry":null,"serverConfirmedExpiry":null},
            "requestSizes":null,"nextCursor":1
        })).unwrap();
        response.fragments[0].kind = Some(codex_app_server_protocol::ThreadContextSourceKind::User);
        response.fragments[0].selection_reason = Some("user_instruction".into());
        let mut other = response.fragments[0].clone();
        other.selected = false;
        other.kind = Some(codex_app_server_protocol::ThreadContextSourceKind::ToolCall);
        other.selection_reason = None;
        response.fragments.push(other);
        crate::i18n::with_locale(crate::i18n::Locale::English, || {
            let rendered = super::render_context_composition(&response);
            assert!(rendered.contains("unknown"));
            assert!(rendered.contains("20 / 12 / 8"));
            assert!(rendered.contains("/context inspect next"));
            assert!(!rendered.contains("never-display-source-id"));
            assert!(rendered.contains("not server-confirmed expiry"));
            assert!(rendered.contains("Fragment 1: user / selected / user instruction"));
            assert!(rendered.contains("Fragment 2: tool call / excluded / unknown"));
        });
        crate::i18n::with_locale(crate::i18n::Locale::Korean, || {
            let rendered = super::render_context_composition(&response);
            assert!(rendered.contains("상주 데이터 / 선택 / 제외 바이트"));
            assert!(!rendered.contains("never-display-source-id"));
        });
        response.request_sizes = Some(serde_json::from_value(serde_json::json!({
            "observationId":"synthetic", "recordedAt":"2026-01-01T00:00:00Z", "scope":"prepared", "provenance":"local",
            "historyVersion":4,"contextIndexRevision":3,"inputSerializedBytes":10,"baseInstructionsSerializedBytes":2,
            "toolDefinitionsSerializedBytes":3,"outputSchemaSerializedBytes":4,"componentPayloadBytes":19
        })).unwrap());
        crate::i18n::with_locale(crate::i18n::Locale::English, || {
            let rendered = super::render_context_composition(&response);
            assert!(rendered.contains("10 / 2 / 3 / 4 / 19 (stale)"));
            assert!(rendered.contains("not provider token usage"));
        });
    }

    #[test]
    fn context_argument_preserves_query_set_and_reset() {
        assert_eq!(parse_context_window_argument(""), Ok(None));
        assert_eq!(
            parse_context_window_argument(" 120000 "),
            Ok(Some(Some(120_000)))
        );
        assert_eq!(parse_context_window_argument("reset"), Ok(Some(None)));
        assert_eq!(parse_context_window_argument("default"), Ok(Some(None)));
        for invalid in ["0", "-1", "1.5", "120000 extra", "9223372036854775808"] {
            assert!(parse_context_window_argument(invalid).is_err(), "{invalid}");
        }
    }
}

#[cfg(test)]
mod context_inspection_app_tests {
    use super::*;
    use crate::app_event_sender::AppEventSender;
    use crate::app_server_session::ThreadParamsMode;
    use crate::render::renderable::Renderable;
    use codex_app_server_client::{AppServerClient, CloudClient, CloudOperation};
    use ratatui::{buffer::Buffer, layout::Rect};

    // Cases: default-25 failure offers the 1-item action; a 1-item failure
    // reports the irreducible boundary without retrying or advancing a cursor.
    #[tokio::test]
    async fn context_event_renders_distinct_oversize_errors_without_retrying() {
        let (client, mut commands, _events) = CloudClient::channel(4);
        let mut server = AppServerSession::new(
            crate::backend_id::BackendId::new(),
            AppServerClient::Cloud(client),
            ThreadParamsMode::Remote,
        );
        let (mut app, mut events, _ops) =
            Box::pin(crate::app::tests::make_test_app_with_channels()).await;
        let thread = ThreadId::new();
        app.active_thread_id = Some(thread);
        let mut tui = crate::tui::test_support::make_test_tui().unwrap();
        for (argument, expected, limit) in [
            (
                "inspect",
                "Context page is too large; restart with /context inspect small.",
                25,
            ),
            (
                "inspect small",
                "A single fragment exceeds the 64 KiB metadata page limit; it cannot be displayed.",
                1,
            ),
        ] {
            Box::pin(app.handle_event(
                &mut tui,
                &mut server,
                AppEvent::ContextCommand {
                    thread_id: Some(thread),
                    args: argument.into(),
                },
            ))
            .await
            .unwrap();
            let command = tokio::time::timeout(std::time::Duration::from_secs(3), commands.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(
                matches!(&command.operation, CloudOperation::Request(request)
                if matches!(request.as_ref(), codex_app_server_protocol::ClientRequest::ThreadContextRead { params, .. }
                    if params.limit == Some(limit) && params.cursor == 0))
            );
            command
                .response
                .send(Ok(Err(codex_app_server_protocol::JSONRPCErrorError {
                    code: -32600,
                    message: "context page exceeds 65536 bytes; request a smaller limit".into(),
                    data: None,
                })))
                .unwrap();
            let result = loop {
                let event = tokio::time::timeout(std::time::Duration::from_secs(3), events.recv())
                    .await
                    .unwrap()
                    .unwrap();
                if matches!(&event, AppEvent::ContextCompositionLoaded { .. }) {
                    break event;
                }
            };
            Box::pin(app.handle_event(&mut tui, &mut server, result))
                .await
                .unwrap();
            let rendered = loop {
                let event = tokio::time::timeout(std::time::Duration::from_secs(3), events.recv())
                    .await
                    .unwrap()
                    .unwrap();
                if let AppEvent::InsertHistoryCell(cell) = event {
                    break cell
                        .display_lines(200)
                        .iter()
                        .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
                        .collect::<String>();
                }
            };
            assert!(
                rendered.contains(crate::i18n::text(expected)),
                "rendered: {rendered}"
            );
            if limit == 1 {
                assert!(!rendered.contains("/context inspect small"));
            }
            assert!(
                !server.has_context_inspection_next(
                    thread,
                    app.chat_widget.connector_scope_generation()
                )
            );
            assert!(
                commands.try_recv().is_err(),
                "an oversize response must not trigger an automatic retry"
            );
        }
    }

    #[tokio::test]
    async fn context_event_keeps_input_and_bounds_request_across_cancel_and_view_change() {
        let (client, mut commands, _events) = CloudClient::channel(4);
        let mut server = AppServerSession::new(
            crate::backend_id::BackendId::new(),
            AppServerClient::Cloud(client),
            ThreadParamsMode::Remote,
        );
        let mut app = crate::app::test_support::make_test_app().await;
        let (tx, mut events) = tokio::sync::mpsc::unbounded_channel();
        app.app_event_tx = AppEventSender::new(tx);
        let thread = ThreadId::new();
        app.active_thread_id = Some(thread);
        let mut tui = crate::tui::test_support::make_test_tui().unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            Box::pin(app.handle_event(
                &mut tui,
                &mut server,
                AppEvent::ContextCommand {
                    thread_id: Some(thread),
                    args: "inspect".into(),
                },
            )),
        )
        .await
        .unwrap()
        .unwrap();
        let command = commands.recv().await.unwrap();
        assert!(
            matches!(&command.operation, CloudOperation::Request(request)
            if matches!(request.as_ref(), codex_app_server_protocol::ClientRequest::ThreadContextRead { params, .. }
                if params.thread_id == thread.to_string() && params.cursor == 0 && params.limit == Some(25) && params.snapshot_id.is_none()))
        );
        app.chat_widget.insert_str("DRAFT_WHILE_CONTEXT_IS_PENDING");
        let other = ThreadId::new();
        app.active_thread_id = Some(other);
        Box::pin(app.handle_event(
            &mut tui,
            &mut server,
            AppEvent::ContextCommand {
                thread_id: Some(other),
                args: "inspect".into(),
            },
        ))
        .await
        .unwrap();
        assert!(
            commands.try_recv().is_err(),
            "a view switch alone must not release the undrained wire slot"
        );
        app.active_thread_id = Some(thread);
        Box::pin(app.handle_event(
            &mut tui,
            &mut server,
            AppEvent::ContextCommand {
                thread_id: Some(thread),
                args: "cancel".into(),
            },
        ))
        .await
        .unwrap();
        app.active_thread_id = Some(ThreadId::new());
        let current = app.active_thread_id;
        Box::pin(app.handle_event(
            &mut tui,
            &mut server,
            AppEvent::ContextCommand {
                thread_id: current,
                args: "inspect".into(),
            },
        ))
        .await
        .unwrap();
        assert!(
            commands.try_recv().is_err(),
            "view switch and cancel must not release the undrained wire slot"
        );
        {
            let view = app.chat_widget.bottom_pane_renderable(Default::default());
            let area = Rect::new(0, 0, 100, view.desired_height(100));
            let mut buffer = Buffer::empty(area);
            view.render(area, &mut buffer);
            let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
            assert!(text.contains("DRAFT_WHILE_CONTEXT_IS_PENDING"));
        }
        assert!(!command.response.is_closed());
        command
            .response
            .send(Err(std::io::Error::other("controlled late reply")))
            .unwrap();
        let event = tokio::time::timeout(std::time::Duration::from_secs(3), events.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(&event, AppEvent::ContextCompositionLoaded { .. }));
        Box::pin(app.handle_event(&mut tui, &mut server, event))
            .await
            .unwrap();
        Box::pin(app.handle_event(
            &mut tui,
            &mut server,
            AppEvent::ContextCommand {
                thread_id: current,
                args: "inspect".into(),
            },
        ))
        .await
        .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(3), commands.recv())
                .await
                .unwrap()
                .is_some()
        );
    }
}
