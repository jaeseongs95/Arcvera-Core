// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Bounded staging and immutable generation preparation.
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::collections::BTreeMap;
use sha2::{Digest, Sha256};

const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_FILES: usize = 10_000;
const MAX_DEPTH: usize = 64;
const MAX_RETAINED_STAGES: usize = 2;
const STAGE_MARKER: &str = ".codex-memory-stage";
const GENERATION_MARKER: &str = ".codex-memory-generation";
const MAX_RETAINED_GENERATIONS: usize = 64;
const MAX_GENERATION_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Default)]
struct Budget { files: usize, bytes: u64 }

fn metadata(path: &Path) -> io::Result<std::fs::Metadata> {
    let metadata = std::fs::symlink_metadata(path)?;
    let redirected = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        redirected || metadata.file_attributes() & 0x400 != 0
    };
    if redirected { return Err(io::Error::other("redirected memory input")); }
    Ok(metadata)
}

pub fn validate_root(path: &Path) -> io::Result<()> {
    if !metadata(path)?.is_dir() { return Err(io::Error::other("memory root is not a directory")); }
    Ok(())
}

use codex_memories_read::publication_notes::open_regular_without_links;

fn copy_tree(source: &Path, destination: &Path, budget: &mut Budget, depth: usize) -> io::Result<()> {
    if depth > MAX_DEPTH { return Err(io::Error::other("memory input depth exceeded")); }
    let entry = match metadata(source) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        other => other?,
    };
    budget.files = budget.files.checked_add(1).ok_or_else(|| io::Error::other("memory input count overflow"))?;
    if budget.files > MAX_FILES { return Err(io::Error::other("memory input count exceeded")); }
    if entry.is_dir() {
        std::fs::create_dir(destination)?;
        for child in std::fs::read_dir(source)? {
            let child = child?;
            copy_tree(&child.path(), &destination.join(child.file_name()), budget, depth + 1)?;
        }
    } else if entry.is_file() {
        let remaining = MAX_BYTES.checked_sub(budget.bytes).ok_or_else(|| io::Error::other("memory input budget exceeded"))?;
        if entry.len() > remaining { return Err(io::Error::other("memory input bytes exceeded")); }
        let mut bytes = Vec::new();
        open_regular_without_links(source)?.take(remaining + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > remaining { return Err(io::Error::other("memory input grew beyond budget")); }
        budget.bytes += bytes.len() as u64;
        let mut output = std::fs::OpenOptions::new().write(true).create_new(true).open(destination)?;
        output.write_all(&bytes)?;
        output.sync_all()?;
    } else { return Err(io::Error::other("unsupported memory input entry")); }
    Ok(())
}

/// Fingerprint only generated legacy artifacts; no original note bodies enter
/// the receipt. The root lock remains held for the full comparison interval.
fn artifact_set(root: &Path, names: &[&str]) -> io::Result<BTreeMap<PathBuf, String>> {
    fn visit(root: &Path, path: &Path, depth: usize, budget: &mut Budget, out: &mut BTreeMap<PathBuf, String>) -> io::Result<()> {
        if depth > MAX_DEPTH { return Err(io::Error::other("legacy memory path depth exceeded")); }
        let meta = match metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            other => other?,
        };
        budget.files += 1;
        if budget.files > MAX_FILES { return Err(io::Error::other("legacy memory entry count exceeded")); }
        if meta.is_dir() {
            for entry in std::fs::read_dir(path)? { visit(root, &entry?.path(), depth + 1, budget, out)?; }
        } else if meta.is_file() {
            let remaining = MAX_BYTES.checked_sub(budget.bytes).ok_or_else(|| io::Error::other("legacy memory budget exceeded"))?;
            if meta.len() > remaining { return Err(io::Error::other("legacy memory bytes exceeded")); }
            let mut bytes = Vec::new();
            open_regular_without_links(path)?.take(remaining + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > remaining { return Err(io::Error::other("legacy memory grew beyond budget")); }
            budget.bytes += bytes.len() as u64;
            let relative = path.strip_prefix(root).map_err(io::Error::other)?.to_path_buf();
            out.insert(relative, format!("{:x}", Sha256::digest(bytes)));
        } else { return Err(io::Error::other("invalid legacy memory artifact")); }
        Ok(())
    }
    let mut out = BTreeMap::new();
    let mut budget = Budget::default();
    for name in names { visit(root, &root.join(name), 0, &mut budget, &mut out)?; }
    Ok(out)
}

