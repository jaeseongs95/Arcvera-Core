// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Bind a memory publication reader to the thread's pinned namespace.
use std::{future::Future, io, pin::Pin};
use codex_memories_read::publication_snapshot::ScopedPublicationLookup;
use codex_memories_read::publication_snapshot::PublicationRecord;
use codex_memories_read::publication_snapshot::{MemorySnapshotCapability, ScopedSnapshotProvider, root_identity};
use std::sync::Arc;

pub(crate) struct BoundMemoryPublication {
    pub(crate) state_db: codex_rollout::StateDbHandle,
    pub(crate) version: codex_protocol::MemoryVersion,
    pub(crate) project_key: Option<String>,
    pub(crate) root_identity: String,
    pub(crate) store: tokio::sync::OnceCell<codex_state::MemoryStore>,
}

/// Bind the existing state service to one host-pinned namespace, never to an
/// arbitrary root supplied by the model or a memory tool.
pub(crate) async fn bind_memory_snapshot(
    config: &crate::config::Config,
    state_db: Option<&codex_rollout::StateDbHandle>,
) -> Option<MemorySnapshotCapability> {
    let root = config.memories.memory_root(&config.codex_home)?;
    let identity = match root_identity(root.as_path()) {
        Ok(identity) => identity,
        Err(error) => { tracing::warn!(%error, "invalid pinned memory root"); return None; }
    };
    let project_key = match config.memories.scope {
        codex_config::types::MemoryScope::Global => None,
        codex_config::types::MemoryScope::Project => Some(config.memories.project.as_ref()?.key.clone()),
    };
    let lookup = state_db.map(|db| Arc::new(BoundMemoryPublication {
        state_db: Arc::clone(db), version: config.memories.version, project_key,
        root_identity: identity, store: tokio::sync::OnceCell::new(),
    }) as Arc<dyn ScopedPublicationLookup>);
    Some(MemorySnapshotCapability(Arc::new(ScopedSnapshotProvider::new(root.to_path_buf(), lookup))))
}

impl ScopedPublicationLookup for BoundMemoryPublication {
    fn generation(&self) -> Pin<Box<dyn Future<Output = io::Result<Option<PublicationRecord>>> + Send + '_>> {
        Box::pin(async move {
            let store = self.store.get_or_try_init(|| async {
                match &self.project_key {
                    Some(key) => self.state_db.memories_for_project(self.version, key).await,
                    None => self.state_db.memories_for_version(self.version).await,
                }
            }).await.map_err(io::Error::other)?;
            Ok(store.published_generation(&self.root_identity).await.map_err(io::Error::other)?
                .map(|r| PublicationRecord { artifact_manifest_digest: r.artifact_manifest_digest, generation_id: r.generation_id, notes_digest: r.notes_digest, provenance: r.provenance, legacy_empty: r.legacy_empty }))
        })
    }
}
