use codex_memories_read::publication::MemoryPublication;
use codex_memories_read::publication::MemoryWorkspaceLock;
use codex_memories_read::publication::load_publication;
use codex_memories_read::publication::publication_path;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use crate::backend::AddAdHocMemoryNoteRequest;
use crate::backend::AddAdHocMemoryNoteResponse;
use crate::backend::MemoriesBackendError;

use super::LocalMemoriesBackend;
use super::path::reject_symlink;

const AD_HOC_NOTES_DIR: &[&str] = &["extensions", "ad_hoc", "notes"];
const AD_HOC_NOTE_FILENAME_MAX_BYTES: usize = 128;
const AD_HOC_NOTE_SLUG_MAX_BYTES: usize = 80;
const TIMESTAMP_PREFIX_LEN: usize = "YYYY-MM-DDTHH-MM-SS-".len();

pub(super) async fn add_ad_hoc_note(
    backend: &LocalMemoriesBackend,
    request: AddAdHocMemoryNoteRequest,
) -> Result<AddAdHocMemoryNoteResponse, MemoriesBackendError> {
    validate_filename(&request.filename)?;
    if request.note.trim().is_empty() {
        return Err(MemoriesBackendError::EmptyAdHocNote);
    }
    if request.note.len() > 16 * 1024 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "memory note must be at most 16384 UTF-8 bytes",
        )
        .into());
    }
    let note = codex_secrets::redact_secrets(request.note);

    ensure_directory(&backend.root).await?;
    let lock = MemoryWorkspaceLock::try_acquire(&backend.root)?;
    let previous = load_publication(&backend.root)?;
    let pending = MemoryPublication::next_pending(previous.as_ref())?;
    let notes_dir = ensure_notes_dir(backend).await?;
    let path = notes_dir.join(&request.filename);
    let root = backend.root.clone();
    tokio::task::spawn_blocking(move || -> Result<(), MemoriesBackendError> {
        // The worker owns the lock even if its async caller is cancelled.
        let _lock = lock;
        let mut file = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                return Err(MemoriesBackendError::AdHocNoteAlreadyExists {
                    filename: request.filename,
                });
            }
            Err(err) => return Err(err.into()),
        };
        // Share the native note RPC's generation. Invalidate derived memory
        // before exposing new note content, without blocking the async worker.
        let result = (|| -> std::io::Result<()> {
            let contents = serde_json::to_string(&pending).map_err(std::io::Error::other)?;
            codex_utils_path::write_atomically(&publication_path(&root), &contents)?;
            file.write_all(note.as_bytes())?;
            file.flush()
        })();
        if let Err(error) = result {
            drop(file);
            std::fs::remove_file(&path).map_err(|cleanup| {
                std::io::Error::other(format!(
                    "memory note write failed: {error}; incomplete note cleanup failed: {cleanup}"
                ))
            })?;
            return Err(error.into());
        }
        Ok(())
    })
    .await
    .map_err(std::io::Error::other)??;

    Ok(AddAdHocMemoryNoteResponse {})
}

async fn ensure_notes_dir(
    backend: &LocalMemoriesBackend,
) -> Result<std::path::PathBuf, MemoriesBackendError> {
    ensure_directory(&backend.root).await?;
    let mut path = backend.root.clone();
    for component in AD_HOC_NOTES_DIR {
        path.push(component);
        ensure_directory(&path).await?;
    }
    Ok(path)
}

async fn ensure_directory(path: &Path) -> Result<(), MemoriesBackendError> {
    match LocalMemoriesBackend::metadata_or_none(path).await? {
        Some(metadata) => {
            reject_symlink(&path.display().to_string(), &metadata)?;
            if metadata.is_dir() {
                return Ok(());
            }
            return Err(MemoriesBackendError::invalid_path(
                path.display().to_string(),
                "must be a directory",
            ));
        }
        None => tokio::fs::create_dir(path).await?,
    }

    let Some(metadata) = LocalMemoriesBackend::metadata_or_none(path).await? else {
        return Err(MemoriesBackendError::NotFound {
            path: path.display().to_string(),
        });
    };
    reject_symlink(&path.display().to_string(), &metadata)?;
    if !metadata.is_dir() {
        return Err(MemoriesBackendError::invalid_path(
            path.display().to_string(),
            "must be a directory",
        ));
    }
    Ok(())
}

fn validate_filename(filename: &str) -> Result<(), MemoriesBackendError> {
    if filename.len() > AD_HOC_NOTE_FILENAME_MAX_BYTES {
        return Err(MemoriesBackendError::invalid_filename(
            filename,
            "must be at most 128 bytes",
        ));
    }
    let Some(stem) = filename.strip_suffix(".md") else {
        return Err(MemoriesBackendError::invalid_filename(
            filename,
            "must end with .md",
        ));
    };
    let Some(slug) = stem.get(TIMESTAMP_PREFIX_LEN..) else {
        return Err(MemoriesBackendError::invalid_filename(
            filename,
            "must use YYYY-MM-DDTHH-MM-SS-<slug>.md",
        ));
    };
    if !has_valid_timestamp_prefix(stem) {
        return Err(MemoriesBackendError::invalid_filename(
            filename,
            "must use YYYY-MM-DDTHH-MM-SS-<slug>.md",
        ));
    }
    if slug.is_empty() || slug.len() > AD_HOC_NOTE_SLUG_MAX_BYTES {
        return Err(MemoriesBackendError::invalid_filename(
            filename,
            "slug must be 1 to 80 bytes",
        ));
    }
    if !slug
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(MemoriesBackendError::invalid_filename(
            filename,
            "slug must contain only lowercase ASCII letters, digits, or hyphens",
        ));
    }

    Ok(())
}

fn has_valid_timestamp_prefix(stem: &str) -> bool {
    let bytes = stem.as_bytes();
    bytes.len() > TIMESTAMP_PREFIX_LEN
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b'-'
        && bytes[16] == b'-'
        && bytes[19] == b'-'
        && are_digits(&bytes[0..4])
        && are_digits(&bytes[5..7])
        && are_digits(&bytes[8..10])
        && are_digits(&bytes[11..13])
        && are_digits(&bytes[14..16])
        && are_digits(&bytes[17..19])
}

fn are_digits(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_digit)
}
