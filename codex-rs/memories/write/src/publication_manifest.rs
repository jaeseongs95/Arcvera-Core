// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Host-owned fingerprints of selected consolidation inputs.
use codex_state::Stage1Output;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const MAX_INPUTS: usize = 10_000;
const MAX_SELECTED: usize = 30_000;
const MAX_SELECTED_BYTES: u64 = 32 * 1024 * 1024;

/// Only fingerprints host-selected inputs. It never serializes or stores their
/// original bodies, and duplicate IDs are an error, not a last-wins update.
pub fn selected_manifest(outputs: &[Stage1Output]) -> io::Result<BTreeMap<String, String>> {
    if outputs.len() > MAX_INPUTS { return Err(io::Error::other("memory input count exceeded")); }
    let mut used = 0_u64;
    let mut manifest = BTreeMap::new();
    for output in outputs {
        let mut hash = Sha256::new();
        let id = output.thread_id.to_string();
        let source_time = output.source_updated_at.timestamp().to_be_bytes();
        let generated_time = output.generated_at.timestamp().to_be_bytes();
        let fields = [
            id.as_bytes(), source_time.as_slice(), generated_time.as_slice(),
            output.raw_memory.as_bytes(), output.rollout_summary.as_bytes(),
            output.rollout_path.as_os_str().as_encoded_bytes(), output.cwd.as_os_str().as_encoded_bytes(),
        ];
        for field in fields {
            used = used.checked_add(field.len() as u64).ok_or_else(|| io::Error::other("memory input size overflow"))?;
            if used > MAX_SELECTED_BYTES { return Err(io::Error::other("memory input bytes exceeded")); }
            hash.update((field.len() as u64).to_be_bytes());
            hash.update(field);
        }
        // Preserve the semantic distinction between absent and empty metadata.
        for field in [&output.rollout_slug, &output.git_branch] {
            match field {
                None => hash.update([0]),
                Some(value) => {
                    used = used.checked_add(value.len() as u64).ok_or_else(|| io::Error::other("memory input size overflow"))?;
                    if used > MAX_SELECTED_BYTES { return Err(io::Error::other("memory input bytes exceeded")); }
                    hash.update([1]);
                    hash.update((value.len() as u64).to_be_bytes());
                    hash.update(value.as_bytes());
                }
            }
        }
        let digest = format!("{:x}", hash.finalize());
        let filename = format!("{}.md", crate::storage::rollout_summary_file_stem(output));
        if manifest.insert(id.clone(), digest.clone()).is_some() {
            return Err(io::Error::other("duplicate selected memory input"));
        }
        manifest.insert(format!("filename:{id}:{filename}"), digest);
    }
    Ok(manifest)
}

/// Add selected extension resource file digests without storing their bodies.
pub fn include_extension_resources(manifest: &mut BTreeMap<String, String>, inputs: &BTreeMap<PathBuf, String>) -> io::Result<()> {
    for (path, digest) in inputs {
        let components = path.components().collect::<Vec<_>>();
        if components.len() < 4 || components[0].as_os_str() != "extensions" || components[2].as_os_str() != "resources" { continue; }
        if components.iter().any(|component| !matches!(component, Component::Normal(_))) { return Err(io::Error::other("invalid extension resource path")); }
        let name = path.to_str().ok_or_else(|| io::Error::other("extension resource path is not UTF-8"))?.replace('\\', "/");
        if name.len() > 1024 || manifest.len() >= MAX_SELECTED { return Err(io::Error::other("memory manifest entry limit exceeded")); }
        if manifest.insert(format!("resource:{name}"), digest.clone()).is_some() { return Err(io::Error::other("duplicate extension resource path")); }
    }
    Ok(())
}

/// Host-generated change classification carries IDs, not retired raw bodies.
pub fn changed_inputs(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>) -> (Vec<String>, Vec<String>, Vec<String>) {
    let added = new.keys().filter(|id| !id.starts_with("filename:") && !old.contains_key(*id)).cloned().collect();
    let changed = new.iter().filter(|(id, digest)| !id.starts_with("filename:") && old.get(*id).is_some_and(|previous| previous != *digest)).map(|(id, _)| id.clone()).collect();
    let removed = old.keys().filter(|id| !id.starts_with("filename:") && !new.contains_key(*id)).cloned().collect();
    (added, changed, removed)
}

