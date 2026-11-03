use codex_protocol::ThreadId;
use codex_protocol::models::BaseInstructions;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::MultiAgentVersion;
use codex_protocol::protocol::SessionSource;
use codex_protocol::protocol::ThreadHistoryMode;
use codex_protocol::protocol::ThreadMemoryMode;
use codex_protocol::protocol::TurnCompleteEvent;
use codex_protocol::protocol::TurnStartedEvent;
use codex_rollout::RolloutItem;
use codex_utils_absolute_path::test_support::PathExt;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

use super::super::LocalThreadStore;
use super::super::test_support::test_config;
use crate::AppendThreadItemsParams;
use crate::ArchiveThreadParams;
use crate::CreateThreadParams;
use crate::DeleteThreadParams;
use crate::ListTurnsParams;
use crate::RevertThreadParams;
use crate::SortDirection;
use crate::StoredTurnItemsView;
use crate::ThreadPersistenceMetadata;
use crate::ThreadStore;

#[tokio::test]
async fn revert_keeps_thread_id_and_hides_suffix_across_repeated_reverts() {
    let home = TempDir::new().expect("temp dir");
    let config = test_config(home.path());
    let state_db = codex_state::StateRuntime::init(
        config.sqlite.clone(),
        config.default_model_provider_id.clone(),
    )
    .await
    .expect("initialize state database");
    let store = LocalThreadStore::new(config, Some(state_db.clone()));
    let thread_id = ThreadId::new();
    create_paginated_thread(&store, thread_id).await;
    store
        .append_items(AppendThreadItemsParams {
            thread_id,
            items: vec![
                turn_started("turn-1"),
                turn_completed("turn-1"),
                turn_started("turn-2"),
                turn_completed("turn-2"),
            ],
        })
        .await
        .expect("append turns");
    let original_path = store
        .live_rollout_path(thread_id)
        .await
        .expect("source rollout path");
    store
        .shutdown_thread(thread_id)
        .await
        .expect("close source writer");
    codex_rollout::state_db::reconcile_rollout(
        Some(state_db.as_ref()),
        original_path.as_path(),
        "test-provider",
        /*builder*/ None,
        &[],
        /*archived_only*/ Some(false),
        /*new_thread_memory_mode*/ None,
    )
    .await;
    compress_rollout(original_path.as_path());

    store
        .revert_thread(RevertThreadParams {
            thread_id,
            before_turn_id: "turn-2".to_string(),
            multi_agent_version: None,
        })
        .await
        .expect("revert before second turn");
    let first_replacement_path = state_db
        .get_thread(thread_id)
        .await
        .expect("read metadata")
        .expect("thread metadata")
        .rollout_path;
    assert_ne!(first_replacement_path, original_path);
    assert_ne!(
        codex_rollout::rollout_id_from_path(first_replacement_path.as_path()),
        Some(thread_id)
    );
    let replacement_meta = codex_rollout::read_session_meta_line(first_replacement_path.as_path())
        .await
        .expect("read replacement metadata")
        .meta;
    assert_eq!(replacement_meta.id, thread_id);
    assert_eq!(
        (
            replacement_meta.creator_user_id.as_deref(),
            replacement_meta.creator_account_id.as_deref()
        ),
        (Some("creator-user"), Some("creator-account")),
    );
    assert_eq!(replacement_meta.memory_mode, None);
    assert_eq!(
        replacement_meta.runtime_workspace_roots,
        Some(vec![home.path().join("workspace")])
    );
    assert_replacement_projected(&store, first_replacement_path.as_path()).await;
    assert_eq!(turn_ids(&store, thread_id).await, vec!["turn-1"]);

    // Simulate an older binary replacing the rollout without the creator fields it does not know.
    let rollout = tokio::fs::read_to_string(&first_replacement_path)
        .await
        .expect("read replacement rollout");
    let (meta_line, remaining_lines) = rollout.split_once('\n').expect("session metadata line");
    let original_meta_line_bytes = meta_line.len();
    let mut meta_line: serde_json::Value = serde_json::from_str(meta_line).expect("parse metadata");
    let payload = meta_line["payload"]
        .as_object_mut()
        .expect("metadata payload");
    payload.remove("creator_user_id");
    payload.remove("creator_account_id");
    // Keep history byte offsets intact: shortening an already projected rollout
    // would independently violate the immutable-history contract under test.
    let legacy_meta_line = meta_line.to_string();
    let padding = " ".repeat(
        original_meta_line_bytes
            .checked_sub(legacy_meta_line.len())
            .expect("omitting creator fields must shorten the metadata line"),
    );
    tokio::fs::write(
        &first_replacement_path,
        format!("{legacy_meta_line}{padding}\n{remaining_lines}"),
    )
    .await
    .expect("write legacy replacement rollout");

    store
        .revert_thread(RevertThreadParams {
            thread_id,
            before_turn_id: "turn-1".to_string(),
            multi_agent_version: None,
        })
        .await
        .expect("revert before first turn");
    let stored = state_db
        .get_thread(thread_id)
        .await
        .expect("read metadata")
        .expect("thread metadata");
    assert_replacement_projected(&store, stored.rollout_path.as_path()).await;
    assert_eq!(turn_ids(&store, thread_id).await, Vec::<String>::new());
    let recovered = codex_rollout::read_session_meta_line(&stored.rollout_path)
        .await
        .expect("read recovered creator metadata")
        .meta;
    assert_eq!(
        (recovered.creator_user_id, recovered.creator_account_id),
        (
            Some("creator-user".to_string()),
            Some("creator-account".to_string())
        ),
    );

    store
        .archive_thread(ArchiveThreadParams { thread_id })
        .await
        .expect("archive reverted thread");
    assert!(
        rollout_paths_for_thread(home.path(), thread_id)
            .await
            .iter()
            .all(|path| path.starts_with(home.path().join("archived_sessions")))
    );
    store
        .unarchive_thread(ArchiveThreadParams { thread_id })
        .await
        .expect("unarchive reverted thread");
    let owned_rollout_paths = rollout_paths_for_thread(home.path(), thread_id).await;
    assert_eq!(owned_rollout_paths.len(), 3);
    assert!(
        owned_rollout_paths
            .iter()
            .all(|path| path.starts_with(home.path().join("sessions")))
    );

    store
        .delete_thread(DeleteThreadParams { thread_id })
        .await
        .expect("delete reverted thread");
    for rollout_path in owned_rollout_paths {
        assert!(!rollout_path.exists());
    }
}

