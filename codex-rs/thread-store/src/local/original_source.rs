// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! Targeted original lookup; never reconstructs model context or a display projection.
use codex_protocol::models::ResponseItem;
use codex_protocol::protocol::ThreadHistoryMode;
use codex_rollout::ReverseJsonlScanner;
use codex_rollout::RolloutItem;
use codex_rollout::ScanOutcome;

use super::LocalThreadStore;
use super::rollout_lineage::RolloutLineageSegment;
use super::thread_rollout_resolver;
use crate::LoadThreadHistoryParams;
use crate::ThreadStoreError;
use crate::ThreadStoreResult;

fn scan_error(error: impl std::fmt::Display) -> ThreadStoreError {
    ThreadStoreError::Internal {
        message: format!("failed to scan original source: {error}"),
    }
}

pub(super) async fn read(
    store: &LocalThreadStore,
    params: LoadThreadHistoryParams,
    source_id: String,
) -> ThreadStoreResult<Option<ResponseItem>> {
    let resolved = if params.include_archived {
        thread_rollout_resolver::resolve_current_including_archived(store, params.thread_id).await?
    } else {
        thread_rollout_resolver::resolve_current(store, params.thread_id).await?
    }
    .ok_or(ThreadStoreError::ThreadNotFound {
        thread_id: params.thread_id,
    })?;
    let meta = codex_rollout::read_session_meta_line(&resolved.path)
        .await
        .map_err(scan_error)?;
    if meta.meta.id != params.thread_id {
        return Err(ThreadStoreError::InvalidRequest {
            message: "original source rollout belongs to another thread".to_string(),
        });
    }
    let paginated = meta.meta.history_mode == ThreadHistoryMode::Paginated;
    let segments = if paginated {
        store
            .resolve_rollout_lineage(params.thread_id, Some(resolved.path))
            .await?
            .segments
    } else {
        vec![RolloutLineageSegment {
            rollout_id: resolved.rollout_id,
            rollout_path: resolved.path,
            start_ordinal: 0,
            end: None,
        }]
    };
    // Dropping this task closes the receiver and makes the blocking I/O return an error.
    let (stop, keep_running) = tokio::sync::oneshot::channel::<()>();
    let result = tokio::task::spawn_blocking(move || {
        scan(&segments, paginated, &source_id, || stop.is_closed())
    })
    .await
    .map_err(scan_error)?;
    drop(keep_running);
    result
}

pub(super) fn scan(
    segments: &[RolloutLineageSegment],
    paginated: bool,
    source_id: &str,
    is_cancelled: impl Fn() -> bool,
) -> ThreadStoreResult<Option<ResponseItem>> {
    // ceiling: O(authorized history bytes) I/O, O(largest decoded record + selected original)
    // memory. A future indexed lookup must still check conflicting originals and fork cutoffs.
    let mut original = None;
    for segment in segments.iter().rev() {
        let file =
            codex_rollout::open_rollout_seekable_reader(&segment.rollout_path, &is_cancelled)
                .map_err(scan_error)?;
        let mut scanner = match segment.end {
            Some(end) => ReverseJsonlScanner::new_at(file, end.end_byte_offset),
            None => ReverseJsonlScanner::new(file),
        }
        .map_err(scan_error)?;
        loop {
            if is_cancelled() {
                return Err(scan_error("rollout read cancelled"));
            }
            let Some(outcome) = scanner.scan_next_rollout_line().map_err(scan_error)? else {
                break;
            };
            let line = match outcome {
                ScanOutcome::Parsed(line) => line,
                ScanOutcome::Rejected(error) => return Err(scan_error(error)),
            };
            if matches!(&line.item, RolloutItem::SessionMeta(_)) {
                // Legacy and copied-prefix forks can retain earlier SessionMeta records.
                // Those headers do not terminate the current thread's authorized raw history.
                continue;
            }
            if paginated {
                let ordinal = line
                    .ordinal
                    .ok_or_else(|| scan_error("paginated original record is missing an ordinal"))?;
                if ordinal < segment.start_ordinal() {
                    continue;
                }
                if segment.end_ordinal().is_some_and(|end| ordinal >= end) {
                    return Err(scan_error("original record exceeds inherited cutoff"));
                }
            }
            if let RolloutItem::ResponseItem(envelope) = line.item {
                crate::store::retain_unique_original(&mut original, &envelope.item, source_id)?;
            }
        }
    }
    if is_cancelled() {
        return Err(scan_error("rollout read cancelled"));
    }
    Ok(original)
}