/// Retain only a host-recorded safe filename, never a retired source body.
pub fn removed_filenames(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>, removed: &[String]) -> Vec<String> {
    removed.iter().filter(|id| !id.starts_with("resource:")).map(|id| {
        let prefix = format!("filename:{id}:");
        let name = old.keys().find_map(|key| key.strip_prefix(&prefix));
        format!("{id}: {}", name.map(|name| format!("rollout_summaries/{name}")).unwrap_or_else(|| "unknown".into()))
    }).chain(old.keys().filter(|key| key.starts_with("filename:") && !new.contains_key(*key))
        .filter_map(|key| key.strip_prefix("filename:").and_then(|rest| rest.split_once(':')))
        .filter(|(id, _)| !removed.iter().any(|removed_id| removed_id == id))
        .map(|(id, name)| format!("{id}: rollout_summaries/{name}"))).collect()
}

/// Reject a pointer that does not refer to a rollout supplied in this stage.
pub fn validate_summary_pointers(summary: &str, selected: &BTreeMap<String, String>) -> io::Result<()> {
    let allowed: std::collections::HashSet<&str> = selected.keys().filter_map(|key|
        key.strip_prefix("filename:").and_then(|rest| rest.split_once(':')).map(|(_, name)| name)).collect();
    for (index, _) in summary.match_indices("rollout_summaries/") {
        let tail = &summary[index + "rollout_summaries/".len()..];
        let end = tail.find(|ch: char| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.'))).unwrap_or(tail.len());
        let candidate = tail[..end].trim_end_matches('.');
        if !candidate.is_empty() && (!candidate.ends_with(".md") || !allowed.contains(candidate)) {
            return Err(io::Error::other("memory summary references an unsupplied rollout"));
        }
    }
    Ok(())
}

/// Create the immutable generation's host-owned manifest before publishing.
/// Caller retains and compares the returned bytes to the files being consumed.
pub fn write_manifest(generation: &Path, manifest: &BTreeMap<String, String>) -> io::Result<()> {
    let bytes = serde_json::to_vec(manifest).map_err(io::Error::other)?;
    if bytes.len() > 2 * 1024 * 1024 { return Err(io::Error::other("memory manifest bytes exceeded")); }
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true)
        .open(generation.join(".codex-memory-input-manifest.json"))?;
    file.write_all(&bytes)?;
    file.sync_all()
}

/// A missing manifest is not an empty consolidated input set. The caller must
/// explicitly handle the legacy-import case and never silently skip work.
pub fn read_manifest(generation: &Path, artifacts: &BTreeMap<String, String>) -> io::Result<BTreeMap<String, String>> {
    let path = generation.join(".codex-memory-input-manifest.json");
    let bytes = codex_memories_read::publication_snapshot::read_verified_artifact(
        generation, &path, artifacts, 2 * 1024 * 1024,
    )?;
    struct UniqueManifest(BTreeMap<String, String>);
    impl<'de> serde::Deserialize<'de> for UniqueManifest {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visitor;
            impl<'de> serde::de::Visitor<'de> for Visitor {
                type Value = UniqueManifest;
                fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("a map of unique input IDs to digests")
                }
                fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                    let mut values = BTreeMap::new();
                    while let Some((key, value)) = map.next_entry::<String, String>()? {
                        if values.len() >= MAX_SELECTED || values.insert(key, value).is_some() {
                            return Err(serde::de::Error::custom("duplicate or excessive memory manifest entries"));
                        }
                    }
                    Ok(UniqueManifest(values))
                }
            }
            deserializer.deserialize_map(Visitor)
        }
    }
    let manifest = serde_json::from_slice::<UniqueManifest>(&bytes).map_err(io::Error::other)?.0;
    if manifest.len() > MAX_SELECTED || manifest.iter().any(|(id, digest)|
        !valid_manifest_key(id)
        || digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))) {
        return Err(io::Error::other("invalid memory input manifest entry"));
    }
    Ok(manifest)
}

