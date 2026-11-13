// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! ROOT-13/14: conservative evidence survives scoped summaries and policy counts stay separate
//! from cumulative spending. These are synthetic in-process regression tests.
use super::*;

#[test]
fn context_index_prepared_tool_suffix_retains_identity_through_cold_checkpoint() {
    use codex_history::context_index::ContextSourceKind;
    let mut history = ContextManager::new();
    history
        .apply_context_index(
            0,
            vec![
                ContextIndexOperation::CreateTask {
                    task_id: "a".into(),
                    label: "A".into(),
                },
                ContextIndexOperation::CreateTask {
                    task_id: "b".into(),
                    label: "B".into(),
                },
                ContextIndexOperation::SetFocus {
                    task_id: "a".into(),
                },
            ],
        )
        .unwrap();
    let call: ResponseItem = serde_json::from_value(serde_json::json!({
        "type":"function_call", "id":"call-original", "call_id":"tool_1", "name":"shell", "arguments":"{}"
    })).unwrap();
    let original: ResponseItem = serde_json::from_value(serde_json::json!({
        "type":"function_call_output", "id":"result-original", "call_id":"tool_1", "output":"FULL_ORIGINAL ".repeat(1000)
    })).unwrap();
    history.record_items([call, original.clone()].iter(), TruncationPolicy::Bytes(64));
    let original_digest = history.context_index().sources()["result-original"]
        .body_sha256
        .clone();
    let resident = history.annotated_items().to_vec();
    assert_ne!(
        resident[1].item, original,
        "fixture must cross real truncation"
    );
    history.replace_compacted(resident.clone(), None);
    assert_eq!(
        history.context_index().sources()["result-original"].kind,
        ContextSourceKind::ToolResult,
        "trusted carried suffix must retain original classification"
    );
    assert_eq!(
        history.context_index().sources()["result-original"].body_sha256,
        original_digest
    );
    let snapshot = history.context_index().clone();
    let mut cold = ContextManager::new();
    cold.replace_annotated(resident.clone());
    cold.restore_context_index(snapshot.clone()).unwrap();
    assert_eq!(
        cold.context_index().sources()["result-original"].kind,
        ContextSourceKind::ToolResult
    );
    assert_eq!(
        cold.context_index().sources()["result-original"]
            .task_id
            .as_deref(),
        Some("a")
    );
    assert_eq!(
        cold.context_index().sources()["result-original"].body_sha256,
        original_digest
    );
    cold.apply_context_index(
        1,
        vec![ContextIndexOperation::SetFocus {
            task_id: "b".into(),
        }],
    )
    .unwrap();
    assert!(
        cold.projected_items().is_empty(),
        "unrelated complete tool group must leave input"
    );
    assert_eq!(cold.raw_items().len(), 2);
    // A new ORIGINAL equal only to an old prepared view is still a conflicting original.
    cold.record_items(
        [resident[1].item.clone()].iter(),
        TruncationPolicy::Bytes(usize::MAX),
    );
    assert_eq!(
        cold.context_index().sources()["result-original"].kind,
        ContextSourceKind::Unknown
    );
    cold.replay_context_index(&ContextIndexEvent::checkpoint(snapshot.clone()).unwrap())
        .unwrap();
    assert_eq!(
        cold.context_index().sources()["result-original"].kind,
        ContextSourceKind::Unknown,
        "a snapshot cannot hide a conflicting resident original behind its prepared digest"
    );
    assert!(cold.context_index().is_active("result-original"));
    let mut altered = resident;
    altered[1]
        .metadata
        .get_or_insert_default()
        .compaction_output = true;
    let mut changed_provenance = ContextManager::new();
    changed_provenance.replace_annotated(altered);
    changed_provenance.restore_context_index(snapshot).unwrap();
    assert_eq!(
        changed_provenance.context_index().sources()["result-original"].kind,
        ContextSourceKind::Unknown,
        "view equality cannot override changed host provenance"
    );
}

#[test]
fn context_index_restore_accounts_anonymous_resident_multiplicity_once() {
    let mut summary = message("unused", "assistant", "ANONYMOUS_SUMMARY");
    if let ResponseItem::Message { id, .. } = &mut summary {
        *id = None;
    }
    let mut history = ContextManager::new();
    history.replace(vec![summary.clone(), summary]);
    history
        .restore_context_index(ContextIndex::default())
        .unwrap();
    assert_eq!(
        history.context_index().sources().len(),
        2,
        "missing snapshot descriptors must account for each anonymous occurrence"
    );
    assert!(
        history
            .context_index()
            .sources()
            .values()
            .all(|source| source.response_item_id.is_none() && source.call_id.is_none())
    );
    let once = history.context_index().clone();
    history.restore_context_index(once).unwrap();
    assert_eq!(
        history.context_index().sources().len(),
        2,
        "reconcile must not grow duplicate descriptors"
    );
    assert_eq!(history.projected_items().len(), 2);
}

