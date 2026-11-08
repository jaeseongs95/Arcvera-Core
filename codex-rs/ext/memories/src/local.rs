use codex_memories_read::publication::MemoryWorkspaceLock;
use codex_memories_read::publication::PublicationStatus;
use codex_memories_read::publication::load_publication;
use codex_memories_read::publication::read_published_artifacts;
use codex_protocol::MemoryVersion;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::BTreeMap;
use std::path::Component;
use std::path::Path;
use std::path::PathBuf;

use crate::backend::AddAdHocMemoryNoteRequest;
use crate::backend::AddAdHocMemoryNoteResponse;
use crate::backend::ListMemoriesRequest;
use crate::backend::ListMemoriesResponse;
use crate::backend::MemoriesBackend;
use crate::backend::MemoriesBackendError;
use crate::backend::MemoryEntry;
use crate::backend::MemoryEntryType;
use crate::backend::ReadMemoryRequest;
use crate::backend::ReadMemoryResponse;
use crate::backend::SearchMemoriesRequest;
use crate::backend::SearchMemoriesResponse;

mod ad_hoc_note;
mod list;
mod path;
mod read;
mod search;

#[derive(Debug, Clone)]
pub(crate) struct LocalMemoriesBackend {
    root: PathBuf,
    version: MemoryVersion,
}

struct MemoryReadScope {
    _lock: Option<MemoryWorkspaceLock>,
    // None is specifically manifest-free legacy v1. An empty map is Cleared.
    published: Option<BTreeMap<String, Vec<u8>>>,
}

impl LocalMemoriesBackend {
    #[cfg(test)]
    pub(crate) fn from_memory_root(root: impl Into<PathBuf>) -> Self {
        Self::from_memory_root_and_version(root, MemoryVersion::V1)
    }

    pub(crate) fn from_memory_root_and_version(
        root: impl Into<PathBuf>,
        version: MemoryVersion,
    ) -> Self {
        Self {
            root: root.into(),
            version,
        }
    }

    /// Versioned stores expose only the complete digest-bound final artifact
    /// snapshot. Raw inputs and notes cannot become current evidence merely
    /// by hashing their unchanged bytes. Legacy v1 keeps its existing reader.
    async fn publication_guard(&self) -> Result<MemoryReadScope, MemoriesBackendError> {
        let root = self.root.clone();
        let version = self.version;
        tokio::task::spawn_blocking(move || {
            let unavailable = |reason: String| MemoriesBackendError::MemoryUnavailable { reason };
            if version == MemoryVersion::V1 && !root.exists() && load_publication(&root)
                .map_err(|error| unavailable(format!("invalid_publication_receipt: {:?}", error.kind())))?.is_none() {
                return Ok(MemoryReadScope { _lock: None, published: None });
            }
            let lock = MemoryWorkspaceLock::try_acquire_shared(&root)
                .map_err(|error| unavailable(if error.kind() == std::io::ErrorKind::WouldBlock {
                    "busy: another memory writer owns this namespace".to_string()
                } else { format!("namespace_unreadable: {:?}", error.kind()) }))?;
            let receipt = match load_publication(&root)
                .map_err(|error| unavailable(format!("invalid_publication_receipt: {:?}", error.kind())))? {
                Some(receipt) => receipt,
                None if version == MemoryVersion::V1 => return Ok(MemoryReadScope { _lock: Some(lock), published: None }),
                None => return Err(unavailable("legacy_unverified: no publication receipt; consolidate this namespace before use".to_string())),
            };
            if receipt.status == PublicationStatus::Cleared { return Ok(MemoryReadScope { _lock: Some(lock), published: Some(BTreeMap::new()) }); }
            if receipt.status != PublicationStatus::Published {
                return Err(unavailable(format!("{:?}: accepted generation {} is not published", receipt.status, receipt.accepted_generation)));
            }
            let root = AbsolutePathBuf::from_absolute_path_checked(root)
                .map_err(|_| unavailable("namespace_invalid".to_string()))?;
            codex_core::config::validate_scoped_memory_root(root.as_path())
                .map_err(|error| unavailable(format!("namespace_invalid: {:?}", error.kind())))?;
            let artifacts = read_published_artifacts(root.as_path(), &receipt, version)
                .map_err(|error| unavailable(format!("published_artifact_snapshot_invalid: {:?}", error.kind())))?;
            Ok(MemoryReadScope { _lock: Some(lock), published: Some(artifacts) })
        }).await.map_err(|_| MemoriesBackendError::MemoryUnavailable { reason: "publication_check_unavailable".to_string() })?
    }