pub fn legacy_artifacts(root: &Path, v1: bool) -> io::Result<BTreeMap<PathBuf, String>> {
    let names: &[&str] = if v1 { &["memory_summary.md", "MEMORY.md", "skills"] } else { &["memory_summary.md"] };
    artifact_set(root, names)
}

pub fn staged_inputs(root: &Path) -> io::Result<BTreeMap<PathBuf, String>> {
    artifact_set(root, &["extensions", "raw_memories.md", "rollout_summaries"])
}

/// Read-only validation of a previously published generation.
pub fn validate_generation(root: &Path, version: codex_protocol::MemoryVersion) -> io::Result<()> {
    let v1 = version == codex_protocol::MemoryVersion::V1;
    let artifacts = legacy_artifacts(root, v1)?;
    if !artifacts.contains_key(Path::new("memory_summary.md")) || (v1 && !artifacts.contains_key(Path::new("MEMORY.md"))) {
        return Err(io::Error::other("published memory artifacts are missing"));
    }
    let mut bytes = Vec::new();
    open_regular_without_links(&root.join("memory_summary.md"))?.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 { return Err(io::Error::other("published memory summary exceeds limit")); }
    let summary = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
    if summary.lines().next() != Some("v1") || (!v1 && !crate::workspace::is_valid_v2_summary(summary)) {
        return Err(io::Error::other("published memory summary is invalid"));
    }
    Ok(())
}

/// Do not publish generated references to a disposable staging directory.
pub fn reject_stage_references(generation: &Path, stage: &Path, v1: bool) -> io::Result<()> {
    fn visit(path: &Path, needles: &[Vec<u8>], budget: &mut Budget, depth: usize) -> io::Result<()> {
        if depth > MAX_DEPTH { return Err(io::Error::other("generated memory path depth exceeded")); }
        let meta = match metadata(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            other => other?,
        };
        budget.files += 1;
        if budget.files > MAX_FILES { return Err(io::Error::other("generated memory file count exceeded")); }
        if meta.is_dir() {
            for entry in std::fs::read_dir(path)? { visit(&entry?.path(), needles, budget, depth + 1)?; }
        } else if meta.is_file() {
            let remaining = MAX_BYTES.checked_sub(budget.bytes).ok_or_else(|| io::Error::other("generated memory bytes exceeded"))?;
            if meta.len() > remaining { return Err(io::Error::other("generated memory bytes exceeded")); }
            let mut bytes = Vec::new();
            open_regular_without_links(path)?.take(remaining + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 > remaining { return Err(io::Error::other("generated memory grew beyond limit")); }
            budget.bytes += bytes.len() as u64;
            if needles.iter().any(|needle| !needle.is_empty() && bytes.windows(needle.len()).any(|window| window == needle)) {
                return Err(io::Error::other("generated memory references its staging directory"));
            }
        } else { return Err(io::Error::other("invalid generated memory entry")); }
        Ok(())
    }
    let path = stage.to_string_lossy();
    let needles = [path.as_bytes().to_vec(), path.replace('\\', "/").into_bytes()];
    let mut names = vec!["memory_summary.md"];
    if v1 { names.extend(["MEMORY.md", "skills"]); }
    let mut budget = Budget::default();
    for name in names { visit(&generation.join(name), &needles, &mut budget, 0)?; }
    Ok(())
}

/// The caller holds the existing root writer lock until the worker has stopped.
/// This function copies only selected paths and does not mutate any original.
pub fn create_stage(root: &Path, previous_derived_root: &Path, v1: bool) -> io::Result<PathBuf> {
    let parent = root.parent().ok_or_else(|| io::Error::other("memory root has no parent"))?;
    let identity = codex_memories_read::publication_snapshot::root_identity(root)?;
    let prefix = format!("codex-memory-stage-{identity}-");
    let mut retained = Vec::new();
    for entry in std::fs::read_dir(parent)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            retained.push(entry.path());
            if retained.len() >= MAX_RETAINED_STAGES {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, format!("retained memory stages require recovery before another run: {}", retained.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", "))));
            }
        }
    }
    let id = uuid::Uuid::new_v4();
    let stage = parent.join(format!("{prefix}{id}"));
    std::fs::create_dir(&stage)?;
    let mut marker = std::fs::OpenOptions::new().write(true).create_new(true).open(stage.join(STAGE_MARKER))?;
    write!(marker, "{identity}\n{id}\n")?;
    marker.sync_all()?;
    let mut budget = Budget::default();
    // Raw/rollout inputs are regenerated inside the stage from the selected
    // Stage1Output rows; copying the live aggregate would include other inputs.
    for name in ["extensions"] {
        copy_tree(&root.join(name), &stage.join(name), &mut budget, 0)?;
    }
    let mut outputs = vec!["memory_summary.md"];
    if v1 { outputs.extend(["MEMORY.md", "skills"]); }
    for name in outputs {
        copy_tree(&previous_derived_root.join(name), &stage.join(name), &mut budget, 0)?;
    }
    Ok(stage)
}

