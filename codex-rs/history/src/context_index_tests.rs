// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
use super::*;
use crate::RolloutItem;
use codex_protocol::ResponseItemId;
use codex_protocol::models::ContentItem;
use codex_protocol::models::FunctionCallOutputPayload;

fn message(id: &str, role: &str) -> ResponseItem {
    ResponseItem::Message {
        id: Some(ResponseItemId::from_server(id.to_owned())),
        role: role.to_owned(),
        content: vec![ContentItem::InputText {
            text: "original body must not be copied into the index".to_owned(),
        }],
        status: None,
        encrypted_content: None,
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn call(id: Option<&str>) -> ResponseItem {
    ResponseItem::FunctionCall {
        id: id.map(|id| ResponseItemId::from_server(id.to_owned())),
        call_id: "job".to_owned(),
        name: "exec".to_owned(),
        namespace: None,
        arguments: "{}".to_owned(),
        encrypted_content: None,
        status: None,
        encrypted_function_args: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

fn output(id: Option<&str>) -> ResponseItem {
    ResponseItem::FunctionCallOutput {
        id: id.map(|id| ResponseItemId::from_server(id.to_owned())),
        call_id: Some("job".to_owned()),
        name: None,
        namespace: None,
        output: FunctionCallOutputPayload::from_text("exact tool result".to_owned()),
        internal_chat_message_metadata_passthrough: None,
    }
}

fn create(id: &str) -> ContextIndexOperation {
    ContextIndexOperation::CreateTask {
        task_id: id.to_owned(),
        label: id.to_owned(),
    }
}

fn bind(task: &str, source: &str) -> ContextIndexOperation {
    ContextIndexOperation::BindSources {
        task_id: task.to_owned(),
        source_ids: vec![source.to_owned()],
    }
}

fn completed(id: &str) -> ContextIndexOperation {
    ContextIndexOperation::SetTaskState {
        task_id: id.to_owned(),
        state: ContextTaskState::Completed,
    }
}

fn depends(task: &str, dependency: &str) -> ContextIndexOperation {
    ContextIndexOperation::AddDependency {
        task_id: task.to_owned(),
        dependency_id: dependency.to_owned(),
    }
}

fn apply(index: &mut ContextIndex, operations: Vec<ContextIndexOperation>) -> ContextIndexEvent {
    index
        .apply(index.revision(), operations)
        .expect("valid fixture operations")
}

#[test]
fn completed_unreferenced_detail_is_excluded_and_reopened() {
    let mut index = ContextIndex::default();
    index.register(&message("detail", "assistant"));
    apply(
        &mut index,
        vec![create("task"), bind("task", "detail"), completed("task")],
    );
    assert!(!index.is_active("detail"));
    assert_eq!(index.audit()[0].excluded, ["detail"]);
    apply(
        &mut index,
        vec![ContextIndexOperation::SetTaskState {
            task_id: "task".to_owned(),
            state: ContextTaskState::Active,
        }],
    );
    assert!(index.is_active("detail"));
    assert_eq!(index.audit()[1].reactivated, ["detail"]);
}

#[test]
fn completed_dependencies_live_until_last_active_reference_is_removed() {
    let mut index = ContextIndex::default();
    index.register(&message("detail", "assistant"));
    apply(
        &mut index,
        vec![
            create("consumer1"),
            create("consumer2"),
            create("producer"),
            bind("producer", "detail"),
            completed("producer"),
            depends("consumer1", "producer"),
            depends("consumer2", "producer"),
        ],
    );
    assert!(index.is_active("detail"));
    apply(&mut index, vec![completed("consumer1")]);
    assert!(index.is_active("detail"));
    apply(
        &mut index,
        vec![ContextIndexOperation::RemoveDependency {
            task_id: "consumer2".to_owned(),
            dependency_id: "producer".to_owned(),
        }],
    );
    assert!(!index.is_active("detail"));
}

#[test]
fn rootless_cycles_are_collected_and_cycles_with_a_root_survive() {
    let mut index = ContextIndex::default();
    index.register(&message("detail", "assistant"));
    apply(
        &mut index,
        vec![
            create("a"),
            create("b"),
            create("root"),
            bind("a", "detail"),
            depends("a", "b"),
            depends("b", "a"),
            completed("a"),
            completed("b"),
        ],
    );
    assert!(!index.is_active("detail"));
    apply(&mut index, vec![depends("root", "a")]);
    assert!(index.is_active("detail"));
    apply(&mut index, vec![completed("root")]);
    assert!(!index.is_active("detail"));
    apply(&mut index, vec![depends("a", "a")]);
    assert!(!index.is_active("detail"));
}

#[test]
fn stale_revision_and_late_invalid_operation_leave_everything_unchanged() {
    let mut index = ContextIndex::default();
    apply(&mut index, vec![create("original")]);
    let before = index.clone();
    assert!(index.apply(0, vec![create("new")]).is_err());
    assert_eq!(index, before);
    assert!(
        index
            .apply(
                index.revision(),
                vec![create("new"), depends("new", "missing")]
            )
            .is_err()
    );
    assert_eq!(index, before);
}

#[test]
fn missing_dependency_and_source_are_rejected_atomically() {
    let mut index = ContextIndex::default();
    let before = index.clone();
    assert!(
        index
            .apply(0, vec![create("task"), bind("task", "missing")])
            .is_err()
    );
    assert_eq!(index, before);
    assert!(
        index
            .apply(0, vec![create("task"), depends("task", "missing")])
            .is_err()
    );
    assert_eq!(index, before);
}

#[test]
fn protected_sources_survive_completed_bindings() {
    let mut index = ContextIndex::default();
    for (id, role) in [
        ("old-user", "user"),
        ("latest-correction", "user"),
        ("permission", "developer"),
        ("unknown", "future-role"),
    ] {
        index.register(&message(id, role));
    }
    let opaque = serde_json::from_value::<ResponseItem>(
        serde_json::json!({"type":"compaction","id":"opaque","encrypted_content":"unreadable"}),
    )
    .expect("opaque fixture");
    index.register(&opaque);
    apply(
        &mut index,
        vec![
            create("task"),
            ContextIndexOperation::BindSources {
                task_id: "task".to_owned(),
                source_ids: vec![
                    "old-user".to_owned(),
                    "latest-correction".to_owned(),
                    "permission".to_owned(),
                    "unknown".to_owned(),
                    "opaque".to_owned(),
                ],
            },
            completed("task"),
        ],
    );
    for id in [
        "old-user",
        "latest-correction",
        "permission",
        "unknown",
        "opaque",
    ] {
        assert!(index.is_active(id), "protected source {id}");
    }
    let json = serde_json::to_string(&index).expect("serialize");
    assert!(!json.contains("original body"));
    assert!(!json.contains("unreadable"));
}

#[test]
fn tool_pairs_are_bound_and_excluded_atomically() {
    let mut index = ContextIndex::default();
    index.register(&call(None));
    index.register(&output(None));
    apply(
        &mut index,
        vec![
            create("task"),
            bind("task", "call:job:result"),
            completed("task"),
        ],
    );
    assert_eq!(
        index.sources()["call:job:call"].task_id.as_deref(),
        Some("task")
    );
    assert!(!index.is_active("call:job:call"));
    assert!(!index.is_active("call:job:result"));
}

#[test]
fn pending_tool_group_survives_until_its_real_result() {
    let mut index = ContextIndex::default();
    index.register(&call(Some("call")));
    apply(
        &mut index,
        vec![create("task"), bind("task", "call"), completed("task")],
    );
    assert!(index.is_active("call"));
    assert_eq!(index.active_reason("call"), Some("pending_tool_group"));
    index.register(&output(Some("result")));
    assert!(!index.is_active("call"));
    assert!(!index.is_active("result"));
}

#[test]
fn snapshot_and_typed_event_roundtrip_preserve_exclusions() {
    let mut index = ContextIndex::default();
    index.register(&message("detail", "assistant"));
    let event = apply(
        &mut index,
        vec![create("task"), bind("task", "detail"), completed("task")],
    );
    let persisted =
        serde_json::to_value(RolloutItem::ContextIndex(event.clone())).expect("persist event");
    assert_eq!(persisted["type"], "context_index");
    let decoded: RolloutItem = serde_json::from_value(persisted).expect("restore event");
    let RolloutItem::ContextIndex(decoded) = decoded else {
        panic!("typed event changed variant")
    };
    assert_eq!(decoded, event);
    let mut replay = ContextIndex::default();
    replay.register(&message("detail", "assistant"));
    replay.replay(&decoded).expect("ordered replay");
    assert_eq!(replay, index);
    let mut restored: ContextIndex =
        serde_json::from_str(&serde_json::to_string(&index).expect("serialize"))
            .expect("deserialize");
    restored.rebuild().expect("validate snapshot");
    assert_eq!(restored, index);
    assert!(
        restored.replay(&decoded).is_err(),
        "duplicate event must fail CAS"
    );
}

#[test]
fn source_registration_does_not_invalidate_prefix_policy_revision() {
    let mut index = ContextIndex::default();
    apply(&mut index, vec![create("task")]);
    let revision = index.revision();
    index.register(&message("live-suffix", "user"));
    assert_eq!(index.revision(), revision);
    index.register(&message("live-suffix", "user"));
    assert_eq!(index.sources().len(), 1);
    apply(&mut index, vec![completed("task")]);
    assert_eq!(index.revision(), revision + 1);
}

#[test]
fn deep_dependency_chain_is_iterative() {
    let mut index = ContextIndex::default();
    // Fixtures seed many tasks directly so the test measures graph traversal, not batch overhead.
    for n in 0..4096 {
        let id = format!("t{n}");
        index.tasks.insert(
            id.clone(),
            ContextTask {
                id,
                label: String::new(),
                state: if n == 0 {
                    ContextTaskState::Active
                } else {
                    ContextTaskState::Completed
                },
                dependencies: if n < 4095 {
                    BTreeSet::from([format!("t{}", n + 1)])
                } else {
                    BTreeSet::new()
                },
            },
        );
    }
    index.register(&message("tail", "assistant"));
    index.sources.get_mut("tail").expect("registered").task_id = Some("t4095".to_owned());
    index.rebuild().expect("finite iterative traversal");
    assert!(index.is_active("tail"));
    apply(&mut index, vec![completed("t0")]);
    assert!(!index.is_active("tail"));
}

#[test]
fn deserialized_snapshot_rejects_missing_references() {
    let mut index = ContextIndex::default();
    apply(&mut index, vec![create("task")]);
    let mut json = serde_json::to_value(index).expect("serialize");
    json["tasks"]["task"]["dependencies"] = serde_json::json!(["missing"]);
    let mut restored: ContextIndex = serde_json::from_value(json).expect("structurally valid");
    assert!(restored.rebuild().is_err());
}

#[test]
fn host_checkpoint_roundtrip_and_invalid_shapes_are_atomic() {
    let mut checkpoint_index = ContextIndex::default();
    checkpoint_index.register(&message("archived", "assistant"));
    let ordinary = apply(
        &mut checkpoint_index,
        vec![create("old"), bind("old", "archived"), completed("old")],
    );
    assert!(
        serde_json::to_value(ordinary)
            .expect("event")
            .get("snapshot")
            .is_none()
    );
    let checkpoint = ContextIndexEvent::checkpoint(checkpoint_index.clone()).expect("checkpoint");
    let wire = serde_json::to_string(&checkpoint).expect("serialize");
    assert!(!wire.contains("original body"));
    let decoded: ContextIndexEvent = serde_json::from_str(&wire).expect("deserialize");
    let mut restored = ContextIndex::default();
    apply(&mut restored, vec![create("discarded")]);
    restored
        .replay(&decoded)
        .expect("replace policy after reset");
    assert_eq!(restored, checkpoint_index);
    for shape in ["operations", "revision", "reference", "host_flags"] {
        let mut invalid = serde_json::to_value(&checkpoint).expect("value");
        match shape {
            "operations" => {
                invalid["operations"] =
                    serde_json::to_value(vec![create("mixed")]).expect("operations")
            }
            "revision" => invalid["revision"] = serde_json::json!(99),
            "reference" => {
                invalid["snapshot"]["sources"]["archived"]["task_id"] = serde_json::json!("missing")
            }
            _ => {
                invalid["snapshot"]["sources"]["archived"]["index_policy_mutation"] =
                    serde_json::json!(true)
            }
        }
        let invalid = serde_json::from_value(invalid).expect("structurally valid event");
        let before = restored.clone();
        assert!(restored.replay(&invalid).is_err(), "{shape}");
        assert_eq!(restored, before, "{shape}");
    }
}

#[test]
fn only_completed_successful_policy_apply_groups_leave_projection() {
    for (name, arguments, result, expected_active) in [
        (
            "context_index",
            r#"{"action":"apply"}"#,
            r#"{"committed":true,"revision":1}"#,
            false,
        ),
        (
            "context_index",
            r#"{"action":"apply"}"#,
            r#"{"committed":false,"revision":1}"#,
            true,
        ),
        (
            "context_index",
            r#"{"action":"apply"}"#,
            r#"{"committed":true}"#,
            true,
        ),
        (
            "context_index",
            r#"{"action":"list"}"#,
            r#"{"committed":true,"revision":1}"#,
            true,
        ),
        (
            "context_index",
            r#"{"action":"read"}"#,
            r#"{"committed":true,"revision":1}"#,
            true,
        ),
        (
            "execute",
            r#"{"action":"apply"}"#,
            r#"{"committed":true,"revision":1}"#,
            true,
        ),
        (
            "context_index",
            "malformed",
            r#"{"committed":true,"revision":1}"#,
            true,
        ),
    ] {
        let mut index = ContextIndex::default();
        let mut mutation = call(Some("mutation"));
        let ResponseItem::FunctionCall {
            name: call_name,
            arguments: call_arguments,
            ..
        } = &mut mutation
        else {
            unreachable!()
        };
        *call_name = name.to_owned();
        *call_arguments = arguments.to_owned();
        index.register(&mutation);
        assert!(index.is_active("mutation"), "pending {name}/{arguments}");
        let mut acknowledgement = output(Some("ack"));
        let ResponseItem::FunctionCallOutput { output, .. } = &mut acknowledgement else {
            unreachable!()
        };
        *output = FunctionCallOutputPayload::from_text(result.to_owned());
        index.register(&acknowledgement);
        assert_eq!(
            index.is_active("mutation"),
            expected_active,
            "{name}/{arguments}/{result}"
        );
        assert_eq!(index.is_active("ack"), expected_active);
        assert_eq!(
            index.sources().len(),
            2,
            "original source metadata remains auditable"
        );
        let mut restored: ContextIndex =
            serde_json::from_value(serde_json::to_value(&index).expect("serialize"))
                .expect("deserialize");
        restored.rebuild().expect("restore flags");
        assert_eq!(restored, index);
    }
}

#[test]
fn focus_excludes_unrelated_active_tasks_and_preserves_dependency_closure() {
    let mut index = ContextIndex::default();
    for id in ["focused", "other", "dependency"] {
        index.register(&message(id, "assistant"));
    }
    index.register(&message("correction", "user"));
    apply(
        &mut index,
        vec![
            create("a"),
            create("b"),
            create("d"),
            bind("a", "focused"),
            bind("b", "other"),
            bind("d", "dependency"),
            completed("d"),
            depends("a", "d"),
            ContextIndexOperation::SetFocus {
                task_id: "a".to_owned(),
            },
        ],
    );
    assert!(index.is_active("focused"));
    assert!(index.is_active("dependency"));
    assert!(index.is_active("correction"));
    assert!(!index.is_active("other"));
    assert_eq!(index.tasks()["b"].state, ContextTaskState::Active);
    apply(
        &mut index,
        vec![ContextIndexOperation::SetFocus {
            task_id: "b".to_owned(),
        }],
    );
    assert!(!index.is_active("focused"));
    assert!(!index.is_active("dependency"));
    assert!(index.is_active("other"));
    let mut restored: ContextIndex =
        serde_json::from_value(serde_json::to_value(&index).expect("snapshot"))
            .expect("restore snapshot");
    restored.rebuild().expect("rebuild focused snapshot");
    assert_eq!(restored.focus_task_id(), Some("b"));
    assert_eq!(restored.active_source_ids(), index.active_source_ids());
}

#[test]
fn new_focus_details_are_bound_incrementally_and_tool_results_keep_original_owner() {
    let mut index = ContextIndex::default();
    apply(
        &mut index,
        vec![
            create("a"),
            create("b"),
            ContextIndexOperation::SetFocus {
                task_id: "a".to_owned(),
            },
        ],
    );
    let revision = index.revision();
    index.register(&message("answer-a", "assistant"));
    index.register(&call(Some("call-a")));
    assert_eq!(index.sources()["answer-a"].task_id.as_deref(), Some("a"));
    assert_eq!(index.sources()["call-a"].task_id.as_deref(), Some("a"));
    assert_eq!(index.revision(), revision);
    apply(
        &mut index,
        vec![ContextIndexOperation::SetFocus {
            task_id: "b".to_owned(),
        }],
    );
    assert!(
        index.is_active("call-a"),
        "pending group remains protected across a focus change"
    );
    assert!(!index.is_active("answer-a"));
    index.register(&output(Some("result-a")));
    assert_eq!(index.sources()["result-a"].task_id.as_deref(), Some("a"));
    assert!(!index.is_active("call-a"));
    assert!(!index.is_active("result-a"));
    index.register(&message("answer-b", "assistant"));
    assert_eq!(index.sources()["answer-b"].task_id.as_deref(), Some("b"));
    assert!(index.is_active("answer-b"));
    index.register(&message("latest-user", "user"));
    assert_eq!(index.sources()["latest-user"].task_id, None);
    assert!(index.is_active("latest-user"));
}

#[test]
fn unknown_focus_and_reused_identity_cannot_hide_control() {
    let mut index = ContextIndex::default();
    index.register(&message("reused", "assistant"));
    apply(
        &mut index,
        vec![create("task"), bind("task", "reused"), completed("task")],
    );
    assert!(!index.is_active("reused"));
    let before = index.clone();
    assert!(
        index
            .apply(
                index.revision(),
                vec![ContextIndexOperation::SetFocus {
                    task_id: "missing".to_owned()
                }]
            )
            .is_err()
    );
    assert_eq!(index, before);
    index.register(&message("reused", "user"));
    assert!(index.is_active("reused"));
    assert_eq!(index.sources()["reused"].task_id, None);
    assert_eq!(index.sources()["reused"].kind, ContextSourceKind::Unknown);
}

#[test]
fn context_index_reused_identity_distinct_body_stays_conservative() {
    let mut index = ContextIndex::default();
    let old = message("reused", "assistant");
    let mut new = old.clone();
    let ResponseItem::Message { content, .. } = &mut new else {
        unreachable!()
    };
    *content = vec![ContentItem::InputText { text: "later distinct original".into() }];
    index.register(&old);
    apply(&mut index, vec![create("a"), create("b"), bind("a", "reused"), completed("a"), ContextIndexOperation::SetFocus { task_id: "b".into() }]);
    index.register(&old);
    assert!(!index.is_active("reused"), "identical original retains its binding");
    let mut restored: ContextIndex = serde_json::from_value(serde_json::to_value(&index).unwrap()).unwrap();
    restored.rebuild().unwrap();
    restored.register(&old);
    assert!(!restored.is_active("reused"), "snapshot retains known equal identity");
    index.register(&new);
    assert!(index.is_active("reused"), "a later distinct original cannot inherit exclusion");
    assert_eq!(index.sources()["reused"].kind, ContextSourceKind::Unknown);
    assert_eq!(index.sources()["reused"].task_id, None);
    apply(&mut index, vec![bind("a", "reused")]);
    assert!(index.is_active("reused"), "ambiguous identity remains conservative after bind");
    let json = serde_json::to_value(&index).unwrap();
    assert!(!json.to_string().contains("original body"));
    assert!(!json.to_string().contains("later distinct original"));
    let mut restored: ContextIndex = serde_json::from_value(json).unwrap();
    restored.rebuild().unwrap();
    restored.register(&new);
    assert!(restored.is_active("reused"));
    assert_eq!(restored.sources()["reused"].kind, ContextSourceKind::Unknown);
}

#[test]
fn context_index_legacy_missing_body_identity_does_not_prove_equivalence() {
    let mut index = ContextIndex::default();
    let item = message("legacy", "assistant");
    index.register(&item);
    apply(&mut index, vec![create("a"), bind("a", "legacy"), completed("a")]);
    let mut json = serde_json::to_value(&index).unwrap();
    json["sources"]["legacy"].as_object_mut().unwrap().remove("body_sha256");
    let mut restored: ContextIndex = serde_json::from_value(json).unwrap();
    restored.rebuild().unwrap();
    assert!(!restored.is_active("legacy"));
    restored.register(&item);
    assert!(restored.is_active("legacy"), "missing digest cannot prove original equality");
    assert_eq!(restored.sources()["legacy"].kind, ContextSourceKind::Unknown);
    assert_eq!(restored.sources()["legacy"].task_id, None);
}
