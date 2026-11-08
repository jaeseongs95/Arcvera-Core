//! Dynamic-tool handlers finish their item lifecycle when cancellation is signalled.

use super::*;
use crate::state::ActiveTurn;
use codex_protocol::protocol::Event;
use codex_protocol::protocol::EventMsg;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn native_mutation_fence_is_tool_context_not_user_authorization() {
    use crate::context::is_contextual_user_fragment;
    use crate::context::is_user_authorization_message;
    let item = Box::new(MemoryMutationFence {
        call_id: "opaque-native-call".into(),
        step_turn_id: "captured-step-turn".into(),
    })
    .into_boxed_response_item();
    assert!(!is_user_authorization_message(&item));
    let codex_protocol::models::ResponseItem::Message {
        content,
        internal_chat_message_metadata_passthrough: Some(metadata),
        ..
    } = item
    else {
        panic!("context message expected");
    };
    assert!(content.iter().all(is_contextual_user_fragment));
    let wire = serde_json::to_string(&content).unwrap();
    assert!(wire.contains("opaque-native-call"));
    assert!(wire.contains("captured-step-turn"));
    assert!(wire.contains("unverified"));
    assert_eq!(
        metadata.content_item_kinds.unwrap()[0].0,
        "memories.mutation_boundary"
    );
    assert!(matches!(
        metadata.content_item_metadata.unwrap()[0].provenance,
        codex_protocol::models::ContentItemProvenance::Tool { .. }
    ));
}

#[tokio::test]
async fn failed_native_response_records_freshness_fence_before_handler_returns_after_cancel() {
    use crate::session::step_context::StepContext;
    let (session, turn, receiver) = crate::session::tests::make_session_and_context_with_rx().await;
    *session.active_turn.lock().await = Some(ActiveTurn::default());
    let cancellation = CancellationToken::new();
    let handler = DynamicToolHandler::new(&DynamicToolFunctionSpec {
        name: "native-control-fixture".into(),
        description: "synthetic control".into(),
        input_schema: json!({"type":"object","properties":{}}),
        defer_loading: false,
    })
    .unwrap();
    let invocation = ToolInvocation {
        session: Arc::clone(&session),
        turn: Arc::clone(&turn),
        step_context: StepContext::for_test(Arc::clone(&turn)),
        cancellation_token: cancellation.clone(),
        tracker: Arc::new(tokio::sync::Mutex::new(
            crate::turn_diff_tracker::TurnDiffTracker::new(),
        )),
        call_id: "native-attempt".into(),
        tool_name: ToolName::plain("native-control-fixture"),
        source: crate::tools::context::ToolCallSource::Direct,
        payload: ToolPayload::Function {
            arguments: "{}".into(),
        },
        native_call: None,
        environments: Default::default(),
    };
    let task = tokio::spawn(async move { handler.handle_call(invocation).await });
    next_item(&receiver).await;
    session
        .notify_dynamic_tool_response(
            "native-attempt",
            DynamicToolResponse {
                content_items: vec![
                    codex_protocol::dynamic_tools::DynamicToolCallOutputContentItem::InputText {
                        text: "RPC outcome unconfirmed".into(),
                    },
                ],
                success: false,
                memory_context: Some(
                    codex_protocol::dynamic_tools::DynamicToolMemoryContext::MutationBoundary,
                ),
            },
        )
        .await;
    let completed = next_item(&receiver).await;
    assert_eq!(completed.success, Some(false));
    cancellation.cancel();
    let output = task
        .await
        .expect("dynamic handler joined")
        .expect("failed RPC remains a completed tool response");
    assert!(!output.success_for_logging());
    let history = session.clone_history().await;
    assert!(history.raw_items().any(|item| matches!(item, codex_protocol::models::ResponseItem::Message { internal_chat_message_metadata_passthrough: Some(metadata), .. }
        if metadata.content_item_kinds.as_ref().is_some_and(|kinds| kinds.iter().any(|kind| kind.0 == "memories.mutation_boundary")))));
    let fence = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let EventMsg::RawResponseItem(event) = receiver.recv().await.unwrap().msg {
                if serde_json::to_string(&event.item)
                    .unwrap()
                    .contains("memories.mutation_boundary")
                {
                    break event.item;
                }
            }
        }
    })
    .await
    .expect("fence raw event follows completed dynamic item");
    assert!(!crate::context::is_user_authorization_message(&fence));
    assert!(
        serde_json::to_string(&fence)
            .unwrap()
            .contains(&turn.sub_id)
    );
}

async fn next_item(receiver: &async_channel::Receiver<Event>) -> DynamicToolCallItem {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let item = match receiver.recv().await.expect("event channel open").msg {
                EventMsg::ItemStarted(event) => event.item,
                EventMsg::ItemCompleted(event) => event.item,
                _ => continue,
            };
            if let TurnItem::DynamicToolCall(item) = item {
                return item;
            }
        }
    })
    .await
    .expect("dynamic tool item should be emitted")
}

#[tokio::test]
async fn response_cancel_race_completes_once() {
    let (session, turn, receiver) = crate::session::tests::make_session_and_context_with_rx().await;
    *session.active_turn.lock().await = Some(ActiveTurn::default());
    let cancellation = CancellationToken::new();
    let lifecycle = tokio::spawn({
        let session = Arc::clone(&session);
        let cancellation = cancellation.clone();
        async move {
            request_dynamic_tool(
                &session,
                &turn,
                "call-1".to_string(),
                ToolName::plain("gate"),
                json!({}),
                cancellation,
            )
            .await
        }
    });
    next_item(&receiver).await;
    let response = DynamicToolResponse {
        memory_context: None,
        content_items: Vec::new(),
        success: true,
    };
    session
        .notify_dynamic_tool_response("call-1", response.clone())
        .await;
    cancellation.cancel();
    let result = lifecycle.await.expect("lifecycle joined");
    assert!(result.is_none() || result == Some(response));
    let item = next_item(&receiver).await;
    assert_eq!(
        (item.status, item.success, item.error),
        if result.is_some() {
            (DynamicToolCallStatus::Completed, Some(true), None)
        } else {
            (
                DynamicToolCallStatus::Failed,
                Some(false),
                Some("dynamic tool call was cancelled before receiving a response".to_string()),
            )
        }
    );
    while let Ok(event) = receiver.try_recv() {
        assert!(!matches!(event.msg, EventMsg::ItemCompleted(_)));
    }
}
