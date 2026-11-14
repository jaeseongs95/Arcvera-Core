// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Host-owned staging and publication for one captured memory input set.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

use codex_core::config::Config;
use codex_protocol::MemoryVersion;
use codex_state::{MemoryStore, Stage1Output};
use codex_memories_read::publication_notes::notes_digest;
use codex_memories_read::publication_snapshot::{generation_root, managed_marker, root_identity};

use crate::MemoryWorkspaceLock;
use crate::publication_manifest::{changed_inputs, include_extension_resources, read_manifest, removed_filenames, selected_manifest, validate_summary_pointers};
use crate::publication_stage::{check_generation_capacity, cleanup_published_stage, create_stage, ensure_managed_marker, legacy_artifacts, prepare_empty_legacy_generation, prepare_generation, reject_stage_references, staged_inputs, validate_generation, validate_root};
use crate::workspace::validate_consolidation_artifacts_for_version;

pub(crate) struct PreparedPublication {
    pub stage: PathBuf,
    pub root_identity: String,
    pub notes_digest: String,
    pub inputs: BTreeMap<PathBuf, String>,
    pub manifest: BTreeMap<String, String>,
    pub gate: Arc<Mutex<()>>,
    pub tool_failed: Arc<AtomicBool>,
    pub previous_generation: String,
    pub previous_artifact_digest: String,
    pub unchanged: bool,
}

fn artifact_digest(root: &Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    codex_memories_read::publication_notes::open_regular_without_links(&root.join(codex_memories_read::publication_snapshot::ARTIFACT_MANIFEST))?
        .take(2 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 2 * 1024 * 1024 { return Err(std::io::Error::other("memory artifact manifest exceeds limit")); }
    Ok(codex_memories_read::publication_snapshot::artifact_manifest_digest(&bytes))
}

async fn io_task<T: Send + 'static>(lock: Arc<MemoryWorkspaceLock>, task: impl FnOnce() -> std::io::Result<T> + Send + 'static) -> std::io::Result<T> {
    tokio::task::spawn_blocking(move || { let _lock = lock; task() }).await.map_err(std::io::Error::other)?
}

/// Initialize legacy state before marker creation, or resolve the pinned managed
/// generation. Existing original artifacts are never overwritten or removed.
async fn previous_generation(
    root: &Path, version: MemoryVersion, db: &MemoryStore,
    lock: Arc<MemoryWorkspaceLock>, identity: &str,
) -> anyhow::Result<codex_state::MemoryPublicationRecord> {
    let v1 = version == MemoryVersion::V1;
    let root_owned = root.to_path_buf();
    let marker = io_task(Arc::clone(&lock), move || managed_marker(&root_owned)).await?;
    let existing = db.published_generation(identity).await?;
    if marker {
        return existing.ok_or_else(|| anyhow::anyhow!("managed memory has no publication record"));
    }
    if existing.as_ref().is_some_and(|record| record.provenance != "legacy_import") {
        anyhow::bail!("managed publication is missing its marker");
    }
    let root_owned = root.to_path_buf();
    let (before, notes, stage) = io_task(Arc::clone(&lock), move || {
        match std::fs::create_dir(&root_owned) {
            Ok(()) => {},
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {},
            Err(error) => return Err(error),
        }
        validate_root(&root_owned)?;
        let before = legacy_artifacts(&root_owned, v1)?;
        let notes = notes_digest(&root_owned)?;
        let stage = create_stage(&root_owned, &root_owned, v1)?;
        if legacy_artifacts(&stage, v1)? != before { return Err(std::io::Error::other("legacy memory changed while staging")); }
        Ok((before, notes, stage))
    }).await?;
    let empty = before.is_empty();
    if !empty { validate_consolidation_artifacts_for_version(&stage, version).await?; }
    // An imported legacy summary does not establish provenance for any current
    // selected source. Do not publish selected raw inputs as part of this baseline.
    let manifest = BTreeMap::new();
    let source = root.to_path_buf();
    let staged = stage.clone();
    let baseline_before = before.clone();
    let expected_notes = notes.clone();
    let (generation_id, generation_root) = io_task(Arc::clone(&lock), move || {
        let result = if empty { prepare_empty_legacy_generation(&source, &manifest)? } else { prepare_generation(&source, &staged, v1, &manifest)? };
        if legacy_artifacts(&source, v1)? != baseline_before
            || legacy_artifacts(&result.1, v1)? != baseline_before
            || notes_digest(&source)? != expected_notes {
            return Err(std::io::Error::other("legacy input changed before baseline registration"));
        }
        Ok(result)
    }).await?;
    let artifact_digest = artifact_digest(&generation_root)?;
    let changed = match existing {
        Some(record) => db.replace_legacy_memory_baseline(identity, &record.generation_id, &generation_id, &notes, &artifact_digest, empty).await?,
        None => db.register_legacy_memory_baseline(identity, &generation_id, &notes, &artifact_digest, empty).await?,
    };
    if !changed { anyhow::bail!("legacy publication baseline lost ownership"); }
    let root_owned = root.to_path_buf();
    io_task(Arc::clone(&lock), move || ensure_managed_marker(&root_owned)).await?;
    let record = db.published_generation(identity).await?.ok_or_else(|| anyhow::anyhow!("legacy baseline disappeared"))?;
    if record.generation_id != generation_id || !generation_root.is_dir() { anyhow::bail!("legacy baseline changed before marker completion"); }
    let root_owned = root.to_path_buf();
    if let Err(error) = io_task(Arc::clone(&lock), move || cleanup_published_stage(&root_owned, &stage)).await {
        tracing::warn!(%error, "published legacy staging workspace retained for recovery");
    }
    Ok(record)
}

