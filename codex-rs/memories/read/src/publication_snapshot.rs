// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Capture one host-bound memory publication for each read operation.
use std::future::Future;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

/// A single captured namespace for one read operation. The original root holds
/// notes and immutable source inputs; derived_root is either the one DB-selected
/// immutable generation or the original root for an unmanaged legacy namespace.
#[derive(Clone, Debug)]
pub struct PublishedMemorySnapshot {
    pub original_root: PathBuf,
    pub derived_root: PathBuf,
    pub generation_id: Option<String>,
    pub artifact_manifest: Option<std::collections::BTreeMap<String, String>>,
    pub provenance: Option<String>,
    pub legacy_empty: bool,
    /// None for legacy; false means notes changed after this generation.
    pub current_for_notes: Option<bool>,
    pub notes_observation: Option<String>,
}

/// Scope and root are bound by the host; consumers cannot ask for another root.
pub trait PublishedMemorySnapshotProvider: Send + Sync {
    fn capture(&self) -> Pin<Box<dyn Future<Output = std::io::Result<PublishedMemorySnapshot>> + Send + '_>>;
}

/// Stored by the host in the thread-scoped extension data.
#[derive(Clone)]
pub struct MemorySnapshotCapability(pub Arc<dyn PublishedMemorySnapshotProvider>);

/// This lookup is installed by the host already bound to one scope/version/root.
pub trait ScopedPublicationLookup: Send + Sync {
    fn generation(&self) -> Pin<Box<dyn Future<Output = io::Result<Option<PublicationRecord>>> + Send + '_>>;
}

#[derive(Clone, Debug)]
pub struct PublicationRecord { pub artifact_manifest_digest: String, pub generation_id: String, pub notes_digest: String, pub provenance: String, pub legacy_empty: bool }

/// The caller supplies the already validated, pinned absolute root. The same
/// spelling must be used by writer and reader; alias equivalence is not inferred.
pub fn root_identity(root: &Path) -> io::Result<String> {
    use sha2::{Digest, Sha256};
    if !root.is_absolute() { return Err(io::Error::new(io::ErrorKind::InvalidInput, "memory root must be absolute")); }
    let mut hash = Sha256::new();
    hash.update(b"codex-memory-root-v1\0");
    hash.update(root.as_os_str().as_encoded_bytes());
    Ok(format!("{:x}", hash.finalize()))
}

pub struct ScopedSnapshotProvider {
    root: PathBuf,
    lookup: Option<Arc<dyn ScopedPublicationLookup>>,
}

impl ScopedSnapshotProvider {
    pub fn new(root: PathBuf, lookup: Option<Arc<dyn ScopedPublicationLookup>>) -> Self {
        Self { root, lookup }
    }
}

fn checked_directory(path: &Path) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    let redirected = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        redirected || metadata.file_attributes() & 0x400 != 0
    };
    if redirected || !metadata.is_dir() {
        return Err(io::Error::other("memory generation path is not a regular directory"));
    }
    Ok(())
}

pub fn generation_root(root: &Path, id: &str) -> io::Result<PathBuf> {
    let uuid = uuid::Uuid::parse_str(id).map_err(io::Error::other)?;
    if uuid.to_string() != id { return Err(io::Error::other("invalid memory generation id")); }
    checked_directory(root)?;
    let generations = root.join(".codex-memory-generations");
    checked_directory(&generations)?;
    let dir = generations.join(id);
    checked_directory(&dir)?;
    use std::io::Read;
    let mut marker = Vec::new();
    crate::publication_notes::open_regular_without_links(&dir.join(".codex-memory-generation"))?
        .take(256).read_to_end(&mut marker)?;
    if marker != format!("{}\n{id}\n", root_identity(root)?).as_bytes() {
        return Err(io::Error::other("invalid memory generation marker"));
    }
    Ok(dir)
}

/// Host-generated digest inventory. It is bound to the publication row, and
/// each returned file is checked using the same handle used to read its bytes.
pub const ARTIFACT_MANIFEST: &str = ".codex-memory-artifacts";
const MAX_ARTIFACT_BYTES: u64 = 32 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;

fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}
fn encoded_path(path: &Path) -> io::Result<String> {
    let text = path.to_str().ok_or_else(|| io::Error::other("non-UTF8 artifact path"))?.replace('\\', "/");
    if text.is_empty() || text.contains('\n') || text.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Err(io::Error::other("invalid artifact path"));
    }
    Ok(text)
}
fn read_bounded(path: &Path, max: u64) -> io::Result<Vec<u8>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    crate::publication_notes::open_regular_without_links(path)?.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max { return Err(io::Error::other("memory artifact exceeds limit")); }
    Ok(bytes)
}
/// Called by the host after copying validated output and before publishing it.
pub fn make_artifact_manifest(root: &Path) -> io::Result<Vec<u8>> {
    let mut pending = vec![root.to_path_buf()];
    let mut entries = std::collections::BTreeMap::new();
    let mut total = 0_u64;
    while let Some(dir) = pending.pop() {
        checked_directory(&dir)?;
        for entry in std::fs::read_dir(&dir)? {
            let path = entry?.path();
            let relative = path.strip_prefix(root).map_err(io::Error::other)?;
            if relative == Path::new(".codex-memory-generation") || relative == Path::new(ARTIFACT_MANIFEST) { continue; }
            let metadata = std::fs::symlink_metadata(&path)?;
            let redirected = metadata.file_type().is_symlink();
            #[cfg(windows)]
            let redirected = { use std::os::windows::fs::MetadataExt; redirected || metadata.file_attributes() & 0x400 != 0 };
            if redirected { return Err(io::Error::other("redirected memory artifact")); }
            if metadata.is_dir() { pending.push(path); continue; }
            if !metadata.is_file() { return Err(io::Error::other("invalid memory artifact")); }
            let bytes = read_bounded(&path, MAX_ARTIFACT_BYTES)?;
            total = total.checked_add(bytes.len() as u64).ok_or_else(|| io::Error::other("memory artifact size overflow"))?;
            if total > MAX_ARTIFACT_BYTES || entries.len() >= 10_000 { return Err(io::Error::other("memory artifact budget exceeded")); }
            entries.insert(encoded_path(relative)?, digest(&bytes));
        }
    }
    let mut result = Vec::new();
    for (name, hash) in entries { result.extend_from_slice(format!("{hash} {name}\n").as_bytes()); }
    if result.len() as u64 > MAX_MANIFEST_BYTES { return Err(io::Error::other("memory artifact manifest exceeds limit")); }
    Ok(result)
}
pub fn artifact_manifest_digest(bytes: &[u8]) -> String { digest(bytes) }

/// Verify the host manifest against the DB publication record.
pub fn load_artifact_manifest(root: &Path, expected: &str) -> io::Result<std::collections::BTreeMap<String, String>> {
    let bytes = read_bounded(&root.join(ARTIFACT_MANIFEST), MAX_MANIFEST_BYTES)?;
    if digest(&bytes) != expected { return Err(io::Error::other("memory artifact manifest digest mismatch")); }
    let text = std::str::from_utf8(&bytes).map_err(io::Error::other)?;
    let mut entries = std::collections::BTreeMap::new();
    for line in text.lines() {
        let (hash, name) = line.split_once(' ').ok_or_else(|| io::Error::other("invalid artifact manifest"))?;
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || encoded_path(Path::new(name))? != name || entries.insert(name.to_owned(), hash.to_owned()).is_some() {
            return Err(io::Error::other("invalid artifact manifest entry"));
        }
    }
    Ok(entries)
}
pub fn read_verified_artifact(root: &Path, path: &Path, manifest: &std::collections::BTreeMap<String, String>, max: u64) -> io::Result<Vec<u8>> {
    let relative = path.strip_prefix(root).map_err(io::Error::other)?;
    let name = encoded_path(relative)?;
    let expected = match manifest.get(&name) {
        Some(expected) => expected,
        None => {
            if let Err(error) = std::fs::symlink_metadata(path) {
                if error.kind() == io::ErrorKind::NotFound { return Err(error); }
            }
            return Err(io::Error::other("unlisted memory artifact"));
        }
    };
    let bytes = read_bounded(path, max.min(MAX_ARTIFACT_BYTES))?;
    if digest(&bytes) != *expected { return Err(io::Error::other("memory artifact digest mismatch")); }
    Ok(bytes)
}

