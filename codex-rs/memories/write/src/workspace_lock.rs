// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Cross-process exclusion between memory imports and consolidation.
//! Lock files live beside memory roots and must not be removed or replaced.

use std::fs::File;
use std::fs::TryLockError;
use std::io;
use std::path::Path;

/// Keeps a memory workspace exclusively owned until all writers have stopped.
pub struct MemoryWorkspaceLock {
    _file: File,
}

impl MemoryWorkspaceLock {
    /// Returns `WouldBlock` when another import or consolidation owns this root.
    pub fn try_acquire(memory_root: &Path) -> io::Result<Self> {
        // Keep this outside the workspace: baseline resets and memory resets must
        // not replace the inode while another process still owns its lock.
        let path = memory_root.with_extension("lock");
        let open = || codex_memories_read::publication_notes::open_regular_for_update_without_links(&path, false);
        let file = match open() {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match codex_memories_read::publication_notes::open_regular_for_update_without_links(&path, true) {
                    Ok(file) => file,
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => open()?,
                    Err(error) => return Err(error),
                }
            }
            Err(error) => return Err(error),
        };
        match file.try_lock() {
            Ok(()) => Ok(Self { _file: file }),
            Err(TryLockError::WouldBlock) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "memory workspace is busy; retry after consolidation or another import finishes",
            )),
            Err(TryLockError::Error(error)) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_hard_linked_lock_file_without_modifying_target() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let target = temp.path().join("target");
        std::fs::write(&target, b"protected")?;
        let root = temp.path().join("memory");
        std::fs::hard_link(&target, root.with_extension("lock"))?;
        assert!(MemoryWorkspaceLock::try_acquire(&root).is_err());
        assert_eq!(std::fs::read(target)?, b"protected");
        Ok(())
    }
}