    fn snapshot_path(path: Option<&str>) -> Result<String, MemoriesBackendError> {
        let path = path.unwrap_or("");
        let mut parts = Vec::new();
        for component in Path::new(path).components() {
            match component {
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(MemoriesBackendError::invalid_path(
                        path,
                        "must stay within the memories root",
                    ));
                }
                Component::Normal(name) if name.to_string_lossy().starts_with('.') => {
                    return Err(MemoriesBackendError::NotFound {
                        path: path.to_string(),
                    });
                }
                Component::Normal(name) => parts.push(name.to_string_lossy().into_owned()),
                Component::CurDir => {}
            }
        }
        Ok(parts.join("/"))
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
        let guard = self.publication_guard().await?;
        if let Some(artifacts) = &guard.published {
            let scope = Self::snapshot_path(request.path.as_deref())?;
            let start = match request.cursor.as_deref() {
                Some(cursor) => cursor.parse::<usize>().map_err(|_| {
                    MemoriesBackendError::invalid_cursor(cursor, "must be a non-negative integer")
                })?,
                None => 0,
            };
            let mut entries = BTreeMap::new();
            if artifacts.contains_key(&scope) {
                entries.insert(scope.clone(), MemoryEntryType::File);
            } else {
                let prefix = if scope.is_empty() {
                    String::new()
                } else {
                    format!("{scope}/")
                };
                for path in artifacts.keys() {
                    if let Some(relative) = path.strip_prefix(&prefix) {
                        let (child, kind) = match relative.split_once('/') {
                            Some((child, _)) => (child, MemoryEntryType::Directory),
                            None => (relative, MemoryEntryType::File),
                        };
                        entries.insert(format!("{prefix}{child}"), kind);
                    }
                }
                if entries.is_empty() && !scope.is_empty() {
                    return Err(MemoriesBackendError::MemoryUnavailable {
                        reason: "requested path is outside the current published artifact set"
                            .to_string(),
                    });
                }
            }
            let entries = entries
                .into_iter()
                .map(|(path, entry_type)| MemoryEntry { path, entry_type })
                .collect::<Vec<_>>();
            if start > entries.len() {
                return Err(MemoriesBackendError::invalid_cursor(
                    start.to_string(),
                    "exceeds result count",
                ));
            }
            let end = start
                .saturating_add(request.max_results.min(crate::MAX_LIST_RESULTS))
                .min(entries.len());
            let next_cursor = (end < entries.len()).then(|| end.to_string());
            return Ok(ListMemoriesResponse {
                path: request.path,
                entries: entries[start..end].to_vec(),
                truncated: next_cursor.is_some(),
                next_cursor,
            });
        }
        list::list(self, request).await
    }

    async fn read(
        &self,
        request: ReadMemoryRequest,
    ) -> Result<ReadMemoryResponse, MemoriesBackendError> {
        let guard = self.publication_guard().await?;
        if let Some(artifacts) = &guard.published {
            let path = Self::snapshot_path(Some(&request.path))?;
            if request.line_offset == 0 {
                return Err(MemoriesBackendError::InvalidLineOffset);
            }
            if request.max_lines == Some(0) {
                return Err(MemoriesBackendError::InvalidMaxLines);
            }
            let bytes =
                artifacts
                    .get(&path)
                    .ok_or_else(|| MemoriesBackendError::MemoryUnavailable {
                        reason: "requested file is outside the current published artifact set"
                            .to_string(),
                    })?;
            let content = std::str::from_utf8(bytes).map_err(|error| {
                MemoriesBackendError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    error,
                ))
            })?;
            return read::read_content(request, content);
        }
        read::read(self, request).await
    }

    async fn search(
        &self,
        mut request: SearchMemoriesRequest,
    ) -> Result<SearchMemoriesResponse, MemoriesBackendError> {
        let guard = self.publication_guard().await?;
        if let Some(artifacts) = &guard.published {
            let original_path = request.path.clone();
            let scope = Self::snapshot_path(request.path.as_deref())?;
            if !scope.is_empty()
                && !artifacts
                    .keys()
                    .any(|path| path == &scope || path.starts_with(&format!("{scope}/")))
            {
                return Err(MemoriesBackendError::MemoryUnavailable {
                    reason: "requested scope is outside the current published artifact set"
                        .to_string(),
                });
            }
            request.path = Some(scope);
            let mut response = search::search_snapshot(&self.root, artifacts, request)?;
            response.path = original_path;
            return Ok(response);
        }
        search::search(self, request).await
    }
}
