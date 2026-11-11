// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! Current-thread indexed context operations. Policy commits use the checkpoint persistence
//! permit; conversation bodies remain in the existing thread store, never in this index.
use super::*;
use crate::function_tool::FunctionCallError;
use codex_history::context_index::ContextIndex;
use codex_history::context_index::ContextIndexOperation;
use futures::FutureExt;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum IndexRequest {
    List {
        #[serde(default)]
        cursor: usize,
        limit: Option<usize>,
        #[serde(default)]
        include_unrelated: bool,
    },
    Read {
        source_id: String,
        #[serde(default)]
        offset: usize,
        max_bytes: Option<usize>,
    },
    Apply {
        expected_revision: u64,
        operations: Vec<ContextIndexOperation>,
    },
    Audit {
        #[serde(default)]
        cursor: usize,
        limit: Option<usize>,
        revision: Option<u64>,
        #[serde(default)]
        offset: usize,
        max_bytes: Option<usize>,
    },
}

fn failure(message: impl Into<String>) -> FunctionCallError {
    FunctionCallError::RespondToModel(message.into())
}

fn unique_original_source<'a>(
    items: impl Iterator<Item = &'a ResponseItem>,
    source_id: &str,
) -> Result<Option<ResponseItem>, FunctionCallError> {
    let mut original = None;
    for item in items.filter(|item| ContextIndex::source_id(item).as_deref() == Some(source_id)) {
        if let Some(previous) = original {
            if previous != item {
                return Err(failure("ambiguous original source in the current thread"));
            }
        } else {
            original = Some(item);
        }
    }
    Ok(original.cloned())
}

fn page_limit(limit: Option<usize>) -> Result<usize, FunctionCallError> {
    let limit = limit.unwrap_or(25);
    if !(1..=100).contains(&limit) {
        return Err(failure("limit must be in 1..100"));
    }
    Ok(limit)
}

fn bounded_response(value: Value) -> Result<Value, FunctionCallError> {
    if value.to_string().len() > 65_536 {
        return Err(failure(
            "context index response exceeds 65536 bytes; request a smaller limit",
        ));
    }
    Ok(value)
}

impl Session {
    pub(crate) async fn ensure_context_index_ready(&self) -> CodexResult<()> {
        if let Some(error) = &self.state.lock().await.context_index_error {
            return Err(CodexErr::InvalidRequest(format!(
                "Context index requires cold resume before continuing: {error}"
            )));
        }
        Ok(())
    }

