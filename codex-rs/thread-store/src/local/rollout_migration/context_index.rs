// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! Preserves lifetime policy when migration removes historical rollback markers.
//!
//! Live Core destructively resets its index on nonzero rollback. The canonical UI planner has
//! already selected surviving response sources; retain only their body-free descriptors before
//! the last reset, then replay subsequent policy and checkpoints. One host snapshot represents
//! the final policy; canonical original response payloads are neither copied nor synthesized.

use codex_history::context_index::ContextIndex;
use codex_history::context_index::ContextIndexError;
use codex_history::context_index::ContextIndexEvent;
use codex_history::context_index::ContextIndexOperation;
use codex_history::context_index::ContextSource;
use codex_history::context_index::ContextSourceKind;
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::EventMsg;
use codex_rollout::RolloutItem;
use std::collections::BTreeSet;

use super::migration_error;
use super::rollback;
use crate::ThreadStoreResult;

pub(super) struct IndexMigration {
    last_reset: Option<usize>,
    index: ContextIndex,
    window: Vec<ContextSource>,
    saw_index: bool,
    original_index: ContextIndex,
    original_window: Vec<ValidationSource>,
    uncertain_legacy_sources: BTreeSet<String>,
}

/// Raw replay validation needs identities and boundary facts, never original message bodies.
struct ValidationSource {
    descriptor: ContextSource,
    boundary: bool,
    pre_turn_context: bool,
    acceptance_order: Option<u64>,
    accepted_detail: bool,
}

impl ValidationSource {
    fn from_item(
        item: &ResponseItem,
        compaction_output: bool,
        acceptance_order: Option<u64>,
    ) -> Self {
        Self {
            descriptor: ContextIndex::describe(item, compaction_output),
            boundary: !compaction_output && rollback::counts_as_boundary(item),
            pre_turn_context: rollback::is_pre_turn_context_update(item),
            acceptance_order,
            accepted_detail: matches!(item, ResponseItem::Message { role, .. } if role == "assistant")
                || matches!(item, ResponseItem::FunctionCall { .. }),
        }
    }
}

impl IndexMigration {
    pub(super) fn new(last_reset: Option<usize>) -> Self {
        Self {
            last_reset,
            index: ContextIndex::default(),
            window: Vec::new(),
            saw_index: false,
            original_index: ContextIndex::default(),
            original_window: Vec::new(),
            uncertain_legacy_sources: BTreeSet::new(),
        }
    }

