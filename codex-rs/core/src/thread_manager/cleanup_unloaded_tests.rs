use super::*;

async fn stored_child(
    manager: &ThreadManager,
    config: &crate::config::Config,
    parent: &super::super::super::NewThread,
) -> super::super::super::NewThread {
    let child = child(manager, config, parent).await;
    manager
        .state
        .agent_graph_store()
        .unwrap()
        .upsert_thread_spawn_edge(
            parent.thread_id,
            child.thread_id,
            codex_agent_graph_store::ThreadSpawnEdgeStatus::Open,
        )
        .await
        .unwrap();
    child
}

async fn unload(manager: &ThreadManager, child: &super::super::super::NewThread) {
    child.thread.io.shutdown_and_wait().await.unwrap();
    assert!(child.thread.io.admission.shutdown_confirmed());
    manager
        .remove_thread_if_matches(&child.thread_id, &child.thread)
        .await
        .unwrap();
}

#[tokio::test]
async fn unloaded_terminal_cleanup_uses_metadata_without_loading_and_deletes_only_selection() {
    let ((_home, manager, config, root), _) = fixture_with_graph_options(None, true).await;
    let done = stored_child(&manager, &config, &root).await;
    let sibling = stored_child(&manager, &config, &root).await;
    complete(&done.thread).await;
    complete(&sibling.thread).await;
    unload(&manager, &done).await;
    unload(&manager, &sibling).await;
    let preview = manager
        .preview_completed_children(root.thread_id, root.thread_id)
        .await
        .unwrap();
    let candidate = preview
        .iter()
        .find(|entry| entry.thread_id == done.thread_id)
        .unwrap();
    assert_eq!(candidate.outcome, "eligible");
    assert_eq!(candidate.reason, "unloaded_terminal");
    let snapshot = candidate.snapshot.clone().unwrap();
    assert!(snapshot.runtime_id.starts_with("stored-v1:"));
    assert!(
        !snapshot
            .runtime_id
            .contains(&config.codex_home.display().to_string())
    );
    assert_eq!(manager.list_thread_ids().await, vec![root.thread_id]);
    let results = manager
        .cleanup_completed_children(root.thread_id, root.thread_id, vec![snapshot])
        .await
        .unwrap();
    assert_eq!(results[0].outcome, "deleted");
    assert!(!stored(&manager, done.thread_id).await);
    assert!(stored(&manager, sibling.thread_id).await);
    assert!(stored(&manager, root.thread_id).await);
    assert!(manager.get_thread(done.thread_id).await.is_err());
    assert!(manager.admit_thread_activity(done.thread_id).is_err());
    assert!(manager.admit_thread_activity(sibling.thread_id).is_ok());
}

#[tokio::test]
async fn unloaded_cleanup_rejects_new_turn_after_preview_and_keeps_history() {
    use std::io::Write;
    let ((_home, manager, config, root), _) = fixture_with_graph_options(None, true).await;
    let done = stored_child(&manager, &config, &root).await;
    complete(&done.thread).await;
    unload(&manager, &done).await;
    let selected = selection(&manager, root.thread_id).await;
    assert_eq!(selected.len(), 1);
    let path = done.thread.rollout_path().unwrap();
    let next_ordinal = std::fs::read_to_string(&path)
        .unwrap()
        .lines()
        .last()
        .map(|line| {
            serde_json::from_str::<serde_json::Value>(line).unwrap()["ordinal"]
                .as_u64()
                .unwrap()
                + 1
        })
        .unwrap();
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(file, "{}", serde_json::json!({"ordinal":next_ordinal, "type":"event_msg", "payload":{"type":"task_started", "turn_id":"new-work"}})).unwrap();
    drop(file);
    let results = manager
        .cleanup_completed_children(root.thread_id, root.thread_id, selected)
        .await
        .unwrap();
    assert_eq!(results[0].outcome, "terminal_turn_unverified");
    assert!(stored(&manager, done.thread_id).await);
    assert!(path.exists());
    assert!(manager.admit_thread_activity(done.thread_id).is_ok());
    let preview = manager
        .preview_completed_children(root.thread_id, root.thread_id)
        .await
        .unwrap();
    assert_eq!(preview[0].outcome, "preserved");
    assert_eq!(preview[0].reason, "terminal_turn_unverified");
    assert!(preview[0].snapshot.is_none());
}

#[tokio::test]
async fn unloaded_cleanup_preserves_a_foreign_manager_writer_and_stale_loaded_snapshot() {
    let ((_home, manager, config, root), _) = fixture_with_graph_options(None, true).await;
    let done = stored_child(&manager, &config, &root).await;
    complete(&done.thread).await;
    let loaded = selection(&manager, root.thread_id).await;
    unload(&manager, &done).await;
    let stale = manager
        .cleanup_completed_children(root.thread_id, root.thread_id, loaded)
        .await
        .unwrap();
    assert_eq!(stale[0].outcome, "changed");
    let selected = selection(&manager, root.thread_id).await;
    let foreign = LocalThreadStore::new(
        codex_thread_store::LocalThreadStoreConfig::from_config(&config),
        None,
    );
    let foreign_writer = foreign
        .reserve_unloaded_cleanup(done.thread_id)
        .await
        .unwrap();
    let preview = manager
        .preview_completed_children(root.thread_id, root.thread_id)
        .await
        .unwrap();
    assert_eq!(preview[0].outcome, "preserved");
    assert_eq!(preview[0].reason, "writer_owned");
    let results = manager
        .cleanup_completed_children(root.thread_id, root.thread_id, selected)
        .await
        .unwrap();
    assert_eq!(results[0].outcome, "writer_owned");
    assert!(stored(&manager, done.thread_id).await);
    drop(foreign_writer);
    assert_eq!(selection(&manager, root.thread_id).await.len(), 1);
}

#[tokio::test]
async fn unloaded_cleanup_owner_keeps_writer_and_admission_after_request_cancellation() {
    let ((_home, manager, config, root), faults) = fixture_with_graph_options(None, true).await;
    let done = stored_child(&manager, &config, &root).await;
    complete(&done.thread).await;
    unload(&manager, &done).await;
    let selected = selection(&manager, root.thread_id).await;
    let barrier = Arc::new(StoreBarrier::default());
    *faults.cold_delete.lock().unwrap() = Some((done.thread_id, Arc::clone(&barrier)));
    let owner = copy_manager(&manager);
    let root_id = root.thread_id;
    let request = tokio::spawn(async move {
        owner
            .cleanup_completed_children(root_id, root_id, selected)
            .await
    });
    wait_for(&barrier.entered).await;
    request.abort();
    assert!(request.await.unwrap_err().is_cancelled());
    assert!(manager.admit_thread_activity(done.thread_id).is_err());
    assert!(manager.admit_thread_activity(root_id).is_ok());
    let foreign = LocalThreadStore::new(
        codex_thread_store::LocalThreadStoreConfig::from_config(&config),
        None,
    );
    assert!(matches!(
        foreign.reserve_unloaded_cleanup(done.thread_id).await,
        Err(store::ThreadStoreError::Conflict { .. })
    ));
    assert!(stored(&manager, done.thread_id).await);
    barrier.release.notify_one();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while stored(&manager, done.thread_id).await {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(stored(&manager, root_id).await);
    assert!(manager.get_thread(done.thread_id).await.is_err());
}
