// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! Synthetic current-thread integration checks. The ignored benchmark measures only local
//! model-input projection/normalization/serialization, never model quality or network latency.
use super::*;
use codex_attachment_store::InlineAttachmentStore;
use codex_history::CompactionResumeMetadata;
use codex_history::context_index::ContextIndex;
use codex_history::context_index::ContextIndexEvent;
use codex_history::context_index::ContextIndexOperation;
use codex_protocol::openai_models::InputModality;
use codex_utils_output_truncation::TruncationPolicy;
use pretty_assertions::assert_eq;
use serde_json::Value;
use std::future::Future;
use std::task::Poll;
use std::time::Duration;
use std::time::Instant;

fn indexed_message(id: &str, role: &str, text: &str) -> ResponseItem {
    ResponseItem::Message {
        id: Some(ResponseItemId::with_suffix("msg", id)),
        role: role.to_string(),
        content: vec![if role == "assistant" {
            ContentItem::OutputText {
                text: text.to_string(),
                annotations: None,
                logprobs: None,
            }
        } else {
            ContentItem::InputText {
                text: text.to_string(),
            }
        }],
        status: None,
        encrypted_content: None,
        phase: None,
        internal_chat_message_metadata_passthrough: None,
    }
}

async fn apply_index(session: &Arc<Session>, turn: &TurnContext, operations: Value) -> Value {
    let revision = session.clone_history().await.context_index_revision();
    session
        .context_index_action(
            turn,
            &json!({"action": "apply", "expected_revision": revision, "operations": operations})
                .to_string(),
        )
        .await
        .expect("valid native index transaction")
}

async fn prepared_json(session: &Session) -> String {
    let items = session
        .clone_history()
        .await
        .projected_for_prompt(&[InputModality::Text], &InlineAttachmentStore)
        .await;
    serde_json::to_string(&items).expect("prepared input JSON")
}

fn index_checkpoint(history: &ContextManager, index: ContextIndex) -> RolloutItem {
    RolloutItem::Compacted(Box::new(CompactedItem {
        message: "Synthetic indexed checkpoint".to_string(),
        replacement_history: Some(history.annotated_items().to_vec()),
        guardian_history: history.guardian_history_checkpoint(),
        retained_context: Some(history.retained_context().clone()),
        mcp_resource_origins: None,
        window_number: Some(1),
        first_window_id: None,
        previous_window_id: None,
        window_id: None,
        compaction_response_id: None,
        latest_token_usage_record: None,
        resume_metadata: Some(CompactionResumeMetadata {
            context_index: Some(index),
            dynamic_tools: None,
            multi_agent_version: None,
            last_started_turn_id: None,
            turn_attribution: None,
            previous_turn_settings: None,
        }),
    }))
}

#[tokio::test]
async fn indexed_context_exclusion_preserves_original_and_guardian_and_dependency_reactivates() {
    let (session, turn) = make_session_and_context().await;
    let session = Arc::new(session);
    session
        .record_conversation_items(
            &turn,
            turn.model_info(),
            &[
                indexed_message("done", "assistant", "COMPLETED_DETAIL_7f21"),
                indexed_message("current", "assistant", "CURRENT_DETAIL_b4e3"),
                indexed_message("correction", "user", "LATEST_CORRECTION_c2a5"),
                indexed_message("control", "developer", "REQUIRED_PERMISSION_9d10"),
            ],
        )
        .await;
    apply_index(
        &session,
        &turn,
        json!([
            {"op":"create_task","task_id":"done","label":"Completed work"},
            {"op":"create_task","task_id":"current","label":"Current work"},
            {"op":"bind_sources","task_id":"done","source_ids":["msg_done"]},
            {"op":"bind_sources","task_id":"current","source_ids":["msg_current"]},
            {"op":"set_task_state","task_id":"done","state":"completed"},
            {"op":"set_focus","task_id":"current"}
        ]),
    )
    .await;
    let projected = prepared_json(&session).await;
    assert!(!projected.contains("COMPLETED_DETAIL_7f21"));
    for required in [
        "CURRENT_DETAIL_b4e3",
        "LATEST_CORRECTION_c2a5",
        "REQUIRED_PERMISSION_9d10",
    ] {
        assert!(
            projected.contains(required),
            "missing required input: {required}"
        );
    }
    let raw = session.clone_history().await;
    let raw_json = serde_json::to_string(&raw.raw_items().collect::<Vec<_>>()).expect("raw JSON");
    assert!(raw_json.contains("COMPLETED_DETAIL_7f21"));
    let guardian = session.conversation_history_snapshot().await;
    let guardian_json =
        serde_json::to_string(&guardian.review_items().collect::<Vec<_>>()).expect("Guardian JSON");
    assert!(guardian_json.contains("COMPLETED_DETAIL_7f21"));
    apply_index(
        &session,
        &turn,
        json!([{"op":"add_dependency","task_id":"current","dependency_id":"done"}]),
    )
    .await;
    assert!(
        prepared_json(&session)
            .await
            .contains("COMPLETED_DETAIL_7f21")
    );
    assert_eq!(
        raw_json,
        serde_json::to_string(
            &session
                .clone_history()
                .await
                .raw_items()
                .collect::<Vec<_>>()
        )
        .expect("preserved raw JSON")
    );
}

