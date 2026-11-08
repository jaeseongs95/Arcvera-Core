use codex_memories_read::publication::MemoryPublication;
use codex_memories_read::publication::PublicationStatus;
use codex_memories_read::publication::load_publication;
use std::path::Path;
use std::path::PathBuf;

pub async fn clear_memory_roots_contents(codex_home: &Path) -> std::io::Result<()> {
    MemoryResetGuard::try_acquire(codex_home)?
        .clear_contents()
        .await
}

/// Acquire every existing reset namespace before clearing the DB. Fixed-order
/// nonblocking acquisition releases earlier guards automatically on failure.
/// This prevents a busy root from causing a DB-only reset, but is not a
/// transaction spanning SQLite and filesystem writes.
/// Acquiring this guard records Resetting intent. Dropping it before a successful
/// clear leaves explicit reset recovery required; it never restores old ready state.
pub struct MemoryResetGuard {
    roots: Vec<PathBuf>,
    _locks: Vec<crate::MemoryWorkspaceLock>,
    publications: std::collections::BTreeMap<PathBuf, MemoryPublication>,
}

impl MemoryResetGuard {
    pub fn try_acquire(codex_home: &Path) -> std::io::Result<Self> {
        if let Err(error) = std::fs::symlink_metadata(codex_home) {
            if error.kind() == std::io::ErrorKind::NotFound {
                return Ok(Self {
                    roots: Vec::new(),
                    _locks: Vec::new(),
                    publications: std::collections::BTreeMap::new(),
                });
            }
            return Err(error);
        }
        let roots = ["memories", "memories_v2", "memories_extensions"]
            .into_iter()
            .map(|name| codex_home.join(name))
            .collect::<Vec<_>>();
        let mut locks = Vec::with_capacity(roots.len());
        for root in &roots {
            validate_reset_root(root)?;
            locks.push(crate::MemoryWorkspaceLock::try_acquire(root)?);
        }
        let mut publications = std::collections::BTreeMap::new();
        // Mark reset intent before DB work. Cancellation after DB clear must
        // not expose the old summary as current merely because FS clear was
        // not reached. A failed preparation remains explicitly incomplete.
        for root in &roots {
            let previous = load_publication(root)?;
            let mut pending = match previous {
                Some(state) if state.status == PublicationStatus::Resetting => state,
                previous => MemoryPublication::next_pending(previous.as_ref())?,
            };
            pending.status = PublicationStatus::Resetting;
            crate::lifecycle::save_publication(root, &pending)?;
            publications.insert(root.clone(), pending);
        }
        Ok(Self {
            roots,
            _locks: locks,
            publications,
        })
    }

    /// Consumes this guard so its locks remain held through the last write.
    /// On IO failure, retain incomplete publication state and report failure;
    /// the caller must not claim a rollback or automatically retry a reset.
    pub async fn clear_contents(self) -> std::io::Result<()> {
        let guard = self;
        tokio::task::spawn_blocking(move || {
            let guard = guard;
            // Own every lock inside the blocking operation even if the async
            // RPC/CLI caller is cancelled while filesystem work is in flight.
            for root in &guard.roots {
                clear_locked_memory_root_contents(
                    root,
                    guard.publications.get(root).expect("prepared reset state"),
                )?;
            }
            Ok(())
        })
        .await
        .map_err(std::io::Error::other)?
    }
}

fn validate_reset_root(memory_root: &Path) -> std::io::Result<bool> {
    match std::fs::symlink_metadata(memory_root) {
        Ok(metadata) => {
            let redirected = metadata.file_type().is_symlink();
            #[cfg(windows)]
            let redirected = {
                use std::os::windows::fs::MetadataExt;
                redirected || metadata.file_attributes() & 0x400 != 0
            };
            if redirected || !metadata.is_dir() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "refusing to clear redirected or non-directory memory root",
                ));
            }
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
pub(crate) async fn clear_memory_root_contents(memory_root: &Path) -> std::io::Result<()> {
    let memory_root = memory_root.to_owned();
    tokio::task::spawn_blocking(move || {
        validate_reset_root(&memory_root)?;
        let _lock = crate::MemoryWorkspaceLock::try_acquire(&memory_root)?;
        let previous = load_publication(&memory_root)?;
        let mut pending = match previous {
            Some(state) if state.status == PublicationStatus::Resetting => state,
            previous => MemoryPublication::next_pending(previous.as_ref())?,
        };
        pending.status = PublicationStatus::Resetting;
        crate::lifecycle::save_publication(&memory_root, &pending)?;
        clear_locked_memory_root_contents(&memory_root, &pending)
    })
    .await
    .map_err(std::io::Error::other)?
}