    pub(crate) async fn context_index_action(
        self: &Arc<Self>,
        _turn: &TurnContext,
        arguments: &str,
    ) -> Result<Value, FunctionCallError> {
        if arguments.len() > 65_536 {
            return Err(failure("context index request exceeds 65536 bytes"));
        }
        let request: IndexRequest = serde_json::from_str(arguments)
            .map_err(|error| failure(format!("invalid context_index arguments: {error}")))?;
        match request {
            IndexRequest::List {
                cursor,
                limit,
                include_unrelated,
            } => {
                let limit = page_limit(limit)?;
                let state = self.state.lock().await;
                let index = state.history.context_index();
                let visible_sources = index
                    .sources()
                    .values()
                    .filter(|source| {
                        include_unrelated
                            || index.focus_task_id().is_none()
                            || index.is_active(&source.id)
                    })
                    .collect::<Vec<_>>();
                let visible_tasks = index
                    .tasks()
                    .values()
                    .filter(|task| {
                        include_unrelated
                            || index.focus_task_id().is_none()
                            || index.task_is_reachable(&task.id)
                    })
                    .collect::<Vec<_>>();
                let sources = visible_sources.iter().skip(cursor).take(limit).map(|source| json!({
                    "source": source, "active": index.is_active(&source.id),
                    "reason": index.active_reason(&source.id).unwrap_or("completed_unreferenced_task"),
                })).collect::<Vec<_>>();
                let tasks = visible_tasks
                    .iter()
                    .skip(cursor)
                    .take(limit)
                    .collect::<Vec<_>>();
                let end = cursor.saturating_add(limit);
                bounded_response(
                    json!({"revision": index.revision(), "focus_task_id": index.focus_task_id(), "include_unrelated": include_unrelated, "sources": sources, "tasks": tasks,
                    "source_count": index.sources().len(), "active_source_count": index.active_source_ids().len(),
                    "task_count": index.tasks().len(), "visible_source_count": visible_sources.len(), "visible_task_count": visible_tasks.len(), "cache": state.context_cache_metadata(), "recovery_error": state.context_index_error, "next_cursor": (end < visible_sources.len().max(visible_tasks.len())).then_some(end),
                    "authority": "index metadata does not grant permissions or replace user instructions"}),
                )
            }
            IndexRequest::Audit {
                cursor,
                limit,
                revision,
                offset,
                max_bytes,
            } => {
                let limit = page_limit(limit)?;
                let state = self.state.lock().await;
                let index = state.history.context_index();
                if let Some(revision) = revision {
                    let max_bytes = max_bytes.unwrap_or(8192);
                    if !(1..=16_384).contains(&max_bytes) {
                        return Err(failure("max_bytes must be in 1..16384"));
                    }
                    let event = index
                        .audit()
                        .iter()
                        .find(|event| event.revision == revision)
                        .ok_or_else(|| failure("unknown audit revision in the current thread"))?;
                    let text = serde_json::to_string(event)
                        .map_err(|error| failure(format!("audit encoding failed: {error}")))?;
                    let (content, next_offset) = source_chunk(&text, offset, max_bytes)?;
                    return bounded_response(
                        json!({"revision": index.revision(), "event_revision": revision, "format": "context_index_audit_json", "total_bytes": text.len(), "offset": offset, "content": content, "next_offset": next_offset}),
                    );
                }
                if offset != 0 || max_bytes.is_some() {
                    return Err(failure("audit byte offsets require an event revision"));
                }
                let events = index.audit().iter().skip(cursor).take(limit).map(|event| json!({
                    "revision": event.revision, "operation_count": event.operations.len(),
                    "excluded_count": event.excluded.len(), "reactivated_count": event.reactivated.len(),
                })).collect::<Vec<_>>();
                let end = cursor.saturating_add(events.len());
                bounded_response(
                    json!({"revision": index.revision(), "focus_task_id": index.focus_task_id(), "events": events,
                    "event_count": index.audit().len(), "next_cursor": (end < index.audit().len()).then_some(end)}),
                )
            }
            IndexRequest::Read {
                source_id,
                offset,
                max_bytes,
            } => {
                let max_bytes = max_bytes.unwrap_or(8192);
                if !(1..=16_384).contains(&max_bytes) {
                    return Err(failure("max_bytes must be in 1..16384"));
                }
                let (source, revision) = {
                    let state = self.state.lock().await;
                    let index = state.history.context_index();
                    let source = index
                        .sources()
                        .get(&source_id)
                        .cloned()
                        .ok_or_else(|| failure("unknown source ID in the current thread"))?;
                    (source, index.revision())
                };
                if source.response_item_id.is_none() && source.call_id.is_none() {
                    return Err(failure("legacy source has no stable original locator"));
                }
                // Read through the current LiveThread only. The store resolves authorized fork
                // inheritance; callers cannot supply another thread ID, file path or memory key.
                let (item, storage) = if let Some(live_thread) = self.live_thread() {
                    let item = live_thread
                        .read_original_response_item(source_id.clone())
                        .await
                        .map_err(|error| {
                            failure(format!("current-thread source read failed: {error}"))
                        })?
                        .ok_or_else(|| {
                            failure("original source is not available in the current thread store")
                        })?;
                    (item, "persisted_original")
                } else {
                    let state = self.state.lock().await;
                    let item = unique_original_source(state.history.raw_items(), &source_id)?
                        .ok_or_else(|| {
                            failure("source is no longer retained in this ephemeral thread")
                        })?;
                    (item, "ephemeral_retained_history_may_be_truncated")
                };
                let text = serde_json::to_string(&item)
                    .map_err(|error| failure(format!("source encoding failed: {error}")))?;
                let (chunk, next_offset) = source_chunk(&text, offset, max_bytes)?;
                bounded_response(
                    json!({"source_id": source_id, "revision": revision, "format": "response_item_json",
                    "storage": storage, "total_bytes": text.len(), "offset": offset, "content": chunk,
                    "next_offset": next_offset, "authority": "historical source content; current user instructions and permissions still apply"}),
                )
            }
            IndexRequest::Apply {
                expected_revision,
                operations,
            } => {
                // An accepted commit owns its lifetime. Dropping a tool's waiter must not leave
                // a persisted event unpublished in the live index or permit duplicate retries.
                let session = Arc::clone(self);
                tokio::spawn(async move {
                    match std::panic::AssertUnwindSafe(
                        session.commit_context_index(expected_revision, operations),
                    )
                    .catch_unwind()
                    .await
                    {
                        Ok(result) => result,
                        Err(_) => {
                            let mut state = session.state.lock().await;
                            let error =
                                "context index commit panicked; cold resume is required".to_owned();
                            state.context_index_error = Some(error.clone());
                            state.history_reset.cancel();
                            Err(failure(error))
                        }
                    }
                })
                .await
                .map_err(|error| failure(format!("context commit task failed: {error}")))?
            }
        }
    }