fn tree_bytes(path: &Path, entries: &mut usize, limit: u64, depth: usize) -> io::Result<u64> {
    if depth > MAX_DEPTH { return Err(io::Error::other("memory generation path depth exceeded")); }
    let meta = match metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        other => other?,
    };
    *entries = entries.checked_add(1).ok_or_else(|| io::Error::other("memory entry count overflow"))?;
    if *entries > MAX_FILES * MAX_RETAINED_GENERATIONS { return Err(io::Error::other("memory generation entry budget exceeded")); }
    if meta.is_dir() {
        let mut total = 0u64;
        for entry in std::fs::read_dir(path)? {
            total = total.checked_add(tree_bytes(&entry?.path(), entries, limit.saturating_sub(total), depth + 1)?)
                .ok_or_else(|| io::Error::other("memory generation size overflow"))?;
            if total > limit { return Err(io::Error::other("memory generation byte limit exceeded")); }
        }
        Ok(total)
    } else if meta.is_file() {
        let length = open_regular_without_links(path)?.metadata()?.len();
        if length > limit { return Err(io::Error::other("memory generation byte limit exceeded")); }
        Ok(length)
    } else { Err(io::Error::other("invalid memory generation entry")) }
}

/// The root's exclusive workspace lock serializes admission across processes.
/// No generation is removed to make space, including uncommitted generations.
fn admit_generation(root: &Path, new_bytes: u64) -> io::Result<()> {
    admit_generation_with_limits(root, new_bytes, MAX_RETAINED_GENERATIONS, MAX_GENERATION_BYTES)
}

fn admit_generation_with_limits(root: &Path, new_bytes: u64, max_count: usize, max_bytes: u64) -> io::Result<()> {
    let generations = root.join(".codex-memory-generations");
    if new_bytes > MAX_BYTES { return Err(io::Error::other("memory generation exceeds per-generation byte limit")); }
    let identity = codex_memories_read::publication_snapshot::root_identity(root)?;
    let mut count = 0usize;
    let mut total = 0u64;
    let mut entries = 0usize;
    for entry in std::fs::read_dir(&generations)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().into_string().map_err(|_| io::Error::other(format!("invalid generation name; recovery required at {}", path.display())))?;
        let id = uuid::Uuid::parse_str(&name).map_err(|error| io::Error::other(format!("invalid generation at {}: {error}", path.display())))?;
        if id.to_string() != name { return Err(io::Error::other(format!("invalid generation name; recovery required at {}", path.display()))); }
        validate_root(&entry.path())?;
        let mut marker = String::new();
        open_regular_without_links(&path.join(GENERATION_MARKER)).map_err(|error| io::Error::other(format!("invalid generation marker at {}: {error}", path.display())))?.take(256).read_to_string(&mut marker)?;
        if marker != format!("{identity}\n{name}\n") { return Err(io::Error::other(format!("unowned memory generation; recovery required at {}", path.display()))); }
        count += 1;
        if count >= max_count { return Err(io::Error::new(io::ErrorKind::WouldBlock, format!("memory generation count limit reached; recovery required at {}", generations.display()))); }
        total = total.checked_add(tree_bytes(&entry.path(), &mut entries, MAX_BYTES, 0).map_err(|error| io::Error::other(format!("cannot validate retained generation {}: {error}", entry.path().display())))?)
            .ok_or_else(|| io::Error::other("memory generation size overflow"))?;
    }
    if total >= max_bytes || total.checked_add(new_bytes).is_none_or(|bytes| bytes > max_bytes) {
        return Err(io::Error::new(io::ErrorKind::WouldBlock, format!("memory generation byte limit reached; recovery required at {}", generations.display())));
    }
    Ok(())
}