pub(crate) async fn prepare(
    root: &Path, version: MemoryVersion, db: &MemoryStore, selected: Arc<Vec<Stage1Output>>,
    config: &Config, lock: Arc<MemoryWorkspaceLock>,
) -> anyhow::Result<PreparedPublication> {
    let identity = root_identity(root)?;
    let previous = previous_generation(root, version, db, Arc::clone(&lock), &identity).await?;
    let selected_for_hash = Arc::clone(&selected);
    let mut manifest = io_task(Arc::clone(&lock), move || selected_manifest(&selected_for_hash)).await?;
    let prior = generation_root(root, &previous.generation_id)?;
    let stored_manifest = codex_memories_read::publication_snapshot::load_artifact_manifest(&prior, &previous.artifact_manifest_digest)?;
    let regenerated = codex_memories_read::publication_snapshot::make_artifact_manifest(&prior)?;
    if codex_memories_read::publication_snapshot::artifact_manifest_digest(&regenerated) != previous.artifact_manifest_digest { anyhow::bail!("previous memory artifacts changed"); }
    let root_owned = root.to_path_buf();
    let prior_owned = prior.clone();
    let v1 = version == MemoryVersion::V1;
    let (stage, notes) = io_task(Arc::clone(&lock), move || {
        let notes = notes_digest(&root_owned)?;
        Ok((create_stage(&root_owned, &prior_owned, v1)?, notes))
    }).await?;
    crate::extensions::seed_extension_instructions(&stage).await?;
    crate::phase2::sync_phase2_workspace_inputs(&stage, &selected, config).await?;
    let staged = stage.clone();
    let inputs = io_task(Arc::clone(&lock), move || staged_inputs(&staged)).await?;
    include_extension_resources(&mut manifest, &inputs)?;
    let previous_manifest = if previous.provenance == "legacy_import" { BTreeMap::new() } else {
        let prior = prior.clone();
        io_task(Arc::clone(&lock), move || read_manifest(&prior, &stored_manifest)).await?
    };
    let (added, changed, removed) = changed_inputs(&previous_manifest, &manifest);
    let unchanged = previous.provenance == "consolidated" && previous_manifest == manifest && notes == previous.notes_digest;
    if unchanged {
        let previous_root = prior.clone();
        io_task(Arc::clone(&lock), move || validate_generation(&previous_root, version)).await?;
    } else {
        let root = root.to_path_buf();
        io_task(Arc::clone(&lock), move || check_generation_capacity(&root)).await?;
    }
    let removed_paths = removed_filenames(&previous_manifest, &manifest, &removed);
    let legacy_unknown = previous.provenance == "legacy_import";
    let diff = format!("Host-selected memory input changes:\nadded: {added:?}\nchanged: {changed:?}\nremoved: {removed:?}\nprevious filenames removed or replaced: {removed_paths:?}\nlegacy source-to-filename mapping unknown: {legacy_unknown}\nnotes changed: {}\n", notes != previous.notes_digest);
    tokio::fs::write(stage.join(crate::workspace_diff::FILENAME), diff).await?;
    let stage_check = stage.clone();
    io_task(Arc::clone(&lock), move || crate::publication_tools::validate_stage_budget(&stage_check)).await?;
    drop(lock);
    Ok(PreparedPublication { stage, root_identity: identity, notes_digest: notes, inputs, manifest, gate: Arc::new(Mutex::new(())), tool_failed: Arc::new(AtomicBool::new(false)), previous_generation: previous.generation_id, previous_artifact_digest: previous.artifact_manifest_digest, unchanged })
}