pub fn managed_marker(root: &Path) -> io::Result<bool> {
    use std::io::Read;
    let path = root.join(".codex-memory-managed");
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
        Ok(metadata) => {
            let redirected = metadata.file_type().is_symlink();
            #[cfg(windows)]
            let redirected = {
                use std::os::windows::fs::MetadataExt;
                redirected || metadata.file_attributes() & 0x400 != 0
            };
            if redirected || !metadata.is_file() || metadata.len() != b"managed-memory-v1\n".len() as u64 {
                return Err(io::Error::other("invalid managed memory marker"));
            }
            let mut bytes = Vec::new();
            crate::publication_notes::open_regular_without_links(&path)?.take(64).read_to_end(&mut bytes)?;
            if bytes != b"managed-memory-v1\n" { return Err(io::Error::other("invalid managed memory marker")); }
            Ok(true)
        }
    }
}

impl PublishedMemorySnapshotProvider for ScopedSnapshotProvider {
    fn capture(&self) -> Pin<Box<dyn Future<Output = io::Result<PublishedMemorySnapshot>> + Send + '_>> {
        Box::pin(async move {
            match checked_directory(&self.root) {
                Ok(()) => {},
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if let Some(lookup) = &self.lookup
                        && matches!(lookup.generation().await, Ok(Some(_)))
                    {
                        return Err(io::Error::other("memory publication root is missing"));
                    }
                    return Ok(PublishedMemorySnapshot {
                        original_root: self.root.clone(), derived_root: self.root.clone(),
                        generation_id: None, artifact_manifest: None, provenance: None, legacy_empty: true, current_for_notes: None, notes_observation: None,
                    });
                }
                Err(error) => return Err(error),
            }
            let managed = managed_marker(&self.root)?;
            let record = match &self.lookup {
                Some(lookup) => match lookup.generation().await {
                    Ok(record) => record,
                    Err(_) if !managed => None,
                    Err(error) => return Err(error),
                },
                None if managed => return Err(io::Error::other("managed memory state is unavailable")),
                None => None,
            };
            if managed && record.is_none() {
                return Err(io::Error::other("managed memory has no current publication"));
            }
            if !managed && record.as_ref().is_some_and(|r| r.provenance != "legacy_import") {
                return Err(io::Error::other("non-legacy memory publication is missing its managed marker"));
            }
            // Before the marker commits, the registered baseline is an equal
            // snapshot of the still-untouched legacy root. Do not present its
            // provenance or note digest as a new managed publication yet.
            let record = if managed { record } else { None };
            let generation = record.as_ref().map(|record| record.generation_id.clone());
            let provenance = record.as_ref().map(|record| record.provenance.clone());
            let legacy_empty = record.as_ref().is_some_and(|record| record.legacy_empty);
            let derived_root = if let Some(id) = generation.as_deref() {
                generation_root(&self.root, id)?
            } else {
                self.root.clone()
            };
            if legacy_empty {
                for name in ["memory_summary.md", "MEMORY.md", "skills"] {
                    match std::fs::symlink_metadata(derived_root.join(name)) {
                        Err(error) if error.kind() == io::ErrorKind::NotFound => {},
                        Err(error) => return Err(error),
                        Ok(_) => return Err(io::Error::other("empty legacy baseline contains generated artifacts")),
                    }
                }
            }
            let artifact_manifest = record.as_ref().map(|record| load_artifact_manifest(&derived_root, &record.artifact_manifest_digest)).transpose()?;
            let notes_observation = match record.as_ref() {
                Some(_) => {
                    let root = self.root.clone();
                    tokio::task::spawn_blocking(move || crate::publication_notes::try_current_notes_digest(&root))
                        .await.map_err(io::Error::other)??
                },
                None => None,
            };
            let current_for_notes = match (&record, &notes_observation) {
                (Some(record), Some(digest)) if record.provenance != "legacy_import" => Some(digest.as_str() == record.notes_digest.as_str()),
                _ => None,
            };
            Ok(PublishedMemorySnapshot { original_root: self.root.clone(), derived_root, generation_id: generation, artifact_manifest, provenance, legacy_empty, current_for_notes, notes_observation })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_generation(root: &Path, id: &str) -> io::Result<PathBuf> {
        let generation = root.join(".codex-memory-generations").join(id);
        std::fs::create_dir_all(&generation)?;
        std::fs::write(generation.join(".codex-memory-generation"), format!("{}\n{id}\n", root_identity(root)?))?;
        let manifest = make_artifact_manifest(&generation)?;
        std::fs::write(generation.join(ARTIFACT_MANIFEST), manifest)?;
        Ok(generation)
    }

    struct Lookup(io::Result<Option<PublicationRecord>>);

    #[test]
    fn artifact_inventory_detects_modified_manifest_and_file() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        std::fs::write(root.join("memory_summary.md"), b"verified")?;
        let bytes = make_artifact_manifest(root)?;
        let expected = artifact_manifest_digest(&bytes);
        std::fs::write(root.join(ARTIFACT_MANIFEST), &bytes)?;
        let manifest = load_artifact_manifest(root, &expected)?;
        assert_eq!(read_verified_artifact(root, &root.join("memory_summary.md"), &manifest, 100)?, b"verified");
        std::fs::write(root.join("memory_summary.md"), b"modified")?;
        assert!(read_verified_artifact(root, &root.join("memory_summary.md"), &manifest, 100).is_err());
        std::fs::write(root.join(ARTIFACT_MANIFEST), b"modified")?;
        assert!(load_artifact_manifest(root, &expected).is_err());
        Ok(())
    }