pub fn check_generation_capacity(root: &Path) -> io::Result<()> {
    admit_generation(root, 0)
}

fn create_generation_root(root: &Path, new_bytes: u64) -> io::Result<(String, PathBuf)> {
    let generations = root.join(".codex-memory-generations");
    match std::fs::create_dir(&generations) {
        Ok(()) => {},
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => validate_root(&generations)?,
        Err(error) => return Err(error),
    }
    admit_generation(root, new_bytes)?;
    let id = uuid::Uuid::new_v4().to_string();
    let output = generations.join(&id);
    std::fs::create_dir(&output)?;
    let identity = codex_memories_read::publication_snapshot::root_identity(root)?;
    let mut marker = std::fs::OpenOptions::new().write(true).create_new(true).open(output.join(GENERATION_MARKER))?;
    write!(marker, "{identity}\n{id}\n")?;
    marker.sync_all()?;
    Ok((id, output))
}

/// Delete only a stage created by this root after successful publication.
/// Failed or unrecognized stages are never removed automatically.
pub fn cleanup_published_stage(root: &Path, stage: &Path) -> io::Result<()> {
    let parent = root.parent().ok_or_else(|| io::Error::other("memory root has no parent"))?.canonicalize()?;
    validate_root(stage)?;
    let canonical = stage.canonicalize()?;
    if canonical.parent() != Some(parent.as_path()) { return Err(io::Error::other("memory stage escaped its parent")); }
    let identity = codex_memories_read::publication_snapshot::root_identity(root)?;
    let prefix = format!("codex-memory-stage-{identity}-");
    let filename = stage.file_name().and_then(|name| name.to_str()).ok_or_else(|| io::Error::other("invalid memory stage name"))?;
    let id = filename.strip_prefix(&prefix).ok_or_else(|| io::Error::other("memory stage owner mismatch"))?;
    let uuid = uuid::Uuid::parse_str(id).map_err(io::Error::other)?;
    if uuid.to_string() != id { return Err(io::Error::other("invalid memory stage id")); }
    let mut marker = String::new();
    open_regular_without_links(&stage.join(STAGE_MARKER))?.take(256).read_to_string(&mut marker)?;
    if marker != format!("{identity}\n{id}\n") { return Err(io::Error::other("memory stage marker mismatch")); }
    crate::publication_tools::validate_stage_budget(stage)?;
    std::fs::remove_dir_all(canonical)
}

/// After worker shutdown and host output validation, prepare one unique
/// unpublished generation. The caller must revalidate inputs and commit its
/// ID under the valid DB lease; a directory alone is never publication.
pub fn prepare_generation(root: &Path, stage: &Path, v1: bool, manifest: &BTreeMap<String, String>) -> io::Result<(String, PathBuf)> {
    let manifest_bytes = serde_json::to_vec(manifest).map_err(io::Error::other)?;
    if manifest_bytes.len() > 2 * 1024 * 1024 { return Err(io::Error::other("memory manifest bytes exceeded")); }
    let mut entries = 0usize;
    let mut new_bytes = manifest_bytes.len() as u64 + 64 + 1 + 36 + 1;
    let mut files = vec!["memory_summary.md", "rollout_summaries", "raw_memories.md"];
    if v1 { files.extend(["MEMORY.md", "skills"]); }
    for name in &files {
        new_bytes = new_bytes.checked_add(tree_bytes(&stage.join(name), &mut entries, MAX_BYTES.saturating_sub(new_bytes), 0)?)
            .ok_or_else(|| io::Error::other("memory generation size overflow"))?;
    }
    if entries.saturating_add(2) > MAX_FILES { return Err(io::Error::other("memory generation entry limit exceeded")); }
    // Reserve the maximum manifest size before admitting the generation.
    let (id, output) = create_generation_root(root, new_bytes + 2 * 1024 * 1024)?;
    let mut budget = Budget::default();
    for name in files {
        copy_tree(&stage.join(name), &output.join(name), &mut budget, 0)?;
    }
    if !metadata(&output.join("memory_summary.md"))?.is_file() {
        return Err(io::Error::other("generation has no summary"));
    }
    if v1 && !metadata(&output.join("MEMORY.md"))?.is_file() {
        return Err(io::Error::other("generation has no MEMORY.md"));
    }
    crate::publication_manifest::write_manifest(&output, manifest)?;
    write_artifact_manifest(&output)?;
    let artifact_bytes = std::fs::metadata(output.join(codex_memories_read::publication_snapshot::ARTIFACT_MANIFEST))?.len();
    let actual = tree_bytes(&output, &mut 0, MAX_BYTES, 0)?;
    if actual != new_bytes + artifact_bytes { return Err(io::Error::other("memory generation size changed during preparation")); }
    // Individual files are sync_all'ed by copy_tree. Directory-entry durability
    // and Windows support must be verified before this helper is integrated.
    Ok((id, output))
}

