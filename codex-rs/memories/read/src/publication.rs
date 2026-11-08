//! Host-owned receipt shared by memory readers and writers.
//! A receipt binds artifact bytes to an accepted generation. It does not certify
//! semantic application of a natural-language correction by a model.

use codex_protocol::MemoryVersion;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::fs::File;
use std::fs::OpenOptions;
use std::fs::TryLockError;
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

pub const PUBLICATION_SCHEMA_VERSION: u32 = 1;
// A receipt must hold the digest and relative path for a complete bounded v1
// final artifact set; the former 16 KiB cap only covered a single summary.
pub const MAX_PUBLICATION_BYTES: u64 = 1024 * 1024;
pub const MAX_PUBLISHED_ARTIFACTS: usize = 256;
pub const MAX_PUBLISHED_ARTIFACT_BYTES: u64 = 1024 * 1024;
pub const MAX_PUBLISHED_TOTAL_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_PUBLISHED_DIRECTORY_ENTRIES: usize = 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PublicationStatus {
    Pending,
    Processing,
    Published,
    Failed,
    Cleared,
    Resetting,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct MemoryPublication {
    pub schema_version: u32,
    pub accepted_generation: u64,
    pub published_generation: Option<u64>,
    pub status: PublicationStatus,
    pub artifact_digests: BTreeMap<String, String>,
    /// Host recovery metadata only. Readers must never inject this path into model context.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery_stage: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicationReadiness {
    Ready,
    Pending,
    Invalid,
    Cleared,
}

impl MemoryPublication {
    /// Persist this state before acknowledging a new user edit.
    pub fn next_pending(previous: Option<&Self>) -> io::Result<Self> {
        if previous.is_some_and(|publication| publication.status == PublicationStatus::Resetting) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "memory reset is incomplete; retry the explicit reset before accepting new memory edits",
            ));
        }
        if let Some(previous) = previous {
            previous.validate()?;
        }
        let accepted_generation = previous
            .map_or(0, |publication| publication.accepted_generation)
            .checked_add(1)
            .ok_or_else(|| io::Error::other("memory publication generation overflow"))?;
        Ok(Self {
            schema_version: PUBLICATION_SCHEMA_VERSION,
            accepted_generation,
            published_generation: previous.and_then(|publication| publication.published_generation),
            status: PublicationStatus::Pending,
            artifact_digests: BTreeMap::new(),
            recovery_stage: previous.and_then(|publication| publication.recovery_stage.clone()),
        })
    }

    /// Check the exact bytes selected for the model, rather than rereading a
    /// path which could already contain a different worker version.
    pub fn readiness_for_summary(&self, summary: &[u8]) -> PublicationReadiness {
        if self.validate().is_err() {
            return PublicationReadiness::Invalid;
        }
        if self.status == PublicationStatus::Cleared {
            return PublicationReadiness::Cleared;
        }
        if self.status != PublicationStatus::Published
            || self.published_generation != Some(self.accepted_generation)
        {
            return PublicationReadiness::Pending;
        }
        if self.artifact_digests.get("memory_summary.md") != Some(&sha256(summary)) {
            return PublicationReadiness::Invalid;
        }
        PublicationReadiness::Ready
    }

    fn validate(&self) -> io::Result<()> {
        if self.schema_version != PUBLICATION_SCHEMA_VERSION
            || self.accepted_generation == 0
            || self
                .published_generation
                .is_some_and(|value| value == 0 || value > self.accepted_generation)
            || self.status == PublicationStatus::Published
                && self.published_generation != Some(self.accepted_generation)
            || self.status == PublicationStatus::Cleared
                && (self.published_generation != Some(self.accepted_generation)
                    || !self.artifact_digests.is_empty())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported or inconsistent memory publication receipt",
            ));
        }
        Ok(())
    }
}