async fn verify_authority(
    context: &crate::runtime::MemoryStartupContext, root: &Path,
    version: MemoryVersion, selected: &[Stage1Output],
) -> anyhow::Result<()> {
    let current = context.current_config().await;
    let version_allowed = current.memories.version == version
        || (current.memories.version == MemoryVersion::V1 && current.memories.dual_write && version == MemoryVersion::V2);
    let mut current_namespace = current.as_ref().clone();
    current_namespace.memories.version = version;
    if !context.generation_enabled().await || !current.features.enabled(codex_features::Feature::MemoryTool) || !version_allowed
        || current_namespace.memories.memory_root(&current_namespace.codex_home).as_ref().is_none_or(|current_root| current_root.as_path() != root)
    {
        anyhow::bail!("memory generation or namespace changed before publication");
    }
    if current.memories.scope == codex_config::types::MemoryScope::Project {
        let Some(project) = current.memories.project.clone() else { anyhow::bail!("memory project authority is unavailable"); };
        let roots = selected.iter().map(|output| output.cwd.clone()).collect::<Vec<_>>();
        let authorized = tokio::task::spawn_blocking(move || roots.iter().all(|root| project.contains_source(root))).await?;
        if !authorized { anyhow::bail!("selected memory source lost project authority"); }
    }
    Ok(())
}

pub(crate) async fn confirm_unchanged(
    context: &crate::runtime::MemoryStartupContext, root: &Path, version: MemoryVersion, db: &MemoryStore, token: &str,
    captured_watermark: i64, completed_watermark: i64, selected: &[Stage1Output],
    prepared: PreparedPublication, lock: Arc<MemoryWorkspaceLock>,
) -> anyhow::Result<bool> {
    if !prepared.unchanged { anyhow::bail!("memory inputs are not unchanged"); }
    let root_owned = root.to_path_buf();
    let expected_notes = prepared.notes_digest.clone();
    let previous = generation_root(root, &prepared.previous_generation)?;
    io_task(Arc::clone(&lock), move || {
        if notes_digest(&root_owned)? != expected_notes { return Err(std::io::Error::other("memory notes changed before confirmation")); }
        validate_generation(&previous, version)
    }).await?;
    let previous = generation_root(root, &prepared.previous_generation)?;
    let previous_digest = prepared.previous_artifact_digest.clone();
    let summary = io_task(Arc::clone(&lock), move || {
        let manifest = codex_memories_read::publication_snapshot::load_artifact_manifest(&previous, &previous_digest)?;
        let bytes = codex_memories_read::publication_snapshot::read_verified_artifact(&previous, &previous.join("memory_summary.md"), &manifest, 32 * 1024 * 1024)?;
        String::from_utf8(bytes).map_err(std::io::Error::other)
    }).await?;
    validate_summary_pointers(&summary, &prepared.manifest)?;
    verify_authority(context, root, version, selected).await?;
    let published = db.publish_global_phase2_generation(token, captured_watermark, completed_watermark, selected,
        &prepared.root_identity, &prepared.previous_generation, &prepared.notes_digest, &prepared.previous_artifact_digest).await?;
    if published {
        let root = root.to_path_buf();
        let stage = prepared.stage.clone();
        if let Err(error) = io_task(lock, move || cleanup_published_stage(&root, &stage)).await {
            tracing::warn!(%error, "confirmed memory staging workspace retained for recovery");
        }
    }
    Ok(published)
}

