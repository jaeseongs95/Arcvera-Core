use super::*;
use codex_protocol::MemoryVersion;
use codex_utils_absolute_path::test_support::PathExt;

#[tokio::test]
async fn consolidation_status_observes_pending_running_failure_retry_and_completion()
-> anyhow::Result<()> {
    let home = super::super::test_support::unique_temp_dir();
    let db = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let store = db.memories();
    assert!(store.consolidation_status().await?.is_none());
    store.enqueue_global_consolidation(42).await?;
    let pending = store.consolidation_status().await?.expect("pending");
    assert_eq!(pending.status, "pending");
    assert_eq!(pending.input_watermark, Some(42));
    let owner = ThreadId::new();
    let Phase2JobClaimOutcome::Claimed {
        ownership_token,
        input_watermark,
    } = store.try_claim_global_phase2_job(owner, 3600).await?
    else {
        panic!("first claim must succeed")
    };
    let running = store.consolidation_status().await?.expect("running");
    assert_eq!(running.status, "running");
    assert_eq!(running.worker_id, Some(owner.to_string()));
    assert!(running.lease_until.is_some());
    assert!(matches!(
        store
            .try_claim_global_phase2_job_for_retry(ThreadId::new(), 3600)
            .await?,
        Phase2JobClaimOutcome::SkippedRunning
    ));
    assert!(
        store
            .mark_global_phase2_job_failed(&ownership_token, "failed_invalid_artifacts", 3600)
            .await?
    );
    let failed = store.consolidation_status().await?.expect("failed");
    assert_eq!(failed.status, "error");
    assert_eq!(
        failed.last_error.as_deref(),
        Some("failed_invalid_artifacts")
    );
    assert!(failed.retry_at.is_some());
    assert!(matches!(
        store.try_claim_global_phase2_job(owner, 3600).await?,
        Phase2JobClaimOutcome::SkippedRetryUnavailable
    ));
    let Phase2JobClaimOutcome::Claimed {
        ownership_token: retry_token,
        ..
    } = store
        .try_claim_global_phase2_job_for_retry(owner, 3600)
        .await?
    else {
        panic!("explicit retry must bypass backoff")
    };
    assert!(
        !store
            .mark_global_phase2_job_succeeded(&ownership_token, input_watermark, &[])
            .await?,
        "old ownership must remain fenced after retry"
    );
    assert!(
        store
            .mark_global_phase2_job_succeeded(&retry_token, input_watermark, &[])
            .await?
    );
    let done = store.consolidation_status().await?.expect("done");
    assert_eq!(done.status, "done");
    assert_eq!(done.last_success_watermark, Some(42));
    assert!(done.finished_at.is_some());
    assert!(matches!(
        store.try_claim_global_phase2_job(owner, 3600).await?,
        Phase2JobClaimOutcome::SkippedCooldown
    ));
    assert!(matches!(
        store
            .try_claim_global_phase2_job_for_retry(owner, 3600)
            .await?,
        Phase2JobClaimOutcome::Claimed { .. }
    ));
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn consolidation_status_and_retry_are_isolated_by_project_and_version() -> anyhow::Result<()>
{
    let home = super::super::test_support::unique_temp_dir();
    let db = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let a = db
        .memories_for_project(MemoryVersion::V2, &"a".repeat(64))
        .await?;
    let b = db
        .memories_for_project(MemoryVersion::V2, &"b".repeat(64))
        .await?;
    let global_v2 = db.memories_for_version(MemoryVersion::V2).await?;
    a.enqueue_global_consolidation(77).await?;
    assert!(matches!(
        a.try_claim_global_phase2_job_for_retry(ThreadId::new(), 3600)
            .await?,
        Phase2JobClaimOutcome::Claimed { .. }
    ));
    assert_eq!(
        a.consolidation_status().await?.expect("a").status,
        "running"
    );
    assert!(b.consolidation_status().await?.is_none());
    assert!(global_v2.consolidation_status().await?.is_none());
    assert!(db.memories().consolidation_status().await?.is_none());
    a.close().await;
    b.close().await;
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn explicit_consolidation_retry_has_one_winner_across_independent_pools() -> anyhow::Result<()>
{
    let home = super::super::test_support::unique_temp_dir();
    let db = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    )
    .await?;
    let key = "c".repeat(64);
    let first = db.memories_for_project(MemoryVersion::V2, &key).await?;
    let second = db.memories_for_project(MemoryVersion::V2, &key).await?;
    let (a, b) = tokio::join!(
        first.try_claim_global_phase2_job_for_retry(ThreadId::new(), 3600),
        second.try_claim_global_phase2_job_for_retry(ThreadId::new(), 3600),
    );
    let outcomes = [a?, b?];
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Phase2JobClaimOutcome::Claimed { .. }))
            .count(),
        1
    );
    assert_eq!(
        outcomes
            .iter()
            .filter(|outcome| matches!(outcome, Phase2JobClaimOutcome::SkippedRunning))
            .count(),
        1
    );
    first.close().await;
    second.close().await;
    db.close().await;
    Ok(())
}