#[tokio::test]
async fn indexed_context_focused_prepared_payload_omits_unrelated_active_task() {
    let (session, turn) = make_session_and_context().await;
    let session = Arc::new(session);
    session
        .record_conversation_items(
            &turn,
            turn.model_info(),
            &[
                indexed_message("focused", "assistant", "FOCUSED_PAYLOAD_17a2"),
                indexed_message("dependency", "assistant", "DEPENDENCY_PAYLOAD_2c41"),
                indexed_message("sibling", "assistant", "UNRELATED_ACTIVE_PAYLOAD_e509"),
                indexed_message("user", "user", "LATEST_USER_CORRECTION_68cb"),
                indexed_message("policy", "developer", "PERMISSION_CONSTRAINT_7f94"),
            ],
        )
        .await;
    apply_index(
        &session,
        &turn,
        json!([
            {"op":"create_task","task_id":"focused","label":"Current"},
            {"op":"create_task","task_id":"dependency","label":"Relevant earlier"},
            {"op":"create_task","task_id":"sibling","label":"Other active work"},
            {"op":"bind_sources","task_id":"focused","source_ids":["msg_focused"]},
            {"op":"bind_sources","task_id":"dependency","source_ids":["msg_dependency"]},
            {"op":"bind_sources","task_id":"sibling","source_ids":["msg_sibling"]},
            {"op":"add_dependency","task_id":"focused","dependency_id":"dependency"},
            {"op":"set_focus","task_id":"focused"}
        ]),
    )
    .await;
    let payload = prepared_json(&session).await;
    assert!(!payload.contains("UNRELATED_ACTIVE_PAYLOAD_e509"));
    for required in [
        "FOCUSED_PAYLOAD_17a2",
        "DEPENDENCY_PAYLOAD_2c41",
        "LATEST_USER_CORRECTION_68cb",
        "PERMISSION_CONSTRAINT_7f94",
    ] {
        assert!(
            payload.contains(required),
            "missing focused payload marker: {required}"
        );
    }
    assert_eq!(
        session.clone_history().await.context_index().tasks()["sibling"].state,
        codex_history::context_index::ContextTaskState::Active
    );
}

#[tokio::test]
async fn indexed_context_checkpoint_suffix_event_roundtrips_through_real_store_and_cold_resume() {
    let codex_home = tempfile::tempdir().expect("isolated current-thread store");
    let (mut session, turn, _events) = make_session_and_context_with_auth_config_home_and_rx(
        CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        codex_home.path(),
        |_| {},
    )
    .await;
    let rollout_path =
        attach_thread_persistence(Arc::get_mut(&mut session).expect("unique fixture")).await;
    session
        .record_conversation_items(
            &turn,
            turn.model_info(),
            &[
                indexed_message("archived", "assistant", "PERSISTED_ORIGINAL_aa19"),
                indexed_message("live", "assistant", "LIVE_WORK_c879"),
                indexed_message("latest", "user", "LATEST_CORRECTION_6a01"),
            ],
        )
        .await;
    apply_index(
        &session,
        &turn,
        json!([
            {"op":"create_task","task_id":"archived","label":"Earlier"},
            {"op":"create_task","task_id":"live","label":"Current"},
            {"op":"bind_sources","task_id":"archived","source_ids":["msg_archived"]},
            {"op":"bind_sources","task_id":"live","source_ids":["msg_live"]},
            {"op":"set_task_state","task_id":"archived","state":"completed"},
            {"op":"set_focus","task_id":"live"}
        ]),
    )
    .await;
    let history = session.clone_history().await;
    assert!(
        !prepared_json(&session)
            .await
            .contains("PERSISTED_ORIGINAL_aa19")
    );
    let checkpoint = index_checkpoint(&history, history.context_index().clone());
    assert!(session.persist_rollout_items(&[checkpoint]).await);
    apply_index(
        &session,
        &turn,
        json!([
            {"op":"add_dependency","task_id":"live","dependency_id":"archived"}
        ]),
    )
    .await;
    let expected = prepared_json(&session).await;
    assert!(expected.contains("PERSISTED_ORIGINAL_aa19"));
    let original = session
        .context_index_action(
            &turn,
            &json!({"action":"read","source_id":"msg_archived","max_bytes":16384}).to_string(),
        )
        .await
        .expect("persisted original read");
    assert_eq!(original["storage"], json!("persisted_original"));
    assert!(
        original["content"]
            .as_str()
            .expect("original content")
            .contains("PERSISTED_ORIGINAL_aa19")
    );
    session
        .flush_rollout()
        .await
        .expect("durable current-thread log");
    let InitialHistory::Resumed(resumed) = RolloutRecorder::get_rollout_history(&rollout_path)
        .await
        .expect("read actual current-thread store")
    else {
        panic!("resumed history");
    };
    let bytes = serde_json::to_vec(resumed.history.as_ref()).expect("checkpoint wire encoding");
    let decoded: Vec<RolloutItem> =
        serde_json::from_slice(&bytes).expect("checkpoint wire decoding");
    let (cold, _turn) = make_session_and_context().await;
    cold.record_initial_history(InitialHistory::Resumed(ResumedHistory {
        conversation_id: resumed.conversation_id,
        history: Arc::new(decoded),
        history_revision: None,
        rollout_path: resumed.rollout_path,
    }))
    .await;
    cold.ensure_context_index_ready()
        .await
        .expect("valid cold resume");
    assert_eq!(expected, prepared_json(&cold).await);
    assert_eq!(cold.clone_history().await.context_index_revision(), 2);
}

