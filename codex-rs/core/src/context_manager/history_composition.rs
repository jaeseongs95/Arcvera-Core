// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use super::{ContextManager, estimate_item_token_count};
use crate::utils::json::serialized_json_bytes;
use serde::Serialize;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::{Arc, Weak};

pub(crate) struct CompositionIdentity {
    items: Weak<Vec<codex_history::ResponseItemEnvelope>>,
    index: Weak<codex_history::context_index::ContextIndex>,
    positions: Weak<BTreeMap<String, Vec<usize>>>,
    active: Weak<BTreeSet<usize>>,
    history_version: u64,
}

impl CompositionIdentity {
    pub(crate) fn matches(&self, history: &ContextManager) -> bool {
        Weak::ptr_eq(&self.items, &Arc::downgrade(&history.items))
            && Weak::ptr_eq(&self.index, &Arc::downgrade(&history.context_index))
            && Weak::ptr_eq(&self.positions, &Arc::downgrade(&history.source_positions))
            && Weak::ptr_eq(&self.active, &Arc::downgrade(&history.active_positions))
            && self.history_version == history.history_version()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionSizes {
    pub resident_serialized_bytes: u64,
    pub selected_resident_serialized_bytes: u64,
    pub excluded_resident_serialized_bytes: u64,
    pub archive_serialized_bytes: Option<u64>,
    pub selected_body_estimated_tokens: u64,
    /// Local heuristic over selected resident items, before request normalization.
    pub estimate_provenance: &'static str,
    pub navigation_estimated_tokens: u64,
    pub base_estimated_tokens: Option<u64>,
    pub tool_estimated_tokens: Option<u64>,
    pub total_estimated_tokens: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositionFragment {
    pub source_id: Option<String>,
    pub source_ordinal: Option<u64>,
    // The position uniquely identifies a resident occurrence only within a snapshot.
    pub resident_occurrence: Option<u64>,
    pub task_id: Option<String>,
    pub kind: Option<String>,
    pub selected: bool,
    pub required: Option<bool>,
    pub selection_reason: Option<String>,
    pub residency: &'static str,
    pub resident_serialized_bytes: Option<u64>,
    pub estimated_tokens: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryComposition {
    pub context_index_revision: u64,
    pub history_version: u64,
    pub sizes: CompositionSizes,
    pub fragments: Vec<CompositionFragment>,
}

fn source_kind(kind: codex_history::context_index::ContextSourceKind) -> String {
    use codex_history::context_index::ContextSourceKind::*;
    match kind {
        User => "user",
        Assistant => "assistant",
        Control => "control",
        Opaque => "opaque",
        ToolCall => "tool_call",
        ToolResult => "tool_result",
        Unknown => "unknown",
    }
    .to_owned()
}

fn required_reason(reason: Option<&str>) -> Option<bool> {
    match reason {
        Some("user_instruction" | "persistent_control") => Some(true),
        _ => None,
    }
}

impl ContextManager {
    pub(crate) fn composition_identity(&self) -> CompositionIdentity {
        CompositionIdentity {
            items: Arc::downgrade(&self.items),
            index: Arc::downgrade(&self.context_index),
            positions: Arc::downgrade(&self.source_positions),
            active: Arc::downgrade(&self.active_positions),
            history_version: self.history_version(),
        }
    }
    // Caller holds the session state lock while capturing this and the cache observation.
    // Errors in serialization, numeric conversion, or inconsistent source mapping propagate.
    pub(crate) async fn history_composition(&self) -> Result<HistoryComposition, String> {
        let mut positions = BTreeMap::new();
        for (id, occurrences) in self.source_positions.iter() {
            for &position in occurrences {
                if !positions.is_empty() && positions.len() % 32 == 0 {
                    tokio::task::yield_now().await;
                }
                if position >= self.items.len() || positions.insert(position, id).is_some() {
                    return Err("invalid context source position mapping".into());
                }
            }
        }
        let mut rows = Vec::with_capacity(self.items.len() + self.context_index.sources().len());
        let (mut all, mut selected) = (0_u64, 0_u64);
        let mut selected_tokens = 0_u64;
        for (position, envelope) in self.items.iter().enumerate() {
            if position != 0 && position % 32 == 0 {
                tokio::task::yield_now().await;
            }
            let id = positions.get(&position).copied();
            let source = id.and_then(|id| self.context_index.sources().get(id));
            if id.is_some() && source.is_none() {
                return Err("resident source is absent from context index".into());
            }
            let bytes =
                u64::try_from(serialized_json_bytes(&envelope.item).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
            let tokens = u64::try_from(estimate_item_token_count(&envelope.item))
                .map_err(|e| e.to_string())?;
            let is_selected = self.active_positions.contains(&position);
            all = all.checked_add(bytes).ok_or("resident byte overflow")?;
            if is_selected {
                selected = selected
                    .checked_add(bytes)
                    .ok_or("selected byte overflow")?;
                selected_tokens = selected_tokens
                    .checked_add(tokens)
                    .ok_or("estimated token overflow")?;
            }
            rows.push(CompositionFragment {
                source_id: id.cloned(),
                source_ordinal: source.map(|s| s.ordinal),
                resident_occurrence: Some(u64::try_from(position).map_err(|e| e.to_string())?),
                task_id: source.and_then(|s| s.task_id.clone()),
                kind: source.map(|s| source_kind(s.kind)),
                selected: is_selected,
                required: required_reason(id.and_then(|id| self.context_index.active_reason(id))),
                selection_reason: id
                    .and_then(|id| self.context_index.active_reason(id))
                    .map(str::to_owned),
                residency: "resident",
                resident_serialized_bytes: Some(bytes),
                estimated_tokens: Some(tokens),
            });
        }
        let has_unmapped_residents = positions.len() != self.items.len();
        for (position, source) in self.context_index.sources().values().enumerate() {
            if position != 0 && position % 32 == 0 {
                tokio::task::yield_now().await;
            }
            if !self.source_positions.contains_key(&source.id) {
                rows.push(CompositionFragment {
                    source_id: Some(source.id.clone()),
                    source_ordinal: Some(source.ordinal),
                    resident_occurrence: None,
                    task_id: source.task_id.clone(),
                    kind: Some(source_kind(source.kind)),
                    selected: self.context_index.is_active(&source.id),
                    required: required_reason(self.context_index.active_reason(&source.id)),
                    selection_reason: self
                        .context_index
                        .active_reason(&source.id)
                        .map(str::to_owned),
                    residency: if has_unmapped_residents {
                        "unknown"
                    } else {
                        "notResident"
                    },
                    resident_serialized_bytes: None,
                    estimated_tokens: None,
                });
            }
        }
        let navigation = self.context_index_manifest().map_or(Ok(0), |item| {
            u64::try_from(estimate_item_token_count(&item.item)).map_err(|e| e.to_string())
        })?;
        Ok(HistoryComposition {
            context_index_revision: self.context_index_revision(),
            history_version: self.history_version(),
            sizes: CompositionSizes {
                resident_serialized_bytes: all,
                selected_resident_serialized_bytes: selected,
                excluded_resident_serialized_bytes: all - selected,
                archive_serialized_bytes: None,
                selected_body_estimated_tokens: selected_tokens,
                estimate_provenance: "selected_resident_before_request_normalization",
                navigation_estimated_tokens: navigation,
                base_estimated_tokens: None,
                tool_estimated_tokens: None,
                total_estimated_tokens: None,
            },
            fragments: rows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::models::ResponseItem;
    use codex_utils_output_truncation::TruncationPolicy;

    #[tokio::test]
    async fn duplicate_resident_occurrences_have_distinct_positions_and_exact_sizes() {
        let item: ResponseItem = serde_json::from_value(serde_json::json!({
            "type": "message", "id": "same-id", "role": "assistant",
            "content": [{"type": "output_text", "text": "한글"}]
        }))
        .unwrap();
        let mut history = ContextManager::new();
        history.record_items(
            [item.clone(), item.clone()].iter(),
            TruncationPolicy::Bytes(usize::MAX),
        );
        let view = history.history_composition().await.unwrap();
        let bytes = serde_json::to_vec(&item).unwrap().len() as u64;
        assert_eq!(view.sizes.resident_serialized_bytes, 2 * bytes);
        assert_eq!(view.sizes.selected_resident_serialized_bytes, 2 * bytes);
        assert_eq!(view.sizes.excluded_resident_serialized_bytes, 0);
        assert_eq!(view.fragments[0].source_id, view.fragments[1].source_id);
        assert_eq!(view.fragments[0].resident_occurrence, Some(0));
        assert_eq!(view.fragments[1].resident_occurrence, Some(1));
        assert_eq!(view.sizes.archive_serialized_bytes, None);
        assert_eq!(view.sizes.total_estimated_tokens, None);
    }

    #[tokio::test]
    async fn weak_identity_rejects_append_and_does_not_retain_history() {
        let item: ResponseItem = serde_json::from_value(serde_json::json!({
            "type":"message", "id":"a", "role":"assistant",
            "content":[{"type":"output_text", "text":"body"}]
        }))
        .unwrap();
        let mut history = ContextManager::new();
        history.record_items([item.clone()].iter(), TruncationPolicy::Bytes(usize::MAX));
        let identity = history.composition_identity();
        assert!(identity.matches(&history));
        let previous = Arc::downgrade(&history.items);
        history.record_items([item].iter(), TruncationPolicy::Bytes(usize::MAX));
        assert!(!identity.matches(&history));
        assert!(
            previous.upgrade().is_none(),
            "identity must not keep old body alive"
        );
    }

    #[tokio::test]
    async fn policy_excludes_only_unrelated_resident_bytes() {
        use codex_history::context_index::ContextIndexOperation as Op;
        let make = |id: &str, body: &str| -> ResponseItem {
            serde_json::from_value(serde_json::json!({
                "type":"message", "id":id, "role":"assistant",
                "content":[{"type":"output_text", "text":body}]
            }))
            .unwrap()
        };
        let a = make("a", "선택");
        let b = make("b", "제외");
        let mut history = ContextManager::new();
        history.record_items(
            [a.clone(), b.clone()].iter(),
            TruncationPolicy::Bytes(usize::MAX),
        );
        history
            .apply_context_index(
                0,
                vec![
                    Op::CreateTask {
                        task_id: "one".into(),
                        label: "one".into(),
                    },
                    Op::CreateTask {
                        task_id: "two".into(),
                        label: "two".into(),
                    },
                    Op::BindSources {
                        task_id: "one".into(),
                        source_ids: vec!["a".into()],
                    },
                    Op::BindSources {
                        task_id: "two".into(),
                        source_ids: vec!["b".into()],
                    },
                    Op::SetFocus {
                        task_id: "one".into(),
                    },
                ],
            )
            .unwrap();
        let page = history.history_composition().await.unwrap();
        let a_size = serde_json::to_vec(&a).unwrap().len() as u64;
        let b_size = serde_json::to_vec(&b).unwrap().len() as u64;
        assert_eq!(page.sizes.selected_resident_serialized_bytes, a_size);
        assert_eq!(page.sizes.excluded_resident_serialized_bytes, b_size);
        assert_eq!(page.sizes.resident_serialized_bytes, a_size + b_size);
        assert!(page.fragments[0].selected);
        assert!(!page.fragments[1].selected);
    }

    #[tokio::test]
    async fn anonymous_restored_sources_do_not_claim_resident_linkage() {
        let item: ResponseItem = serde_json::from_value(serde_json::json!({
            "type":"message", "role":"assistant",
            "content":[{"type":"output_text", "text":"anonymous"}]
        }))
        .unwrap();
        let mut history = ContextManager::new();
        history.replace(vec![item.clone(), item]);
        history
            .restore_context_index(codex_history::context_index::ContextIndex::default())
            .unwrap();
        let page = history.history_composition().await.unwrap();
        let residents: Vec<_> = page
            .fragments
            .iter()
            .filter(|row| row.residency == "resident")
            .collect();
        assert_eq!(residents.len(), 2);
        assert!(residents.iter().all(|row| row.source_id.is_none()));
        assert_ne!(
            residents[0].resident_occurrence,
            residents[1].resident_occurrence
        );
        assert!(
            page.fragments
                .iter()
                .filter(|row| row.source_id.is_some())
                .all(|row| row.residency == "unknown")
        );
        assert_eq!(page.sizes.archive_serialized_bytes, None);
    }
}
