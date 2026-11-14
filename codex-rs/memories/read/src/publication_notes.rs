// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Bounded note fingerprints and link-safe file access.
use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const MAX_NOTES: usize = 10_000;
const MAX_BYTES: u64 = 32 * 1024 * 1024;

pub fn open_regular_without_links(path: &Path) -> io::Result<std::fs::File> {
    open_regular(path, false, false)
}

pub fn open_regular_for_update_without_links(path: &Path, create_new: bool) -> io::Result<std::fs::File> {
    open_regular(path, true, create_new)
}

fn open_regular(path: &Path, write: bool, create_new: bool) -> io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    if write { options.write(true); }
    if create_new { options.create_new(true); }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() { return Err(io::Error::other("memory input is not a regular file")); }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if file.metadata()?.nlink() != 1 { return Err(io::Error::other("hard-linked memory input")); }
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle, FILE_ATTRIBUTE_REPARSE_POINT};
        // SAFETY: info is a plain output structure and file remains open for the call.
        let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: the open file handle and writable output pointer remain valid.
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 { return Err(io::Error::last_os_error()); }
        if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || info.nNumberOfLinks != 1 {
            return Err(io::Error::other("linked memory input"));
        }
    }
    #[cfg(not(any(unix, windows)))]
    return Err(io::Error::new(io::ErrorKind::Unsupported, "memory file link validation is unsupported"));
    #[cfg(any(unix, windows))]
    Ok(file)
}


fn metadata(path: &Path) -> io::Result<std::fs::Metadata> {
    let metadata = std::fs::symlink_metadata(path)?;
    let redirected = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let redirected = {
        use std::os::windows::fs::MetadataExt;
        redirected || metadata.file_attributes() & 0x400 != 0
    };
    if redirected { return Err(io::Error::other("redirected memory note path")); }
    Ok(metadata)
}

/// Deterministic bounded fingerprint of note filenames and bytes. A missing
/// notes directory is an empty set; invalid entries and I/O failures fail closed.
/// Caller must serialize legitimate note writes using the root workspace lock.
pub fn notes_digest(root: &Path) -> io::Result<String> {
    let mut dir = PathBuf::from(root);
    let mut hash = Sha256::new();
    for part in ["extensions", "ad_hoc", "notes"] {
        dir.push(part);
        match metadata(&dir) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(format!("{:x}", hash.finalize())),
            Err(error) => return Err(error),
            Ok(meta) if meta.is_dir() => {},
            Ok(_) => return Err(io::Error::other("memory note ancestor is not a directory")),
        }
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        if entries.len() >= MAX_NOTES { return Err(io::Error::other("memory note count exceeded")); }
        let entry = entry?;
        let name = entry.file_name().into_string().map_err(|_| io::Error::other("memory note name is not UTF-8"))?;
        entries.push((name, entry.path()));
    }
    entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let mut used = 0_u64;
    for (name, path) in entries {
        let meta = metadata(&path)?;
        if !meta.is_file() { return Err(io::Error::other("memory note is not a file")); }
        let remaining = MAX_BYTES.checked_sub(used).ok_or_else(|| io::Error::other("memory note bytes exceeded"))?;
        if meta.len() > remaining { return Err(io::Error::other("memory note bytes exceeded")); }
        let mut bytes = Vec::new();
        open_regular_without_links(&path)?.take(remaining + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > remaining { return Err(io::Error::other("memory note grew beyond budget")); }
        used += bytes.len() as u64;
        hash.update((name.len() as u64).to_be_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(&bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// A busy writer must not prevent reading the old generation, but its note
/// currentness cannot be asserted until the writer lock can be shared.
pub fn try_current_notes_digest(root: &Path) -> io::Result<Option<String>> {
    use std::fs::TryLockError;
    let lock_path = root.with_extension("lock");
    if !metadata(&lock_path)?.is_file() { return Err(io::Error::other("invalid memory workspace lock")); }
    let lock = std::fs::File::open(lock_path)?;
    match lock.try_lock_shared() {
        Ok(()) => Ok(Some(notes_digest(root)?)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_set_detects_content_and_new_note_and_is_order_stable() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let empty = notes_digest(root)?;
        let dir = root.join("extensions/ad_hoc/notes");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("b.md"), b"b")?;
        std::fs::write(dir.join("a.md"), b"a")?;
        let initial = notes_digest(root)?;
        assert_ne!(empty, initial);
        assert_eq!(initial, notes_digest(root)?);
        std::fs::write(dir.join("a.md"), b"different")?;
        assert_ne!(initial, notes_digest(root)?);
        Ok(())
    }

    #[test]
    fn busy_writer_yields_unknown_note_currentness() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path();
        let lock_path = root.with_extension("lock");
        let lock = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(lock_path)?;
        lock.try_lock().map_err(io::Error::other)?;
        assert_eq!(try_current_notes_digest(root)?, None);
        drop(lock);
        assert!(try_current_notes_digest(root)?.is_some());
        Ok(())
    }
}
