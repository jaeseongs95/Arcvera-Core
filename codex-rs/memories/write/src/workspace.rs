use anyhow::Context;
use codex_git_utils::GitBaselineDiff;
use codex_git_utils::diff_since_latest_init;
use codex_git_utils::ensure_git_baseline_repository;
use codex_git_utils::reset_git_repository;
use codex_protocol::MemoryVersion;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path::write_atomically;
use std::io::Read;
use std::io::Write;
#[cfg(windows)]
use std::os::windows::fs::FileTypeExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

const STAGE_MAX_BYTES: u64 = 32 * 1024 * 1024;
const STAGE_MAX_FILES: usize = 10_000;
const MAX_RETAINED_STAGES: usize = 2;
const MAX_RETAINED_STAGE_BYTES: u64 = 80 * 1024 * 1024;

/// A selected, bounded input copy, held until the real worker has terminated.
/// Failed stages are preserved because they may contain the only proposed
/// correction or recovery evidence. Only fully published stages are disposable.
pub(crate) struct MemoryStage {
    workspace_lock: Arc<crate::MemoryWorkspaceLock>,
    root: AbsolutePathBuf,
    directory: Option<tempfile::TempDir>,
    cleanup: AtomicBool,
}

impl MemoryStage {
    pub(crate) fn root(&self) -> &AbsolutePathBuf {
        &self.root
    }
    pub(crate) fn allow_cleanup(&self) {
        self.cleanup.store(true, Ordering::Release);
    }
    pub(crate) fn preserve_outputs(&self) {
        self.cleanup.store(false, Ordering::Release);
    }

    pub(crate) async fn prepare(
        root: &Path,
        version: MemoryVersion,
        workspace_lock: Arc<crate::MemoryWorkspaceLock>,
    ) -> std::io::Result<Self> {
        let source = root.to_owned();
        tokio::task::spawn_blocking(move || {
            let parent = source
                .parent()
                .ok_or_else(|| std::io::Error::other("memory root has no parent"))?;
            let prefix = format!("codex-memory-stage-{}-", source.file_name().ok_or_else(|| std::io::Error::other("memory root has no name"))?.to_string_lossy());
            check_retained_stage_budget(parent, &prefix, &source)?;
            let directory = tempfile::Builder::new()
                .prefix(&prefix)
                .tempdir_in(parent)?;
            std::fs::write(directory.path().join(".codex-memory-stage.json"), serde_json::to_vec(&serde_json::json!({
                "schema_version": 1,
                "source_root": codex_memories_read::publication::sha256(source.to_string_lossy().as_bytes()),
            }))?)?;
            let mut names = vec![
                PathBuf::from("memory_summary.md"),
                PathBuf::from(crate::workspace_diff::FILENAME),
                PathBuf::from("rollout_summaries"),
                PathBuf::from("extensions"),
            ];
            if version == MemoryVersion::V1 {
                names.extend([
                    PathBuf::from("MEMORY.md"),
                    PathBuf::from("raw_memories.md"),
                    PathBuf::from("skills"),
                ]);
            }
            let mut budget = StageBudget::default();
            for name in names {
                copy_selected_tree(
                    &source.join(&name),
                    &directory.path().join(name),
                    &mut budget,
                )?;
            }
            Ok(Self {
                workspace_lock,
                root: AbsolutePathBuf::from_absolute_path(directory.path())?,
                directory: Some(directory),
                cleanup: AtomicBool::new(true),
            })
        })
        .await
        .map_err(std::io::Error::other)?
    }

    /// Copy back final consolidated outputs only. Inputs and correction notes
    /// are never copied back from a worker-controlled workspace.
    pub(crate) async fn publish_outputs(
        &self,
        root: &Path,
        version: MemoryVersion,
    ) -> std::io::Result<()> {
        let source = self.root.to_path_buf();
        let target = root.to_owned();
        let workspace_lock = Arc::clone(&self.workspace_lock);
        tokio::task::spawn_blocking(move || {
            let _workspace_lock = workspace_lock;
            // Use the reader's exact allowlist and aggregate output budget.
            let outputs =
                codex_memories_read::publication::collect_published_artifacts(&source, version)?;
            let skills = outputs
                .iter()
                .filter_map(|(path, bytes)| {
                    path.strip_prefix("skills/")
                        .map(|relative| (PathBuf::from(relative), bytes.clone()))
                })
                .collect();
            for (name, bytes) in outputs
                .iter()
                .filter(|(path, _)| !path.starts_with("skills/"))
            {
                let contents = std::str::from_utf8(bytes).map_err(std::io::Error::other)?;
                write_atomically(&target.join(name), contents)?;
            }
            if version == MemoryVersion::V1 {
                publish_skills(&target.join("skills"), &skills)?;
            }
            Ok(())
        })
        .await
        .map_err(std::io::Error::other)?
    }
}

