// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use super::*;
use codex_utils_absolute_path::test_support::PathExt;

#[tokio::test]
async fn publication_commit_fences_expired_and_replaced_leases() -> anyhow::Result<()> {
    let home = super::super::test_support::unique_temp_dir();
    let db = StateRuntime::init(
        crate::SqliteConfig::new_for_testing(home.as_path().abs()),
        "test-provider".to_string(),
    ).await?;
    let store = db.memories();
    let root = "fixture-root";
    let old = uuid::Uuid::new_v4().to_string();
    let new = uuid::Uuid::new_v4().to_string();
    let notes = "a".repeat(64);
    let Phase2JobClaimOutcome::Claimed { ownership_token, input_watermark } =
        store.try_claim_global_phase2_job(ThreadId::new(), 3600).await? else { panic!("claim") };
    assert!(store.bind_phase2_publication_root(&ownership_token, root).await?);
    assert!(store.publish_global_phase2_generation(&ownership_token, input_watermark, input_watermark, &[], root, &old, &notes, &"b".repeat(64)).await?);
    assert_eq!(store.published_generation(root).await?.map(|r| r.generation_id), Some(old.clone()));
    let Phase2JobClaimOutcome::Claimed { ownership_token: stale, input_watermark } =
        store.try_claim_global_phase2_job_for_retry(ThreadId::new(), 3600).await? else { panic!("retry claim") };
    assert!(store.bind_phase2_publication_root(&stale, root).await?);
    sqlx::query("UPDATE jobs SET lease_until = 0 WHERE kind = 'memory_consolidate_global' AND job_key = 'global'")
        .execute(store.pool.as_ref()).await?;
    assert!(!store.heartbeat_global_phase2_job(&stale, 3600).await?);
    assert!(!store.publish_global_phase2_generation(&stale, input_watermark, input_watermark, &[], root, &new, &notes, &"b".repeat(64)).await?);
    assert_eq!(store.published_generation(root).await?.map(|r| r.generation_id), Some(old.clone()));
    let Phase2JobClaimOutcome::Claimed { ownership_token: winner, input_watermark } =
        store.try_claim_global_phase2_job_for_retry(ThreadId::new(), 3600).await? else { panic!("takeover") };
    assert!(store.bind_phase2_publication_root(&winner, root).await?);
    assert!(!store.publish_global_phase2_generation(&stale, input_watermark, input_watermark, &[], root, &new, &notes, &"b".repeat(64)).await?);
    assert_eq!(store.published_generation(root).await?.map(|r| r.generation_id), Some(old));
    assert!(store.publish_global_phase2_generation(&winner, input_watermark, input_watermark, &[], root, &new, &notes, &"b".repeat(64)).await?);
    assert_eq!(store.published_generation(root).await?.map(|r| r.generation_id), Some(new));
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn publication_rejects_invalid_generation_and_isolates_roots() -> anyhow::Result<()> {
    let home = super::super::test_support::unique_temp_dir();
    let db = StateRuntime::init(crate::SqliteConfig::new_for_testing(home.as_path().abs()), "test-provider".into()).await?;
    let store = db.memories();
    let Phase2JobClaimOutcome::Claimed { ownership_token, input_watermark } =
        store.try_claim_global_phase2_job(ThreadId::new(), 3600).await? else { panic!("claim") };
    assert!(store.bind_phase2_publication_root(&ownership_token, "root-a").await?);
    assert!(!store.bind_phase2_publication_root(&ownership_token, "root-b").await?);
    let notes = "a".repeat(64);
    assert!(store.publish_global_phase2_generation(&ownership_token, input_watermark, input_watermark, &[], "root-a", "../escape", &notes, &"b".repeat(64)).await.is_err());
    assert!(store.published_generation("root-a").await?.is_none());
    let generation = uuid::Uuid::new_v4().to_string();
    assert!(store.publish_global_phase2_generation(&ownership_token, input_watermark, input_watermark, &[], "root-a", &generation, &notes, &"b".repeat(64)).await?);
    assert!(store.published_generation("root-b").await?.is_none());
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn later_input_remains_pending_after_captured_generation_commit() -> anyhow::Result<()> {
    let home = super::super::test_support::unique_temp_dir();
    let db = StateRuntime::init(crate::SqliteConfig::new_for_testing(home.as_path().abs()), "test-provider".into()).await?;
    let store = db.memories();
    store.enqueue_global_consolidation(100).await?;
    let Phase2JobClaimOutcome::Claimed { ownership_token, input_watermark } =
        store.try_claim_global_phase2_job(ThreadId::new(), 3600).await? else { panic!("claim") };
    assert!(store.bind_phase2_publication_root(&ownership_token, "root").await?);
    store.enqueue_global_consolidation(50).await?;
    let generation = uuid::Uuid::new_v4().to_string();
    assert!(store.publish_global_phase2_generation(&ownership_token, input_watermark, 100, &[], "root", &generation, &"a".repeat(64), &"b".repeat(64)).await?);
    let status = store.consolidation_status().await?.expect("job");
    assert_eq!(status.status, "pending");
    assert!(status.input_watermark.unwrap() > input_watermark);
    assert_eq!(store.published_generation("root").await?.map(|r| r.generation_id), Some(generation));
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn legacy_baseline_registration_is_idempotent_and_never_overwrites() -> anyhow::Result<()> {
    let home = super::super::test_support::unique_temp_dir();
    let db = StateRuntime::init(crate::SqliteConfig::new_for_testing(home.as_path().abs()), "test-provider".into()).await?;
    let store = db.memories();
    let first = uuid::Uuid::new_v4().to_string();
    let second = uuid::Uuid::new_v4().to_string();
    let digest = "a".repeat(64);
    assert!(store.register_legacy_memory_baseline("root", &first, &digest, &"b".repeat(64), true).await?);
    assert!(!store.register_legacy_memory_baseline("root", &second, &digest, &"b".repeat(64), false).await?);
    let saved = store.published_generation("root").await?.expect("baseline");
    assert_eq!(saved.generation_id, first);
    assert_eq!(saved.provenance, "legacy_import");
    assert!(saved.legacy_empty);
    assert_eq!(saved.notes_digest, digest);
    assert!(!store.replace_legacy_memory_baseline("root", &second, &second, &digest, &"b".repeat(64), false).await?);
    assert!(store.replace_legacy_memory_baseline("root", &first, &second, &digest, &"b".repeat(64), false).await?);
    assert!(!store.replace_legacy_memory_baseline("root", &first, &first, &digest, &"b".repeat(64), true).await?);
    let updated = store.published_generation("root").await?.expect("replacement");
    assert_eq!(updated.generation_id, second);
    assert!(!updated.legacy_empty);
    db.close().await;
    Ok(())
}

#[tokio::test]
async fn memory_reset_removes_publication_pointer_with_job_data() -> anyhow::Result<()> {
    let home = super::super::test_support::unique_temp_dir();
    let db = StateRuntime::init(crate::SqliteConfig::new_for_testing(home.as_path().abs()), "test-provider".into()).await?;
    let store = db.memories();
    let generation = uuid::Uuid::new_v4().to_string();
    assert!(store.register_legacy_memory_baseline("root", &generation, &"a".repeat(64), &"b".repeat(64), false).await?);
    store.enqueue_global_consolidation(42).await?;
    store.clear_memory_data().await?;
    assert!(store.published_generation("root").await?.is_none());
    assert!(store.consolidation_status().await?.is_none());
    db.close().await;
    Ok(())
}
