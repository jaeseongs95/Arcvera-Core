// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! Actual agent fork boundaries; synthetic current-thread fixtures only.
use super::*;
use codex_attachment_store::InlineAttachmentStore;
use codex_protocol::openai_models::InputModality;
use pretty_assertions::assert_eq;

#[test_case::test_case(true; "preserved_prefix")]
#[test_case::test_case(false; "sanitized_prefix")]
#[tokio::test]
async fn indexed_context_agent_fork_preserves_originals_and_respects_prefix_mode(
    preserve_prefix: bool,
) {
    let mut harness = AgentControlHarness::new().await;
    harness
        .config
        .features
        .enable(Feature::MultiAgentV2)
        .expect("enable V2 fixture");
    harness.config.multi_agent_v2.preserve_fork_prefix = preserve_prefix;
    let (parent_id, parent) = harness.start_paginated_thread().await;
    let mut archived = assistant_message(
        "R41_FORK_ARCHIVED_ORIGINAL",
        Some(MessagePhase::FinalAnswer),
    );
    let mut active = assistant_message("R41_FORK_ACTIVE_ORIGINAL", Some(MessagePhase::FinalAnswer));
    if let ResponseItem::Message { id, .. } = &mut archived {
        *id = Some(ResponseItemId::with_suffix("msg", "fork_archived"));
    }
    if let ResponseItem::Message { id, .. } = &mut active {
        *id = Some(ResponseItemId::with_suffix("msg", "fork_active"));
    }
    parent
        .inject_response_items(vec![
            archived,
            active,
            user_message("R41_FORK_USER_CORRECTION"),
        ])
        .await
        .expect("seed actual parent");
    let turn = parent.session.new_default_turn().await;
    let result = parent
        .session
        .context_index_action(
            turn.as_ref(),
            &serde_json::json!({
                "action":"apply", "expected_revision":0, "operations":[
                    {"op":"create_task","task_id":"old","label":"Completed"},
                    {"op":"create_task","task_id":"current","label":"Current"},
                    {"op":"bind_sources","task_id":"old","source_ids":["msg_fork_archived"]},
                    {"op":"bind_sources","task_id":"current","source_ids":["msg_fork_active"]},
                    {"op":"set_task_state","task_id":"old","state":"completed"},
                    {"op":"set_focus","task_id":"current"}
                ]
            })
            .to_string(),
        )
        .await
        .expect("durable native parent policy");
    assert_eq!(result["revision"], 1);
    let call_id = "r41-fork-boundary-call".to_string();
    parent
        .session
        .record_conversation_items(
            turn.as_ref(),
            turn.model_info(),
            &[spawn_agent_call(&call_id)],
        )
        .await;
    let child_id = harness
        .spawn_anonymous_child(
            parent_id,
            SpawnAgentOptions {
                fork_parent_spawn_call_id: Some(call_id),
                fork_mode: Some(SpawnAgentForkMode::FullHistory),
                ..Default::default()
            },
        )
        .await;
    let child = harness
        .manager
        .get_thread(child_id)
        .await
        .expect("actual child");
    let history = child.session.clone_history().await;
    assert_eq!(
        history.context_index_revision(),
        if preserve_prefix { 1 } else { 0 }
    );
    let raw = serde_json::to_string(&history.raw_items().collect::<Vec<_>>())
        .expect("child original history");
    assert!(raw.contains("R41_FORK_ARCHIVED_ORIGINAL"));
    assert!(raw.contains("R41_FORK_USER_CORRECTION"));
    let prepared = history
        .projected_for_prompt(&[InputModality::Text], &InlineAttachmentStore)
        .await;
    let prepared = serde_json::to_string(&prepared).expect("actual child projection");
    assert_eq!(
        prepared.contains("R41_FORK_ARCHIVED_ORIGINAL"),
        !preserve_prefix
    );
    assert!(prepared.contains("R41_FORK_ACTIVE_ORIGINAL"));
    assert!(prepared.contains("R41_FORK_USER_CORRECTION"));
    if preserve_prefix {
        let child_turn = child.session.new_default_turn().await;
        let read = child.session.context_index_action(child_turn.as_ref(),
            &serde_json::json!({"action":"read","source_id":"msg_fork_archived","max_bytes":16384}).to_string())
            .await.expect("current child original is still readable");
        assert!(
            read["content"]
                .as_str()
                .expect("original content")
                .contains("R41_FORK_ARCHIVED_ORIGINAL")
        );
    }
    assert_eq!(
        parent
            .session
            .clone_history()
            .await
            .context_index_revision(),
        1
    );
}