/// Outside the managed worker's writable root. Disabled/external sandboxes
/// still require host-side lock ownership and publication-state validation.
pub fn publication_path(root: &Path) -> PathBuf {
    root.with_extension("publication.json")
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn is_published_artifact_path(path: &str, version: MemoryVersion) -> bool {
    if path.is_empty() || path.len() > 1024 || path.contains(['\\', ':']) {
        return false;
    }
    let parts = path.split('/').collect::<Vec<_>>();
    if parts
        .iter()
        .any(|part| part.is_empty() || part.starts_with('.'))
    {
        return false;
    }
    match version {
        MemoryVersion::V2 => path == "memory_summary.md",
        MemoryVersion::V1 => {
            matches!(path, "memory_summary.md" | "MEMORY.md")
                || parts.len() >= 3 && parts[0] == "skills"
        }
    }
}

fn require_regular_entry(path: &Path, directory: bool) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    let redirected = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        redirected || metadata.file_attributes() & 0x400 != 0
    };
    if redirected
        || if directory {
            !metadata.is_dir()
        } else {
            !metadata.is_file()
        }
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "published artifact path is redirected or has the wrong type",
        ));
    }
    Ok(())
}

/// Collect only final outputs. Raw extraction, rollout inputs and user notes
/// are deliberately outside this set, even when their bytes have not changed.
/// Callers hold the same namespace lock as writers for the entire snapshot.
pub fn collect_published_artifacts(
    root: &Path,
    version: MemoryVersion,
) -> io::Result<BTreeMap<String, Vec<u8>>> {
    require_regular_entry(root, true)?;
    let mut paths = vec!["memory_summary.md".to_string()];
    if version == MemoryVersion::V1 {
        paths.push("MEMORY.md".to_string());
        let skills = root.join("skills");
        match std::fs::symlink_metadata(&skills) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
            Ok(_) => {
                require_regular_entry(&skills, true)?;
                let mut pending = vec!["skills".to_string()];
                let mut entries = 0;
                while let Some(relative) = pending.pop() {
                    for entry in std::fs::read_dir(root.join(&relative))? {
                        entries += 1;
                        if entries > MAX_PUBLISHED_DIRECTORY_ENTRIES {
                            return Err(io::Error::other(
                                "published skills exceed directory entry budget",
                            ));
                        }
                        let entry = entry?;
                        let name = entry.file_name().into_string().map_err(|_| {
                            io::Error::other("published artifact path is not Unicode")
                        })?;
                        let path = format!("{relative}/{name}");
                        if !is_published_artifact_path(&path, version) {
                            // A skill directory is not itself a file key.
                            if relative != "skills"
                                || name.is_empty()
                                || name.starts_with('.')
                                || name.contains(['/', '\\', ':'])
                            {
                                return Err(io::Error::other("invalid published skill path"));
                            }
                        }
                        let metadata = std::fs::symlink_metadata(entry.path())?;
                        if metadata.is_dir() {
                            require_regular_entry(&entry.path(), true)?;
                            pending.push(path);
                        } else {
                            require_regular_entry(&entry.path(), false)?;
                            if !is_published_artifact_path(&path, version) {
                                return Err(io::Error::other(
                                    "published skill files require their skill directory",
                                ));
                            }
                            paths.push(path);
                            if paths.len() > MAX_PUBLISHED_ARTIFACTS {
                                return Err(io::Error::other(
                                    "published memory exceeds artifact count budget",
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    let mut artifacts = BTreeMap::new();
    let mut total_bytes = 0u64;
    for relative in paths {
        let path = root.join(&relative);
        require_regular_entry(&path, false)?;
        let mut bytes = Vec::new();
        File::open(path)?
            .take(MAX_PUBLISHED_ARTIFACT_BYTES + 1)
            .read_to_end(&mut bytes)?;
        total_bytes = total_bytes.saturating_add(bytes.len() as u64);
        if bytes.len() as u64 > MAX_PUBLISHED_ARTIFACT_BYTES
            || total_bytes > MAX_PUBLISHED_TOTAL_BYTES
        {
            return Err(io::Error::other(
                "published memory exceeds artifact byte budget",
            ));
        }
        artifacts.insert(relative, bytes);
    }
    Ok(artifacts)
}

pub fn published_artifact_digests(
    artifacts: &BTreeMap<String, Vec<u8>>,
) -> BTreeMap<String, String> {
    artifacts
        .iter()
        .map(|(path, bytes)| (path.clone(), sha256(bytes)))
        .collect()
}

pub fn read_published_artifacts(
    root: &Path,
    receipt: &MemoryPublication,
    version: MemoryVersion,
) -> io::Result<BTreeMap<String, Vec<u8>>> {
    receipt.validate()?;
    if receipt.status != PublicationStatus::Published
        || receipt.published_generation != Some(receipt.accepted_generation)
        || receipt.artifact_digests.len() > MAX_PUBLISHED_ARTIFACTS
        || receipt
            .artifact_digests
            .keys()
            .any(|path| !is_published_artifact_path(path, version))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "memory artifact set is not a current publication",
        ));
    }
    let artifacts = collect_published_artifacts(root, version)?;
    if published_artifact_digests(&artifacts) != receipt.artifact_digests {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "published artifact set or bytes do not match the receipt",
        ));
    }
    Ok(artifacts)
}

/// Cross-process exclusion shared by note acceptance, imports and consolidation.
/// Lock files live beside memory roots and must never be replaced or removed.
pub struct MemoryWorkspaceLock {
    _file: File,
}

impl MemoryWorkspaceLock {
    pub fn try_acquire(root: &Path) -> io::Result<Self> {
        Self::try_acquire_inner(root, false)
    }

    pub fn try_acquire_shared(root: &Path) -> io::Result<Self> {
        Self::try_acquire_inner(root, true)
    }

    fn try_acquire_inner(root: &Path, shared: bool) -> io::Result<Self> {
        let lock_path = root.with_extension("lock");
        match std::fs::symlink_metadata(&lock_path) {
            Ok(metadata) => {
                let redirected = metadata.file_type().is_symlink();
                #[cfg(windows)]
                let redirected = {
                    use std::os::windows::fs::MetadataExt;
                    redirected || metadata.file_attributes() & 0x400 != 0
                };
                if redirected || !metadata.is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "memory lock must be a regular file",
                    ));
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        let result = if shared {
            file.try_lock_shared()
        } else {
            file.try_lock()
        };
        match result {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "memory workspace is busy; retry after consolidation or another import finishes",
            )),
            Err(TryLockError::Error(error)) => Err(error),
        }
    }
}

/// Absence is legacy/unversioned. Errors must never be treated as legacy.
pub fn load_publication(root: &Path) -> io::Result<Option<MemoryPublication>> {
    let path = publication_path(root);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(value) => value,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let redirected = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        redirected || metadata.file_attributes() & 0x400 != 0
    };
    if redirected || !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "publication receipt must be a regular file",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_PUBLICATION_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_PUBLICATION_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "publication receipt exceeds input limit",
        ));
    }
    let publication: MemoryPublication = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    publication.validate()?;
    Ok(Some(publication))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepted_edit_cannot_inherit_an_earlier_publication() {
        let mut publication = MemoryPublication::next_pending(None).unwrap();
        publication.status = PublicationStatus::Published;
        publication.published_generation = Some(1);
        publication
            .artifact_digests
            .insert("memory_summary.md".to_string(), sha256(b"summary"));
        assert_eq!(
            publication.readiness_for_summary(b"summary"),
            PublicationReadiness::Ready
        );
        let pending = MemoryPublication::next_pending(Some(&publication)).unwrap();
        assert_eq!(pending.accepted_generation, 2);
        assert_eq!(pending.published_generation, Some(1));
        assert_eq!(
            pending.readiness_for_summary(b"summary"),
            PublicationReadiness::Pending
        );
    }

    #[test]
    fn receipt_is_bound_to_selected_bytes_and_generation() {
        let mut publication = MemoryPublication::next_pending(None).unwrap();
        publication.status = PublicationStatus::Published;
        publication.published_generation = Some(1);
        publication
            .artifact_digests
            .insert("memory_summary.md".to_string(), sha256(b"new"));
        assert_eq!(
            publication.readiness_for_summary(b"old"),
            PublicationReadiness::Invalid
        );
        publication.published_generation = Some(2);
        assert_eq!(
            publication.readiness_for_summary(b"new"),
            PublicationReadiness::Invalid
        );
        publication.accepted_generation = u64::MAX;
        assert!(MemoryPublication::next_pending(Some(&publication)).is_err());
    }

    #[test]
    fn incomplete_reset_blocks_new_edits_and_old_summary_while_recovery_metadata_is_preserved() {
        let mut publication = MemoryPublication::next_pending(None).unwrap();
        publication.recovery_stage = Some("host-only-recovery-evidence".to_string());
        publication.status = PublicationStatus::Failed;
        let pending = MemoryPublication::next_pending(Some(&publication)).unwrap();
        assert_eq!(pending.recovery_stage, publication.recovery_stage);
        publication.status = PublicationStatus::Resetting;
        assert_eq!(
            MemoryPublication::next_pending(Some(&publication))
                .unwrap_err()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        assert_eq!(
            publication.readiness_for_summary(b"old"),
            PublicationReadiness::Pending
        );
        publication.status = PublicationStatus::Cleared;
        publication.published_generation = Some(publication.accepted_generation);
        assert_eq!(
            publication.readiness_for_summary(b"old"),
            PublicationReadiness::Cleared
        );
    }

    #[test]
    fn published_v1_snapshot_binds_final_skills_and_rejects_raw_or_changed_artifacts() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("memories");
        std::fs::create_dir_all(root.join("skills/example")).unwrap();
        std::fs::create_dir_all(root.join("rollout_summaries")).unwrap();
        std::fs::write(root.join("memory_summary.md"), b"current summary").unwrap();
        std::fs::write(root.join("MEMORY.md"), b"current details").unwrap();
        std::fs::write(root.join("skills/example/SKILL.md"), b"current skill").unwrap();
        std::fs::write(
            root.join("rollout_summaries/old.md"),
            b"OLD_DERIVED_NOT_PUBLISHED",
        )
        .unwrap();
        let artifacts = collect_published_artifacts(&root, MemoryVersion::V1).unwrap();
        assert_eq!(artifacts.len(), 3);
        assert!(!artifacts.contains_key("rollout_summaries/old.md"));
        let mut receipt = MemoryPublication::next_pending(None).unwrap();
        receipt.status = PublicationStatus::Published;
        receipt.published_generation = Some(receipt.accepted_generation);
        receipt.artifact_digests = published_artifact_digests(&artifacts);
        assert_eq!(
            read_published_artifacts(&root, &receipt, MemoryVersion::V1).unwrap(),
            artifacts
        );
        std::fs::write(root.join("skills/example/SKILL.md"), b"later changed skill").unwrap();
        assert!(read_published_artifacts(&root, &receipt, MemoryVersion::V1).is_err());
        assert_eq!(artifacts["skills/example/SKILL.md"], b"current skill");
        for path in [
            "../memory_summary.md",
            "skills/example/../raw.md",
            "skills/.hidden/a",
            "raw_memories.md",
            "rollout_summaries/old.md",
            "skills/example/a:stream",
        ] {
            assert!(
                !is_published_artifact_path(path, MemoryVersion::V1),
                "unexpected allowlist path: {path}"
            );
        }
    }

    #[test]
    fn invalid_receipt_and_artifact_budgets_fail_closed_before_snapshot_use() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("memories_v2");
        std::fs::create_dir_all(&root).unwrap();
        let mut receipt = MemoryPublication::next_pending(None).unwrap();
        receipt.published_generation = Some(0);
        std::fs::write(
            publication_path(&root),
            serde_json::to_vec(&receipt).unwrap(),
        )
        .unwrap();
        assert_eq!(
            load_publication(&root).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        std::fs::write(
            root.join("memory_summary.md"),
            vec![b'x'; MAX_PUBLISHED_ARTIFACT_BYTES as usize + 1],
        )
        .unwrap();
        assert!(collect_published_artifacts(&root, MemoryVersion::V2).is_err());
        std::fs::write(root.join("memory_summary.md"), b"small").unwrap();
        let held = MemoryWorkspaceLock::try_acquire(&root).unwrap();
        assert_eq!(
            MemoryWorkspaceLock::try_acquire_shared(&root)
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::WouldBlock
        );
        drop(held);
        assert!(MemoryWorkspaceLock::try_acquire_shared(&root).is_ok());
    }
}