#[tokio::test]
async fn indexed_context_legacy_rollback_migration_preserves_prepared_payload() {
    use codex_protocol::protocol::SessionMeta;
    use codex_protocol::protocol::SessionMetaLine;
    use codex_protocol::protocol::ThreadRolledBackEvent;
    use codex_protocol::protocol::TurnCompleteEvent;
    use codex_protocol::protocol::TurnStartedEvent;
    use codex_protocol::protocol::UserMessageEvent;
    use codex_rollout::RolloutLine;
    use codex_thread_store::LoadThreadHistoryParams;
    use codex_thread_store::LocalThreadStore;
    use codex_thread_store::LocalThreadStoreConfig;
    use codex_thread_store::RolloutMigrationMode;
    use codex_thread_store::RolloutMigrationOptions;
    use codex_thread_store::RolloutMigrationStatus;
    use codex_thread_store::ThreadStore;

    let home = tempfile::tempdir().expect("isolated migration home");
    let config = build_test_config(home.path()).await;
    let thread_id = ThreadId::new();
    let timestamp = "2025-01-03T12:00:00Z";
    let directory = home.path().join("sessions/2025/01/03");
    std::fs::create_dir_all(&directory).expect("create synthetic rollout directory");
    let path = directory.join(format!("rollout-2025-01-03T12-00-00-{thread_id}.jsonl"));
    let started = |turn_id: &str| {
        RolloutItem::EventMsg(EventMsg::TurnStarted(TurnStartedEvent {
            turn_attribution: None,
            turn_id: turn_id.to_owned(),
            root_turn_id: None,
            trace_id: None,
            started_at: Some(1_735_905_600),
            model_context_window: None,
            collaboration_mode_kind: Default::default(),
        }))
    };
    let completed = |turn_id: &str| {
        RolloutItem::EventMsg(EventMsg::TurnComplete(TurnCompleteEvent {
            root_turn_id: None,
            turn_id: turn_id.to_owned(),
            last_agent_message: None,
            error: None,
            started_at: Some(1_735_905_600),
            completed_at: Some(1_735_905_601),
            duration_ms: Some(1_000),
            time_to_first_token_ms: None,
        }))
    };
    let user_event = |message: &str| {
        RolloutItem::EventMsg(EventMsg::UserMessage(UserMessageEvent {
            message: message.to_owned(),
            ..UserMessageEvent::default()
        }))
    };
    let mut discarded_index = ContextIndex::default();
    let discarded_event = discarded_index
        .apply(
            0,
            vec![ContextIndexOperation::CreateTask {
                task_id: "rolled-back".to_owned(),
                label: "Removed turn".to_owned(),
            }],
        )
        .expect("valid policy before rollback");
    let unrelated = indexed_message("unrelated", "assistant", "COMPLETED_DETAIL_9a57");
    let live = indexed_message("live", "assistant", "CURRENT_DETAIL_09dc");
    let mut final_index = ContextIndex::default();
    final_index.register(&unrelated);
    final_index.register(&live);
    // This is a native delta after destructive rollback, not a manufactured migration snapshot.
    let final_event = final_index
        .apply(
            0,
            vec![
                ContextIndexOperation::CreateTask {
                    task_id: "done".to_owned(),
                    label: "Completed".to_owned(),
                },
                ContextIndexOperation::CreateTask {
                    task_id: "live".to_owned(),
                    label: "Current".to_owned(),
                },
                ContextIndexOperation::BindSources {
                    task_id: "done".to_owned(),
                    source_ids: vec!["msg_unrelated".to_owned()],
                },
                ContextIndexOperation::BindSources {
                    task_id: "live".to_owned(),
                    source_ids: vec!["msg_live".to_owned()],
                },
                ContextIndexOperation::SetTaskState {
                    task_id: "done".to_owned(),
                    state: codex_history::context_index::ContextTaskState::Completed,
                },
                ContextIndexOperation::SetFocus {
                    task_id: "live".to_owned(),
                },
            ],
        )
        .expect("valid policy after rollback");
    let records = vec![
        RolloutItem::SessionMeta(Box::new(SessionMetaLine {
            meta: SessionMeta {
                session_id: thread_id.into(),
                id: thread_id,
                timestamp: timestamp.to_owned(),
                cwd: home.path().to_path_buf(),
                originator: "test-originator".to_owned(),
                cli_version: "0.0.0".to_owned(),
                source: SessionSource::Cli,
                model_provider: Some(config.model_provider_id.clone()),
                history_mode: ThreadHistoryMode::Legacy,
                ..SessionMeta::default()
            },
            git: None,
        })),
        started("legacy"),
        RolloutItem::ResponseItem(indexed_message("legacy", "user", "KEEP_LEGACY_3a01").into()),
        user_event("KEEP_LEGACY_3a01"),
        completed("legacy"),
        // Supported old text checkpoint: it is before the first indexed event.
        RolloutItem::Compacted(Box::new(CompactedItem {
            message: "LEGACY_SUMMARY_328f".to_owned(),
            replacement_history: None,
            guardian_history: None,
            retained_context: None,
            mcp_resource_origins: None,
            window_number: None,
            first_window_id: None,
            previous_window_id: None,
            window_id: None,
            compaction_response_id: None,
            latest_token_usage_record: None,
            resume_metadata: None,
        })),
        started("removed"),
        RolloutItem::ResponseItem(indexed_message("removed", "user", "ROLLED_BACK_61a3").into()),
        user_event("ROLLED_BACK_61a3"),
        RolloutItem::ContextIndex(discarded_event),
        completed("removed"),
        RolloutItem::EventMsg(EventMsg::ThreadRolledBack(ThreadRolledBackEvent {
            num_turns: 1,
        })),
        started("replacement"),
        RolloutItem::ResponseItem(
            indexed_message("correction", "user", "CURRENT_CORRECTION_d992").into(),
        ),
        user_event("CURRENT_CORRECTION_d992"),
        RolloutItem::ResponseItem(unrelated.into()),
        RolloutItem::ResponseItem(live.into()),
        RolloutItem::ContextIndex(final_event),
        completed("replacement"),
    ];
    let original = records
        .into_iter()
        .map(|item| {
            serde_json::to_string(&RolloutLine {
                timestamp: timestamp.to_owned(),
                ordinal: None,
                item,
            })
            .expect("encode synthetic original rollout")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    std::fs::write(&path, &original).expect("write synthetic original rollout");
    let state_db = codex_rollout::state_db::try_init(&config)
        .await
        .expect("backfill synthetic legacy thread metadata");
    let store = LocalThreadStore::new(LocalThreadStoreConfig::from_config(&config), Some(state_db));
    let params = LoadThreadHistoryParams {
        thread_id,
        include_archived: false,
    };
    let before = store
        .load_latest_model_context(params.clone())
        .await
        .expect("load original legacy context through store");
    let (before_session, _) = make_session_and_context().await;
    before_session
        .state
        .lock()
        .await
        .session_configuration
        .history_mode = ThreadHistoryMode::Legacy;
    before_session
        .record_initial_history(InitialHistory::Resumed(ResumedHistory {
            conversation_id: thread_id,
            history: Arc::new(before.items),
            history_revision: before.revision,
            rollout_path: Some(path.clone()),
        }))
        .await;
    before_session
        .ensure_context_index_ready()
        .await
        .expect("original index replay");
    let expected = prepared_json(&before_session).await;
    for marker in [
        "KEEP_LEGACY_3a01",
        "LEGACY_SUMMARY_328f",
        "CURRENT_CORRECTION_d992",
        "CURRENT_DETAIL_09dc",
    ] {
        assert!(
            expected.contains(marker),
            "original preparation lost {marker}"
        );
    }
    for marker in ["ROLLED_BACK_61a3", "COMPLETED_DETAIL_9a57"] {
        assert!(
            !expected.contains(marker),
            "original preparation leaked {marker}"
        );
    }
    assert_eq!(
        std::fs::read(&path).expect("original after preparation"),
        original.as_bytes()
    );
    drop(before_session);
    let report = store
        .migrate_rollouts(RolloutMigrationOptions {
            mode: RolloutMigrationMode::Apply,
            thread_ids: vec![thread_id],
            max_mib_per_second: Some(1024),
        })
        .await
        .expect("run actual migration Apply");
    assert_eq!(report.outcomes.len(), 1);
    assert_eq!(report.outcomes[0].thread_id, Some(thread_id));
    assert_eq!(report.outcomes[0].status, RolloutMigrationStatus::Migrated);
    let meta = codex_rollout::read_session_meta_line(&path)
        .await
        .expect("read migrated metadata");
    assert_eq!(meta.meta.history_mode, ThreadHistoryMode::Paginated);
    let after = store
        .load_latest_model_context(params)
        .await
        .expect("reload actual paginated model context");
    let persisted = serde_json::to_string(&after.items).expect("encode actual migrated context");
    assert!(
        persisted.contains("COMPLETED_DETAIL_9a57"),
        "excluded original must remain preserved"
    );
    assert!(
        !persisted.contains("ROLLED_BACK_61a3"),
        "migration must drop rolled-back turn"
    );
    let (after_session, _) = make_session_and_context().await;
    after_session
        .state
        .lock()
        .await
        .session_configuration
        .history_mode = ThreadHistoryMode::Paginated;
    after_session
        .record_initial_history(InitialHistory::Resumed(ResumedHistory {
            conversation_id: thread_id,
            history: Arc::new(after.items),
            history_revision: after.revision,
            rollout_path: Some(path),
        }))
        .await;
    after_session
        .ensure_context_index_ready()
        .await
        .expect("migrated index replay");
    assert_eq!(prepared_json(&after_session).await, expected);
}

async fn assert_replay_blocks_admission(items: Vec<RolloutItem>) {
    let (session, turn) = make_session_and_context().await;
    let session = Arc::new(session);
    session
        .record_initial_history(InitialHistory::Forked(items))
        .await;
    assert!(session.state.lock().await.context_index_error.is_some());
    assert!(session.ensure_context_index_ready().await.is_err());
    assert!(
        session
            .capture_step_context(Arc::new(turn), &CancellationToken::new())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn indexed_context_invalid_event_and_corrupt_snapshot_fail_closed_before_step_admission() {
    assert_replay_blocks_admission(vec![RolloutItem::ContextIndex(ContextIndexEvent {
        snapshot: None,
        expected_revision: 9,
        revision: 10,
        operations: vec![ContextIndexOperation::CreateTask {
            task_id: "invalid".to_string(),
            label: "Invalid revision".to_string(),
        }],
    })])
    .await;
    let mut history = ContextManager::new();
    let source = indexed_message("corrupt", "assistant", "PRESERVED_CORRUPT_SOURCE");
    history.record_items([&source], TruncationPolicy::Bytes(usize::MAX));
    let mut corrupt = serde_json::to_value(history.context_index()).expect("index JSON");
    corrupt["sources"]["msg_corrupt"]["task_id"] = json!("missing-task");
    let corrupt: ContextIndex =
        serde_json::from_value(corrupt).expect("decodable invalid references");
    assert_replay_blocks_admission(vec![index_checkpoint(&history, corrupt)]).await;
}

#[tokio::test]
async fn indexed_context_concurrent_same_revision_commits_once() {
    let codex_home = tempfile::tempdir().expect("isolated concurrent store");
    let (mut session, turn, _events) = make_session_and_context_with_auth_config_home_and_rx(
        CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        codex_home.path(),
        |_| {},
    )
    .await;
    let rollout_path =
        attach_thread_persistence(Arc::get_mut(&mut session).expect("unique fixture")).await;
    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let mut callers = Vec::new();
    for task_id in ["race-a", "race-b"] {
        let session = Arc::clone(&session);
        let turn = Arc::clone(&turn);
        let barrier = Arc::clone(&barrier);
        callers.push(tokio::spawn(async move {
            barrier.wait().await;
            session
                .context_index_action(
                    &turn,
                    &json!({"action":"apply","expected_revision":0,
                "operations":[{"op":"create_task","task_id":task_id,"label":task_id}]})
                    .to_string(),
                )
                .await
        }));
    }
    barrier.wait().await;
    let mut committed = 0;
    let mut rejected = 0;
    for caller in callers {
        match caller.await.expect("caller task") {
            Ok(value) => {
                assert_eq!(value["committed"], json!(true));
                committed += 1;
            }
            Err(FunctionCallError::RespondToModel(_)) => rejected += 1,
            Err(other) => panic!("unexpected failure: {other}"),
        }
    }
    assert_eq!((committed, rejected), (1, 1));
    let history = session.clone_history().await;
    assert_eq!(history.context_index_revision(), 1);
    assert_eq!(history.context_index().tasks().len(), 1);
    assert_eq!(history.context_index().audit().len(), 1);
    session
        .flush_rollout()
        .await
        .expect("concurrent transaction flushed");
    let InitialHistory::Resumed(persisted) = RolloutRecorder::get_rollout_history(&rollout_path)
        .await
        .expect("persisted race result")
    else {
        panic!("persisted history");
    };
    assert_eq!(
        persisted
            .history
            .iter()
            .filter(|item| matches!(item, RolloutItem::ContextIndex(_)))
            .count(),
        1
    );
}

#[tokio::test]
async fn indexed_context_accepted_commit_outlives_cancelled_caller() {
    let codex_home = tempfile::tempdir().expect("isolated cancellation store");
    let (mut session, turn, _events) = make_session_and_context_with_auth_config_home_and_rx(
        CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        codex_home.path(),
        |_| {},
    )
    .await;
    let rollout_path =
        attach_thread_persistence(Arc::get_mut(&mut session).expect("unique fixture")).await;
    let guard = session
        .thread_settings_persistence
        .acquire()
        .await
        .expect("persistence barrier");
    let request = json!({"action":"apply","expected_revision":0,
        "operations":[{"op":"create_task","task_id":"survives-caller","label":"Owned transaction"}]}).to_string();
    let mut caller = Box::pin(session.context_index_action(&turn, &request));
    // One explicit poll accepts the owned commit, then its JoinHandle waits behind the real
    // persistence semaphore. Dropping this waiter is the tool-cancellation boundary.
    let pending =
        std::future::poll_fn(|cx| Poll::Ready(caller.as_mut().poll(cx).is_pending())).await;
    assert!(pending);
    assert_eq!(session.clone_history().await.context_index_revision(), 0);
    drop(caller);
    drop(guard);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if session.clone_history().await.context_index_revision() == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("accepted transaction finishes after waiter cancellation");
    let history = session.clone_history().await;
    assert!(
        history
            .context_index()
            .tasks()
            .contains_key("survives-caller")
    );
    assert_eq!(history.context_index().audit().len(), 1);
    session
        .flush_rollout()
        .await
        .expect("owned transaction flushed");
    let InitialHistory::Resumed(persisted) = RolloutRecorder::get_rollout_history(&rollout_path)
        .await
        .expect("persisted cancellation result")
    else {
        panic!("persisted history");
    };
    assert_eq!(
        persisted
            .history
            .iter()
            .filter(|item| matches!(item, RolloutItem::ContextIndex(_)))
            .count(),
        1
    );
}

fn percentile(samples: &[u128], percentile: usize) -> u128 {
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    ordered[(ordered.len() * percentile).div_ceil(100).saturating_sub(1)]
}

#[tokio::test]
#[ignore = "coordinated projection-only benchmark; emits 30 paired samples for each workload"]
async fn indexed_context_projection_benchmark() {
    const SAMPLES: usize = 30;
    for source_count in [100, 1000, 10000] {
        let mut history = ContextManager::new();
        let body = "synthetic-context-detail ".repeat(48);
        let items = (0..source_count)
            .map(|id| indexed_message(&format!("bench-{id}"), "assistant", &format!("{id}:{body}")))
            .chain([
                indexed_message("bench-control", "developer", "BENCH_REQUIRED_CONTROL"),
                indexed_message("bench-user", "user", "BENCH_LATEST_CORRECTION"),
            ])
            .collect::<Vec<_>>();
        history.record_items(items.iter(), TruncationPolicy::Bytes(usize::MAX));
        let exclude_count = source_count * 9 / 10;
        let mut operations = vec![ContextIndexOperation::CreateTask {
            task_id: "completed-batch".to_string(),
            label: "Completed synthetic details".to_string(),
        }];
        let excluded_ids = (0..exclude_count)
            .map(|id| format!("msg_bench-{id}"))
            .collect::<Vec<_>>();
        for batch in excluded_ids.chunks(1024) {
            operations.push(ContextIndexOperation::BindSources {
                task_id: "completed-batch".to_string(),
                source_ids: batch.to_vec(),
            });
        }
        operations.push(ContextIndexOperation::SetTaskState {
            task_id: "completed-batch".to_string(),
            state: codex_history::context_index::ContextTaskState::Completed,
        });
        history
            .apply_context_index(0, operations)
            .expect("benchmark task setup");
        assert_eq!(history.raw_items().count(), source_count + 2);
        assert_eq!(
            history.projected_items().len(),
            source_count - exclude_count + 2
        );
        // Same synthetic history, warmup count, text modality and serializer. Alternate order
        // each round; projection time includes clone, manifest, normalization and serialization.
        let mut baseline = Vec::new();
        let mut indexed = Vec::new();
        let mut baseline_bytes = 0;
        let mut indexed_bytes = 0;
        for iteration in 0..SAMPLES + 5 {
            for projected in if iteration % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                let started = Instant::now();
                let input = if projected {
                    history
                        .clone()
                        .projected_for_prompt(&[InputModality::Text], &InlineAttachmentStore)
                        .await
                } else {
                    history
                        .clone()
                        .for_prompt(&[InputModality::Text], &InlineAttachmentStore)
                        .await
                };
                let encoded = serde_json::to_vec(&input).expect("prepared benchmark input");
                let nanos = started.elapsed().as_nanos();
                std::hint::black_box(&encoded);
                if iteration >= 5 {
                    let variant = if projected {
                        "indexed_active"
                    } else {
                        "baseline_unfiltered"
                    };
                    println!(
                        "INDEXED_CONTEXT_PROJECTION_SAMPLE {}",
                        json!({"source_count":source_count,"sample":iteration-5,"variant":variant,"nanos":nanos,"bytes":encoded.len()})
                    );
                    if projected {
                        indexed.push(nanos);
                        indexed_bytes = encoded.len();
                    } else {
                        baseline.push(nanos);
                        baseline_bytes = encoded.len();
                    }
                }
            }
        }
        assert!(indexed_bytes < baseline_bytes);
        println!(
            "INDEXED_CONTEXT_PROJECTION_SUMMARY {}",
            json!({
                "scope":"local projection/normalization/serialization only; no model summary/network/app workload",
                "source_count":source_count,"samples_per_variant":SAMPLES,"warmup_pairs":5,
                "baseline":{"p50_nanos":percentile(&baseline,50),"p95_nanos":percentile(&baseline,95),"bytes":baseline_bytes},
                "indexed":{"p50_nanos":percentile(&indexed,50),"p95_nanos":percentile(&indexed,95),"bytes":indexed_bytes}
            })
        );
    }
}

#[tokio::test]
async fn indexed_context_original_read_rejects_conflicting_reused_locator() {
    for persisted in [false, true] {
        let codex_home = tempfile::tempdir().expect("isolated original locator store");
        let (mut session, turn, _events) = make_session_and_context_with_auth_config_home_and_rx(
            CodexAuth::from_api_key("Test API Key"),
            Vec::new(),
            codex_home.path(),
            |_| {},
        )
        .await;
        if persisted {
            attach_thread_persistence(Arc::get_mut(&mut session).expect("unique fixture")).await;
        } else {
            assert!(session.live_thread().is_none());
        }
        let original = indexed_message("reused-original", "assistant", "FIRST_ORIGINAL_BODY");
        session
            .record_conversation_items(&turn, turn.model_info(), &[original.clone(), original])
            .await;
        let request = json!({"action":"read","source_id":"msg_reused-original","max_bytes":16384})
            .to_string();
        let read = session
            .context_index_action(&turn, &request)
            .await
            .expect("identical duplicate has a unique body");
        assert!(
            read["content"]
                .as_str()
                .expect("body chunk")
                .contains("FIRST_ORIGINAL_BODY")
        );
        session
            .record_conversation_items(
                &turn,
                turn.model_info(),
                &[indexed_message(
                    "reused-original",
                    "assistant",
                    "CONFLICTING_ORIGINAL_BODY",
                )],
            )
            .await;
        let failure = session
            .context_index_action(&turn, &request)
            .await
            .expect_err("a conflicting locator cannot select the first original silently");
        assert!(failure.to_string().contains("ambiguous original source"));
        let history = session.clone_history().await;
        let raw = serde_json::to_string(&history.raw_items().collect::<Vec<_>>())
            .expect("preserved history");
        assert!(raw.contains("FIRST_ORIGINAL_BODY"));
        assert!(raw.contains("CONFLICTING_ORIGINAL_BODY"));
        assert!(session.state.lock().await.context_index_error.is_none());
    }
}

#[tokio::test]
async fn indexed_context_real_writer_failure_blocks_apply_and_preserves_original() {
    let codex_home = tempfile::tempdir().expect("isolated failed-append store");
    let (mut session, turn, _events) = make_session_and_context_with_auth_config_home_and_rx(
        CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        codex_home.path(),
        |_| {},
    )
    .await;
    let rollout_path =
        attach_thread_persistence(Arc::get_mut(&mut session).expect("unique fixture")).await;
    session
        .live_thread()
        .expect("test-owned writer")
        .shutdown()
        .await
        .expect("close test-owned writer");
    let original = tokio::fs::read(&rollout_path)
        .await
        .expect("original bytes");
    let request = json!({"action":"apply","expected_revision":0,
        "operations":[{"op":"create_task","task_id":"must-not-publish","label":"Failed append"}]})
    .to_string();
    let failure = session
        .context_index_action(&turn, &request)
        .await
        .expect_err("closed local writer must fail");
    assert!(failure.to_string().contains("append outcome is uncertain"));
    let history = session.clone_history().await;
    assert_eq!(history.context_index_revision(), 0);
    assert!(
        !history
            .context_index()
            .tasks()
            .contains_key("must-not-publish")
    );
    assert!(session.state.lock().await.context_index_error.is_some());
    assert!(session.state.lock().await.history_reset.is_cancelled());
    assert!(session.ensure_context_index_ready().await.is_err());
    assert!(
        session
            .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
            .await
            .is_err()
    );
    assert!(session.context_index_action(&turn, &request).await.is_err());
    assert_eq!(
        tokio::fs::read(&rollout_path)
            .await
            .expect("preserved bytes"),
        original
    );
}

#[tokio::test]
async fn indexed_context_fresh_window_reports_real_persistence_failure() {
    let codex_home = tempfile::tempdir().expect("isolated failed-window store");
    let (mut session, turn, _events) = make_session_and_context_with_auth_config_home_and_rx(
        CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        codex_home.path(),
        |_| {},
    )
    .await;
    let rollout_path =
        attach_thread_persistence(Arc::get_mut(&mut session).expect("unique fixture")).await;
    let step_context = session
        .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
        .await
        .expect("healthy admission");
    let world_state = Arc::new(
        session
            .build_world_state_for_step(&step_context, true)
            .await
            .expect("test world state"),
    );
    session
        .live_thread()
        .expect("test-owned writer")
        .shutdown()
        .await
        .expect("close test-owned writer");
    let original = tokio::fs::read(&rollout_path)
        .await
        .expect("original bytes");
    assert!(
        session
            .start_new_context_window(&step_context, world_state)
            .await
            .is_err()
    );
    assert!(session.state.lock().await.context_index_error.is_some());
    assert!(session.state.lock().await.history_reset.is_cancelled());
    assert!(session.ensure_context_index_ready().await.is_err());
    assert!(
        session
            .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(
        tokio::fs::read(&rollout_path)
            .await
            .expect("preserved bytes"),
        original
    );
}

#[tokio::test]
async fn indexed_context_response_append_failure_blocks_binding_before_policy_commit() {
    let codex_home = tempfile::tempdir().expect("isolated failed-source store");
    let (mut session, turn, _events) = make_session_and_context_with_auth_config_home_and_rx(
        CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        codex_home.path(),
        |_| {},
    )
    .await;
    let rollout_path =
        attach_thread_persistence(Arc::get_mut(&mut session).expect("unique fixture")).await;
    session
        .record_conversation_items(
            &turn,
            turn.model_info(),
            &[indexed_message(
                "durable-control",
                "developer",
                "DURABLE_PERMISSION_ROOT",
            )],
        )
        .await;
    session
        .live_thread()
        .expect("test-owned writer")
        .shutdown()
        .await
        .expect("close test-owned writer");
    let original = tokio::fs::read(&rollout_path)
        .await
        .expect("original bytes");
    session
        .record_conversation_items(
            &turn,
            turn.model_info(),
            &[indexed_message(
                "memory-only",
                "assistant",
                "UNPERSISTED_SOURCE",
            )],
        )
        .await;
    // Assert before apply: commit's own append-error handling cannot mask this regression.
    assert!(session.state.lock().await.context_index_error.is_some());
    assert!(session.state.lock().await.history_reset.is_cancelled());
    assert!(session.ensure_context_index_ready().await.is_err());
    let request = json!({"action":"apply","expected_revision":0,"operations":[
        {"op":"create_task","task_id":"unpersisted","label":"Must reject"},
        {"op":"bind_sources","task_id":"unpersisted","source_ids":["msg_memory-only"]}
    ]})
    .to_string();
    let failure = session
        .context_index_action(&turn, &request)
        .await
        .expect_err("readiness must reject before append");
    assert!(
        failure
            .to_string()
            .contains("response item append outcome is uncertain")
    );
    let history = session.clone_history().await;
    assert_eq!(history.context_index_revision(), 0);
    assert!(history.context_index().tasks().is_empty());
    let raw = serde_json::to_string(&history.raw_items().collect::<Vec<_>>()).expect("raw history");
    assert!(raw.contains("DURABLE_PERMISSION_ROOT"));
    assert!(raw.contains("UNPERSISTED_SOURCE"));
    assert!(
        session
            .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(
        tokio::fs::read(&rollout_path)
            .await
            .expect("preserved bytes"),
        original
    );

    let (healthy, healthy_turn) = make_session_and_context().await;
    assert!(
        healthy.live_thread().is_none(),
        "control is genuinely ephemeral"
    );
    healthy
        .record_conversation_items(
            &healthy_turn,
            healthy_turn.model_info(),
            &[indexed_message(
                "ephemeral",
                "assistant",
                "EPHEMERAL_SOURCE",
            )],
        )
        .await;
    assert!(healthy.state.lock().await.context_index_error.is_none());
    assert!(!healthy.state.lock().await.history_reset.is_cancelled());
    assert!(healthy.ensure_context_index_ready().await.is_ok());
}

#[tokio::test]
async fn indexed_context_inter_agent_append_failure_blocks_policy_before_commit() {
    let codex_home = tempfile::tempdir().expect("isolated failed-agent store");
    let (mut session, turn, _events) = make_session_and_context_with_auth_config_home_and_rx(
        CodexAuth::from_api_key("Test API Key"),
        Vec::new(),
        codex_home.path(),
        |_| {},
    )
    .await;
    let path = attach_thread_persistence(Arc::get_mut(&mut session).expect("unique fixture")).await;
    session
        .live_thread()
        .expect("test-owned writer")
        .shutdown()
        .await
        .expect("close writer");
    let original = tokio::fs::read(&path)
        .await
        .expect("original rollout bytes");
    let communication = InterAgentCommunication::new(
        AgentPath::root().join("worker").expect("worker path"),
        AgentPath::root(),
        Vec::new(),
        "UNPERSISTED_AGENT_ORIGINAL".to_owned(),
        false,
    );
    session
        .record_inter_agent_communication(&turn, turn.model_info(), communication)
        .await;
    assert!(
        session.state.lock().await.context_index_error.is_some(),
        "failed agent append must poison before a policy commit"
    );
    assert!(session.state.lock().await.history_reset.is_cancelled());
    assert!(session.ensure_context_index_ready().await.is_err());
    let raw = serde_json::to_string(
        &session
            .clone_history()
            .await
            .raw_items()
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(raw.contains("UNPERSISTED_AGENT_ORIGINAL"));
    assert!(
        session
            .capture_step_context(Arc::clone(&turn), &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(tokio::fs::read(&path).await.unwrap(), original);
}