#[tokio::test]
async fn revert_projection_failure_preserves_current_history_and_allows_retry_and_delete() {
    let home = TempDir::new().expect("temp dir");
    let config = test_config(home.path());
    let state_db = codex_state::StateRuntime::init(
        config.sqlite.clone(),
        config.default_model_provider_id.clone(),
    )
    .await
    .expect("initialize state database");
    let store = LocalThreadStore::new(config, Some(state_db.clone()));
    let thread_id = ThreadId::new();
    create_paginated_thread(&store, thread_id).await;
    store
        .append_items(AppendThreadItemsParams {
            thread_id,
            items: vec![
                turn_started("turn-1"),
                turn_completed("turn-1"),
                turn_started("turn-2"),
                turn_completed("turn-2"),
            ],
        })
        .await
        .expect("append original turns");
    let source_path = store
        .live_rollout_path(thread_id)
        .await
        .expect("source path");
    store
        .shutdown_thread(thread_id)
        .await
        .expect("close source writer");
    codex_rollout::state_db::reconcile_rollout(
        Some(state_db.as_ref()),
        &source_path,
        "test-provider",
        /*builder*/ None,
        &[],
        /*archived_only*/ Some(false),
        /*new_thread_memory_mode*/ None,
    )
    .await;
    super::super::thread_history_materialization::materialize_to_sqlite(
        &store,
        thread_id,
        &source_path,
    )
    .await
    .expect("project original history before fault injection");
    let source_bytes = tokio::fs::read(&source_path).await.expect("source bytes");
    let checkpoint = super::super::thread_history::projection_state(&store, thread_id)
        .await
        .expect("source projection")
        .expect("source checkpoint");
    let source_checkpoint = (checkpoint.next_byte_offset, checkpoint.next_ordinal);
    let pool = store.thread_history_db().await.expect("history database");
    // Use the existing SQLite surface, without a production hook. The original
    // rollout ID remains writable; only the new replacement checkpoint is rejected.
    sqlx::query(
        "CREATE TRIGGER reject_replacement_projection
         BEFORE INSERT ON thread_history_projection_state
         WHEN NOT EXISTS (SELECT 1 FROM thread_history_projection_state
                          WHERE thread_id = NEW.thread_id)
         BEGIN SELECT RAISE(ABORT, 'r36 replacement projection rejected'); END",
    )
    .execute(pool)
    .await
    .expect("install replacement-only projection failure");
    let params = RevertThreadParams {
        thread_id,
        before_turn_id: "turn-2".to_string(),
        multi_agent_version: None,
    };
    let result = store.revert_thread(params.clone()).await;
    assert!(
        matches!(&result, Err(crate::ThreadStoreError::Internal { message })
            if message.contains("r36 replacement projection rejected")),
        "the replacement projection failure must reach the caller: {result:?}"
    );
    assert_eq!(
        state_db
            .get_thread(thread_id)
            .await
            .expect("read current metadata")
            .expect("original thread metadata")
            .rollout_path,
        source_path,
        "a failed replacement projection must not publish the new current path"
    );
    assert_eq!(
        tokio::fs::read(&source_path)
            .await
            .expect("source bytes after failure"),
        source_bytes
    );
    let checkpoint = super::super::thread_history::projection_state(&store, thread_id)
        .await
        .expect("source projection after failure")
        .expect("source checkpoint retained");
    assert_eq!(
        (checkpoint.next_byte_offset, checkpoint.next_ordinal),
        source_checkpoint
    );
    assert_eq!(turn_ids(&store, thread_id).await, vec!["turn-1", "turn-2"]);

    // The persisted but unselected replacement has the same owner, so ordinary
    // deletion must include it rather than treating it as an external reference.
    let failed_paths = rollout_paths_for_thread(home.path(), thread_id).await;
    assert_eq!(
        failed_paths.len(),
        2,
        "reach the post-persist projection failure boundary"
    );
    let failed_path = failed_paths
        .iter()
        .find(|path| **path != source_path)
        .expect("failed replacement path");
    let failed_meta = codex_rollout::read_session_meta_line(failed_path)
        .await
        .expect("failed replacement metadata")
        .meta;
    assert_eq!(failed_meta.id, thread_id);
    assert_eq!(
        failed_meta
            .history_base
            .expect("retained source prefix")
            .thread_id,
        thread_id
    );
    let failed_rollout_id =
        codex_rollout::rollout_id_from_path(failed_path).expect("failed replacement rollout ID");
    assert!(
        super::super::thread_history::projection_state(&store, failed_rollout_id)
            .await
            .expect("failed replacement projection")
            .is_none()
    );

    sqlx::query("DROP TRIGGER reject_replacement_projection")
        .execute(pool)
        .await
        .expect("remove isolated failure injection");
    store
        .revert_thread(params)
        .await
        .expect("retry revert after projection recovery");
    assert_eq!(turn_ids(&store, thread_id).await, vec!["turn-1"]);
    assert_eq!(
        tokio::fs::read(&source_path)
            .await
            .expect("source after retry"),
        source_bytes
    );
    let owned_paths = rollout_paths_for_thread(home.path(), thread_id).await;
    assert_eq!(
        owned_paths.len(),
        3,
        "original, failed, and successful replacements"
    );
    store
        .delete_thread(DeleteThreadParams { thread_id })
        .await
        .expect("failed replacement must not permanently block source deletion");
    for path in owned_paths {
        assert!(!path.exists(), "owned rollout remains: {}", path.display());
    }
    assert!(
        state_db
            .get_thread(thread_id)
            .await
            .expect("deleted metadata")
            .is_none()
    );
    assert!(
        rollout_paths_for_thread(home.path(), thread_id)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn revert_preserves_resolved_multi_agent_version_in_session_metadata() {
    let home = TempDir::new().expect("temp dir");
    let config = test_config(home.path());
    let state_db = codex_state::StateRuntime::init(
        config.sqlite.clone(),
        config.default_model_provider_id.clone(),
    )
    .await
    .expect("initialize state database");
    let store = LocalThreadStore::new(config, Some(state_db.clone()));

    for version in [
        MultiAgentVersion::V1,
        MultiAgentVersion::V2,
        MultiAgentVersion::Disabled,
    ] {
        let thread_id = ThreadId::new();
        create_paginated_thread(&store, thread_id).await;
        store
            .append_items(AppendThreadItemsParams {
                thread_id,
                items: vec![
                    turn_started("turn-1"),
                    turn_completed("turn-1"),
                    turn_started("turn-2"),
                    turn_completed("turn-2"),
                ],
            })
            .await
            .expect("append turns");
        let path = store
            .live_rollout_path(thread_id)
            .await
            .expect("source rollout path");
        store
            .shutdown_thread(thread_id)
            .await
            .expect("close source writer");
        codex_rollout::state_db::reconcile_rollout(
            Some(state_db.as_ref()),
            path.as_path(),
            "test-provider",
            /*builder*/ None,
            &[],
            /*archived_only*/ Some(false),
            /*new_thread_memory_mode*/ None,
        )
        .await;

        // The second revert has no live version to supply and must keep the persisted one.
        for (before_turn_id, multi_agent_version) in [("turn-2", Some(version)), ("turn-1", None)] {
            store
                .revert_thread(RevertThreadParams {
                    thread_id,
                    before_turn_id: before_turn_id.to_string(),
                    multi_agent_version,
                })
                .await
                .expect("revert thread");
            let path = state_db
                .get_thread(thread_id)
                .await
                .expect("read metadata")
                .expect("thread metadata")
                .rollout_path;
            let meta = codex_rollout::read_session_meta_line(&path)
                .await
                .expect("read replacement metadata");
            assert_eq!(meta.meta.multi_agent_version, Some(version));
        }
    }
}

async fn rollout_paths_for_thread(
    home: &std::path::Path,
    thread_id: ThreadId,
) -> Vec<std::path::PathBuf> {
    codex_rollout::RolloutReferenceIndex::scan(home)
        .await
        .expect("scan rollout references")
        .rollouts_for_thread(thread_id)
        .map(|(_, path)| path.to_path_buf())
        .collect()
}

async fn create_paginated_thread(store: &LocalThreadStore, thread_id: ThreadId) {
    store
        .create_thread(CreateThreadParams {
            attached_thread: false,
            creator_user_id: Some("creator-user".to_string()),
            creator_account_id: Some("creator-account".to_string()),
            session_id: thread_id.into(),
            thread_id,
            extra_config: None,
            forked_from_id: None,
            parent_thread_id: None,
            source: SessionSource::Exec,
            thread_source: None,
            originator: "test_originator".to_string(),
            base_instructions: BaseInstructions::default(),
            dynamic_tools: Vec::new(),
            selected_capability_roots: Vec::new(),
            multi_agent_version: None,
            history_mode: ThreadHistoryMode::Paginated,
            history_base: None,
            subagent_history_start_ordinal: None,
            initial_window_id: "window-1".to_string(),
            runtime_workspace_roots: Some(vec![store.config.codex_home.join("workspace").abs()]),
            metadata: ThreadPersistenceMetadata {
                cwd: Some(std::env::current_dir().expect("cwd")),
                model_provider: "test-provider".to_string(),
                memory_mode: ThreadMemoryMode::Enabled,
            },
        })
        .await
        .expect("create paginated thread");
}

// Revert must publish a readable replacement, including an empty retained history.
// Check the actual rollout ID: the thread ID still names the original history projection.
async fn assert_replacement_projected(store: &LocalThreadStore, path: &std::path::Path) {
    let rollout_id = codex_rollout::rollout_id_from_path(path).expect("replacement rollout ID");
    let checkpoint = super::super::thread_history::projection_state(store, rollout_id)
        .await
        .expect("read replacement projection")
        .expect("revert must project replacement before publishing the current path");
    assert_eq!(
        checkpoint.next_byte_offset,
        tokio::fs::metadata(path)
            .await
            .expect("replacement metadata")
            .len(),
        "replacement projection must cover the complete durable header"
    );
}

async fn turn_ids(store: &LocalThreadStore, thread_id: ThreadId) -> Vec<String> {
    store
        .list_turns(ListTurnsParams {
            thread_id,
            include_archived: false,
            cursor: None,
            page_size: 10,
            sort_direction: SortDirection::Asc,
            items_view: StoredTurnItemsView::NotLoaded,
        })
        .await
        .expect("list turns")
        .turns
        .into_iter()
        .map(|turn| turn.turn_id)
        .collect()
}

fn turn_started(turn_id: &str) -> RolloutItem {
    RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
        turn_attribution: None,
        turn_id: turn_id.to_string(),
        root_turn_id: None,
        trace_id: None,
        started_at: Some(10),
        model_context_window: None,
        collaboration_mode_kind: Default::default(),
    }))
}

fn turn_completed(turn_id: &str) -> RolloutItem {
    RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
        root_turn_id: None,
        turn_id: turn_id.to_string(),
        last_agent_message: None,
        error: None,
        started_at: Some(10),
        completed_at: Some(20),
        duration_ms: Some(10_000),
        time_to_first_token_ms: None,
    }))
}

fn compress_rollout(path: &std::path::Path) {
    let contents = std::fs::read(path).expect("read rollout");
    let compressed = zstd::stream::encode_all(contents.as_slice(), 3).expect("compress rollout");
    std::fs::write(path.with_extension("jsonl.zst"), compressed).expect("write compressed rollout");
    std::fs::remove_file(path).expect("remove plain rollout");
}
