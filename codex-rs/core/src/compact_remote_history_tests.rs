// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use super::*;
use codex_history::CodexHarnessMetadata;

fn indexed_compaction_history() -> ContextManager {
    use codex_history::context_index::{ContextIndexOperation, ContextTaskState};
    let mut history = ContextManager::new();
    let items: Vec<ResponseItem> = serde_json::from_value(serde_json::json!([
        {"type":"message","id":"authorization","role":"user","content":[{"type":"input_text","text":"Keep the authorized scope"}]},
        {"type":"message","id":"finished","role":"assistant","content":[{"type":"output_text","text":"EXCLUDED_COMPLETED_DETAIL ".repeat(10000)}]},
        {"type":"function_call","id":"active-call","call_id":"active-job","name":"exec","arguments":"{}"},
        {"type":"function_call_output","id":"active-output","call_id":"active-job","output":"ACTIVE_RESULT ".repeat(2000)}
    ])).unwrap();
    history.record_items(
        items.iter(),
        codex_utils_output_truncation::TruncationPolicy::Bytes(usize::MAX),
    );
    history
        .apply_context_index(
            0,
            vec![
                ContextIndexOperation::CreateTask {
                    task_id: "done".into(),
                    label: "done".into(),
                },
                ContextIndexOperation::BindSources {
                    task_id: "done".into(),
                    source_ids: vec!["finished".into()],
                },
                ContextIndexOperation::SetTaskState {
                    task_id: "done".into(),
                    state: ContextTaskState::Completed,
                },
                ContextIndexOperation::CreateTask {
                    task_id: "current".into(),
                    label: "current".into(),
                },
                ContextIndexOperation::BindSources {
                    task_id: "current".into(),
                    source_ids: vec!["active-call".into()],
                },
                ContextIndexOperation::SetFocus {
                    task_id: "current".into(),
                },
            ],
        )
        .unwrap();
    history
}

#[tokio::test]
async fn core110_indexed_compaction_does_not_trim_for_excluded_history() {
    let (session, mut turn) = crate::session::tests::make_session_and_context().await;
    std::sync::Arc::make_mut(&mut turn.config)
        .features
        .disable(Feature::RetainClientToolOutputs)
        .unwrap();
    let mut history = indexed_compaction_history();
    let before = history.annotated_items().to_vec();
    let index = history.context_index().clone();
    let base = BaseInstructions {
        text: "base instructions".into(),
        provenance: None,
    };
    let budget = history
        .estimate_projected_token_count_with_base_instructions(&base)
        .unwrap();
    let (rewrites, deleted) =
        trim_function_call_history_to_fit_context_window(&mut history, &turn, Some(budget), &base);
    assert_eq!(
        rewrites, 0,
        "excluded history must not consume the active budget"
    );
    assert_eq!(deleted, 0);
    assert_eq!(history.annotated_items(), before);
    assert_eq!(history.context_index(), &index);
    let prompt = history
        .projected_for_prompt_annotated(
            &turn.model_info().input_modalities,
            session.services.image_store.as_ref(),
        )
        .await;
    let wire = serde_json::to_string(
        &prompt
            .iter()
            .map(|envelope| &envelope.item)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(!wire.contains("EXCLUDED_COMPLETED_DETAIL"));
    assert!(wire.contains("ACTIVE_RESULT"));
    let sum = prompt
        .iter()
        .map(|item| estimate_item_token_count(&item.item))
        .fold(0i64, i64::saturating_add);
    assert_eq!(
        budget,
        sum + i64::try_from(approx_token_count(&base.text)).unwrap()
    );
}

#[tokio::test]
async fn core110_indexed_compaction_rewrite_preserves_policy_and_original_identity() {
    let (session, mut turn) = crate::session::tests::make_session_and_context().await;
    std::sync::Arc::make_mut(&mut turn.config)
        .features
        .disable(Feature::RetainClientToolOutputs)
        .unwrap();
    let live = indexed_compaction_history();
    let before = live.annotated_items().to_vec();
    let index = live.context_index().clone();
    let mut history = live.clone();
    let base = BaseInstructions {
        text: String::new(),
        provenance: None,
    };
    let (rewrites, deleted) =
        trim_function_call_history_to_fit_context_window(&mut history, &turn, Some(1), &base);
    assert_eq!(rewrites, 1);
    assert!(deleted > 0);
    assert_eq!(
        history.context_index_revision(),
        index.revision(),
        "output preparation must not reset policy"
    );
    assert_eq!(history.context_index().focus_task_id(), Some("current"));
    assert_eq!(history.context_index().tasks(), index.tasks());
    for (id, original) in index.sources() {
        let prepared = &history.context_index().sources()[id];
        assert_eq!(
            prepared.body_sha256, original.body_sha256,
            "original identity changed for {id}"
        );
        assert_eq!(prepared.task_id, original.task_id);
        assert_eq!(history.context_index().is_active(id), index.is_active(id));
    }
    assert_eq!(
        live.annotated_items(),
        before,
        "request-only rewriting cannot alter live history"
    );
    assert_eq!(live.context_index(), &index);
    let prompt = history
        .projected_for_prompt_annotated(
            &turn.model_info().input_modalities,
            session.services.image_store.as_ref(),
        )
        .await;
    let wire = serde_json::to_string(
        &prompt
            .iter()
            .map(|envelope| &envelope.item)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(!wire.contains("EXCLUDED_COMPLETED_DETAIL"));
    assert!(wire.contains(CONTEXT_WINDOW_TRUNCATED_OUTPUT_MESSAGE));
    assert!(wire.contains("Keep the authorized scope"));
}

#[test]
fn rewritten_output_preserves_harness_metadata() {
    let envelope = ResponseItemEnvelope {
        item: ResponseItem::FunctionCallOutput {
            id: None,
            call_id: Some("call-1".to_string()),
            name: None,
            namespace: None,
            output: FunctionCallOutputPayload {
                body: FunctionCallOutputBody::Text("large output".repeat(100)),
                success: Some(true),
            },
            internal_chat_message_metadata_passthrough: None,
        },
        metadata: Some(CodexHarnessMetadata::default()),
    };

    let rewritten = rewritten_output_for_context_window(&envelope)
        .expect("function output should be rewritten");

    assert_eq!(rewritten.metadata, envelope.metadata);
    assert_ne!(rewritten.item, envelope.item);
}
