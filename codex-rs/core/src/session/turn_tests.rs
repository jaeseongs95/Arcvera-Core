// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use super::*;
use codex_extension_api::ExtensionData;
use codex_extension_api::TurnItemContributor;
use codex_protocol::ResponseItemId;
use codex_protocol::items::AgentMessageContent;
use pretty_assertions::assert_eq;
use std::sync::Arc;
use tracing_subscriber::prelude::*;

#[tokio::test]
async fn indexed_context_keeps_selected_originals_instead_of_running_auto_compaction() {
    use codex_history::context_index::ContextIndexOperation;
    let (session, turn) = crate::session::tests::make_session_and_context().await;
    let session = Arc::new(session);
    let turn = Arc::new(turn);
    session
        .state
        .lock()
        .await
        .history
        .apply_context_index(
            0,
            vec![
                ContextIndexOperation::CreateTask {
                    task_id: "work".into(),
                    label: "Current work".into(),
                },
                ContextIndexOperation::SetFocus {
                    task_id: "work".into(),
                },
            ],
        )
        .expect("focused task");
    let original = assistant_output_text("SELECTED_ORIGINAL_kept_without_summary");
    session
        .record_conversation_items(&turn, turn.model_info(), &[original])
        .await;
    let before = session.clone_history().await;
    let step = StepContext::for_test(Arc::clone(&turn));
    let mut client = session.services.model_client.new_session();
    let result = run_auto_compact(
        &session,
        Arc::clone(&step),
        step,
        &mut client,
        CompactionReason::ContextLimit,
        CompactionPhase::MidTurn,
        None,
    )
    .await
    .expect("selection owns the budget");
    assert!(
        !result,
        "automatic summary must not replace indexed evidence"
    );
    let after = session.clone_history().await;
    assert_eq!(before.annotated_items(), after.annotated_items());
    assert_eq!(before.context_index(), after.context_index());
}

#[tokio::test]
async fn indexed_context_hard_limit_fails_with_reclamation_guidance_and_keeps_originals() {
    use codex_history::context_index::ContextIndexOperation;
    use codex_protocol::protocol::TokenUsage;
    let (session, turn) = crate::session::tests::make_session_and_context().await;
    session
        .state
        .lock()
        .await
        .history
        .apply_context_index(
            0,
            vec![
                ContextIndexOperation::CreateTask {
                    task_id: "required".into(),
                    label: "Required".into(),
                },
                ContextIndexOperation::SetFocus {
                    task_id: "required".into(),
                },
            ],
        )
        .expect("focus");
    session
        .update_token_usage_info(
            &turn,
            Some(&TokenUsage {
                input_tokens: i64::MAX / 2,
                total_tokens: i64::MAX / 2,
                ..Default::default()
            }),
            false,
        )
        .await
        .expect("test session has no rollout spending limit; usage setup must succeed");
    let before = session.clone_history().await;
    let error = indexed_context_owns_budget(&session, &turn)
        .await
        .expect_err("selected input exceeds the test model hard limit");
    assert!(error.to_string().contains("context_index"));
    assert!(error.to_string().contains("preserved"));
    let after = session.clone_history().await;
    assert_eq!(before.annotated_items(), after.annotated_items());
    assert_eq!(before.context_index(), after.context_index());
}

struct RewriteAgentMessageContributor;

impl TurnItemContributor for RewriteAgentMessageContributor {
    fn contribute<'a>(
        &'a self,
        _thread_store: &'a ExtensionData,
        _turn_store: &'a ExtensionData,
        item: &'a mut TurnItem,
    ) -> codex_extension_api::ExtensionFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if let TurnItem::AgentMessage(agent_message) = item {
                agent_message.content = vec![AgentMessageContent::Text {
                    text: "plan contributed assistant text".to_string(),
                }];
            }
            Ok(())
        })
    }
}

fn assistant_output_text(text: &str) -> ResponseItem {
    ResponseItem::Message {
        status: None,
        encrypted_content: None,
        id: Some(ResponseItemId::with_suffix("msg", "1")),
        role: "assistant".to_string(),
        content: vec![ContentItem::OutputText {
            annotations: None,
            logprobs: None,
            text: text.to_string(),
        }],
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

#[test]
fn post_sampling_token_estimate_is_disabled_by_always_on_sinks() {
    let feedback = codex_feedback::CodexFeedback::new();
    let subscriber = tracing_subscriber::registry()
        .with(feedback.logger_layer())
        .with(tracing_subscriber::fmt::layer().with_filter(codex_state::log_db::default_filter()));

    static METADATA: tracing::Metadata<'static> = tracing::metadata! {
        name: "post sampling token estimate filter probe",
        target: POST_SAMPLING_TOKEN_ESTIMATE_TARGET,
        level: tracing::Level::TRACE,
        fields: &["turn_id", "estimated_token_count", "message"],
        callsite: &CALLSITE,
        kind: tracing::metadata::Kind::EVENT.hint(),
    };
    static CALLSITE: tracing::callsite::DefaultCallsite =
        tracing::callsite::DefaultCallsite::new(&METADATA);

    assert!(tracing::Subscriber::register_callsite(&subscriber, &METADATA).is_never());
}

#[tokio::test]
async fn plan_mode_uses_contributed_turn_item_for_last_agent_message() {
    let (mut session, turn_context) = crate::session::tests::make_session_and_context().await;
    let mut builder = codex_extension_api::ExtensionRegistryBuilder::new();
    builder.turn_item_contributor(Arc::new(RewriteAgentMessageContributor));
    session.services.extensions = Arc::new(builder.build());
    let turn_store = ExtensionData::new(turn_context.sub_id.clone());
    let mut state = PlanModeStreamState::new(&turn_context.sub_id);
    let mut last_agent_message = None;
    let item = assistant_output_text("original assistant text");

    let step_context = StepContext::for_test(Arc::new(turn_context));
    let handled = handle_assistant_item_done_in_plan_mode(
        &session,
        &step_context,
        &turn_store,
        &item,
        &mut state,
        /*previously_active_item*/ None,
        &mut last_agent_message,
    )
    .await;

    assert!(handled);
    assert_eq!(
        last_agent_message.as_deref(),
        Some("plan contributed assistant text")
    );
}

#[test]
fn realtime_user_verification_notice_excludes_request_payload() {
    let event = EventMsg::ElicitationRequest(codex_protocol::approvals::ElicitationRequestEvent {
        origin: Some(codex_protocol::approvals::ElicitationOrigin::McpServer),
        turn_id: None,
        server_name: "private-server-name".to_string(),
        id: codex_protocol::mcp::RequestId::String("private-request-id".to_string()),
        request: codex_protocol::approvals::ElicitationRequest::UserVerification {
            meta: None,
            title: "private-title".to_string(),
            description: "private-description".to_string(),
            challenge: "private-challenge".to_string(),
        },
    });
    assert_eq!(
        realtime_text_for_event(&event),
        Some(RealtimeEventText::Handoff(
            "<user_verification_notice>User verification is required. Please respond in the app.</user_verification_notice>".to_string(),
            None,
        )),
    );
}
