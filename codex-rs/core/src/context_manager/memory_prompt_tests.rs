use super::*;
use codex_context_fragments::message_from_parts;
use codex_protocol::models::ContentItemKind;
use codex_protocol::models::ContentItemMetadata;
use codex_protocol::models::FunctionCallOutputPayload;

fn message(
    role: &str,
    text: &str,
    kind: &str,
    metadata: ContentItemMetadata,
) -> ResponseItemEnvelope {
    message_from_parts(
        role,
        vec![AnnotatedContent::text(
            text,
            ContentItemKind(kind.to_string()),
            metadata,
        )],
    )
    .into()
}

fn user(text: &str) -> ResponseItemEnvelope {
    message("user", text, "user.text", ContentItemMetadata::user())
}
fn host(text: &str, kind: &str) -> ResponseItemEnvelope {
    message("user", text, kind, ContentItemMetadata::harness())
}
fn wire(items: &[ResponseItemEnvelope]) -> String {
    serde_json::to_string(
        &items
            .iter()
            .map(|envelope| &envelope.item)
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn call(name: &str, call_id: &str) -> ResponseItemEnvelope {
    ResponseItem::FunctionCall {
        encrypted_content: None,
        status: None,
        id: None,
        name: name.to_string(),
        namespace: Some("memories".to_string()),
        arguments: "{}".to_string(),
        encrypted_function_args: None,
        call_id: call_id.to_string(),
        internal_chat_message_metadata_passthrough: None,
    }
    .into()
}
fn output(text: &str, call_id: &str) -> ResponseItemEnvelope {
    ResponseItem::FunctionCallOutput {
        id: None,
        call_id: Some(call_id.to_string()),
        name: None,
        namespace: None,
        output: FunctionCallOutputPayload::from_text(text.to_string()),
        internal_chat_message_metadata_passthrough: None,
    }
    .into()
}

fn typed_tool_output(text: &str, call_id: &str, kind: &str) -> ResponseItemEnvelope {
    let mut output = output(text, call_id);
    if let ResponseItem::FunctionCallOutput {
        internal_chat_message_metadata_passthrough,
        ..
    } = &mut output.item
    {
        *internal_chat_message_metadata_passthrough = Some(
            codex_protocol::models::InternalChatMessageMetadataPassthrough {
                content_item_kinds: Some(vec![ContentItemKind(kind.to_string())]),
                content_item_metadata: Some(vec![ContentItemMetadata::tool(None)]),
                ..Default::default()
            },
        );
    }
    output
}

#[test]
fn native_memory_snapshot_expires_by_typed_output_and_paired_id_without_tool_name_guessing() {
    let mut native_call = call("arbitrary-control-name", "native-view");
    if let ResponseItem::FunctionCall { namespace, .. } = &mut native_call.item {
        *namespace = Some("arbitrary-client".into());
    }
    let mut orphan = typed_tool_output("ORPHAN_DERIVED_SNAPSHOT", "missing", TOOL_SNAPSHOT_KIND);
    if let ResponseItem::FunctionCallOutput { call_id, .. } = &mut orphan.item {
        *call_id = None;
    }
    let mut fake = typed_tool_output("USER_SUPPLIED_LIFETIME_KIND", "fake", TOOL_SNAPSHOT_KIND);
    if let ResponseItem::FunctionCallOutput {
        internal_chat_message_metadata_passthrough: Some(metadata),
        ..
    } = &mut fake.item
    {
        metadata.content_item_metadata = Some(vec![ContentItemMetadata::user()]);
    }
    let mut items = vec![
        user("first query"),
        host("v2", QUERY_STATE_KIND),
        native_call,
        typed_tool_output("OLD_NATIVE_VIEW_CANARY", "native-view", TOOL_SNAPSHOT_KIND),
        orphan,
        output(
            "UNCLASSIFIED_MCP_TEXT memory_context snapshot _meta",
            "unrelated-mcp",
        ),
        fake,
        user("next query"),
        host("new-v2", QUERY_STATE_KIND),
        call("arbitrary-new-view", "new-view"),
        typed_tool_output("CURRENT_NATIVE_VIEW", "new-view", TOOL_SNAPSHOT_KIND),
    ];
    project(&mut items);
    let prompt = wire(&items);
    assert!(!prompt.contains("OLD_NATIVE_VIEW_CANARY"));
    assert!(!prompt.contains("ORPHAN_DERIVED_SNAPSHOT"));
    assert!(!prompt.contains("native-view"));
    assert!(prompt.contains("UNCLASSIFIED_MCP_TEXT memory_context snapshot _meta"));
    assert!(prompt.contains("USER_SUPPLIED_LIFETIME_KIND"));
    assert!(prompt.contains("CURRENT_NATIVE_VIEW"));
}

#[test]
fn content_free_nested_mutation_fence_expires_late_snapshot_by_actual_call_start() {
    let mut items = vec![
        user("current request"),
        host("OLD_READY_STATE", QUERY_STATE_KIND),
        call("native-view", "started-before-mutation"),
        // Native mutation boundary survives the nested result's JSON conversion.
        message(
            "user",
            "<external_memory_mutation>effect unknown; opaque-call-id</external_memory_mutation>",
            MUTATION_BOUNDARY_KIND,
            ContentItemMetadata::tool(None),
        ),
        typed_tool_output(
            "LATE_OLD_VIEW_CANARY",
            "started-before-mutation",
            TOOL_SNAPSHOT_KIND,
        ),
        call("fresh-native-view", "started-after-mutation"),
        typed_tool_output(
            "CURRENT_VIEW_AFTER_MUTATION",
            "started-after-mutation",
            TOOL_SNAPSHOT_KIND,
        ),
        output("MIXED_CODE_OUTPUT_COPY_REMAINS_UNCLASSIFIED", "code-cell"),
    ];
    let before = wire(&items);
    let mut projected = items.clone();
    project(&mut projected);
    let prompt = wire(&projected);
    assert!(!prompt.contains("LATE_OLD_VIEW_CANARY"));
    assert!(!prompt.contains("started-before-mutation"));
    assert!(!prompt.contains("OLD_READY_STATE"));
    assert!(prompt.contains("CURRENT_VIEW_AFTER_MUTATION"));
    assert!(prompt.contains("effect unknown"));
    assert!(prompt.contains("MIXED_CODE_OUTPUT_COPY_REMAINS_UNCLASSIFIED"));
    assert_eq!(wire(&items), before);
    items.push(user("later unrelated request"));
    project(&mut items);
    assert!(
        !wire(&items).contains("effect unknown"),
        "old content-free fences do not accumulate in model input"
    );
}

#[test]
fn ambiguous_or_missing_snapshot_call_ids_are_unverified_without_deleting_other_calls() {
    let mut items = vec![
        user("current request"),
        host("v2", QUERY_STATE_KIND),
        call("first-tool", "duplicate"),
        call("other-tool", "duplicate"),
        typed_tool_output(
            "AMBIGUOUS_SNAPSHOT_CONTENT",
            "duplicate",
            TOOL_SNAPSHOT_KIND,
        ),
        output("UNRELATED_OUTPUT_SAME_DUPLICATE_ID", "duplicate"),
        typed_tool_output(
            "MISSING_START_SNAPSHOT_CONTENT",
            "missing-start",
            TOOL_SNAPSHOT_KIND,
        ),
    ];
    project(&mut items);
    let prompt = wire(&items);
    assert!(!prompt.contains("AMBIGUOUS_SNAPSHOT_CONTENT"));
    assert!(!prompt.contains("MISSING_START_SNAPSHOT_CONTENT"));
    assert!(prompt.contains("currentness is unverified"));
    assert!(prompt.contains("UNRELATED_OUTPUT_SAME_DUPLICATE_ID"));
    assert!(prompt.contains("first-tool"));
    assert!(prompt.contains("other-tool"));
}

#[test]
fn failed_native_mutation_attempt_invalidates_old_state_without_claiming_success() {
    let mut mutation = typed_tool_output(
        "RPC_FAILED_OUTCOME_UNCONFIRMED",
        "native-forget",
        MUTATION_BOUNDARY_KIND,
    );
    if let ResponseItem::FunctionCallOutput { output, .. } = &mut mutation.item {
        output.success = Some(false);
    }
    let mut items = vec![
        user("forget the prior memory"),
        host("OLD_READY_PUBLICATION_STATE", QUERY_STATE_KIND),
        host("OLD_AUTO_DERIVED_CANARY", RETRIEVED_KIND),
        call("read", "before-native-attempt"),
        output("OLD_DEDICATED_CANARY", "before-native-attempt"),
        typed_tool_output("OLD_NATIVE_CANARY", "old-native", TOOL_SNAPSHOT_KIND),
        mutation,
    ];
    project(&mut items);
    let prompt = wire(&items);
    for expired in [
        "OLD_READY_PUBLICATION_STATE",
        "OLD_AUTO_DERIVED_CANARY",
        "OLD_DEDICATED_CANARY",
        "OLD_NATIVE_CANARY",
    ] {
        assert!(!prompt.contains(expired));
    }
    assert!(prompt.contains("RPC_FAILED_OUTCOME_UNCONFIRMED"));
    assert!(prompt.contains("forget the prior memory"));
}

#[test]
fn current_query_keeps_relevant_memory_and_drops_older_derived_payload_only() {
    let original = vec![
        host("LEGACY_DERIVED_CANARY", LEGACY_INSTRUCTIONS_KIND),
        user("original user fact must stay"),
        host("old-v2-boundary", QUERY_STATE_KIND),
        host("FORGOTTEN_DERIVED_CANARY", RETRIEVED_KIND),
        call("read", "old-memory-read"),
        output("FORGOTTEN_TOOL_CANARY", "old-memory-read"),
        user("previous R39 decision"),
        host("current-v2-boundary", QUERY_STATE_KIND),
        host("RELEVANT_CURRENT_DECISION", RETRIEVED_KIND),
        call("search", "current-memory-search"),
        output("CURRENT_TOOL_EVIDENCE", "current-memory-search"),
    ];
    let before = wire(&original);
    let mut projected = original.clone();
    project(&mut projected);
    let prompt = wire(&projected);
    assert!(prompt.contains("RELEVANT_CURRENT_DECISION"));
    assert!(prompt.contains("CURRENT_TOOL_EVIDENCE"));
    assert!(prompt.contains("original user fact must stay"));
    assert!(!prompt.contains("FORGOTTEN_DERIVED_CANARY"));
    assert!(!prompt.contains("FORGOTTEN_TOOL_CANARY"));
    assert!(!prompt.contains("LEGACY_DERIVED_CANARY"));
    assert!(!prompt.contains("old-memory-read"));
    assert_eq!(wire(&original), before, "stored snapshot remains unchanged");
}

#[test]
fn unrelated_next_query_has_no_memory_and_original_user_text_is_not_redacted() {
    let original_canary = "FORGOTTEN_ORIGINAL_USER_CANARY memories.retrieved";
    let mut items = vec![
        user(original_canary),
        host("old-boundary", QUERY_STATE_KIND),
        message(
            "assistant",
            "ORIGINAL_ASSISTANT_RECOLLECTION_CANARY",
            "assistant.text",
            ContentItemMetadata::default(),
        ),
        host("FORGOTTEN_DERIVED_CANARY", RETRIEVED_KIND),
        user("Translate this independent sentence"),
        host("new-boundary", QUERY_STATE_KIND),
        message(
            "user",
            "USER_SUPPLIED_KIND_CANARY",
            RETRIEVED_KIND,
            ContentItemMetadata::user(),
        ),
    ];
    project(&mut items);
    let prompt = wire(&items);
    assert!(prompt.contains(original_canary));
    assert!(prompt.contains("ORIGINAL_ASSISTANT_RECOLLECTION_CANARY"));
    assert!(prompt.contains("USER_SUPPLIED_KIND_CANARY"));
    assert!(!prompt.contains("FORGOTTEN_DERIVED_CANARY"));
}

#[test]
fn steered_real_input_without_a_new_marker_still_expires_previous_memory() {
    let mut items = vec![
        host("V2_LEGACY_DERIVED_CANARY", LEGACY_INSTRUCTIONS_KIND),
        user("previous R39 context"),
        host("v2 marker before steering", QUERY_STATE_KIND),
        host("STEERED_DERIVED_CANARY", RETRIEVED_KIND),
        call("read", "before-steering"),
        output("STEERED_TOOL_CANARY", "before-steering"),
        // session/turn.rs records this pending input directly and does not
        // rerun TurnInputContributor at that boundary.
        user("new instruction while the same turn is active"),
    ];
    project(&mut items);
    let prompt = wire(&items);
    assert!(!prompt.contains("STEERED_DERIVED_CANARY"));
    assert!(!prompt.contains("STEERED_TOOL_CANARY"));
    assert!(!prompt.contains("V2_LEGACY_DERIVED_CANARY"));
    assert!(prompt.contains("new instruction while the same turn is active"));
}

#[test]
fn explicitly_annotated_user_quoting_context_markers_still_advances_the_query_boundary() {
    let quoted = "<external_memory_query>user supplied quoted text</external_memory_query>";
    let mut items = vec![
        user("prior request"),
        host("old-v2", QUERY_STATE_KIND),
        host("OLD_DERIVED_BEFORE_QUOTED_USER", RETRIEVED_KIND),
        call("read", "prior-read"),
        output("OLD_TOOL_BEFORE_QUOTED_USER", "prior-read"),
        user(quoted),
    ];
    project(&mut items);
    let prompt = wire(&items);
    assert!(prompt.contains(quoted));
    assert!(!prompt.contains("OLD_DERIVED_BEFORE_QUOTED_USER"));
    assert!(!prompt.contains("OLD_TOOL_BEFORE_QUOTED_USER"));
}

#[test]
fn unannotated_legacy_external_context_is_not_a_new_user_query() {
    let mut legacy = user("<external_memory_query>legacy context only</external_memory_query>");
    if let ResponseItem::Message {
        internal_chat_message_metadata_passthrough,
        ..
    } = &mut legacy.item
    {
        *internal_chat_message_metadata_passthrough = None;
    }
    let mut items = vec![
        user("request"),
        host("v2", QUERY_STATE_KIND),
        host("CURRENT_DERIVED_STILL_RELEVANT", RETRIEVED_KIND),
        legacy,
    ];
    project(&mut items);
    assert!(wire(&items).contains("CURRENT_DERIVED_STILL_RELEVANT"));
}

#[test]
fn v2_to_v1_inherited_history_expires_only_v2_calls_and_keeps_v1_behavior() {
    let mut items = vec![
        user("old v2 query"),
        host("v2", QUERY_STATE_KIND),
        host("OLD_V2_DERIVED_CANARY", RETRIEVED_KIND),
        call("read", "v2-read"),
        output("OLD_V2_TOOL_CANARY", "v2-read"),
        user("new fork using v1"),
        host("v1", V1_QUERY_STATE_KIND),
        message(
            "developer",
            "CURRENT_V1_SUMMARY_CANARY",
            LEGACY_INSTRUCTIONS_KIND,
            ContentItemMetadata::harness(),
        ),
        call("read", "v1-read"),
        output("V1_TOOL_CANARY", "v1-read"),
        // A user cannot change mode by quoting host marker text or its kind.
        user("<external_memory_query>memories.query_state v2</external_memory_query>"),
    ];
    project(&mut items);
    let prompt = wire(&items);
    assert!(!prompt.contains("OLD_V2_DERIVED_CANARY"));
    assert!(!prompt.contains("OLD_V2_TOOL_CANARY"));
    assert!(prompt.contains("CURRENT_V1_SUMMARY_CANARY"));
    assert!(prompt.contains("V1_TOOL_CANARY"));
    assert!(prompt.contains("v1-read"));
    assert!(
        prompt.contains("<external_memory_query>memories.query_state v2</external_memory_query>")
    );
}

#[test]
fn legacy_to_versioned_v1_expires_earlier_typed_memory_calls() {
    let mut items = vec![
        host("OLD_LEGACY_V1_SUMMARY_CANARY", LEGACY_INSTRUCTIONS_KIND),
        user("legacy query"),
        host("legacy-v1", V1_QUERY_STATE_KIND),
        call("read", "legacy-read"),
        output("LEGACY_TOOL_FORGOTTEN_CANARY", "legacy-read"),
        // Current versioned v1 acceptance supersedes earlier derived evidence.
        user("correct the prior memory"),
        host("pending versioned-v1", VERSIONED_V1_QUERY_STATE_KIND),
        ResponseItem::FunctionCall {
            encrypted_content: None,
            status: None,
            id: None,
            name: "read".to_string(),
            namespace: Some("unrelated".to_string()),
            arguments: "{}".to_string(),
            encrypted_function_args: None,
            call_id: "unrelated-read".to_string(),
            internal_chat_message_metadata_passthrough: None,
        }
        .into(),
        output("UNRELATED_TOOL_EVIDENCE", "unrelated-read"),
    ];
    project(&mut items);
    let prompt = wire(&items);
    assert!(!prompt.contains("OLD_LEGACY_V1_SUMMARY_CANARY"));
    assert!(!prompt.contains("LEGACY_TOOL_FORGOTTEN_CANARY"));
    assert!(prompt.contains("UNRELATED_TOOL_EVIDENCE"));
    assert!(prompt.contains("correct the prior memory"));
}

#[test]
fn v1_shared_instruction_kind_and_unrelated_tools_are_preserved() {
    let mut items = vec![
        message(
            "developer",
            "V1_MEMORY_CANARY",
            LEGACY_INSTRUCTIONS_KIND,
            ContentItemMetadata::harness(),
        ),
        user("first"),
        call("read", "v1-read"),
        output("V1_TOOL_CANARY", "v1-read"),
        user("second"),
    ];
    let before = wire(&items);
    project(&mut items);
    assert_eq!(wire(&items), before);
}

#[test]
fn completed_same_turn_edit_expires_earlier_memory_without_claiming_out_of_band_refresh() {
    let mut items = vec![
        user("Correct the previous R39 memory"),
        host("v2", QUERY_STATE_KIND),
        host("BEFORE_CORRECTION_CANARY", RETRIEVED_KIND),
        call("read", "before-edit"),
        output("BEFORE_EDIT_TOOL_CANARY", "before-edit"),
        call("add_ad_hoc_note", "edit"),
        output("{}", "edit"),
        call("read", "after-edit"),
        output("memory is unavailable: Pending", "after-edit"),
    ];
    project(&mut items);
    let prompt = wire(&items);
    assert!(!prompt.contains("BEFORE_CORRECTION_CANARY"));
    assert!(!prompt.contains("BEFORE_EDIT_TOOL_CANARY"));
    assert!(prompt.contains("Pending"));
    assert!(prompt.contains("add_ad_hoc_note"));
}

#[tokio::test]
async fn real_prompt_projection_preserves_original_context_manager_snapshot() {
    let mut history = crate::context_manager::ContextManager::new();
    let original = vec![
        user("original user evidence"),
        host("old-v2", QUERY_STATE_KIND),
        host("DERIVED_ONLY_FORGET_CANARY", RETRIEVED_KIND),
        user("an unrelated new query"),
        host("new-v2", QUERY_STATE_KIND),
    ];
    history.record_items(
        original.iter().map(|envelope| &envelope.item),
        codex_utils_output_truncation::TruncationPolicy::Bytes(64_000),
    );
    let before = history.raw_items().cloned().collect::<Vec<_>>();
    let prompt = history
        .clone()
        .for_prompt(
            &codex_protocol::openai_models::default_input_modalities(),
            &codex_attachment_store::InlineAttachmentStore,
        )
        .await;
    let prompt = serde_json::to_string(&prompt).unwrap();
    assert!(!prompt.contains("DERIVED_ONLY_FORGET_CANARY"));
    assert!(prompt.contains("original user evidence"));
    assert_eq!(before, history.raw_items().cloned().collect::<Vec<_>>());
}
