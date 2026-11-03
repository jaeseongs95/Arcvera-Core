//! Metadata-only inspection and exact deletion while an unloaded thread's writer is reserved.

use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::path::Path;

use codex_protocol::ThreadId;
use codex_protocol::protocol::MultiAgentVersion;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::SubAgentSource;
use codex_protocol::protocol::ThreadHistoryMode;
use serde::Deserialize;
use tokio::sync::OwnedMutexGuard;
use tokio::sync::OwnedRwLockWriteGuard;

use super::LocalThreadStore;
use super::WriterLockGuard;
use crate::ReadThreadParams;
use crate::ThreadStore;
use crate::ThreadStoreError;
use crate::ThreadStoreResult;

pub struct UnloadedThreadCleanupGuard {
    store: LocalThreadStore,
    thread_id: ThreadId,
    _lifecycle: OwnedRwLockWriteGuard<()>,
    _writer: OwnedMutexGuard<()>,
    writer_lock: WriterLockGuard,
    _reference_deletion: std::fs::File,
}

#[derive(Debug)]
pub struct UnloadedThreadCleanupSnapshot {
    pub revision: String,
    pub parent_thread_id: ThreadId,
    pub last_turn_id: String,
    pub terminal_status: String,
}

#[derive(Debug)]
pub struct UnloadedThreadCleanupPreview {
    pub snapshot: Option<UnloadedThreadCleanupSnapshot>,
    pub reason: &'static str,
}

impl UnloadedThreadCleanupPreview {
    fn preserved(reason: &'static str) -> Self {
        Self {
            snapshot: None,
            reason,
        }
    }
}

impl LocalThreadStore {
    /// Does not open a recorder or restore a runtime. A different process's live writer
    /// returns Conflict; a detached local recorder also prevents cold deletion.
    pub async fn reserve_unloaded_cleanup(
        &self,
        thread_id: ThreadId,
    ) -> ThreadStoreResult<UnloadedThreadCleanupGuard> {
        let lifecycle = self.live_writer_locks.lock_lifecycle(thread_id).await;
        let writer = self.live_writer_locks.lock(thread_id).await;
        self.ensure_live_recorder_absent(thread_id).await?;
        let writer_lock = self.acquire_writer_lock(thread_id)?;
        let reference_deletion = self.reserve_reference_deletion()?;
        Ok(UnloadedThreadCleanupGuard {
            store: self.clone(),
            thread_id,
            _lifecycle: lifecycle,
            _writer: writer,
            writer_lock,
            _reference_deletion: reference_deletion,
        })
    }
}

impl UnloadedThreadCleanupGuard {
    pub async fn preview(&self) -> ThreadStoreResult<UnloadedThreadCleanupPreview> {
        let stored = self
            .store
            .read_thread(ReadThreadParams {
                thread_id: self.thread_id,
                include_archived: true,
                include_history: false,
            })
            .await?;
        let Some(path) = stored.rollout_path else {
            return Ok(UnloadedThreadCleanupPreview::preserved("missing_metadata"));
        };
        let Some(before) = super::history_revision::read(&path).await else {
            return Ok(UnloadedThreadCleanupPreview::preserved("missing_metadata"));
        };
        let id = self.thread_id;
        let scan_path = path.clone();
        let mut preview = tokio::task::spawn_blocking(move || scan_metadata(&scan_path, id))
            .await
            .map_err(|error| ThreadStoreError::Internal {
                message: error.to_string(),
            })??;
        if super::history_revision::read(&path).await.as_ref() != Some(&before) {
            return Ok(UnloadedThreadCleanupPreview::preserved("changed"));
        }
        if let Some(snapshot) = &mut preview.snapshot {
            // The header, persisted index and source must all name the same direct parent.
            if stored.parent_thread_id != Some(snapshot.parent_thread_id)
                || stored.source.parent_thread_id() != Some(snapshot.parent_thread_id)
            {
                return Ok(UnloadedThreadCleanupPreview::preserved(
                    "ownership_unverified",
                ));
            }
            snapshot.revision = before;
        }
        Ok(preview)
    }

    /// The caller must keep its owner task, admission, ancestry, queue and goal fences.
    /// Retain these store locks through the existing reference checks and exact deletion.
    pub async fn delete(self, before_mutation: impl FnOnce()) -> ThreadStoreResult<()> {
        let mut locks = vec![self.writer_lock.clone()];
        super::delete_thread::delete_threads_under_writer_locks(
            &self.store,
            vec![self.thread_id],
            &mut locks,
            before_mutation,
            &self._reference_deletion,
        )
        .await
    }
}

// Deserialize only lifecycle metadata. User messages, tool results and model context are
// ignored by serde, never restored, retained in a history Vec or sent to a model.
#[derive(Deserialize)]
struct MetadataLine {
    ordinal: Option<u64>,
    #[serde(flatten)]
    record: MetadataRecord,
}