fn write_artifact_manifest(root: &Path) -> io::Result<()> {
    let bytes = codex_memories_read::publication_snapshot::make_artifact_manifest(root)?;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true)
        .open(root.join(codex_memories_read::publication_snapshot::ARTIFACT_MANIFEST))?;
    file.write_all(&bytes)?;
    file.sync_all()
}

/// Called only after the host has verified that no legacy derived artifacts
/// exist. The DB row must label this generation as an empty legacy baseline.
pub fn prepare_empty_legacy_generation(root: &Path, manifest: &BTreeMap<String, String>) -> io::Result<(String, PathBuf)> {
    let bytes = serde_json::to_vec(manifest).map_err(io::Error::other)?;
    if bytes.len() > 2 * 1024 * 1024 { return Err(io::Error::other("memory manifest bytes exceeded")); }
    let generation = create_generation_root(root, bytes.len() as u64 + 64 + 1 + 36 + 1 + 2 * 1024 * 1024)?;
    crate::publication_manifest::write_manifest(&generation.1, manifest)?;
    write_artifact_manifest(&generation.1)?;
    let artifact_bytes = std::fs::metadata(generation.1.join(codex_memories_read::publication_snapshot::ARTIFACT_MANIFEST))?.len();
    let actual = tree_bytes(&generation.1, &mut 0, MAX_BYTES, 0)?;
    if actual != bytes.len() as u64 + 64 + 1 + 36 + 1 + artifact_bytes { return Err(io::Error::other("empty baseline size changed during preparation")); }
    Ok(generation)
}

