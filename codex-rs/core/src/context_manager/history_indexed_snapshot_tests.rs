// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! Additional snapshot trust-boundary cases; see resume-snapshot-boundary-cases.json.
use super::*;
use codex_history::context_index::ContextSourceKind;

#[test]
fn context_index_snapshot_omission_never_assigns_resident_to_new_focus() {
    let original: ResponseItem = serde_json::from_value(serde_json::json!({
        "type":"message", "id":"original", "role":"assistant",
        "content":[{"type":"output_text","text":"RESIDENT_ORIGINAL"}]
    }))
    .unwrap();
    let mut history = ContextManager::new();
    history.record_items(
        [original.clone()].iter(),
        TruncationPolicy::Bytes(usize::MAX),
    );
    let mut incoming = ContextIndex::default();
    incoming
        .apply(
            0,
            vec![
                ContextIndexOperation::CreateTask {
                    task_id: "other".into(),
                    label: "Other".into(),
                },
                ContextIndexOperation::SetFocus {
                    task_id: "other".into(),
                },
            ],
        )
        .unwrap();
    history
        .replay_context_index(&ContextIndexEvent::checkpoint(incoming).unwrap())
        .unwrap();
    let source = &history.context_index().sources()["original"];
    assert_eq!(source.kind, ContextSourceKind::Unknown);
    assert_eq!(source.task_id, None);
    assert!(!source.focus_scoped);
    assert_eq!(source.body_sha256, None);
    assert_eq!(source.projection_sha256, None);
    history
        .apply_context_index(
            1,
            vec![ContextIndexOperation::SetTaskState {
                task_id: "other".into(),
                state: codex_history::context_index::ContextTaskState::Completed,
            }],
        )
        .unwrap();
    assert!(history.context_index().is_active("original"));
    assert_eq!(history.projected_items()[0].item, original);
}

#[test]
fn context_index_serialized_view_requires_known_original_digest() {
    let output: ResponseItem = serde_json::from_value(serde_json::json!({
        "type":"function_call_output", "id":"result", "call_id":"normal",
        "output":"ACTUAL_FULL_TOOL_OUTPUT ".repeat(1000)
    }))
    .unwrap();
    let mut history = ContextManager::new();
    history.record_items([output.clone()].iter(), TruncationPolicy::Bytes(64));
    let resident = history.annotated_items().to_vec();
    assert_ne!(resident[0].item, output);
    let encoded = serde_json::to_vec(history.context_index()).unwrap();
    let mut restored = ContextManager::new();
    restored.replace_annotated(resident.clone());
    restored
        .restore_context_index(serde_json::from_slice(&encoded).unwrap())
        .unwrap();
    assert_eq!(
        restored.context_index().sources()["result"].kind,
        ContextSourceKind::ToolResult
    );
    assert_eq!(
        restored.context_index().sources()["result"].body_sha256,
        history.context_index().sources()["result"].body_sha256
    );
    let mut legacy: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    legacy["sources"]["result"]
        .as_object_mut()
        .unwrap()
        .remove("body_sha256");
    let mut conservative = ContextManager::new();
    conservative.replace_annotated(resident);
    conservative
        .restore_context_index(serde_json::from_value(legacy).unwrap())
        .unwrap();
    assert_eq!(
        conservative.context_index().sources()["result"].kind,
        ContextSourceKind::Unknown
    );
    assert!(conservative.context_index().is_active("result"));
    assert_eq!(
        conservative.context_index().sources()["result"].projection_sha256,
        None
    );
}