/// Backpressure preserves failed evidence instead of deleting it automatically.
/// These are preparation/retention budgets, not a live worker disk quota.
fn check_retained_stage_budget(parent: &Path, prefix: &str, source: &Path) -> std::io::Result<()> {
    let owner = codex_memories_read::publication::sha256(source.to_string_lossy().as_bytes());
    let mut stages = Vec::new();
    let mut bytes = 0_u64;
    let mut entries_seen = 0_usize;
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with(prefix) {
            continue;
        }
        let path = entry.path();
        if !stage_metadata(&path)?.is_dir() {
            return Err(std::io::Error::other(format!(
                "memory stage recovery required at {}",
                path.display()
            )));
        }
        let marker = path.join(".codex-memory-stage.json");
        let metadata = stage_metadata(&marker).map_err(|error| {
            std::io::Error::other(format!(
                "memory stage ownership is unknown at {}: {error}",
                path.display()
            ))
        })?;
        if !metadata.is_file() || metadata.len() > 1024 {
            return Err(std::io::Error::other(format!(
                "memory stage ownership is unknown at {}",
                path.display()
            )));
        }
        let mut marker_bytes = Vec::new();
        std::fs::File::open(marker)?
            .take(1025)
            .read_to_end(&mut marker_bytes)?;
        if marker_bytes.len() > 1024 {
            return Err(std::io::Error::other(
                "memory stage ownership marker exceeds its budget",
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(&marker_bytes).map_err(|error| {
            std::io::Error::other(format!(
                "memory stage ownership is unknown at {}: {error}",
                path.display()
            ))
        })?;
        if value["schema_version"] != 1 || value["source_root"].as_str() != Some(owner.as_str()) {
            return Err(std::io::Error::other(format!(
                "memory stage ownership is unknown at {}",
                path.display()
            )));
        }
        stages.push(path.clone());
        let mut paths = vec![path];
        while let Some(path) = paths.pop() {
            entries_seen += 1;
            if entries_seen > 2 * STAGE_MAX_FILES {
                return Err(std::io::Error::other(
                    "memory stage recovery required: retained directory budget exceeded",
                ));
            }
            let metadata = stage_metadata(&path)?;
            if metadata.is_file() {
                bytes = bytes.saturating_add(metadata.len());
            } else if metadata.is_dir() {
                for child in std::fs::read_dir(path)? {
                    if paths.len() + entries_seen >= 2 * STAGE_MAX_FILES {
                        return Err(std::io::Error::other(
                            "memory stage recovery required: retained directory budget exceeded",
                        ));
                    }
                    paths.push(child?.path());
                }
            } else {
                return Err(std::io::Error::other(
                    "memory stage recovery required: non-regular retained entry",
                ));
            }
            if bytes.saturating_add(STAGE_MAX_BYTES) > MAX_RETAINED_STAGE_BYTES {
                break;
            }
        }
        if stages.len() >= MAX_RETAINED_STAGES
            || bytes.saturating_add(STAGE_MAX_BYTES) > MAX_RETAINED_STAGE_BYTES
        {
            return Err(std::io::Error::other(format!(
                "memory stage recovery required before starting a worker: {}",
                stages
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }
    Ok(())
}

impl Drop for MemoryStage {
    fn drop(&mut self) {
        let Some(directory) = self.directory.take() else {
            return;
        };
        if !self.cleanup.load(Ordering::Acquire) {
            let path = directory.keep();
            tracing::warn!(stage = %path.display(), "preserving unpublished memory stage for recovery");
        } else if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let workspace_lock = Arc::clone(&self.workspace_lock);
            runtime.spawn_blocking(move || {
                close_completed_stage(directory);
                drop(workspace_lock);
            });
        } else {
            close_completed_stage(directory);
        }
    }
}

fn close_completed_stage(directory: tempfile::TempDir) {
    let path = directory.path().to_owned();
    if let Err(error) = directory.close() {
        tracing::warn!(stage = %path.display(), %error, "completed memory stage cleanup failed; retained paths require review");
    }
}

struct StageBudget {
    files: usize,
    bytes: u64,
    entries: usize,
    max_files: usize,
    max_bytes: u64,
    max_file_bytes: u64,
    max_entries: usize,
}

impl Default for StageBudget {
    fn default() -> Self {
        Self {
            files: 0,
            bytes: 0,
            entries: 0,
            max_files: STAGE_MAX_FILES,
            max_bytes: STAGE_MAX_BYTES,
            max_file_bytes: STAGE_MAX_BYTES,
            max_entries: STAGE_MAX_FILES,
        }
    }
}

impl StageBudget {
    fn published() -> Self {
        use codex_memories_read::publication::MAX_PUBLISHED_ARTIFACT_BYTES;
        use codex_memories_read::publication::MAX_PUBLISHED_ARTIFACTS;
        use codex_memories_read::publication::MAX_PUBLISHED_DIRECTORY_ENTRIES;
        use codex_memories_read::publication::MAX_PUBLISHED_TOTAL_BYTES;
        Self {
            max_files: MAX_PUBLISHED_ARTIFACTS,
            max_bytes: MAX_PUBLISHED_TOTAL_BYTES,
            max_file_bytes: MAX_PUBLISHED_ARTIFACT_BYTES,
            max_entries: MAX_PUBLISHED_DIRECTORY_ENTRIES,
            ..Self::default()
        }
    }

    fn visit(&mut self) -> std::io::Result<()> {
        self.entries += 1;
        if self.entries > self.max_entries {
            return Err(std::io::Error::other(
                "memory stage directory budget exceeded",
            ));
        }
        Ok(())
    }
}

fn stage_metadata(path: &Path) -> std::io::Result<std::fs::Metadata> {
    let metadata = std::fs::symlink_metadata(path)?;
    let redirected = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        redirected || metadata.file_attributes() & 0x400 != 0
    };
    if redirected {
        return Err(std::io::Error::other(
            "memory stage cannot contain redirected paths",
        ));
    }
    Ok(metadata)
}

fn read_stage_file(path: &Path, budget: &mut StageBudget) -> std::io::Result<Vec<u8>> {
    let metadata = stage_metadata(path)?;
    let remaining = (budget.max_bytes - budget.bytes).min(budget.max_file_bytes);
    if !metadata.is_file() || metadata.len() > remaining || budget.files >= budget.max_files {
        return Err(std::io::Error::other(
            "memory stage input exceeds its file or byte budget",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(remaining + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > remaining {
        return Err(std::io::Error::other(
            "memory stage input grew beyond its budget",
        ));
    }
    budget.bytes += bytes.len() as u64;
    budget.files += 1;
    Ok(bytes)
}

fn copy_selected_tree(
    source: &Path,
    target: &Path,
    budget: &mut StageBudget,
) -> std::io::Result<()> {
    if source.components().count() > 128 {
        return Err(std::io::Error::other(
            "memory stage path nesting exceeds its budget",
        ));
    }
    let metadata = match stage_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    budget.visit()?;
    if metadata.is_dir() {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            if entry.file_name() == ".git" || entry.file_name() == ".codex-baseline" {
                continue;
            }
            copy_selected_tree(&entry.path(), &target.join(entry.file_name()), budget)?;
        }
    } else {
        let bytes = read_stage_file(source, budget)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, bytes)?;
    }
    Ok(())
}

fn collect_stage_files(
    root: &Path,
    budget: &mut StageBudget,
) -> std::io::Result<std::collections::BTreeMap<PathBuf, Vec<u8>>> {
    let mut files = std::collections::BTreeMap::new();
    match stage_metadata(root) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(error) => return Err(error),
    }
    let mut paths = vec![root.to_owned()];
    while let Some(path) = paths.pop() {
        budget.visit()?;
        if stage_metadata(&path)?.is_dir() {
            for entry in std::fs::read_dir(path)? {
                if budget.entries + paths.len() >= budget.max_entries {
                    return Err(std::io::Error::other(
                        "generated skill directory budget exceeded",
                    ));
                }
                paths.push(entry?.path());
            }
        } else {
            let relative = path
                .strip_prefix(root)
                .map_err(std::io::Error::other)?
                .to_owned();
            let key = format!("skills/{}", relative.to_string_lossy().replace('\\', "/"));
            if !codex_memories_read::publication::is_published_artifact_path(
                &key,
                MemoryVersion::V1,
            ) {
                return Err(std::io::Error::other("invalid generated memory skill path"));
            }
            let bytes = read_stage_file(&path, budget)?;
            files.insert(relative, bytes);
        }
    }
    Ok(files)
}

fn publish_skills(
    root: &Path,
    skills: &std::collections::BTreeMap<PathBuf, Vec<u8>>,
) -> std::io::Result<()> {
    // Validate existing generated paths before writing/removing any file. Empty
    // directories may remain; original inputs and notes are outside this tree.
    let existing = collect_stage_files(root, &mut StageBudget::published())?;
    for (relative, contents) in skills {
        let target = root.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let parent = target
            .parent()
            .ok_or_else(|| std::io::Error::other("generated skill has no parent"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(contents)?;
        temporary.persist(&target).map_err(std::io::Error::other)?;
    }
    for relative in existing.keys().filter(|path| !skills.contains_key(*path)) {
        std::fs::remove_file(root.join(relative))?;
    }
    let mut parents = std::collections::BTreeSet::new();
    for relative in existing.keys().filter(|path| !skills.contains_key(*path)) {
        let mut parent = root.join(relative).parent().map(Path::to_owned);
        while let Some(path) = parent {
            if path == root {
                break;
            }
            parent = path.parent().map(Path::to_owned);
            parents.insert(path);
        }
    }
    let mut parents = parents.into_iter().collect::<Vec<_>>();
    parents.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for parent in parents {
        match std::fs::remove_dir(parent) {
            Ok(()) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                ) => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(test)]
mod stage_tests {
    use super::*;

    async fn prepare_stage(root: &Path, version: MemoryVersion) -> std::io::Result<MemoryStage> {
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(root)?);
        MemoryStage::prepare(root, version, lock).await
    }

    #[tokio::test]
    async fn preserved_failure_stages_apply_backpressure_without_deleting_recovery_evidence()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "supported summary")?;
        let mut retained = Vec::new();
        for _ in 0..MAX_RETAINED_STAGES {
            let stage = prepare_stage(&root, MemoryVersion::V2).await?;
            stage.preserve_outputs();
            retained.push(stage.root().to_path_buf());
            drop(stage);
        }
        let error = match prepare_stage(&root, MemoryVersion::V2).await {
            Ok(_) => anyhow::bail!("retained-stage budget must prevent another worker workspace"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("recovery required"));
        assert!(retained.iter().all(|path| path.is_dir()));
        assert_eq!(
            std::fs::read_to_string(root.join("memory_summary.md"))?,
            "supported summary"
        );
        Ok(())
    }

    #[tokio::test]
    async fn stage_edits_cannot_accidentally_modify_original_notes_or_copy_them_back()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir_all(root.join("extensions/ad_hoc/notes"))?;
        std::fs::write(
            root.join("extensions/ad_hoc/notes/source.md"),
            "original correction",
        )?;
        std::fs::write(root.join("memory_summary.md"), "old fact")?;
        let stage = prepare_stage(&root, MemoryVersion::V2).await?;
        std::fs::write(
            stage.root().join("extensions/ad_hoc/notes/source.md"),
            "worker copy changed",
        )?;
        std::fs::write(stage.root().join("memory_summary.md"), "new useful fact")?;
        assert_eq!(
            std::fs::read_to_string(root.join("extensions/ad_hoc/notes/source.md"))?,
            "original correction"
        );
        stage.publish_outputs(&root, MemoryVersion::V2).await?;
        assert_eq!(
            std::fs::read_to_string(root.join("memory_summary.md"))?,
            "new useful fact"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("extensions/ad_hoc/notes/source.md"))?,
            "original correction"
        );
        stage.allow_cleanup();
        Ok(())
    }

    #[tokio::test]
    async fn v1_publishes_the_exact_final_skill_set_without_preserving_deleted_skill_claims()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories");
        std::fs::create_dir_all(root.join("skills/old"))?;
        std::fs::write(
            root.join("skills/old/SKILL.md"),
            "obsolete synthetic skill canary",
        )?;
        std::fs::write(root.join("memory_summary.md"), "same summary")?;
        std::fs::write(root.join("MEMORY.md"), "same index")?;
        let stage = prepare_stage(&root, MemoryVersion::V1).await?;
        std::fs::remove_file(stage.root().join("skills/old/SKILL.md"))?;
        std::fs::create_dir_all(stage.root().join("skills/new"))?;
        std::fs::write(
            stage.root().join("skills/new/SKILL.md"),
            "correct useful scoped procedure",
        )?;
        stage.publish_outputs(&root, MemoryVersion::V1).await?;
        let artifacts = codex_memories_read::publication::collect_published_artifacts(
            &root,
            MemoryVersion::V1,
        )?;
        assert!(!artifacts.contains_key("skills/old/SKILL.md"));
        assert_eq!(
            artifacts.get("skills/new/SKILL.md").unwrap(),
            b"correct useful scoped procedure"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("memory_summary.md"))?,
            "same summary"
        );
        stage.allow_cleanup();
        Ok(())
    }

    #[tokio::test]
    async fn stage_budget_is_checked_before_a_worker_receives_oversized_input() -> anyhow::Result<()>
    {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir_all(root.join("rollout_summaries"))?;
        std::fs::File::create(root.join("rollout_summaries/large.md"))?
            .set_len(STAGE_MAX_BYTES + 1)?;
        assert!(prepare_stage(&root, MemoryVersion::V2).await.is_err());
        Ok(())
    }
}

/// Prepares the memory directory for git-baseline diffing.
///
/// This keeps an existing current-version `.git/` baseline intact. Missing, outdated or unusable
/// metadata is replaced with an empty baseline so existing memory files are reconsidered. Any stale
/// generated `phase2_workspace_diff.md` is removed so it does not enter the next diff.
pub async fn prepare_memory_workspace(root: &Path) -> anyhow::Result<()> {
    crate::ensure_layout(root)
        .await
        .with_context(|| format!("prepare memory workspace {}", root.display()))?;
    remove_workspace_diff(root).await?;
    ensure_git_baseline_repository(root).await?;
    Ok(())
}

/// Returns the current workspace diff after removing any stale generated diff artifact.
///
/// Memory artifacts are left intact. The baseline is invalidated if a tracked file has grown
/// beyond the snapshot size limit since preparation.
pub async fn memory_workspace_diff(root: &Path) -> anyhow::Result<GitBaselineDiff> {
    remove_workspace_diff(root).await?;
    diff_since_latest_init(root).await
}

/// Writes `phase2_workspace_diff.md` with a bounded git-style diff from the current baseline.
pub async fn write_workspace_diff(root: &Path, diff: &GitBaselineDiff) -> anyhow::Result<()> {
    let path = root.join(crate::workspace_diff::FILENAME);
    tokio::fs::write(&path, render_workspace_diff_file(diff))
        .await
        .with_context(|| format!("write memory workspace diff file {}", path.display()))
}

/// Marks the current memory root as the new baseline.
///
/// The generated diff file is removed before resetting the baseline so deleted memory content is
/// not retained in the prompt artifact or in unreachable git objects.
pub async fn reset_memory_workspace_baseline(root: &Path) -> anyhow::Result<()> {
    remove_workspace_diff(root).await?;
    reset_git_repository(root).await
}

/// Sums regular-file contents without reading them, excluding git metadata and symbolic links.
pub(crate) async fn memory_storage_bytes(root: &Path) -> std::io::Result<u64> {
    let mut paths = vec![root.to_path_buf()];
    let mut bytes = 0_u64;
    while let Some(path) = paths.pop() {
        let metadata = tokio::fs::symlink_metadata(&path).await?;
        if metadata.is_file() {
            bytes = bytes.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            let mut entries = tokio::fs::read_dir(&path).await?;
            while let Some(entry) = entries.next_entry().await? {
                if entry.file_name() != ".git" {
                    paths.push(entry.path());
                }
            }
        }
    }
    Ok(bytes)
}

/// Verifies that a completed consolidation run left the required memory artifacts in place.
pub async fn validate_consolidation_artifacts(root: &Path) -> anyhow::Result<()> {
    validate_consolidation_artifacts_for_version(root, MemoryVersion::V1).await
}

pub(crate) async fn validate_consolidation_artifacts_for_version(
    root: &Path,
    version: MemoryVersion,
) -> anyhow::Result<()> {
    let removed_symlinks = remove_memory_symlinks(root).await?;
    anyhow::ensure!(
        removed_symlinks == 0,
        "removed {removed_symlinks} symbolic links from consolidated memory workspace"
    );

    if version == MemoryVersion::V1 {
        let memory_path = root.join("MEMORY.md");
        let memory_metadata = tokio::fs::metadata(&memory_path).await.with_context(|| {
            format!(
                "read consolidated memory artifact {}",
                memory_path.display()
            )
        })?;
        anyhow::ensure!(
            memory_metadata.is_file(),
            "consolidated memory artifact is not a file: {}",
            memory_path.display()
        );
    }

    let summary_path = root.join("memory_summary.md");
    let summary = tokio::fs::read_to_string(&summary_path)
        .await
        .with_context(|| format!("read memory summary artifact {}", summary_path.display()))?;
    anyhow::ensure!(
        summary.lines().next() == Some("v1"),
        "memory summary artifact does not start with v1: {}",
        summary_path.display()
    );

    if version == MemoryVersion::V2 {
        anyhow::ensure!(is_valid_v2_summary(&summary), "invalid v2 memory summary");
    }
    Ok(())
}

/// Checks the artifact shared by the writer and the read-only readiness endpoint.
pub fn is_valid_v2_summary(summary: &str) -> bool {
    summary.lines().next() == Some("v1")
        && summary.len() < 10_000
        && [
            "## User Profile",
            "## User preferences",
            "## General Tips",
            "## What's in Memory",
        ]
        .iter()
        .all(|heading| summary.lines().any(|line| line.trim() == *heading))
}

pub(crate) async fn remove_memory_symlinks(root: &Path) -> std::io::Result<usize> {
    let mut directories = vec![root.to_path_buf()];
    let mut removed = 0;

    while let Some(directory) = directories.pop() {
        let mut entries = tokio::fs::read_dir(directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;
            if file_type.is_symlink() {
                #[cfg(windows)]
                if file_type.is_symlink_dir() {
                    tokio::fs::remove_dir(&path).await?;
                } else {
                    tokio::fs::remove_file(&path).await?;
                }
                #[cfg(not(windows))]
                tokio::fs::remove_file(&path).await?;

                tracing::warn!(
                    "removed symbolic link from memory workspace: {}",
                    path.display()
                );
                removed += 1;
            } else if file_type.is_dir() {
                directories.push(path);
            }
        }
    }

    Ok(removed)
}

/// Removes the generated `phase2_workspace_diff.md` prompt artifact.
///
/// This does not remove `.git/`, reset the baseline, or delete memory content. It is used before
/// diffing and before baseline reset so the generated diff file itself is not treated as memory
/// workspace input.
pub(super) async fn remove_workspace_diff(root: &Path) -> anyhow::Result<()> {
    let path = root.join(crate::workspace_diff::FILENAME);
    match tokio::fs::remove_file(&path).await {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err)
            .with_context(|| format!("remove memory workspace diff file {}", path.display())),
    }
}

fn render_workspace_diff_file(diff: &GitBaselineDiff) -> String {
    let mut rendered = String::from(
        "# Memory Workspace Diff\n\n\
         Generated by Codex before Phase 2 memory consolidation. Read this file first and do not edit it.\n\n\
         ## Status\n",
    );

    if !diff.has_changes() {
        rendered.push_str("- none\n");
        return rendered;
    }

    for change in &diff.changes {
        rendered.push_str(&format!("- {} {}\n", change.status.label(), change.path));
    }
    rendered.push_str("\n## Diff\n\n```diff\n");
    append_bounded_diff(&mut rendered, &diff.unified_diff);
    rendered.push_str("```\n");
    rendered
}

fn append_bounded_diff(rendered: &mut String, diff: &str) {
    if diff.len() <= crate::workspace_diff::MAX_BYTES {
        rendered.push_str(diff);
        if !diff.ends_with('\n') {
            rendered.push('\n');
        }
        return;
    }

    let boundary = previous_char_boundary(diff, crate::workspace_diff::MAX_BYTES);
    rendered.push_str(&diff[..boundary]);
    if !rendered.ends_with('\n') {
        rendered.push('\n');
    }
    rendered.push_str(&format!(
        "\n[workspace diff truncated at {} bytes]\n",
        crate::workspace_diff::MAX_BYTES
    ));
}

fn previous_char_boundary(value: &str, max_bytes: usize) -> usize {
    if max_bytes >= value.len() {
        return value.len();
    }
    let mut index = max_bytes;
    while !value.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;
