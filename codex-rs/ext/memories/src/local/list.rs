// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use crate::MAX_LIST_RESULTS;
use crate::backend::ListMemoriesRequest;
use crate::backend::ListMemoriesResponse;
use crate::backend::MemoriesBackendError;
use crate::backend::MemoryEntry;
use crate::backend::MemoryEntryType;

use super::LocalMemoriesBackend;
use super::path::display_relative_path;
use super::path::is_hidden_path;
use super::path::read_sorted_dir_entries;
use super::path::reject_symlink;

pub(super) async fn list(
    backend: &LocalMemoriesBackend,
    request: ListMemoriesRequest,
) -> Result<ListMemoriesResponse, MemoriesBackendError> {
    let max_results = request.max_results.min(MAX_LIST_RESULTS);
    let start = backend.resolve_scoped_path(request.path.as_deref()).await?;
    let start_index = backend.cursor_offset(request.cursor.as_deref())?;
    let Some(metadata) = LocalMemoriesBackend::metadata_or_none(&start).await? else {
        return Err(MemoriesBackendError::NotFound {
            path: request.path.unwrap_or_default(),
        });
    };
    reject_symlink(&display_relative_path(&backend.root, &start), &metadata)?;

    if metadata.is_dir() && let Some(manifest) = &backend.artifact_manifest {
        let relative = start.strip_prefix(&backend.root).map_err(std::io::Error::other)?;
        let prefix = relative.to_string_lossy().replace('\\', "/");
        let prefix = if prefix.is_empty() { String::new() } else { format!("{prefix}/") };
        for name in manifest.keys() {
            let Some(child) = name.strip_prefix(&prefix).and_then(|tail| tail.split('/').next()) else { continue; };
            if child.starts_with('.') { continue; }
            if LocalMemoriesBackend::metadata_or_none(&start.join(child)).await?.is_none() {
                return Err(std::io::Error::other("published memory artifact is missing").into());
            }
        }
    }

    let mut entries = if metadata.is_file() {
        backend.read_artifact_bytes(&start).await?;
        vec![MemoryEntry {
            path: display_relative_path(&backend.root, &start),
            entry_type: MemoryEntryType::File,
        }]
    } else if metadata.is_dir() {
        let mut entries = Vec::new();
        for (path, file_type) in read_sorted_dir_entries(&start).await? {
            if is_hidden_path(&path) {
                continue;
            }
            let Some(metadata) = LocalMemoriesBackend::metadata_or_none(&path).await? else { continue; };
            reject_symlink(&display_relative_path(&backend.root, &path), &metadata)?;

            if file_type.is_file() { backend.read_artifact_bytes(&path).await?; }
            let entry_type = if file_type.is_dir() {
                MemoryEntryType::Directory
            } else if file_type.is_file() {
                MemoryEntryType::File
            } else {
                continue;
            };
            entries.push(MemoryEntry {
                path: display_relative_path(&backend.root, &path),
                entry_type,
            });
        }
        entries
    } else {
        Vec::new()
    };
    if request.path.is_none() && backend.root != backend.original_root {
        let extension_root = backend.original_root.join("extensions");
        if let Some(metadata) = LocalMemoriesBackend::metadata_or_none(&extension_root).await? {
            reject_symlink("extensions", &metadata)?;
            if metadata.is_dir() && !entries.iter().any(|entry| entry.path == "extensions") {
                entries.push(MemoryEntry { path: "extensions".into(), entry_type: MemoryEntryType::Directory });
                entries.sort_by(|left, right| left.path.cmp(&right.path));
            }
        }
    }
    if start_index > entries.len() {
        return Err(MemoriesBackendError::invalid_cursor(
            start_index.to_string(),
            "exceeds result count",
        ));
    }

    let end_index = start_index.saturating_add(max_results).min(entries.len());
    let next_cursor = (end_index < entries.len()).then(|| backend.cursor(end_index));
    let truncated = next_cursor.is_some();
    Ok(ListMemoriesResponse {
        path: request.path,
        entries: entries.drain(start_index..end_index).collect(),
        next_cursor,
        truncated,
    })
}