fn clear_locked_memory_root_contents(
    memory_root: &Path,
    pending: &MemoryPublication,
) -> std::io::Result<()> {
    let root_exists = validate_reset_root(memory_root)?;

    // The reset guard already owns this root's lock. Keep the original reset
    // targets; project stores are not included by the legacy global API.
    let mut publication = pending.clone();

    if !root_exists {
        publication.status = PublicationStatus::Cleared;
        publication.published_generation = Some(publication.accepted_generation);
        return crate::lifecycle::save_publication(memory_root, &publication);
    }

    std::fs::create_dir_all(memory_root)?;

    for entry in std::fs::read_dir(memory_root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            std::fs::remove_dir_all(path)?;
        } else {
            std::fs::remove_file(path)?;
        }
    }

    publication.status = PublicationStatus::Cleared;
    publication.published_generation = Some(publication.accepted_generation);
    crate::lifecycle::save_publication(memory_root, &publication)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn incomplete_reset_blocks_note_repopulation_and_explicit_retry_can_clear_it()
    -> anyhow::Result<()> {
        let home = tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "old synthetic fact")?;
        let guard = MemoryResetGuard::try_acquire(home.path())?;
        let generation = load_publication(&root)?.unwrap().accepted_generation;
        drop(guard); // Simulate DB failure/caller cancellation before FS clear.
        assert_eq!(
            load_publication(&root)?.unwrap().status,
            PublicationStatus::Resetting
        );
        assert_eq!(
            crate::add_user_memory_note(&root, "new input while reset incomplete")
                .await
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::WouldBlock
        );
        let view =
            crate::memory_publication_snapshot(&root, codex_protocol::MemoryVersion::V2).await?;
        assert_eq!(view.status, "resetting");
        assert!(view.summary.is_none());
        let retry = MemoryResetGuard::try_acquire(home.path())?;
        retry.clear_contents().await?;
        let cleared = load_publication(&root)?.unwrap();
        assert_eq!(cleared.status, PublicationStatus::Cleared);
        assert_eq!(cleared.accepted_generation, generation);
        Ok(())
    }

    #[tokio::test]
    async fn clear_memory_root_contents_preserves_root_directory() {
        let dir = tempdir().expect("tempdir");
        let root = dir.path().join("memories");
        let nested_dir = root.join("rollout_summaries");
        tokio::fs::create_dir_all(&nested_dir)
            .await
            .expect("create rollout summaries dir");
        tokio::fs::write(root.join("MEMORY.md"), "stale memory index\n")
            .await
            .expect("write memory index");
        tokio::fs::write(nested_dir.join("rollout.md"), "stale rollout\n")
            .await
            .expect("write rollout summary");

        clear_memory_root_contents(&root)
            .await
            .expect("clear memory root contents");

        assert!(
            tokio::fs::try_exists(&root)
                .await
                .expect("check memory root existence"),
            "memory root should still exist after clearing contents"
        );
        let mut entries = tokio::fs::read_dir(&root)
            .await
            .expect("read memory root after clear");
        assert!(
            entries
                .next_entry()
                .await
                .expect("read next entry")
                .is_none(),
            "memory root should be empty after clearing contents"
        );
        let publication = load_publication(&root)
            .expect("read cleared publication")
            .expect("cleared state");
        assert_eq!(publication.status, PublicationStatus::Cleared);
        assert_eq!(
            publication.published_generation,
            Some(publication.accepted_generation)
        );
        assert!(publication.artifact_digests.is_empty());
    }

    #[tokio::test]
    async fn reset_guard_releases_earlier_roots_if_a_later_root_is_busy() -> anyhow::Result<()> {
        let home = tempdir()?;
        let busy = crate::MemoryWorkspaceLock::try_acquire(&home.path().join("memories_v2"))?;
        let error = match MemoryResetGuard::try_acquire(home.path()) {
            Ok(_) => anyhow::bail!("busy reset must be rejected before DB work"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        let first = crate::MemoryWorkspaceLock::try_acquire(&home.path().join("memories"))?;
        drop(first);
        drop(busy);
        let guard = MemoryResetGuard::try_acquire(home.path())?;
        guard.clear_contents().await?;
        let again = MemoryResetGuard::try_acquire(home.path())?;
        drop(again);
        Ok(())
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn clear_memory_root_contents_rejects_symlinked_root() {
        let dir = tempdir().expect("tempdir");
        let target = dir.path().join("outside");
        tokio::fs::create_dir_all(&target)
            .await
            .expect("create symlink target dir");
        let target_file = target.join("keep.txt");
        tokio::fs::write(&target_file, "keep\n")
            .await
            .expect("write target file");

        let root = dir.path().join("memories");
        std::os::unix::fs::symlink(&target, &root).expect("create memory root symlink");

        let err = clear_memory_root_contents(&root)
            .await
            .expect_err("symlinked memory root should be rejected");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(
            tokio::fs::try_exists(&target_file)
                .await
                .expect("check target file existence"),
            "rejecting a symlinked memory root should not delete the symlink target"
        );
    }
}