/// Only after a validated legacy baseline record has committed. Existing
/// markers are checked but never replaced or removed.
pub fn ensure_managed_marker(root: &Path) -> io::Result<()> {
    let path = root.join(".codex-memory-managed");
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            file.write_all(b"managed-memory-v1\n")?;
            file.sync_all()
        },
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let existing = metadata(&path)?;
            if !existing.is_file() || existing.len() != b"managed-memory-v1\n".len() as u64 {
                return Err(io::Error::other("invalid managed marker"));
            }
            let mut bytes = Vec::new();
            std::fs::File::open(&path)?.take(64).read_to_end(&mut bytes)?;
            if bytes != b"managed-memory-v1\n" { return Err(io::Error::other("invalid managed marker")); }
            Ok(())
        },
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_keeps_notes_and_previous_outputs_but_not_unselected_raw() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("memories");
        std::fs::create_dir_all(root.join("extensions/ad_hoc/notes"))?;
        std::fs::create_dir_all(root.join("rollout_summaries"))?;
        std::fs::write(root.join("extensions/ad_hoc/notes/n.md"), b"protected note")?;
        std::fs::write(root.join("rollout_summaries/unselected.md"), b"do not copy")?;
        std::fs::write(root.join("raw_memories.md"), b"unselected")?;
        std::fs::write(root.join("memory_summary.md"), b"old")?;
        let stage = create_stage(&root, &root, false)?;
        assert_eq!(std::fs::read(stage.join("memory_summary.md"))?, b"old");
        assert_eq!(std::fs::read(stage.join("extensions/ad_hoc/notes/n.md"))?, b"protected note");
        assert!(!stage.join("raw_memories.md").exists());
        assert!(!stage.join("rollout_summaries/unselected.md").exists());
        std::fs::write(stage.join("memory_summary.md"), b"candidate")?;
        assert_eq!(std::fs::read(root.join("memory_summary.md"))?, b"old");
        assert_eq!(std::fs::read(root.join("extensions/ad_hoc/notes/n.md"))?, b"protected note");
        Ok(())
    }

    #[test]
    fn unpublished_generation_contains_selected_sources_without_notes() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("root");
        let stage = temp.path().join("stage");
        std::fs::create_dir(&root)?;
        std::fs::create_dir_all(stage.join("rollout_summaries"))?;
        std::fs::create_dir_all(stage.join("extensions/ad_hoc/notes"))?;
        std::fs::write(stage.join("memory_summary.md"), b"candidate")?;
        std::fs::write(stage.join("rollout_summaries/selected.md"), b"selected")?;
        std::fs::write(stage.join("extensions/ad_hoc/notes/private.md"), b"note")?;
        let (_, output) = prepare_generation(&root, &stage, false, &BTreeMap::new())?;
        assert_eq!(std::fs::read(output.join("memory_summary.md"))?, b"candidate");
        assert_eq!(std::fs::read(output.join("rollout_summaries/selected.md"))?, b"selected");
        assert!(!output.join("extensions").exists());
        Ok(())
    }

    #[test]
    fn marker_retries_refuse_partial_contents_without_overwrite() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let marker = root.join(".codex-memory-managed");
        std::fs::write(&marker, b"partial")?;
        assert!(ensure_managed_marker(root).is_err());
        assert_eq!(std::fs::read(marker)?, b"partial");
        Ok(())
    }

    #[test]
    fn retained_stages_are_bounded_and_only_owned_published_stage_is_removed() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("memory");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), b"original")?;
        let first = create_stage(&root, &root, false)?;
        let second = create_stage(&root, &root, false)?;
        assert!(create_stage(&root, &root, false).is_err());
        std::fs::write(second.join(STAGE_MARKER), b"invalid")?;
        assert!(cleanup_published_stage(&root, &second).is_err());
        assert!(second.exists());
        cleanup_published_stage(&root, &first)?;
        assert!(!first.exists());
        assert_eq!(std::fs::read(root.join("memory_summary.md"))?, b"original");
        Ok(())
    }

    #[test]
    fn generation_count_limit_preserves_existing_generations() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("memory");
        std::fs::create_dir(&root)?;
        let _lock = crate::MemoryWorkspaceLock::try_acquire(&root)?;
        let manifest = BTreeMap::new();
        let (_, first) = prepare_empty_legacy_generation(&root, &manifest)?;
        let first_marker = std::fs::read(first.join(GENERATION_MARKER))?;
        for _ in 1..MAX_RETAINED_GENERATIONS { prepare_empty_legacy_generation(&root, &manifest)?; }
        let error = prepare_empty_legacy_generation(&root, &manifest).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
        assert!(error.to_string().contains("memory generation count limit"));
        assert_eq!(std::fs::read(first.join(GENERATION_MARKER))?, first_marker);
        assert_eq!(std::fs::read_dir(root.join(".codex-memory-generations"))?.count(), MAX_RETAINED_GENERATIONS);
        Ok(())
    }

    #[test]
    fn generation_byte_admission_includes_existing_and_new_logical_bytes() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("memory");
        std::fs::create_dir(&root)?;
        let _lock = crate::MemoryWorkspaceLock::try_acquire(&root)?;
        let (_, existing) = prepare_empty_legacy_generation(&root, &BTreeMap::new())?;
        let actual = tree_bytes(&existing, &mut 0, MAX_BYTES, 0)?;
        assert!(admit_generation_with_limits(&root, 10, 64, actual + 10).is_ok());
        assert_eq!(admit_generation_with_limits(&root, 11, 64, actual + 10).unwrap_err().kind(), io::ErrorKind::WouldBlock);
        assert!(existing.exists());
        Ok(())
    }

    #[test]
    fn root_lock_serializes_generation_admission_across_owners() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("memory");
        std::fs::create_dir(&root)?;
        let first = crate::MemoryWorkspaceLock::try_acquire(&root)?;
        assert_eq!(crate::MemoryWorkspaceLock::try_acquire(&root).err().expect("busy").kind(), io::ErrorKind::WouldBlock);
        drop(first);
        let _second = crate::MemoryWorkspaceLock::try_acquire(&root)?;
        Ok(())
    }

    #[test]
    fn stage_rejects_hard_linked_input_without_touching_source() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("root");
        std::fs::create_dir(&root)?;
        let original = temp.path().join("original");
        std::fs::write(&original, b"protected")?;
        std::fs::hard_link(&original, root.join("memory_summary.md"))?;
        assert!(create_stage(&root, &root, false).is_err());
        assert_eq!(std::fs::read(original)?, b"protected");
        Ok(())
    }
}