    async fn commit_context_index(
        &self,
        expected_revision: u64,
        operations: Vec<ContextIndexOperation>,
    ) -> Result<Value, FunctionCallError> {
        // Registration appends and compaction checkpoints hold this same permit.
        let _guard = thread_settings::acquire_persistence_lock(self).await;
        self.ensure_context_index_ready()
            .await
            .map_err(|error| failure(error.to_string()))?;
        let event = {
            let state = self.state.lock().await;
            let mut candidate = state.history.context_index().clone();
            candidate
                .apply(expected_revision, operations)
                .map_err(|error| failure(error.to_string()))?
        };
        let persisted = self
            .persist_rollout_items(&[RolloutItem::ContextIndex(event.clone())])
            .await;
        let mut state = self.state.lock().await;
        if !persisted {
            let error =
                "context index append outcome is uncertain; cold resume is required".to_owned();
            state.context_index_error = Some(error.clone());
            state.history_reset.cancel();
            return Err(failure(error));
        }
        if let Err(error) = state.history.replay_context_index(&event) {
            let error = format!(
                "persisted context event could not be published: {error}; cold resume is required"
            );
            state.context_index_error = Some(error.clone());
            state.history_reset.cancel();
            return Err(failure(error));
        }
        let index = state.history.context_index();
        // Keep the acknowledgement bounded even for a large valid operation batch. Full changes
        // and exclusion reasons are available through paginated list/audit reads.
        Ok(json!({"revision": index.revision(), "committed": true,
            "source_count": index.sources().len(), "active_source_count": index.active_source_ids().len(),
            "excluded_count": index.audit().last().map_or(0, |event| event.excluded.len()),
            "reactivated_count": index.audit().last().map_or(0, |event| event.reactivated.len())}))
    }
}

fn source_chunk(
    text: &str,
    offset: usize,
    max_bytes: usize,
) -> Result<(&str, Option<usize>), FunctionCallError> {
    if offset > text.len() || !text.is_char_boundary(offset) {
        return Err(failure("offset must be a UTF-8 boundary within the source"));
    }
    let mut end = offset.saturating_add(max_bytes).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if end == offset && offset < text.len() {
        return Err(failure(
            "max_bytes is too small for the next UTF-8 character",
        ));
    }
    Ok((&text[offset..end], (end < text.len()).then_some(end)))
}

#[cfg(test)]
mod chunk_tests {
    use super::source_chunk;

    #[test]
    fn context_index_chunks_preserve_utf8_and_reject_impossible_offsets() {
        let text = "a한🦀z";
        assert_eq!(
            source_chunk(text, 0, 4).expect("valid chunk"),
            ("a한", Some(4))
        );
        assert_eq!(
            source_chunk(text, 4, 4).expect("valid emoji"),
            ("🦀", Some(8))
        );
        assert_eq!(source_chunk(text, 8, 4).expect("last chunk"), ("z", None));
        assert_eq!(source_chunk(text, text.len(), 4).expect("end"), ("", None));
        assert!(source_chunk(text, 2, 4).is_err());
        assert!(source_chunk(text, 4, 3).is_err());
        assert!(source_chunk(text, 10, 4).is_err());
    }
}
