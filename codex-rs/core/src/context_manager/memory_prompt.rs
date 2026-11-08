//! Model-only projection of typed memory payloads. Stored conversations,
//! retained user evidence and reviewer history are never rewritten here.

use crate::context::is_contextual_user_fragment;
use crate::context::is_user_authorization_message;
use codex_context_fragments::AnnotatedContent;
use codex_context_fragments::set_annotated_content;
use codex_context_fragments::to_annotated_content;
use codex_history::ResponseItemEnvelope;
use codex_protocol::models::ContentItemProvenance;
use codex_protocol::models::ResponseItem;
use std::collections::HashMap;
use std::collections::HashSet;

const QUERY_STATE_KIND: &str = "memories.query_state";
const V1_QUERY_STATE_KIND: &str = "memories.query_state.v1";
const VERSIONED_V1_QUERY_STATE_KIND: &str = "memories.query_state.v1.versioned";
const RETRIEVED_KIND: &str = "memories.retrieved";
const LEGACY_INSTRUCTIONS_KIND: &str = "memories.instructions";
const TOOL_SNAPSHOT_KIND: &str = "memories.tool_snapshot";
const MUTATION_BOUNDARY_KIND: &str = "memories.mutation_boundary";

#[derive(Clone, Copy, PartialEq, Eq)]
enum MemoryMode {
    V1,
    VersionedV1,
    V2,
}

fn host_mode(item: &ResponseItem) -> Option<MemoryMode> {
    if is_host_kind(item, VERSIONED_V1_QUERY_STATE_KIND) {
        Some(MemoryMode::VersionedV1)
    } else if is_host_kind(item, V1_QUERY_STATE_KIND) {
        Some(MemoryMode::V1)
    } else if is_host_kind(item, QUERY_STATE_KIND) {
        Some(MemoryMode::V2)
    } else {
        None
    }
}

fn is_host_kind(item: &ResponseItem, expected: &str) -> bool {
    let ResponseItem::Message {
        internal_chat_message_metadata_passthrough: Some(metadata),
        ..
    } = item
    else {
        return false;
    };
    metadata
        .content_item_kinds
        .as_deref()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .any(|(index, kind)| {
            kind.0 == expected
                && metadata
                    .content_item_metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get(index))
                    .is_some_and(|metadata| {
                        matches!(
                            metadata.provenance,
                            ContentItemProvenance::Harness { hook: false }
                        )
                    })
        })
}

fn is_memory_tool_kind(item: &ResponseItem, expected: &str) -> bool {
    let (count, metadata) = match item {
        ResponseItem::FunctionCallOutput {
            output,
            internal_chat_message_metadata_passthrough: Some(metadata),
            ..
        } => {
            let count = match &output.body {
                codex_protocol::models::FunctionCallOutputBody::Text(_) => 1,
                codex_protocol::models::FunctionCallOutputBody::ContentItems(items) => items.len(),
            };
            (count, metadata)
        }
        ResponseItem::Message {
            role,
            content,
            internal_chat_message_metadata_passthrough: Some(metadata),
            ..
        } if role == "user" && expected == MUTATION_BOUNDARY_KIND => (content.len(), metadata),
        _ => return false,
    };
    let Some(kinds) = metadata.content_item_kinds.as_ref() else {
        return false;
    };
    let Some(sources) = metadata.content_item_metadata.as_ref() else {
        return false;
    };
    count > 0
        && kinds.len() == count
        && sources.len() == count
        && kinds.iter().all(|kind| kind.0 == expected)
        && sources
            .iter()
            .all(|source| matches!(source.provenance, ContentItemProvenance::Tool { .. }))
}

