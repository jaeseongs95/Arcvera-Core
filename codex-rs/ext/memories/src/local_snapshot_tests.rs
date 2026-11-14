// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
// Proposed local backend tests; synthetic provider is not end-to-end DB evidence.
use super::*;
use crate::backend::{ListMemoriesRequest, MemoriesBackend, ReadMemoryRequest, SearchMatchMode, SearchMemoriesRequest};
use codex_memories_read::publication_snapshot::{MemorySnapshotCapability, PublicationRecord, ScopedPublicationLookup, ScopedSnapshotProvider};
use std::{future::Future, io, pin::Pin, sync::{Arc, Mutex}};

struct Lookup(Mutex<PublicationRecord>);
impl ScopedPublicationLookup for Lookup {
    fn generation(&self) -> Pin<Box<dyn Future<Output = io::Result<Option<PublicationRecord>>> + Send + '_>> {
        Box::pin(async move { Ok(Some(self.0.lock().unwrap().clone())) })
    }
}

#[tokio::test]
async fn generation_read_routes_notes_and_rejects_old_cursor_after_switch() -> io::Result<()> {
    let temp = tempfile::tempdir()?;
    let root = temp.path().join("memory");
    std::fs::create_dir_all(root.join("extensions/ad_hoc/notes"))?;
    std::fs::write(root.join(".codex-memory-managed"), b"managed-memory-v1\n")?;
    std::fs::write(root.with_extension("lock"), b"")?;
    std::fs::write(root.join("extensions/ad_hoc/notes/fixture.md"), b"note-source")?;
    let old = uuid::Uuid::new_v4().to_string();
    let new = uuid::Uuid::new_v4().to_string();
    for id in [&old, &new] {
        let generation = root.join(".codex-memory-generations").join(id);
        std::fs::create_dir_all(&generation)?;
        std::fs::write(generation.join("MEMORY.md"), id)?;
        std::fs::write(generation.join("memory_summary.md"), id)?;
        std::fs::write(generation.join(".codex-memory-generation"), format!("{}\n{id}\n", codex_memories_read::publication_snapshot::root_identity(&root)?))?;
        let manifest = codex_memories_read::publication_snapshot::make_artifact_manifest(&generation)?;
        std::fs::write(generation.join(codex_memories_read::publication_snapshot::ARTIFACT_MANIFEST), manifest)?;
    }
    let digest = codex_memories_read::publication_notes::notes_digest(&root)?;
    let old_digest = codex_memories_read::publication_snapshot::artifact_manifest_digest(&std::fs::read(root.join(".codex-memory-generations").join(&old).join(codex_memories_read::publication_snapshot::ARTIFACT_MANIFEST))?);
    let new_digest = codex_memories_read::publication_snapshot::artifact_manifest_digest(&std::fs::read(root.join(".codex-memory-generations").join(&new).join(codex_memories_read::publication_snapshot::ARTIFACT_MANIFEST))?);
    let lookup = Arc::new(Lookup(Mutex::new(PublicationRecord { artifact_manifest_digest: old_digest, generation_id: old.clone(), notes_digest: digest, provenance: "consolidated".into(), legacy_empty: false })));
    let capability = MemorySnapshotCapability(Arc::new(ScopedSnapshotProvider::new(root.clone(), Some(lookup.clone()))));
    let backend = LocalMemoriesBackend::from_snapshot(root, Some(capability));
    let note = backend.read(ReadMemoryRequest { path: "extensions/ad_hoc/notes/fixture.md".into(), line_offset: 1, max_lines: None, max_tokens: 100 }).await.map_err(io::Error::other)?;
    assert_eq!(note.content, "note-source");
    let summary_path = backend.original_root.join(".codex-memory-generations").join(&old).join("memory_summary.md");
    std::fs::write(&summary_path, b"corrupt")?;
    assert!(backend.read(ReadMemoryRequest { path: "memory_summary.md".into(), line_offset: 1, max_lines: None, max_tokens: 100 }).await.is_err());
    assert!(backend.list(ListMemoriesRequest { path: None, cursor: None, max_results: 10 }).await.is_err());
    assert!(backend.search(SearchMemoriesRequest { queries: vec!["corrupt".into()], match_mode: SearchMatchMode::Any, path: None, cursor: None, context_lines: 0, case_sensitive: false, normalized: false, max_results: 10 }).await.is_err());
    std::fs::write(&summary_path, old.as_bytes())?;
    let first = backend.list(ListMemoriesRequest { path: None, cursor: None, max_results: 1 }).await.map_err(io::Error::other)?;
    let cursor = first.next_cursor.expect("at least two entries");
    let mut current = lookup.0.lock().unwrap();
    current.generation_id = new;
    current.artifact_manifest_digest = new_digest;
    drop(current);
    assert!(backend.list(ListMemoriesRequest { path: None, cursor: Some(cursor), max_results: 1 }).await.is_err());
    Ok(())
}
