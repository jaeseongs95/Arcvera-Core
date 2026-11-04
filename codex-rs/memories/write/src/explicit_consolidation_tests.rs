use super::*;
use crate::MemoryConsolidationStartOutcome;
use crate::MemoryWorkspaceLock;
use crate::consolidate_memories;
use pretty_assertions::assert_eq;

async fn consolidate(
    test: &TestCodex,
    retry: bool,
) -> anyhow::Result<MemoryConsolidationStartOutcome> {
    let mut config = test.config.clone();
    config.features.enable(Feature::MemoryTool)?;
    let permissions = config.permissions.effective_permission_profile();
    let snapshot = test.codex.config_snapshot().await;
    consolidate_memories(
        Arc::clone(&test.thread_manager),
        test.thread_manager.auth_manager(),
        test.session_configured.thread_id,
        Arc::clone(&test.codex),
        Arc::new(config),
        permissions,
        &snapshot.session_source,
        retry,
    )
    .await
}

#[tokio::test]
async fn explicit_consolidation_reports_workspace_busy_without_claim_or_model_request()
-> anyhow::Result<()> {
    let server = start_mock_server().await;
    let home = Arc::new(TempDir::new()?);
    let test = build_test_codex(&server, Arc::clone(&home)).await?;
    let root = home.path().join("memories");
    let lock = MemoryWorkspaceLock::try_acquire(&root)?;
    let outcome = consolidate(&test, true).await?;
    assert_eq!(outcome.status, "blocked");
    assert_eq!(outcome.reason.as_deref(), Some("skipped_workspace_busy"));
    assert!(
        test.codex
            .state_db()
            .expect("state")
            .memories()
            .consolidation_status()
            .await?
            .is_none()
    );
    assert!(
        !server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.url.path().ends_with("/responses"))
    );
    drop(lock);
    shutdown_test_codex(&test).await?;
    Ok(())
}

#[tokio::test]
async fn explicit_consolidation_respects_live_generation_disabled_without_creating_workspace()
-> anyhow::Result<()> {
    let server = start_mock_server().await;
    let home = Arc::new(TempDir::new()?);
    let test = build_test_codex(&server, Arc::clone(&home)).await?;
    let db = test.codex.state_db().expect("state");
    let worker = test.session_configured.thread_id;
    let metadata = codex_state::ThreadMetadataBuilder::new(
        worker,
        home.path().join("worker.jsonl"),
        chrono::Utc::now(),
        SessionSource::Cli,
    )
    .build("test-provider");
    db.upsert_thread(&metadata).await?;
    db.set_thread_memory_mode(worker, "disabled").await?;
    let outcome = consolidate(&test, true).await?;
    assert_eq!(outcome.status, "blocked");
    assert_eq!(outcome.reason.as_deref(), Some("generation_disabled"));
    assert!(!home.path().join("memories").exists());
    assert!(db.memories().consolidation_status().await?.is_none());
    assert!(
        !server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.url.path().ends_with("/responses"))
    );
    shutdown_test_codex(&test).await?;
    Ok(())
}

#[tokio::test]
async fn explicit_consolidation_uses_only_phase2_and_reports_invalid_artifacts_as_failure()
-> anyhow::Result<()> {
    let server = start_mock_server().await;
    let home = Arc::new(TempDir::new()?);
    let test = build_test_codex(&server, Arc::clone(&home)).await?;
    let db = test.codex.state_db().expect("state");
    // If phase 1 were accidentally called, this eligible source would be extracted.
    seed_stage1_candidate(
        &db,
        home.path(),
        chrono::Utc::now() - chrono::Duration::hours(2),
        "explicit-source",
    )
    .await?;
    let response = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("explicit-phase2"),
            ev_assistant_message(
                "explicit-message",
                "finished without writing required artifacts",
            ),
            ev_completed("explicit-phase2"),
        ]),
    )
    .await;
    let outcome = consolidate(&test, false).await?;
    assert_eq!(outcome.status, "accepted");
    let request = wait_for_single_request(&response).await;
    assert!(!request.body_contains_text("remember this startup test conversation"));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = db.memories().consolidation_status().await?
            && status.status == "error"
        {
            assert_eq!(
                status.last_error.as_deref(),
                Some("failed_invalid_artifacts")
            );
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "phase2 failure was not persisted"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        db.memories()
            .list_stage1_outputs_for_global(10)
            .await?
            .is_empty()
    );
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path().ends_with("/responses"))
            .count(),
        1
    );
    shutdown_test_codex(&test).await?;
    Ok(())
}

#[tokio::test]
async fn explicit_consolidation_completion_is_job_state_not_a_note_application_receipt()
-> anyhow::Result<()> {
    let server = start_mock_server().await;
    let home = Arc::new(TempDir::new()?);
    let test = build_test_codex(&server, Arc::clone(&home)).await?;
    let root = home.path().join("memories");
    seed_required_memory_artifacts(&root).await?;
    let previous_summary = tokio::fs::read_to_string(root.join("memory_summary.md")).await?;
    let note = crate::add_user_memory_note(&root, "Explicit correction from this test").await?;
    let response = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("explicit-completion"),
            ev_assistant_message("explicit-completion-message", "worker completed"),
            ev_completed("explicit-completion"),
        ]),
    )
    .await;
    assert_eq!(consolidate(&test, false).await?.status, "accepted");
    wait_for_single_request(&response).await;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = crate::memory_consolidation_status(&test.codex, &test.config).await?
            && status.status == "done"
        {
            assert!(status.finished_at.is_some());
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "phase2 completion was not persisted"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!diff_since_latest_init(&root).await?.has_changes());
    assert!(note.is_file());
    assert_eq!(
        tokio::fs::read_to_string(root.join("memory_summary.md")).await?,
        previous_summary
    );
    let locked = MemoryWorkspaceLock::try_acquire(&root)?;
    drop(locked);
    let cooldown = consolidate(&test, false).await?;
    assert_eq!(cooldown.status, "blocked");
    assert_eq!(cooldown.reason.as_deref(), Some("skipped_cooldown"));
    let retry = consolidate(&test, true).await?;
    assert_eq!(retry.status, "completed");
    assert_eq!(retry.reason.as_deref(), Some("no_workspace_changes"));
    // A note accepted after the last job cannot inherit that job's completion.
    crate::add_user_memory_note(&root, "Later correction not processed by this worker").await?;
    assert_eq!(
        crate::memory_consolidation_status(&test.codex, &test.config)
            .await?
            .expect("previous job")
            .status,
        "done"
    );
    assert!(diff_since_latest_init(&root).await?.has_changes());
    shutdown_test_codex(&test).await?;
    Ok(())
}