pub(crate) async fn finish(
    context: &crate::runtime::MemoryStartupContext, root: &Path, version: MemoryVersion, db: &MemoryStore, token: &str,
    captured_watermark: i64, completed_watermark: i64, selected: &[Stage1Output],
    prepared: PreparedPublication, lock: Arc<MemoryWorkspaceLock>,
) -> anyhow::Result<bool> {
    let _gate = prepared.gate.lock().await;
    if prepared.tool_failed.load(Ordering::Acquire) { anyhow::bail!("a stage tool operation failed"); }
    validate_consolidation_artifacts_for_version(&prepared.stage, version).await?;
    let summary_path = prepared.stage.join("memory_summary.md");
    let summary = io_task(Arc::clone(&lock), move || {
        use std::io::Read;
        let mut bytes = Vec::new();
        codex_memories_read::publication_notes::open_regular_without_links(&summary_path)?.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 32 * 1024 * 1024 { return Err(std::io::Error::other("memory summary exceeds limit")); }
        String::from_utf8(bytes).map_err(std::io::Error::other)
    }).await?;
    validate_summary_pointers(&summary, &prepared.manifest)?;
    let stage = prepared.stage.clone();
    let root_owned = root.to_path_buf();
    let expected_inputs = prepared.inputs.clone();
    let expected_notes = prepared.notes_digest.clone();
    let manifest = prepared.manifest.clone();
    let v1 = version == MemoryVersion::V1;
    let (generation, generation_path) = io_task(Arc::clone(&lock), move || {
        if staged_inputs(&stage)? != expected_inputs || notes_digest(&root_owned)? != expected_notes {
            return Err(std::io::Error::other("memory inputs changed during consolidation"));
        }
        let generation = prepare_generation(&root_owned, &stage, v1, &manifest)?;
        reject_stage_references(&generation.1, &stage, v1)?;
        Ok(generation)
    }).await?;
    let artifact_digest = artifact_digest(&generation_path)?;
    verify_authority(context, root, version, selected).await?;
    let published = db.publish_global_phase2_generation(token, captured_watermark, completed_watermark, selected,
        &prepared.root_identity, &generation, &prepared.notes_digest, &artifact_digest).await?;
    if published {
        let root = root.to_path_buf();
        let stage = prepared.stage.clone();
        if let Err(error) = io_task(lock, move || cleanup_published_stage(&root, &stage)).await {
            tracing::warn!(%error, "published memory staging workspace retained for recovery");
        }
    }
    Ok(published)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_test_support::responses::start_mock_server;
    use core_test_support::test_codex::test_codex;

    #[tokio::test]
    async fn failed_or_cancelled_stage_leaves_legacy_baseline_and_notes_unchanged() -> anyhow::Result<()> {
        let home = Arc::new(tempfile::tempdir()?);
        let server = start_mock_server().await;
        let test = test_codex().with_home(Arc::clone(&home))
            .with_config(|config| config.memories.version = MemoryVersion::V1)
            .build_with_auto_env(&server).await?;
        let root = home.path().join("memories");
        std::fs::create_dir_all(root.join("extensions/ad_hoc/notes"))?;
        let summary = b"v1\nlegacy memory\n";
        std::fs::write(root.join("memory_summary.md"), summary)?;
        std::fs::write(root.join("MEMORY.md"), b"legacy handbook")?;
        std::fs::write(root.join("extensions/ad_hoc/notes/n.md"), b"original note")?;
        let config = test.config.clone();
        let state = test.codex.state_db().expect("test state");
        let store = state.memories();
        let lock = Arc::new(MemoryWorkspaceLock::try_acquire(&root)?);
        let prepared = prepare(&root, MemoryVersion::V1, store, Arc::new(Vec::new()), &config, lock).await?;
        let baseline = store.published_generation(&root_identity(&root)?).await?.expect("baseline");
        assert_eq!(baseline.provenance, "legacy_import");
        std::fs::write(prepared.stage.join("memory_summary.md"), b"unpublished worker output")?;
        assert_eq!(std::fs::read(root.join("memory_summary.md"))?, summary);
        assert_eq!(std::fs::read(root.join("extensions/ad_hoc/notes/n.md"))?, b"original note");
        let generation = generation_root(&root, &baseline.generation_id)?;
        assert_eq!(std::fs::read(generation.join("memory_summary.md"))?, summary);
        drop(prepared);
        test.codex.shutdown_and_wait().await?;
        Ok(())
    }

    #[tokio::test]
    async fn finish_rejects_unsupplied_rollout_pointer_before_publication() -> anyhow::Result<()> {
        let home = Arc::new(tempfile::tempdir()?);
        let server = start_mock_server().await;
        let test = test_codex().with_home(Arc::clone(&home))
            .with_config(|config| config.memories.version = MemoryVersion::V1)
            .build_with_auto_env(&server).await?;
        let root = home.path().join("memories");
        std::fs::create_dir_all(&root)?;
        let state = test.codex.state_db().expect("test state");
        let store = state.memories();
        let lock = Arc::new(MemoryWorkspaceLock::try_acquire(&root)?);
        let prepared = prepare(&root, MemoryVersion::V1, store, Arc::new(Vec::new()), &test.config, Arc::clone(&lock)).await?;
        let baseline = store.published_generation(&root_identity(&root)?).await?.expect("baseline");
        std::fs::write(prepared.stage.join("memory_summary.md"), b"v1\n- rollout_summaries/removed.md\n")?;
        std::fs::write(prepared.stage.join("MEMORY.md"), b"v1\n")?;
        let source = test.codex.config_snapshot().await.session_source;
        let context = crate::runtime::MemoryStartupContext::new(
            Arc::clone(&test.thread_manager), test.thread_manager.auth_manager(),
            test.session_configured.thread_id, Arc::clone(&test.codex), &test.config, source,
        );
        let error = finish(&context, &root, MemoryVersion::V1, store, "unused", 0, 0, &[], prepared, lock).await.unwrap_err();
        assert!(error.to_string().contains("unsupplied rollout"));
        assert_eq!(store.published_generation(&root_identity(&root)?).await?.unwrap().generation_id, baseline.generation_id);
        test.codex.shutdown_and_wait().await?;
        Ok(())
    }
}
