// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use crate::backend::{AddAdHocMemoryNoteRequest, AddAdHocMemoryNoteResponse, MemoriesBackendError};
use super::LocalMemoriesBackend;

pub(super) async fn add_ad_hoc_note(
    backend: &LocalMemoriesBackend,
    request: AddAdHocMemoryNoteRequest,
) -> Result<AddAdHocMemoryNoteResponse, MemoriesBackendError> {
    codex_memories_write::validate_memory_note_filename(&request.filename)
        .map_err(|reason| MemoriesBackendError::invalid_filename(&request.filename, reason))?;
    if request.note.trim().is_empty() {
        return Err(MemoriesBackendError::EmptyAdHocNote);
    }
    match codex_memories_write::add_user_memory_note_named(
        &backend.original_root, &request.filename, &request.note
    ).await {
        Ok(_) => Ok(AddAdHocMemoryNoteResponse {}),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists =>
            Err(MemoriesBackendError::AdHocNoteAlreadyExists { filename: request.filename }),
        Err(error) => Err(error.into()),
    }
}
#[cfg(test)]
mod consolidation_lock_tests {
    use super::*;
    #[tokio::test]
    async fn busy_consolidation_rejects_note_without_creating_it() -> std::io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("memories");
        std::fs::create_dir(&root)?;
        let _lock = codex_memories_write::MemoryWorkspaceLock::try_acquire(&root)?;
        let backend = LocalMemoriesBackend::from_memory_root(&root);
        let result = add_ad_hoc_note(&backend, AddAdHocMemoryNoteRequest {
            filename: "2026-11-14T10-20-30-test.md".into(),
            note: "synthetic note".into(),
        }).await;
        assert!(result.is_err());
        assert!(!root.join("extensions/ad_hoc/notes/2026-11-14T10-20-30-test.md").exists());
        Ok(())
    }
}
