// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use crate::backend::AddAdHocMemoryNoteRequest;
use crate::backend::AddAdHocMemoryNoteResponse;
use crate::backend::ListMemoriesRequest;
use crate::backend::ListMemoriesResponse;
use crate::backend::MemoriesBackend;
use crate::backend::MemoriesBackendError;
use crate::backend::ReadMemoryRequest;
use crate::backend::ReadMemoryResponse;
use crate::backend::SearchMemoriesRequest;
use crate::backend::SearchMemoriesResponse;

mod ad_hoc_note;
mod list;
mod path;
mod read;
mod search;

#[cfg(test)]
#[path = "local_snapshot_tests.rs"]
mod snapshot_tests;

#[derive(Clone)]
pub(crate) struct LocalMemoriesBackend {
    root: PathBuf,
    original_root: PathBuf,
    snapshot: Option<codex_memories_read::publication_snapshot::MemorySnapshotCapability>,
    snapshot_required: bool,
    generation: Option<String>,
    notes_observation: Option<String>,
    artifact_manifest: Option<std::sync::Arc<std::collections::BTreeMap<String, String>>>,
}

impl LocalMemoriesBackend {
    pub(crate) fn from_memory_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self { root: root.clone(), original_root: root, snapshot: None, snapshot_required: false, generation: None, notes_observation: None, artifact_manifest: None }
    }

    pub(crate) fn from_snapshot(root: PathBuf, snapshot: Option<codex_memories_read::publication_snapshot::MemorySnapshotCapability>) -> Self {
        Self { root: root.clone(), original_root: root, snapshot, snapshot_required: true, generation: None, notes_observation: None, artifact_manifest: None }
    }

    async fn view(&self, path: Option<&str>) -> Result<Self, MemoriesBackendError> {
        let Some(capability) = &self.snapshot else {
            if self.snapshot_required { return Err(std::io::Error::other("memory snapshot is unavailable").into()); }
            return Ok(self.clone());
        };
        let snapshot = capability.0.capture().await?;
        if snapshot.original_root != self.original_root { return Err(std::io::Error::other("memory snapshot root changed").into()); }
        let is_notes = path.is_some_and(|path| Path::new(path).components()
            .find(|component| !matches!(component, Component::CurDir))
            .is_some_and(|component| matches!(component, Component::Normal(name) if name == "extensions")));
        Ok(Self {
            root: if is_notes { snapshot.original_root.clone() } else { snapshot.derived_root },
            original_root: snapshot.original_root,
            snapshot: None, snapshot_required: false, generation: snapshot.generation_id, notes_observation: snapshot.notes_observation, artifact_manifest: if is_notes { None } else { snapshot.artifact_manifest.map(std::sync::Arc::new) },
        })
    }

    async fn read_artifact(&self, path: &Path) -> std::io::Result<String> {
        String::from_utf8(self.read_artifact_bytes(path).await?)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    async fn read_artifact_bytes(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        let root = self.root.clone();
        let path = path.to_path_buf();
        let manifest = if path.starts_with(&root) { self.artifact_manifest.clone() } else { None };
        tokio::task::spawn_blocking(move || {
            use std::io::Read;
            let bytes = if let Some(manifest) = manifest {
                codex_memories_read::publication_snapshot::read_verified_artifact(&root, &path, &manifest, 32 * 1024 * 1024)?
            } else {
                let mut bytes = Vec::new();
                codex_memories_read::publication_notes::open_regular_without_links(&path)?.take(32 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
                if bytes.len() > 32 * 1024 * 1024 { return Err(std::io::Error::other("memory artifact exceeds limit")); }
                bytes
            };
            Ok(bytes)
        }).await.map_err(std::io::Error::other)?
    }

    fn cursor_offset(&self, cursor: Option<&str>) -> Result<usize, MemoriesBackendError> {
        let Some(cursor) = cursor else { return Ok(0); };
        let numeric = match &self.generation {
            Some(id) => cursor.strip_prefix(&format!("g:{id}:{}:", self.notes_observation.as_deref().unwrap_or("unknown")))
                .ok_or_else(|| MemoriesBackendError::invalid_cursor(cursor, "belongs to another memory generation"))?,
            None => cursor,
        };
        numeric.parse::<usize>().map_err(|_| MemoriesBackendError::invalid_cursor(cursor, "must contain a non-negative index"))
    }

    fn cursor(&self, index: usize) -> String {
        match &self.generation { Some(id) => format!("g:{id}:{}:{index}", self.notes_observation.as_deref().unwrap_or("unknown")), None => index.to_string() }
    }

    async fn resolve_scoped_path(
        &self,
        relative_path: Option<&str>,
    ) -> Result<PathBuf, MemoriesBackendError> {
        let Some(relative_path) = relative_path else {
            return Ok(self.root.clone());
        };
        let relative = Path::new(relative_path);
        if relative.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        }) {
            return Err(MemoriesBackendError::invalid_path(
                relative_path,
                "must stay within the memories root",
            ));
        }
        if relative.components().any(path::is_hidden_component) {
            return Err(MemoriesBackendError::NotFound {
                path: relative_path.to_string(),
            });
        }

        let components = relative.components().collect::<Vec<_>>();
        let mut scoped_path = self.root.clone();
        for (idx, component) in components.iter().enumerate() {
            scoped_path.push(component.as_os_str());

            let Some(metadata) = Self::metadata_or_none(&scoped_path).await? else {
                for remaining_component in components.iter().skip(idx + 1) {
                    scoped_path.push(remaining_component.as_os_str());
                }
                return Ok(scoped_path);
            };

            path::reject_symlink(
                &path::display_relative_path(&self.root, &scoped_path),
                &metadata,
            )?;
            if idx + 1 < components.len() && !metadata.is_dir() {
                return Err(MemoriesBackendError::invalid_path(
                    relative_path,
                    "traverses through a non-directory path component",
                ));
            }
        }

        Ok(scoped_path)
    }

    async fn metadata_or_none(
        path: &Path,
    ) -> Result<Option<std::fs::Metadata>, MemoriesBackendError> {
        match tokio::fs::symlink_metadata(path).await {
            Ok(metadata) => Ok(Some(metadata)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err.into()),
        }
    }
}

impl MemoriesBackend for LocalMemoriesBackend {
    async fn add_ad_hoc_note(
        &self,
        request: AddAdHocMemoryNoteRequest,
    ) -> Result<AddAdHocMemoryNoteResponse, MemoriesBackendError> {
        ad_hoc_note::add_ad_hoc_note(self, request).await
    }

    async fn list(
        &self,
        request: ListMemoriesRequest,
    ) -> Result<ListMemoriesResponse, MemoriesBackendError> {
        let view = self.view(request.path.as_deref()).await?;
        list::list(&view, request).await
    }

    async fn read(
        &self,
        request: ReadMemoryRequest,
    ) -> Result<ReadMemoryResponse, MemoriesBackendError> {
        let view = self.view(Some(&request.path)).await?;
        read::read(&view, request).await
    }

    async fn search(
        &self,
        request: SearchMemoriesRequest,
    ) -> Result<SearchMemoriesResponse, MemoriesBackendError> {
        let view = self.view(request.path.as_deref()).await?;
        search::search(&view, request).await
    }
}