    /// Validate every original policy delta before canonical pruning can remove it. Replaying
    /// only surviving UI records would both reject valid bindings to removed turns and conceal
    /// invalid deltas whose sticky Core diagnostic outlives a destructive rollback reset.
    pub(super) fn observe_original(&mut self, item: &RolloutItem) -> ThreadStoreResult<()> {
        match item {
            RolloutItem::ContextIndex(event) => {
                self.saw_index = true;
                if event.operations.iter().any(|operation| {
                    matches!(operation,
                    ContextIndexOperation::BindSources { source_ids, .. }
                    if source_ids.iter().any(|id| self.uncertain_legacy_sources.contains(id)))
                }) {
                    return Err(migration_error(
                        "cannot validate a binding to a source retained only conservatively across legacy text compaction; original rollout retained",
                    ));
                }
                replay_with_residents(
                    &mut self.original_index,
                    event,
                    self.original_window
                        .iter()
                        .map(|source| source.descriptor.id.as_str()),
                )
                .map_err(|error| {
                    migration_error(format!(
                        "invalid original context index event; original rollout retained: {error}"
                    ))
                })?;
            }
            RolloutItem::ResponseItem(envelope) => {
                let metadata = envelope.metadata.as_ref();
                let source = ValidationSource::from_item(
                    &envelope.item,
                    metadata.is_some_and(|metadata| metadata.compaction_output),
                    metadata
                        .filter(|metadata| !metadata.inherited_user_message)
                        .and_then(|metadata| metadata.user_input_order),
                );
                self.original_index.register_descriptor(&source.descriptor);
                self.original_window.push(source);
            }
            RolloutItem::InterAgentCommunication(communication) => {
                let source =
                    ValidationSource::from_item(&communication.to_model_input_item(), false, None);
                self.original_index.register_descriptor(&source.descriptor);
                self.original_window.push(source);
            }
            RolloutItem::Compacted(compacted) => {
                if let Some(history) = &compacted.replacement_history {
                    self.uncertain_legacy_sources.clear();
                    self.original_window = history
                        .iter()
                        .map(|envelope| {
                            let metadata = envelope.metadata.as_ref();
                            ValidationSource::from_item(
                                &envelope.item,
                                metadata.is_some_and(|metadata| metadata.compaction_output),
                                metadata
                                    .filter(|metadata| !metadata.inherited_user_message)
                                    .and_then(|metadata| metadata.user_input_order),
                            )
                        })
                        .collect();
                    let snapshot = compacted
                        .resume_metadata
                        .as_ref()
                        .and_then(|metadata| metadata.context_index.as_ref());
                    self.saw_index |= snapshot.is_some();
                    self.original_index = snapshot.cloned().unwrap_or_default();
                    self.original_index.rebuild().map_err(migration_error)?;
                    self.original_index.reconcile_checkpoint_descriptors(
                        self.original_window.iter().map(|source| &source.descriptor),
                    );
                } else {
                    if self.saw_index {
                        return Err(migration_error(
                            "indexed rollout followed by legacy text compaction without replacement history is unsupported for rollback migration; original rollout retained",
                        ));
                    }
                    self.original_window.retain(|source| {
                        source.descriptor.kind == ContextSourceKind::User
                            && source.descriptor.response_item_id.is_some()
                    });
                    self.uncertain_legacy_sources.extend(
                        self.original_window
                            .iter()
                            .map(|source| source.descriptor.id.clone()),
                    );
                    self.reset_original_index();
                }
            }
            RolloutItem::EventMsg(EventMsg::ThreadRolledBack(rollback))
                if rollback.num_turns > 0 =>
            {
                let positions: Vec<_> = self
                    .original_window
                    .iter()
                    .enumerate()
                    .filter_map(|(index, source)| source.boundary.then_some(index))
                    .collect();
                if let Some(&first) = positions.first() {
                    let count = usize::try_from(rollback.num_turns).unwrap_or(usize::MAX);
                    let mut cut = if count >= positions.len() {
                        first
                    } else {
                        positions[positions.len() - count]
                    };
                    let acceptance_order = self.original_window[cut].acceptance_order;
                    while cut > first && self.original_window[cut - 1].pre_turn_context {
                        cut -= 1;
                    }
                    self.original_window.truncate(cut);
                    self.original_window.retain(|source| {
                        !source.accepted_detail
                            || !acceptance_order.is_some_and(|boundary| {
                                source
                                    .acceptance_order
                                    .is_some_and(|order| order >= boundary)
                            })
                    });
                }
                self.reset_original_index();
            }
            _ => {}
        }
        Ok(())
    }

    fn reset_original_index(&mut self) {
        self.original_index = ContextIndex::default();
        for source in &self.original_window {
            self.original_index.register_descriptor(&source.descriptor);
        }
    }

    pub(super) fn reset_at(&mut self, record_index: usize) {
        if self.last_reset != Some(record_index) {
            return;
        }
        self.index = ContextIndex::default();
        for source in &self.window {
            self.index.register_descriptor(source);
        }
    }

    /// Index events are metadata effects, not UI turn records. Their post-reset effects survive
    /// even when a late event is assigned to an older UI turn by canonical rollback planning.
    pub(super) fn observe_event(
        &mut self,
        record_index: usize,
        event: &ContextIndexEvent,
    ) -> ThreadStoreResult<()> {
        self.saw_index = true;
        if self.last_reset.is_none_or(|reset| record_index > reset) {
            replay_with_residents(
                &mut self.index,
                event,
                self.window.iter().map(|source| source.id.as_str()),
            )
            .map_err(migration_error)?;
        }
        Ok(())
    }

