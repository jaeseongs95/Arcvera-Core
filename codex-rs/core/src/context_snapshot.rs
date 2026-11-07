// Modified for OpenDIY Codex R39 (2026-11-07); see MODIFICATIONS.md.
//! Aggregate shared history after releasing the session lock, without returning content.
use crate::context::BaseInstructionsFragment;
use codex_history::ResponseItemEnvelope;
use codex_protocol::context_snapshot::*;
use codex_protocol::models::ContentItemMetadata;
use codex_protocol::models::ContentItemProvenance;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::TokenUsageInfo;

pub(crate) fn summarize(
    items: &[ResponseItemEnvelope],
    history_revision: u64,
    usage: Option<TokenUsageInfo>,
    window_number: u64,
    base_instruction_bytes: u64,
) -> ThreadContextSnapshot {
    let mut result = ThreadContextSnapshot {
        scope: ContextSnapshotScope::RetainedLocalHistory,
        history_revision,
        window_number,
        item_count: 0,
        roles: ContextRoleCounts::default(),
        items: ContextItemCounts::default(),
        message_sources: ContextMessageSourceCounts::default(),
        base_instructions: ContextBaseInstructions {
            utf8_bytes: base_instruction_bytes,
            placement: ContextBasePlacement::OutsideHistory,
        },
        tracked_usage: usage.map(|info| info.last_token_usage),
        tracked_usage_provenance: ContextMeasurementStatus::Unverified,
        server_measured_usage: None,
        next_request_tools: ContextMeasurementStatus::NotCaptured,
    };
    for envelope in items {
        let item = &envelope.item;
        result.item_count = result.item_count.saturating_add(1);
        if BaseInstructionsFragment::text_from_item(item).is_some() {
            result.base_instructions.placement = ContextBasePlacement::History;
        }
        let count = match item {
            ResponseItem::Message {
                role,
                content,
                internal_chat_message_metadata_passthrough,
                ..
            } => {
                let count = match role.as_str() {
                    "system" => &mut result.roles.system,
                    "developer" => &mut result.roles.developer,
                    "user" => &mut result.roles.user,
                    "assistant" => &mut result.roles.assistant,
                    _ => &mut result.roles.other,
                };
                *count = count.saturating_add(1);
                count_sources(
                    &mut result.message_sources,
                    content.len(),
                    internal_chat_message_metadata_passthrough
                        .as_ref()
                        .and_then(|m| m.content_item_metadata.as_deref()),
                );
                &mut result.items.messages
            }
            ResponseItem::AgentMessage {
                content,
                internal_chat_message_metadata_passthrough,
                ..
            } => {
                count_sources(
                    &mut result.message_sources,
                    content.len(),
                    internal_chat_message_metadata_passthrough
                        .as_ref()
                        .and_then(|m| m.content_item_metadata.as_deref()),
                );
                &mut result.items.agent_messages
            }
            ResponseItem::AdditionalTools { .. } => &mut result.items.tool_declarations,
            ResponseItem::LocalShellCall { .. }
            | ResponseItem::FunctionCall { .. }
            | ResponseItem::CustomToolCall { .. }
            | ResponseItem::ToolSearchCall { .. }
            | ResponseItem::WebSearchCall { .. }
            | ResponseItem::ImageGenerationCall { .. } => &mut result.items.tool_calls,
            ResponseItem::FunctionCallOutput { .. }
            | ResponseItem::CustomToolCallOutput { .. }
            | ResponseItem::ToolSearchOutput { .. } => &mut result.items.tool_outputs,
            ResponseItem::Reasoning { .. } => &mut result.items.reasoning,
            ResponseItem::Compaction { .. } | ResponseItem::ContextCompaction { .. } => {
                &mut result.items.compaction
            }
            ResponseItem::ConfigurationUpdate { .. } => &mut result.items.configuration,
            _ => &mut result.items.other,
        };
        *count = count.saturating_add(1);
    }
    result
}

fn count_sources(
    counts: &mut ContextMessageSourceCounts,
    part_count: usize,
    metadata: Option<&[ContentItemMetadata]>,
) {
    for index in 0..part_count {
        let count = match metadata
            .and_then(|parts| parts.get(index))
            .map(|part| &part.provenance)
        {
            Some(ContentItemProvenance::DeveloperInstructions { .. }) => {
                &mut counts.developer_instructions
            }
            Some(ContentItemProvenance::Skills) => &mut counts.skills,
            Some(ContentItemProvenance::AgentsMd) => &mut counts.agents_md,
            Some(ContentItemProvenance::Harness { hook: false }) => &mut counts.harness,
            Some(ContentItemProvenance::Harness { hook: true }) => &mut counts.hooks,
            Some(ContentItemProvenance::Tool { .. }) => &mut counts.tool,
            Some(ContentItemProvenance::User) => &mut counts.user,
            Some(ContentItemProvenance::Unknown) | None => &mut counts.unknown,
        };
        *count = count.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_manager::ContextManager;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::InternalChatMessageMetadataPassthrough;
    use codex_utils_output_truncation::TruncationPolicy;

    #[test]
    fn inventory_preserves_unknown_parts_without_exposing_or_mutating_content() {
        let mut history = ContextManager::new();
        history.record_items(
            [ResponseItem::Message {
                id: None,
                encrypted_content: None,
                status: None,
                role: "user".to_string(),
                content: (0..3)
                    .map(|_| ContentItem::InputText {
                        text: "SYNTHETIC_CONTEXT_SECRET_CANARY".to_string(),
                    })
                    .collect(),
                phase: None,
                internal_chat_message_metadata_passthrough: Some(
                    InternalChatMessageMetadataPassthrough {
                        content_item_metadata: Some(vec![
                            ContentItemMetadata::agents_md(),
                            ContentItemMetadata::command_hook(),
                        ]),
                        ..Default::default()
                    },
                ),
            }],
            TruncationPolicy::Bytes(4096),
        );
        let before = history.raw_items().cloned().collect::<Vec<_>>();
        let (items, revision, usage) = history.context_inventory_snapshot();
        let inventory = summarize(&items, revision, usage, 4, 123);
        assert_eq!(inventory.item_count, 1);
        assert_eq!(inventory.roles.user, 1);
        assert_eq!(inventory.message_sources.agents_md, 1);
        assert_eq!(inventory.message_sources.hooks, 1);
        assert_eq!(inventory.message_sources.unknown, 1);
        assert_eq!(inventory.window_number, 4);
        assert_eq!(inventory.base_instructions.utf8_bytes, 123);
        assert_eq!(inventory.server_measured_usage, None);
        assert_eq!(inventory.tracked_usage, None);
        assert!(
            !serde_json::to_string(&inventory)
                .unwrap()
                .contains("CANARY")
        );
        assert_eq!(before, history.raw_items().cloned().collect::<Vec<_>>());
    }

    #[test]
    fn synthetic_usage_is_never_certified_as_server_measurement() {
        let mut history = ContextManager::new();
        history.set_token_usage_full(1000);
        let (items, revision, usage) = history.context_inventory_snapshot();
        let inventory = summarize(&items, revision, usage, 0, 0);
        assert_eq!(inventory.tracked_usage.unwrap().total_tokens, 1000);
        assert_eq!(
            inventory.tracked_usage_provenance,
            ContextMeasurementStatus::Unverified
        );
        assert_eq!(inventory.server_measured_usage, None);
        assert_eq!(
            inventory.next_request_tools,
            ContextMeasurementStatus::NotCaptured
        );
    }
}