pub(super) fn project(items: &mut Vec<ResponseItemEnvelope>) {
    let last_user = items.iter().rposition(|envelope| {
        is_user_authorization_message(&envelope.item)
            && matches!(&envelope.item, ResponseItem::Message { content, internal_chat_message_metadata_passthrough, .. }
                if internal_chat_message_metadata_passthrough.as_ref().is_some_and(|metadata|
                    metadata.content_item_kinds.as_ref().is_some_and(|kinds|
                        kinds.len() == content.len() && kinds.iter().any(|kind| kind.0.starts_with("user."))))
                    || !content.is_empty() && !content.iter().all(is_contextual_user_fragment))
    });
    // A steered input advances the real-user boundary without a new marker.
    // Keep mode and freshness separate. Each call is attributed to the latest
    // explicit host mode at its position, including histories inherited by a
    // fork whose new runtime chooses a different memory version.
    let mut mode = None;
    let modes = items
        .iter()
        .map(|envelope| {
            if let Some(updated) = host_mode(&envelope.item) {
                mode = Some(updated);
            }
            mode
        })
        .collect::<Vec<_>>();
    let last_marker = items
        .iter()
        .rposition(|envelope| host_mode(&envelope.item).is_some());
    let mut versioned_mode = matches!(mode, Some(MemoryMode::VersionedV1 | MemoryMode::V2));
    let fresh_marker = last_marker.is_some_and(|marker| last_user.is_none_or(|user| marker > user));
    let boundary = match (last_user, last_marker) {
        (Some(user), Some(marker)) => Some(user.max(marker)),
        (user, marker) => user.or(marker),
    };
    let Some(mut cutoff) = boundary else {
        return;
    };

    let mut call_starts = HashMap::<String, Option<usize>>::new();
    for (index, envelope) in items.iter().enumerate() {
        if let ResponseItem::FunctionCall { call_id, .. } = &envelope.item {
            call_starts
                .entry(call_id.clone())
                .and_modify(|start| *start = None)
                .or_insert(Some(index));
        }
    }

    if let Some(attempt) = items
        .iter()
        .rposition(|entry| is_memory_tool_kind(&entry.item, MUTATION_BOUNDARY_KIND))
    {
        // An attempted note/forget may invalidate publication before returning
        // an error. This is a freshness fence, never an operation-success ACK.
        if attempt > cutoff {
            versioned_mode = true;
        }
        cutoff = cutoff.max(attempt);
    }

    {
        // After a completed in-turn correction, earlier memory payloads cannot
        // remain current simply because the user turn id is unchanged.
        let note_calls: HashSet<&str> = items
            .iter()
            .enumerate()
            .filter_map(|(_index, envelope)| match &envelope.item {
                ResponseItem::FunctionCall {
                    namespace: Some(namespace),
                    name,
                    call_id,
                    ..
                } if namespace == "memories" && name == "add_ad_hoc_note" => Some(call_id.as_str()),
                _ => None,
            })
            .collect();
        if let Some(completed_edit) = items
            .iter()
            .enumerate()
            .filter_map(|(index, envelope)| match &envelope.item {
                ResponseItem::FunctionCallOutput {
                    call_id: Some(call_id),
                    ..
                } if note_calls.contains(call_id.as_str()) => Some(index),
                _ => None,
            })
            .max()
        {
            // R39 note acceptance creates a publication generation even if
            // this query started as manifest-free legacy v1.
            if completed_edit > cutoff {
                versioned_mode = true;
            }
            cutoff = cutoff.max(completed_edit);
        }
    }
    let mut expired_snapshots = HashSet::new();
    let mut unverified_snapshots = HashSet::new();
    for (index, envelope) in items.iter().enumerate() {
        if !is_memory_tool_kind(&envelope.item, TOOL_SNAPSHOT_KIND) {
            continue;
        }
        let ResponseItem::FunctionCallOutput { call_id, .. } = &envelope.item else {
            continue;
        };
        let start = call_id
            .as_ref()
            .and_then(|id| call_starts.get(id))
            .copied()
            .flatten();
        match start {
            Some(start) if start < cutoff || index < cutoff => {
                expired_snapshots.insert(index);
            }
            Some(start) if start < index => {}
            _ if index < cutoff => {
                expired_snapshots.insert(index);
            }
            _ => {
                unverified_snapshots.insert(index);
            }
        }
    }
    let expired_calls: HashSet<String> = {
        items
            .iter()
            .enumerate()
            .filter_map(|(index, envelope)| match &envelope.item {
                ResponseItem::FunctionCall {
                    namespace: Some(namespace),
                    name,
                    call_id,
                    ..
                } if namespace == "memories"
                    && matches!(name.as_str(), "read" | "search" | "list")
                    && (versioned_mode
                        || matches!(
                            modes[index],
                            Some(MemoryMode::VersionedV1 | MemoryMode::V2)
                        ))
                    && index < cutoff =>
                {
                    (call_starts.get(call_id) == Some(&Some(index))).then(|| call_id.clone())
                }
                ResponseItem::FunctionCallOutput {
                    call_id: Some(call_id),
                    ..
                } if expired_snapshots.contains(&index) => call_starts
                    .get(call_id)
                    .copied()
                    .flatten()
                    .filter(|start| *start < index)
                    .map(|_| call_id.clone()),
                _ => None,
            })
            .collect()
    };

    let mut index = 0;
    items.retain_mut(|envelope| {
        let current_index = index;
        index += 1;
        if expired_snapshots.contains(&current_index)
            || current_index < cutoff && matches!(&envelope.item, ResponseItem::Message { .. }) && is_memory_tool_kind(&envelope.item, MUTATION_BOUNDARY_KIND) {
            return false;
        }
        if unverified_snapshots.contains(&current_index) {
            // Only this typed derived payload is replaced in the model clone.
            // Ambiguous IDs never authorize deleting another call or output.
            if let ResponseItem::FunctionCallOutput { output, internal_chat_message_metadata_passthrough: Some(metadata), .. } = &mut envelope.item {
                output.body = codex_protocol::models::FunctionCallOutputBody::Text("Memory snapshot currentness is unverified: missing or ambiguous call-start evidence. Retrieve current published memory again; this is not an empty-memory result.".into());
                metadata.content_item_kinds = Some(vec![codex_protocol::models::ContentItemKind(TOOL_SNAPSHOT_KIND.into())]);
                metadata.content_item_metadata = Some(vec![codex_protocol::models::ContentItemMetadata::tool(None)]);
            }
        }
        if matches!(&envelope.item,
            ResponseItem::FunctionCall { call_id, .. } | ResponseItem::FunctionCallOutput { call_id: Some(call_id), .. }
                if expired_calls.contains(call_id)) { return false; }
        let remove_retrieved = current_index < cutoff;
        let remove_marker = !fresh_marker || Some(current_index) != last_marker || current_index < cutoff;
        if !(versioned_mode && is_host_kind(&envelope.item, LEGACY_INSTRUCTIONS_KIND)
            || remove_retrieved && is_host_kind(&envelope.item, RETRIEVED_KIND)
            || remove_marker && host_mode(&envelope.item).is_some())
        { return true; }
        let Some(parts) = to_annotated_content(&mut envelope.item) else { return true; };
        let parts = parts.into_iter().filter_map(|part| {
            let (content, kind, metadata) = part.into_parts();
            let host = matches!(metadata.provenance, ContentItemProvenance::Harness { hook: false });
                if host && (versioned_mode && kind.0 == LEGACY_INSTRUCTIONS_KIND
                || remove_retrieved && kind.0 == RETRIEVED_KIND
                || remove_marker && matches!(kind.0.as_str(), QUERY_STATE_KIND | V1_QUERY_STATE_KIND | VERSIONED_V1_QUERY_STATE_KIND))
            { None } else { Some(AnnotatedContent::from_parts(content, kind, metadata)) }
        }).collect::<Vec<_>>();
        let keep = !parts.is_empty();
        set_annotated_content(&mut envelope.item, parts);
        keep
    });
}

#[cfg(test)]
#[path = "memory_prompt_tests.rs"]
mod tests;