    pub(super) fn observe_preserved(
        &mut self,
        record_index: usize,
        item: &RolloutItem,
    ) -> ThreadStoreResult<()> {
        let after_reset = self.last_reset.is_none_or(|reset| record_index > reset);
        match item {
            RolloutItem::ResponseItem(envelope) => {
                let source = ContextIndex::describe(
                    &envelope.item,
                    envelope
                        .metadata
                        .as_ref()
                        .is_some_and(|metadata| metadata.compaction_output),
                );
                if after_reset {
                    self.index.register_descriptor(&source);
                }
                self.window.push(source);
            }
            RolloutItem::InterAgentCommunication(communication) => {
                let source = ContextIndex::describe(&communication.to_model_input_item(), false);
                if after_reset {
                    self.index.register_descriptor(&source);
                }
                self.window.push(source);
            }
            RolloutItem::Compacted(compacted) => {
                if let Some(history) = &compacted.replacement_history {
                    self.saw_index |= compacted
                        .resume_metadata
                        .as_ref()
                        .is_some_and(|metadata| metadata.context_index.is_some());
                    self.window = history
                        .iter()
                        .map(|envelope| {
                            ContextIndex::describe(
                                &envelope.item,
                                envelope
                                    .metadata
                                    .as_ref()
                                    .is_some_and(|metadata| metadata.compaction_output),
                            )
                        })
                        .collect();
                    if after_reset {
                        self.index = compacted
                            .resume_metadata
                            .as_ref()
                            .and_then(|metadata| metadata.context_index.clone())
                            .unwrap_or_default();
                        self.index.rebuild().map_err(migration_error)?;
                        self.index.reconcile_checkpoint_descriptors(&self.window);
                    }
                } else {
                    // Official histories without index events keep their existing migration
                    // path. For an indexed hybrid, guessing Core's private legacy user-budget
                    // reconstruction would fabricate source identities. Preserve its original
                    // rollout and report this unsupported shape instead of silently rebasing it.
                    if self.saw_index {
                        return Err(migration_error(
                            "indexed rollout followed by legacy text compaction without replacement history is unsupported for rollback migration; original rollout retained",
                        ));
                    }
                    // An official legacy checkpoint may precede the first indexed R41 turn.
                    // Retain known user identities conservatively; their original payload and
                    // Core's legacy budget/parser remain authoritative for actual visibility.
                    // ID-less roots need no guessed ordinal: projection always preserves them.
                    self.window.retain(|source| {
                        source.kind == ContextSourceKind::User && source.response_item_id.is_some()
                    });
                    if after_reset {
                        self.index = ContextIndex::default();
                        for source in &self.window {
                            self.index.register_descriptor(source);
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn finish(self) -> ThreadStoreResult<ContextIndexEvent> {
        ContextIndexEvent::checkpoint(self.index).map_err(migration_error)
    }
}

/// General snapshots reconcile against current resident ORIGINAL descriptors. Window hashes
/// describe prepared bytes, so only their native/call IDs are used to find original evidence.
fn replay_with_residents<'a>(
    index: &mut ContextIndex,
    event: &ContextIndexEvent,
    resident_ids: impl Iterator<Item = &'a str>,
) -> Result<(), ContextIndexError> {
    if event.snapshot.is_none() {
        return index.replay(event);
    }
    let mut candidate = index.clone();
    candidate.replay(event)?;
    for id in resident_ids {
        if let Some(original) = index.sources().get(id)
            && (original.response_item_id.is_some() || original.call_id.is_some())
        {
            candidate.reconcile_original_descriptor(original);
        }
    }
    *index = candidate;
    Ok(())
}