#[derive(Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
enum MetadataRecord {
    SessionMeta(Header),
    EventMsg(TurnMetadata),
    TokenUsageRecord(serde::de::IgnoredAny),
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
struct Header {
    id: ThreadId,
    source: SessionSource,
    parent_thread_id: Option<ThreadId>,
    multi_agent_version: Option<MultiAgentVersion>,
    history_base: Option<codex_protocol::protocol::HistoryPosition>,
    forked_from_id: Option<ThreadId>,
    #[serde(default)]
    history_mode: ThreadHistoryMode,
    subagent_history_start_ordinal: Option<u64>,
}

#[derive(Deserialize)]
struct TurnMetadata {
    #[serde(rename = "type")]
    kind: String,
    turn_id: Option<String>,
    error: Option<serde::de::IgnoredAny>,
}

fn scan_metadata(path: &Path, id: ThreadId) -> ThreadStoreResult<UnloadedThreadCleanupPreview> {
    const MAX_SCAN_BYTES: u64 = 64 * 1024 * 1024;
    let file = codex_rollout::open_rollout_seekable_reader(path, || false).map_err(|error| {
        ThreadStoreError::Internal {
            message: error.to_string(),
        }
    })?;
    let mut reader = BufReader::new(file.take(MAX_SCAN_BYTES + 1));
    let mut line = Vec::new();
    let mut bytes = 0;
    let mut parent = None;
    let mut next_ordinal = 0;
    let mut history_start = 0;
    let mut paginated = false;
    let mut last_turn = None;
    let mut status = None;
    let mut post_terminal_activity = false;
    loop {
        line.clear();
        let read =
            reader
                .read_until(b'\n', &mut line)
                .map_err(|error| ThreadStoreError::Internal {
                    message: error.to_string(),
                })?;
        if read == 0 {
            break;
        }
        bytes += read as u64;
        if bytes > MAX_SCAN_BYTES {
            return Ok(UnloadedThreadCleanupPreview::preserved(
                "metadata_scan_limit",
            ));
        }
        // A crash tail, invalid record or a replay rollback cannot prove the latest turn.
        if !line.ends_with(b"\n") {
            return Ok(UnloadedThreadCleanupPreview::preserved(
                "incomplete_metadata",
            ));
        }
        let Ok(line) = serde_json::from_slice::<MetadataLine>(&line) else {
            return Ok(UnloadedThreadCleanupPreview::preserved("invalid_metadata"));
        };
        if parent.is_none() {
            let MetadataRecord::SessionMeta(header) = line.record else {
                return Ok(UnloadedThreadCleanupPreview::preserved("missing_metadata"));
            };
            if header.id != id {
                return Ok(UnloadedThreadCleanupPreview::preserved(
                    "ownership_unverified",
                ));
            }
            if header.multi_agent_version != Some(MultiAgentVersion::V2) {
                return Ok(UnloadedThreadCleanupPreview::preserved(
                    "unsupported_generation",
                ));
            }
            let SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id, ..
            }) = header.source
            else {
                return Ok(UnloadedThreadCleanupPreview::preserved(
                    "unsupported_source",
                ));
            };
            if header.parent_thread_id != Some(parent_thread_id) {
                return Ok(UnloadedThreadCleanupPreview::preserved(
                    "ownership_unverified",
                ));
            }
            paginated = header.history_mode == ThreadHistoryMode::Paginated;
            let first_ordinal = header
                .history_base
                .map_or(0, |base| base.end_ordinal_exclusive);
            if (paginated || line.ordinal.is_some()) && line.ordinal != Some(first_ordinal) {
                return Ok(UnloadedThreadCleanupPreview::preserved(
                    "invalid_metadata_ordinal",
                ));
            }
            if header.forked_from_id.is_some()
                && header.subagent_history_start_ordinal.is_none()
                && header.history_base.is_none()
            {
                return Ok(UnloadedThreadCleanupPreview::preserved(
                    "inherited_history_unverified",
                ));
            }
            next_ordinal = first_ordinal.saturating_add(1);
            history_start = header
                .subagent_history_start_ordinal
                .unwrap_or(next_ordinal);
            parent = Some(parent_thread_id);
            continue;
        }
        if (paginated || line.ordinal.is_some()) && line.ordinal != Some(next_ordinal) {
            return Ok(UnloadedThreadCleanupPreview::preserved(
                "invalid_metadata_ordinal",
            ));
        }
        let inherited = next_ordinal < history_start;
        next_ordinal = next_ordinal.saturating_add(1);
        if inherited {
            continue;
        }
        let event = match line.record {
            MetadataRecord::EventMsg(event) => event,
            MetadataRecord::TokenUsageRecord(_) => continue,
            _ => {
                post_terminal_activity |= status.is_some();
                continue;
            }
        };
        match event.kind.as_str() {
            "task_started" | "turn_started" => {
                let Some(turn) = event.turn_id.filter(|turn| !turn.is_empty()) else {
                    return Ok(UnloadedThreadCleanupPreview::preserved("invalid_metadata"));
                };
                last_turn = Some(turn);
                status = None;
                post_terminal_activity = false;
            }
            "task_complete" | "turn_complete" | "turn_aborted" => {
                if last_turn.is_none() || event.turn_id != last_turn {
                    return Ok(UnloadedThreadCleanupPreview::preserved(
                        "terminal_turn_mismatch",
                    ));
                }
                status = Some(if event.kind == "turn_aborted" {
                    "cancelled"
                } else if event.error.is_some() {
                    "failed"
                } else {
                    "completed"
                });
            }
            "thread_rolled_back" => {
                return Ok(UnloadedThreadCleanupPreview::preserved(
                    "rollback_unverified",
                ));
            }
            "token_count" | "thread_settings_applied" | "thread_goal_updated" => {}
            _ => post_terminal_activity |= status.is_some(),
        }
    }
    if post_terminal_activity {
        return Ok(UnloadedThreadCleanupPreview::preserved(
            "activity_after_terminal",
        ));
    }
    let (Some(parent_thread_id), Some(last_turn_id), Some(terminal_status)) =
        (parent, last_turn, status)
    else {
        return Ok(UnloadedThreadCleanupPreview::preserved(
            "terminal_turn_unverified",
        ));
    };
    Ok(UnloadedThreadCleanupPreview {
        snapshot: Some(UnloadedThreadCleanupSnapshot {
            revision: String::new(),
            parent_thread_id,
            last_turn_id,
            terminal_status: terminal_status.into(),
        }),
        reason: "unloaded_terminal",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn inspect(
        events: &[(&str, &str)],
        history_start: Option<u64>,
        tail: &str,
    ) -> UnloadedThreadCleanupPreview {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("rollout.jsonl");
        let id = ThreadId::new();
        let parent = ThreadId::new();
        let header = serde_json::json!({
            "ordinal":0, "type":"session_meta", "payload":{
                "id":id, "parent_thread_id":parent,
                "source": SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                    parent_thread_id: parent, depth: 1, agent_path: None,
                    agent_nickname: None, agent_role: None,
                }),
                "multi_agent_version":MultiAgentVersion::V2,
                "history_mode":ThreadHistoryMode::Paginated,
                "subagent_history_start_ordinal":history_start,
            }
        });
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(file, "{header}").unwrap();
        for (index, (kind, turn)) in events.iter().enumerate() {
            writeln!(file, "{}", serde_json::json!({"ordinal":index + 1, "type":"event_msg", "payload":{"type":kind, "turn_id":turn}})).unwrap();
        }
        write!(file, "{tail}").unwrap();
        drop(file);
        scan_metadata(&path, id).unwrap()
    }

    #[test]
    fn metadata_cleanup_requires_matching_latest_terminal_and_rejects_uncertain_tail() {
        for (events, tail, reason) in [
            (
                vec![("task_started", "one"), ("task_complete", "one")],
                "",
                "unloaded_terminal",
            ),
            (
                vec![
                    ("task_started", "one"),
                    ("task_complete", "one"),
                    ("task_started", "two"),
                ],
                "",
                "terminal_turn_unverified",
            ),
            (
                vec![("task_started", "one"), ("task_complete", "other")],
                "",
                "terminal_turn_mismatch",
            ),
            (
                vec![("task_started", "one"), ("turn_aborted", "one")],
                "",
                "unloaded_terminal",
            ),
            (
                vec![("task_started", "one"), ("task_complete", "one")],
                "{\"type\":",
                "incomplete_metadata",
            ),
            (
                vec![("task_started", "one"), ("task_complete", "one")],
                "bad\n",
                "invalid_metadata",
            ),
            (
                vec![
                    ("task_started", "one"),
                    ("task_complete", "one"),
                    ("thread_rolled_back", "one"),
                ],
                "",
                "rollback_unverified",
            ),
            (
                vec![
                    ("task_started", "one"),
                    ("task_complete", "one"),
                    ("user_message", "one"),
                ],
                "",
                "activity_after_terminal",
            ),
            (
                vec![
                    ("task_started", "one"),
                    ("task_complete", "one"),
                    ("future_turn_started", "two"),
                ],
                "",
                "activity_after_terminal",
            ),
        ] {
            let result = inspect(&events, None, tail);
            assert_eq!(result.reason, reason);
            assert_eq!(result.snapshot.is_some(), reason == "unloaded_terminal");
        }
    }

    #[test]
    fn metadata_cleanup_never_uses_inherited_parent_completion_as_child_completion() {
        let inherited = [("task_started", "parent"), ("task_complete", "parent")];
        let result = inspect(&inherited, Some(3), "");
        assert_eq!(result.reason, "terminal_turn_unverified");
        assert!(result.snapshot.is_none());
        let result = inspect(
            &[
                ("task_started", "parent"),
                ("task_complete", "parent"),
                ("task_started", "child"),
                ("task_complete", "child"),
            ],
            Some(3),
            "",
        );
        assert_eq!(result.snapshot.unwrap().last_turn_id, "child");
        let result = inspect(
            &[("task_started", "one"), ("task_complete", "one")],
            None,
            "{\"ordinal\":9,\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"turn_id\":\"one\"}}\n",
        );
        assert_eq!(result.reason, "invalid_metadata_ordinal");
        assert!(result.snapshot.is_none());
    }
}