fn valid_manifest_key(id: &str) -> bool {
    if let Some(rest) = id.strip_prefix("filename:") {
        return rest.split_once(':').is_some_and(|(id, name)|
            uuid::Uuid::parse_str(id).is_ok_and(|value| value.to_string() == id)
            && name.len() <= 100 && name.ends_with(".md")
            && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.')));
    }
    if let Some(path) = id.strip_prefix("resource:") {
        let components = Path::new(path).components().collect::<Vec<_>>();
        return path.len() <= 1024 && !path.contains('\\') && components.len() >= 4
            && components.iter().all(|component| matches!(component, Component::Normal(_)))
            && components[0].as_os_str() == "extensions" && components[2].as_os_str() == "resources";
    }
    uuid::Uuid::parse_str(id).is_ok_and(|value| value.to_string() == id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Stage1Output {
        let timestamp = chrono::DateTime::from_timestamp(100, 0).unwrap();
        Stage1Output {
            thread_id: codex_protocol::ThreadId::new(), rollout_path: "synthetic-rollout".into(),
            source_updated_at: timestamp, raw_memory: "synthetic raw".into(),
            rollout_summary: "synthetic summary".into(), rollout_slug: None,
            cwd: "synthetic-workspace".into(), git_branch: None, generated_at: timestamp,
        }
    }

    #[test]
    fn same_timestamp_changed_body_detected_and_removed_body_not_in_diff() {
        let old = sample();
        let mut changed = old.clone();
        changed.raw_memory = "changed without timestamp".into();
        let previous = selected_manifest(std::slice::from_ref(&old)).unwrap();
        let updated = selected_manifest(std::slice::from_ref(&changed)).unwrap();
        let (_, changed_ids, removed) = changed_inputs(&previous, &updated);
        assert_eq!(changed_ids, vec![old.thread_id.to_string()]);
        assert!(removed.is_empty());
        let (_, _, removed) = changed_inputs(&previous, &BTreeMap::new());
        assert_eq!(removed, vec![old.thread_id.to_string()]);
        assert!(!format!("{removed:?}").contains("synthetic raw"));
        let filenames = removed_filenames(&previous, &BTreeMap::new(), &removed);
        assert_eq!(filenames, vec![format!("{}: rollout_summaries/{}.md", old.thread_id, crate::storage::rollout_summary_file_stem(&old))]);
        assert!(!format!("{filenames:?}").contains("synthetic raw"));
        assert!(selected_manifest(&[old.clone(), old]).is_err());
    }

    #[test]
    fn old_mapping_is_unknown_and_unsupplied_summary_pointer_fails() {
        let old = sample();
        let mut legacy = BTreeMap::new();
        legacy.insert(old.thread_id.to_string(), "a".repeat(64));
        assert_eq!(removed_filenames(&legacy, &BTreeMap::new(), &[old.thread_id.to_string()]), vec![format!("{}: unknown", old.thread_id)]);
        let selected = selected_manifest(std::slice::from_ref(&old)).unwrap();
        let supplied = format!("rollout_summaries/{}.md", crate::storage::rollout_summary_file_stem(&old));
        assert!(validate_summary_pointers(&format!("- `{supplied}` — supported"), &selected).is_ok());
        assert!(validate_summary_pointers("- rollout_summaries/removed.md — stale", &selected).is_err());
    }

    #[test]
    fn prior_input_manifest_is_checked_against_captured_artifacts() -> io::Result<()> {
        let directory = tempfile::tempdir()?;
        let selected = selected_manifest(&[sample()])?;
        write_manifest(directory.path(), &selected)?;
        let bytes = codex_memories_read::publication_snapshot::make_artifact_manifest(directory.path())?;
        let digest = codex_memories_read::publication_snapshot::artifact_manifest_digest(&bytes);
        std::fs::write(directory.path().join(codex_memories_read::publication_snapshot::ARTIFACT_MANIFEST), bytes)?;
        let artifacts = codex_memories_read::publication_snapshot::load_artifact_manifest(directory.path(), &digest)?;
        assert_eq!(read_manifest(directory.path(), &artifacts)?, selected);
        std::fs::write(directory.path().join(".codex-memory-input-manifest.json"), b"{}")?;
        assert!(read_manifest(directory.path(), &artifacts).is_err());
        Ok(())
    }
}