#[test]
fn context_index_reused_identity_preserves_distinct_projection_and_compaction_roots() {
    let mut history = ContextManager::new();
    let old = message("reused", "assistant", "OLD_EXCLUDED_ORIGINAL");
    let new = message("reused", "assistant", "NEW_DISTINCT_ORIGINAL");
    history.record_items([old.clone()].iter(), TruncationPolicy::Bytes(usize::MAX));
    history
        .apply_context_index(
            0,
            vec![
                ContextIndexOperation::CreateTask {
                    task_id: "a".into(),
                    label: "A".into(),
                },
                ContextIndexOperation::CreateTask {
                    task_id: "b".into(),
                    label: "B".into(),
                },
                ContextIndexOperation::BindSources {
                    task_id: "a".into(),
                    source_ids: vec!["reused".into()],
                },
                ContextIndexOperation::SetTaskState {
                    task_id: "a".into(),
                    state: codex_history::context_index::ContextTaskState::Completed,
                },
                ContextIndexOperation::SetFocus {
                    task_id: "b".into(),
                },
            ],
        )
        .unwrap();
    assert!(history.projected_items().is_empty());
    // A captured summary omitted OLD, then a same-ID distinct live suffix arrived.
    history.record_items([new.clone()].iter(), TruncationPolicy::Bytes(usize::MAX));
    assert_eq!(
        history.raw_items().cloned().collect::<Vec<_>>(),
        vec![old.clone(), new.clone()]
    );
    assert_eq!(
        history
            .projected_items()
            .iter()
            .map(|e| e.item.clone())
            .collect::<Vec<_>>(),
        vec![old.clone(), new.clone()]
    );
    let summary = ResponseItemEnvelope::new(message("summary", "assistant", "CAPTURED_SUMMARY"));
    let mut replacement = vec![summary, ResponseItemEnvelope::new(new.clone())];
    history.preserve_context_index_roots(&mut replacement);
    history.replace_compacted(replacement, None);
    let raw = history.raw_items().cloned().collect::<Vec<_>>();
    assert!(
        raw.contains(&old),
        "same ID in suffix is not equality evidence for old root"
    );
    assert!(raw.contains(&new));
    let active = history
        .projected_items()
        .iter()
        .map(|e| e.item.clone())
        .collect::<Vec<_>>();
    assert!(active.contains(&old));
    assert!(active.contains(&new));
}

fn message(id: &str, role: &str, text: &str) -> ResponseItem {
    serde_json::from_value(serde_json::json!({"type":"message","id":id,"role":role,"content":[{"type":"input_text","text":text}]})).expect("typed fixture")
}

#[test]
fn context_index_scoped_summary_does_not_replace_conservative_roots() {
    let mut history = ContextManager::new();
    history.record_items(
        [
            message("user", "user", "latest user constraint"),
            message("control", "developer", "current permissions"),
            message("detail", "assistant", "task A detail"),
        ]
        .iter(),
        TruncationPolicy::Bytes(usize::MAX),
    );
    history
        .apply_context_index(
            0,
            vec![
                ContextIndexOperation::CreateTask {
                    task_id: "a".into(),
                    label: "A".into(),
                },
                ContextIndexOperation::CreateTask {
                    task_id: "b".into(),
                    label: "B".into(),
                },
                ContextIndexOperation::BindSources {
                    task_id: "a".into(),
                    source_ids: vec!["detail".into()],
                },
                ContextIndexOperation::SetFocus {
                    task_id: "a".into(),
                },
            ],
        )
        .expect("focused task");
    let mut summary =
        ResponseItemEnvelope::new(message("summary", "user", "host-derived A summary"));
    summary.metadata.get_or_insert_default().compaction_output = true;
    let mut replacement = vec![summary];
    history.preserve_context_index_roots(&mut replacement);
    history.replace_compacted(replacement, None);
    assert!(history.context_index().is_active("summary"));
    history
        .apply_context_index(
            1,
            vec![ContextIndexOperation::SetFocus {
                task_id: "b".into(),
            }],
        )
        .expect("switch focus");
    let active = history
        .projected_items()
        .iter()
        .filter_map(|item| item.item.id().map(ToString::to_string))
        .collect::<Vec<_>>();
    assert!(active.contains(&"user".into()));
    assert!(active.contains(&"control".into()));
    assert!(!active.contains(&"summary".into()));
    assert!(
        history
            .raw_items()
            .any(|item| item.id().is_some_and(|id| id.as_str() == "summary"))
    );
}

#[test]
fn context_index_policy_invalidates_window_count_without_erasing_spending() {
    let mut history = ContextManager::new();
    history.record_items(
        [
            message("user", "user", "current constraint"),
            message("detail", "assistant", &"completed detail ".repeat(5000)),
        ]
        .iter(),
        TruncationPolicy::Bytes(usize::MAX),
    );
    let info = TokenUsageInfo {
        total_token_usage: TokenUsage {
            total_tokens: 123456,
            ..Default::default()
        },
        last_token_usage: TokenUsage {
            total_tokens: 100000,
            ..Default::default()
        },
        last_usage_includes_reasoning: Some(true),
        model_context_window: None,
    };
    let boundary = history.raw_items().len();
    history.set_token_info_with_boundary(Some(info.clone()), boundary);
    assert_eq!(history.get_total_token_usage(), 100000);
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
                    source_ids: vec!["detail".into()],
                },
                ContextIndexOperation::SetTaskState {
                    task_id: "done".into(),
                    state: codex_history::context_index::ContextTaskState::Completed,
                },
            ],
        )
        .expect("exclude task");
    let active_count = history.get_total_token_usage();
    assert!(active_count > 0 && active_count < 100000);
    assert_eq!(
        history
            .token_info()
            .expect("spending preserved")
            .total_token_usage,
        info.total_token_usage
    );
    let boundary = history.raw_items().len();
    history.set_token_info_with_boundary(Some(info), boundary);
    history.mark_last_usage_context_index_revision(Some(0));
    assert_eq!(
        history.get_total_token_usage(),
        active_count,
        "late old-policy count stays invalid"
    );
}