    #[test]
    fn generation_marker_must_match_root_and_id() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let id = uuid::Uuid::new_v4().to_string();
        let generation = create_generation(temp.path(), &id)?;
        assert_eq!(generation_root(temp.path(), &id)?, generation);
        std::fs::write(generation.join(".codex-memory-generation"), b"wrong")?;
        assert!(generation_root(temp.path(), &id).is_err());
        Ok(())
    }
    impl ScopedPublicationLookup for Lookup {
        fn generation(&self) -> Pin<Box<dyn Future<Output = io::Result<Option<PublicationRecord>>> + Send + '_>> {
            Box::pin(async move {
                match &self.0 {
                    Ok(value) => Ok(value.clone()),
                    Err(error) => Err(io::Error::other(error.to_string())),
                }
            })
        }
    }

    #[tokio::test]
    async fn legacy_only_without_marker_managed_missing_or_db_error_fails_closed() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path().to_path_buf();
        let legacy = ScopedSnapshotProvider::new(root.clone(), None).capture().await?;
        assert_eq!(legacy.derived_root, root);
        std::fs::write(root.join(".codex-memory-managed"), b"managed-memory-v1\n")?;
        assert!(ScopedSnapshotProvider::new(root.clone(), None).capture().await.is_err());
        assert!(ScopedSnapshotProvider::new(root.clone(), Some(Arc::new(Lookup(Ok(None))))).capture().await.is_err());
        assert!(ScopedSnapshotProvider::new(root, Some(Arc::new(Lookup(Err(io::Error::other("db failed")))))).capture().await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn first_transition_keeps_legacy_before_marker_and_reports_import_after() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path().to_path_buf();
        std::fs::write(root.join("memory_summary.md"), b"legacy")?;
        let baseline = uuid::Uuid::new_v4().to_string();
        let generation = create_generation(&root, &baseline)?;
        std::fs::write(generation.join("memory_summary.md"), b"legacy")?;
        std::fs::write(generation.join(ARTIFACT_MANIFEST), make_artifact_manifest(&generation)?)?;
        std::fs::write(root.with_extension("lock"), b"")?;
        let record = PublicationRecord { artifact_manifest_digest: artifact_manifest_digest(&std::fs::read(generation.join(ARTIFACT_MANIFEST))?), generation_id: baseline, notes_digest: crate::publication_notes::notes_digest(&root)?, provenance: "legacy_import".into(), legacy_empty: false };
        let before = ScopedSnapshotProvider::new(root.clone(), Some(Arc::new(Lookup(Ok(Some(record.clone())))))).capture().await?;
        assert_eq!(std::fs::read(before.derived_root.join("memory_summary.md"))?, b"legacy");
        let unavailable_db = ScopedSnapshotProvider::new(root.clone(), Some(Arc::new(Lookup(Err(io::Error::other("db")))))).capture().await?;
        assert_eq!(unavailable_db.derived_root, root);
        std::fs::write(root.join(".codex-memory-managed"), b"managed-memory-v1\n")?;
        let after = ScopedSnapshotProvider::new(root.clone(), Some(Arc::new(Lookup(Ok(Some(record)))))).capture().await?;
        assert_eq!(after.provenance.as_deref(), Some("legacy_import"));
        assert_eq!(std::fs::read(after.derived_root.join("memory_summary.md"))?, b"legacy");
        assert!(ScopedSnapshotProvider::new(root, Some(Arc::new(Lookup(Err(io::Error::other("db")))))).capture().await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn registered_empty_legacy_is_distinct_from_missing_database_record() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path().to_path_buf();
        std::fs::write(root.join(".codex-memory-managed"), b"managed-memory-v1\n")?;
        std::fs::write(root.with_extension("lock"), b"")?;
        let id = uuid::Uuid::new_v4().to_string();
        create_generation(&root, &id)?;
        let record = PublicationRecord { artifact_manifest_digest: artifact_manifest_digest(&std::fs::read(root.join(".codex-memory-generations").join(&id).join(ARTIFACT_MANIFEST))?), generation_id: id, notes_digest: crate::publication_notes::notes_digest(&root)?, provenance: "legacy_import".into(), legacy_empty: true };
        let snapshot = ScopedSnapshotProvider::new(root.clone(), Some(Arc::new(Lookup(Ok(Some(record)))))).capture().await?;
        assert!(snapshot.legacy_empty);
        assert!(!snapshot.derived_root.join("memory_summary.md").exists());
        assert!(ScopedSnapshotProvider::new(root, Some(Arc::new(Lookup(Ok(None))))).capture().await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn capture_keeps_one_generation_after_publication_advances() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let root = tmp.path().to_path_buf();
        std::fs::write(root.join(".codex-memory-managed"), b"managed-memory-v1\n")?;
        std::fs::write(root.with_extension("lock"), b"")?;
        let old = uuid::Uuid::new_v4().to_string();
        let new = uuid::Uuid::new_v4().to_string();
        for id in [&old, &new] { create_generation(&root, id)?; }
        let digest = crate::publication_notes::notes_digest(&root)?;
        let old_record = PublicationRecord { artifact_manifest_digest: artifact_manifest_digest(&std::fs::read(root.join(".codex-memory-generations").join(&old).join(ARTIFACT_MANIFEST))?), generation_id: old.clone(), notes_digest: digest.clone(), provenance: "consolidated".into(), legacy_empty: false };
        let captured = ScopedSnapshotProvider::new(root.clone(), Some(Arc::new(Lookup(Ok(Some(old_record)))))).capture().await?;
        assert_eq!(captured.current_for_notes, Some(true));
        std::fs::create_dir_all(root.join("extensions/ad_hoc/notes"))?;
        std::fs::write(root.join("extensions/ad_hoc/notes/new.md"), b"new note")?;
        let new_record = PublicationRecord { artifact_manifest_digest: artifact_manifest_digest(&std::fs::read(root.join(".codex-memory-generations").join(&new).join(ARTIFACT_MANIFEST))?), generation_id: new.clone(), notes_digest: digest, provenance: "consolidated".into(), legacy_empty: false };
        let next = ScopedSnapshotProvider::new(root.clone(), Some(Arc::new(Lookup(Ok(Some(new_record)))))).capture().await?;
        assert_eq!(next.current_for_notes, Some(false));
        assert_eq!(captured.derived_root, root.join(".codex-memory-generations").join(old));
        assert_eq!(next.derived_root, root.join(".codex-memory-generations").join(new));
        Ok(())
    }
}
