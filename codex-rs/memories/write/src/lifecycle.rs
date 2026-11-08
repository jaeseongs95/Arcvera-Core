//! Bind accepted changes to host-published artifacts without treating a model's
//! completed job as proof of semantic correction or forgetting.

use codex_memories_read::publication::MemoryPublication;
use codex_memories_read::publication::PublicationStatus;
use codex_memories_read::publication::load_publication;
use codex_memories_read::publication::publication_path;
use codex_memories_read::publication::sha256;
use codex_protocol::MemoryVersion;
use codex_utils_path::write_atomically;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

const DECISION_FILENAME: &str = "memory_application.json";
const MAX_SOURCE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_SOURCE_FILES: usize = 10_000;

/// Observed artifact currentness, independent of persisted phase-2 job state.
/// A published generation is an artifact receipt, not a semantic correctness
/// verdict for natural-language corrections.
pub struct MemoryPublicationView {
    pub summary: Option<String>,
    pub status: &'static str,
    pub publication_status: &'static str,
    pub accepted_generation: Option<u64>,
    pub published_generation: Option<u64>,
    pub current: bool,
    pub recovery_stage_root: Option<String>,
}

impl MemoryPublicationView {
    fn unavailable(status: &'static str, publication: Option<&MemoryPublication>) -> Self {
        Self {
            summary: None,
            status,
            publication_status: publication.map_or(status, |state| match state.status {
                PublicationStatus::Pending => "pending",
                PublicationStatus::Processing => "processing",
                PublicationStatus::Published => "published",
                PublicationStatus::Failed => "failed",
                PublicationStatus::Cleared => "cleared",
                PublicationStatus::Resetting => "resetting",
            }),
            accepted_generation: publication.map(|state| state.accepted_generation),
            published_generation: publication.and_then(|state| state.published_generation),
            current: false,
            recovery_stage_root: publication.and_then(|state| state.recovery_stage.clone()),
        }
    }
}

/// Read metadata and bounded summary bytes in one shared-lock interval. The
/// caller must resolve and validate the thread's selected root/environment.
pub async fn memory_publication_snapshot(
    root: &Path,
    version: MemoryVersion,
) -> io::Result<MemoryPublicationView> {
    use codex_memories_read::publication::MemoryWorkspaceLock;
    let root = root.to_owned();
    tokio::task::spawn_blocking(move || {
        let _lock = match MemoryWorkspaceLock::try_acquire_shared(&root) {
            Ok(lock) => lock,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                return MemoryPublicationView::unavailable("busy", None);
            }
            Err(_) => return MemoryPublicationView::unavailable("unreadable", None),
        };
        let publication = match load_publication(&root) {
            Ok(publication) => publication,
            Err(_) => return MemoryPublicationView::unavailable("invalid", None),
        };
        if let Some(state) = &publication {
            let unavailable = match state.status {
                PublicationStatus::Pending => Some("pending"),
                PublicationStatus::Processing => Some("processing"),
                PublicationStatus::Failed => Some("failed"),
                PublicationStatus::Cleared => Some("cleared"),
                PublicationStatus::Resetting => Some("resetting"),
                PublicationStatus::Published => None,
            };
            if let Some(status) = unavailable {
                return MemoryPublicationView::unavailable(status, Some(state));
            }
        } else if version == MemoryVersion::V2 {
            return MemoryPublicationView::unavailable("unknown", None);
        }
        let summary_path = root.join("memory_summary.md");
        if std::fs::symlink_metadata(&summary_path)
            .is_ok_and(|metadata| metadata.len() > 1024 * 1024)
        {
            return MemoryPublicationView::unavailable("oversized", publication.as_ref());
        }
        let bytes = match publication.as_ref() {
            Some(state) => match codex_memories_read::publication::read_published_artifacts(
                &root, state, version,
            ) {
                Ok(mut artifacts) => artifacts
                    .remove("memory_summary.md")
                    .expect("published summary required"),
                Err(_) => return MemoryPublicationView::unavailable("invalid", Some(state)),
            },
            None => match read_regular_file(&root.join("memory_summary.md"), 1024 * 1024) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    return MemoryPublicationView::unavailable("missing", publication.as_ref());
                }
                Err(_) => {
                    return MemoryPublicationView::unavailable("unreadable", publication.as_ref());
                }
            },
        };
        let summary = match String::from_utf8(bytes) {
            Ok(summary) if !summary.trim().is_empty() => summary,
            Ok(_) => return MemoryPublicationView::unavailable("missing", publication.as_ref()),
            Err(_) => {
                return MemoryPublicationView::unavailable("unreadable", publication.as_ref());
            }
        };
        let mut view = MemoryPublicationView::unavailable("ready", publication.as_ref());
        view.summary = Some(summary);
        view.current = publication.is_some();
        if publication.is_none() {
            // Preserve explicit V1 browsing while labelling it as an unbound
            // historical preview. V2 never injects or previews unknown bytes.
            view.publication_status = "legacy";
        }
        view
    })
    .await
    .map_err(io::Error::other)
}

pub(crate) struct ConsolidationPublication {
    workspace_lock: Arc<crate::MemoryWorkspaceLock>,
    publication: MemoryPublication,
    input_digest: String,
    previous_artifact_digests: BTreeMap<String, String>,
    version: MemoryVersion,
    source_digests: BTreeMap<PathBuf, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApplicationDecision {
    generation: u64,
    input_digest: String,
    decision: Decision,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum Decision {
    Updated,
    Unchanged,
}

/// The caller holds the root's cross-process lock. Atomic replacement keeps
/// readers from seeing partial JSON. This is process-crash consistency, not a
/// guarantee against storage-device or power-loss failures.
pub(crate) fn save_publication(root: &Path, state: &MemoryPublication) -> io::Result<()> {
    // Validate an existing receipt before replacing it, including reparse-point
    // rejection; a malformed receipt must not be silently reset to legacy.
    load_publication(root)?;
    let contents = serde_json::to_string(state).map_err(io::Error::other)?;
    write_atomically(&publication_path(root), &contents)
}

pub(crate) async fn save_publication_async(
    root: &Path,
    state: &MemoryPublication,
    workspace_lock: Arc<crate::MemoryWorkspaceLock>,
) -> io::Result<()> {
    let root = root.to_owned();
    let state = state.clone();
    tokio::task::spawn_blocking(move || {
        let _workspace_lock = workspace_lock;
        save_publication(&root, &state)
    })
    .await
    .map_err(io::Error::other)?
}

pub(crate) async fn has_pending_inputs(root: &Path, version: MemoryVersion) -> io::Result<bool> {
    let root = root.to_owned();
    tokio::task::spawn_blocking(move || {
        Ok(match load_publication(&root)? {
            Some(state) => {
                !matches!(
                    state.status,
                    PublicationStatus::Published | PublicationStatus::Cleared
                ) || state.published_generation != Some(state.accepted_generation)
            }
            None => version == MemoryVersion::V2,
        })
    })
    .await
    .map_err(io::Error::other)?
}

/// A partial input sync is not an unchanged, current publication. Invalidate
/// before releasing the writer lock even when no worker could be dispatched.
pub(crate) async fn reject_unpublished_inputs(
    root: &Path,
    workspace_lock: Arc<crate::MemoryWorkspaceLock>,
) -> io::Result<()> {
    let root = root.to_owned();
    tokio::task::spawn_blocking(move || {
        let _workspace_lock = workspace_lock;
        let previous = load_publication(&root)?;
        let mut state = match previous {
            Some(state) if state.status == PublicationStatus::Resetting => {
                return Err(io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "memory reset is incomplete",
                ));
            }
            Some(state)
                if matches!(
                    state.status,
                    PublicationStatus::Pending
                        | PublicationStatus::Processing
                        | PublicationStatus::Failed
                ) =>
            {
                state
            }
            previous => MemoryPublication::next_pending(previous.as_ref())?,
        };
        state.status = PublicationStatus::Failed;
        state.artifact_digests.clear();
        save_publication(&root, &state)
    })
    .await
    .map_err(io::Error::other)?
}

impl ConsolidationPublication {
    pub(crate) async fn bind_stage(&mut self, root: &Path, stage: &Path) -> io::Result<()> {
        self.publication.recovery_stage = Some(stage.display().to_string());
        save_publication_async(root, &self.publication, Arc::clone(&self.workspace_lock)).await
    }
    /// Called after input sync and before any agent may edit derived artifacts.
    pub(crate) async fn begin(
        root: &Path,
        input_diff: &str,
        version: MemoryVersion,
        workspace_lock: Arc<crate::MemoryWorkspaceLock>,
    ) -> io::Result<Self> {
        let input_digest = sha256(input_diff.as_bytes());
        let root = root.to_owned();
        tokio::task::spawn_blocking(move || {
            let previous = load_publication(&root)?;
            if previous.as_ref().is_some_and(|state| state.status == PublicationStatus::Resetting) {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, "memory reset is incomplete; explicit reset recovery is required"));
            }
            let pending_generation = previous.as_ref().is_some_and(|state| {
                !matches!(
                    state.status,
                    PublicationStatus::Published | PublicationStatus::Cleared
                ) || state.published_generation != Some(state.accepted_generation)
            });
            let mut publication = if pending_generation {
                previous.expect("pending publication was present")
            } else {
                MemoryPublication::next_pending(previous.as_ref())?
            };
            publication.status = PublicationStatus::Processing;
            publication.artifact_digests.clear();
            // Block consumers before inspecting inputs.
            save_publication(&root, &publication)?;
            let snapshot = (|| -> io::Result<_> {
                Ok((artifact_digests(&root, version, true)?, source_digests(&root, version)?))
            })();
            let (previous_artifact_digests, source_digests) = match snapshot {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    publication.status = PublicationStatus::Failed;
                    if let Err(recording) = save_publication(&root, &publication) {
                        return Err(io::Error::other(format!("memory input snapshot failed: {error}; recording failure failed: {recording}")));
                    }
                    return Err(error);
                }
            };
            Ok(Self {
                workspace_lock,
                publication,
                input_digest,
                previous_artifact_digests,
                version,
                source_digests,
            })
        })
        .await
        .map_err(io::Error::other)?
    }

    pub(crate) fn decision_prompt(&self) -> String {
        format!(
            "\nHost input generation: {}\nHost input digest: {}\n\
             Process the supplied additions, corrections, and forget requests. \
             Read all existing notes in extensions/ad_hoc/notes so older source \
             summaries cannot revive a claim that was already corrected. \
             Preserve original source and note files. After updating derived memory, \
             write {DECISION_FILENAME} in the memory root as exactly \
             {{\"generation\":{},\"input_digest\":\"{}\",\"decision\":\"updated\"}}. \
             Use decision \"unchanged\" instead when the existing final artifacts \
             already satisfy the requests or the input has no lasting use. \
             Do not add memory merely to change the output. A completed response \
             alone does not acknowledge these inputs. This decision records \
             processing; it does not certify semantic accuracy.\n",
            self.publication.accepted_generation,
            self.input_digest,
            self.publication.accepted_generation,
            self.input_digest,
        )
    }

    /// Validate before resetting the baseline or marking the DB job complete.
    /// Original source modifications are detected rather than committed as a
    /// successful consolidation. They are never silently overwritten here.
    pub(crate) async fn validate_decision(
        &self,
        root: &Path,
        output_root: &Path,
    ) -> io::Result<()> {
        let root = root.to_owned();
        let output_root = output_root.to_owned();
        let expected_sources = self.source_digests.clone();
        let generation = self.publication.accepted_generation;
        let digest = self.input_digest.clone();
        let previous_artifact_digests = self.previous_artifact_digests.clone();
        let version = self.version;
        let workspace_lock = Arc::clone(&self.workspace_lock);
        tokio::task::spawn_blocking(move || {
            let _workspace_lock = workspace_lock;
            if source_digests(&root, version)? != expected_sources
                || source_digests(&output_root, version)? != expected_sources
            {
                return Err(io::Error::other(
                    "consolidation modified original memory inputs",
                ));
            }
            let path = output_root.join(DECISION_FILENAME);
            let decision: ApplicationDecision =
                serde_json::from_slice(&read_regular_file(&path, 1024)?)
                    .map_err(io::Error::other)?;
            if decision.generation != generation || decision.input_digest != digest {
                return Err(io::Error::other(
                    "memory application decision belongs to other inputs",
                ));
            }
            let unchanged =
                artifact_digests(&output_root, version, false)? == previous_artifact_digests;
            if (decision.decision == Decision::Unchanged) != unchanged {
                return Err(io::Error::other(
                    "memory application decision disagrees with artifact changes",
                ));
            }
            Ok(())
        })
        .await
        .map_err(io::Error::other)?
    }

    /// Publish only after artifact validation, ownership fencing, baseline reset,
    /// and DB completion. A failure at any earlier boundary stays unavailable.
    pub(crate) async fn publish(&self, root: &Path, version: MemoryVersion) -> io::Result<()> {
        if version != self.version {
            return Err(io::Error::other("memory publication changed version"));
        }
        let root = root.to_owned();
        let mut publication = self.publication.clone();
        let workspace_lock = Arc::clone(&self.workspace_lock);
        tokio::task::spawn_blocking(move || {
            let _workspace_lock = workspace_lock;
            let current = load_publication(&root)?
                .ok_or_else(|| io::Error::other("memory publication disappeared"))?;
            if current != publication {
                return Err(io::Error::other(
                    "memory publication generation lost ownership",
                ));
            }
            // Publish only final consolidated outputs. Phase-1 summaries and
            // original notes may still contain corrected claims and are not
            // made current merely by hashing their bytes.
            publication.artifact_digests = artifact_digests(&root, version, false)?;
            publication.published_generation = Some(publication.accepted_generation);
            publication.status = PublicationStatus::Published;
            publication.recovery_stage = None;
            save_publication(&root, &publication)
        })
        .await
        .map_err(io::Error::other)?
    }

    pub(crate) async fn fail(&self, root: &Path) -> io::Result<()> {
        let root = root.to_owned();
        let mut failed = self.publication.clone();
        let workspace_lock = Arc::clone(&self.workspace_lock);
        tokio::task::spawn_blocking(move || {
            let _workspace_lock = workspace_lock;
            // The caller still holds the root lock. No legitimate note writer
            // or new consolidation can advance this state. Do not preserve a
            // parseable Published state forged by an unmanaged worker.
            failed.status = PublicationStatus::Failed;
            failed.artifact_digests.clear();
            save_publication(&root, &failed)
        })
        .await
        .map_err(io::Error::other)?
    }
}

fn artifact_digests(
    root: &Path,
    version: MemoryVersion,
    allow_missing: bool,
) -> io::Result<BTreeMap<String, String>> {
    match codex_memories_read::publication::collect_published_artifacts(root, version) {
        Ok(artifacts) => {
            Ok(codex_memories_read::publication::published_artifact_digests(&artifacts))
        }
        Err(error) if allow_missing && error.kind() == io::ErrorKind::NotFound => {
            Ok(BTreeMap::new())
        }
        Err(error) => Err(error),
    }
}

fn source_digests(root: &Path, version: MemoryVersion) -> io::Result<BTreeMap<PathBuf, String>> {
    let mut digests = BTreeMap::new();
    let mut total_bytes = 0_u64;
    let mut paths = vec![root.join("rollout_summaries"), root.join("extensions")];
    if version == MemoryVersion::V1 {
        paths.push(root.join("raw_memories.md"));
    }
    let mut entries_seen = 0_usize;
    while let Some(directory) = paths.pop() {
        entries_seen += 1;
        if entries_seen > MAX_SOURCE_FILES {
            return Err(io::Error::other("memory source directory budget exceeded"));
        }
        match std::fs::symlink_metadata(&directory) {
            Ok(metadata) if is_redirected(&metadata) => {
                return Err(io::Error::other(
                    "memory source directory must not be redirected",
                ));
            }
            Ok(metadata) if metadata.is_file() => {
                if digests.len() >= MAX_SOURCE_FILES {
                    return Err(io::Error::other("memory source file budget exceeded"));
                }
                let bytes = read_regular_file(&directory, MAX_SOURCE_BYTES - total_bytes)?;
                total_bytes += bytes.len() as u64;
                digests.insert(
                    directory
                        .strip_prefix(root)
                        .map_err(io::Error::other)?
                        .to_owned(),
                    sha256(&bytes),
                );
                continue;
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::other(
                    "memory source must be a regular file or directory",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        }
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        for entry in entries {
            let entry = entry?;
            if entry.file_name() != ".git" && entry.file_name() != ".codex-baseline" {
                if paths.len() + entries_seen >= MAX_SOURCE_FILES {
                    return Err(io::Error::other("memory source directory budget exceeded"));
                }
                paths.push(entry.path());
            }
        }
    }
    Ok(digests)
}

fn read_regular_file(path: &Path, budget: u64) -> io::Result<Vec<u8>> {
    let metadata = std::fs::symlink_metadata(path)?;
    if is_redirected(&metadata) || !metadata.is_file() || metadata.len() > budget {
        return Err(io::Error::other(
            "memory input is redirected, non-regular, or exceeds its byte budget",
        ));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(budget + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > budget {
        return Err(io::Error::other("memory input grew beyond its byte budget"));
    }
    Ok(bytes)
}

fn is_redirected(metadata: &std::fs::Metadata) -> bool {
    let redirected = metadata.file_type().is_symlink();
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        redirected || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    redirected
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_memories_read::publication::PublicationReadiness;

    #[tokio::test]
    async fn generations_are_local_to_each_selected_root_and_do_not_rebind_a_project_view()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let global = home.path().join("memories_v2");
        let project = home
            .path()
            .join("project_memories")
            .join("a".repeat(64))
            .join("memories_v2");
        crate::add_user_memory_note(&global, "global correction one").await?;
        crate::add_user_memory_note(&project, "project A correction one").await?;
        crate::add_user_memory_note(&global, "global correction two").await?;
        let project_view = memory_publication_snapshot(&project, MemoryVersion::V2).await?;
        let global_view = memory_publication_snapshot(&global, MemoryVersion::V2).await?;
        assert_eq!(project_view.accepted_generation, Some(1));
        assert_eq!(global_view.accepted_generation, Some(2));
        assert_eq!(project_view.status, "pending");
        assert!(project_view.summary.is_none());
        // These values are separate root-local counters. A larger value in the
        // global store conveys no ordering or applied state for project A.
        Ok(())
    }

    #[tokio::test]
    async fn real_note_view_reports_pending_then_failed_recovery_without_stale_summary()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "old summary")?;
        crate::add_user_memory_note(&root, "Correct the synthetic old fact").await?;
        let pending = memory_publication_snapshot(&root, MemoryVersion::V2).await?;
        assert_eq!(pending.status, "pending");
        assert_eq!(pending.accepted_generation, Some(1));
        assert!(!pending.current && pending.summary.is_none());
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&root)?);
        let mut publication = ConsolidationPublication::begin(
            &root,
            "+correction",
            MemoryVersion::V2,
            Arc::clone(&lock),
        )
        .await?;
        let stage =
            crate::workspace::MemoryStage::prepare(&root, MemoryVersion::V2, Arc::clone(&lock))
                .await?;
        publication.bind_stage(&root, stage.root()).await?;
        let busy = memory_publication_snapshot(&root, MemoryVersion::V2).await?;
        assert_eq!(busy.status, "busy");
        assert!(busy.summary.is_none());
        stage.preserve_outputs();
        publication.fail(&root).await?;
        let expected_stage = stage.root().display().to_string();
        drop(publication);
        drop(stage);
        drop(lock);
        let failed = memory_publication_snapshot(&root, MemoryVersion::V2).await?;
        assert_eq!(failed.status, "failed");
        assert!(failed.summary.is_none() && !failed.current);
        assert_eq!(
            failed.recovery_stage_root.as_deref(),
            Some(expected_stage.as_str())
        );
        Ok(())
    }

    #[tokio::test]
    async fn legacy_view_is_explicitly_unbound_and_v2_never_previews_unknown_bytes()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "historical preview")?;
        let legacy = memory_publication_snapshot(&root, MemoryVersion::V1).await?;
        assert_eq!(legacy.summary.as_deref(), Some("historical preview"));
        assert_eq!(legacy.publication_status, "legacy");
        assert!(!legacy.current);
        let unknown = memory_publication_snapshot(&root, MemoryVersion::V2).await?;
        assert_eq!(unknown.status, "unknown");
        assert!(unknown.summary.is_none() && !unknown.current);
        Ok(())
    }

    fn write_decision(
        root: &Path,
        publication: &ConsolidationPublication,
        decision: &str,
    ) -> io::Result<()> {
        std::fs::write(
            root.join(DECISION_FILENAME),
            serde_json::to_vec(&serde_json::json!({
                "generation": publication.publication.accepted_generation,
                "input_digest": publication.input_digest,
                "decision": decision,
            }))?,
        )
    }

    #[tokio::test]
    async fn input_sync_failure_cannot_keep_an_earlier_published_generation_current()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "old summary")?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&root)?);
        let publication = ConsolidationPublication::begin(
            &root,
            "+old inputs",
            MemoryVersion::V2,
            Arc::clone(&lock),
        )
        .await?;
        write_decision(&root, &publication, "unchanged")?;
        publication.validate_decision(&root, &root).await?;
        publication.publish(&root, MemoryVersion::V2).await?;
        drop(publication);
        reject_unpublished_inputs(&root, Arc::clone(&lock)).await?;
        let state = load_publication(&root)?.unwrap();
        assert_eq!(state.status, PublicationStatus::Failed);
        assert_eq!(state.accepted_generation, 2);
        assert_eq!(state.published_generation, Some(1));
        reject_unpublished_inputs(&root, Arc::clone(&lock)).await?;
        assert_eq!(load_publication(&root)?.unwrap().accepted_generation, 2);
        drop(lock);
        let view = memory_publication_snapshot(&root, MemoryVersion::V2).await?;
        assert!(view.summary.is_none() && !view.current);
        Ok(())
    }

    #[tokio::test]
    async fn new_generation_without_notes_still_requires_worker_decision() -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "supported existing summary")?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&root)?);
        let publication = ConsolidationPublication::begin(
            &root,
            "+ordinary extraction without lasting new facts",
            MemoryVersion::V2,
            Arc::clone(&lock),
        )
        .await?;
        assert!(
            publication
                .decision_prompt()
                .contains("Host input generation:")
        );
        assert!(publication.validate_decision(&root, &root).await.is_err());
        write_decision(&root, &publication, "unchanged")?;
        publication.validate_decision(&root, &root).await?;
        publication.publish(&root, MemoryVersion::V2).await?;
        Ok(())
    }

    #[tokio::test]
    async fn accepted_note_requires_a_decision_but_no_new_fact_is_required() -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "supported existing summary")?;
        let note =
            crate::add_user_memory_note(&root, "Do not retain this temporary synthetic status")
                .await?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&root)?);
        let publication = ConsolidationPublication::begin(
            &root,
            "+temporary input",
            MemoryVersion::V2,
            Arc::clone(&lock),
        )
        .await?;
        assert!(publication.validate_decision(&root, &root).await.is_err());
        assert_eq!(
            load_publication(&root)?.unwrap().status,
            PublicationStatus::Processing
        );
        write_decision(&root, &publication, "unchanged")?;
        publication.validate_decision(&root, &root).await?;
        publication.publish(&root, MemoryVersion::V2).await?;
        assert_eq!(
            load_publication(&root)?
                .unwrap()
                .readiness_for_summary(b"supported existing summary"),
            PublicationReadiness::Ready
        );
        assert_eq!(
            std::fs::read_to_string(note)?,
            "Do not retain this temporary synthetic status"
        );
        drop(lock);
        Ok(())
    }

    #[tokio::test]
    async fn corrected_canary_is_excluded_from_published_summary_without_erasing_source()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "obsolete-synthetic-canary")?;
        let note = crate::add_user_memory_note(&root, "Forget obsolete-synthetic-canary").await?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&root)?);
        let publication = ConsolidationPublication::begin(
            &root,
            "+forget request",
            MemoryVersion::V2,
            Arc::clone(&lock),
        )
        .await?;
        std::fs::write(
            root.join("memory_summary.md"),
            "remaining useful scoped fact",
        )?;
        write_decision(&root, &publication, "updated")?;
        publication.validate_decision(&root, &root).await?;
        publication.publish(&root, MemoryVersion::V2).await?;
        let bytes = std::fs::read(root.join("memory_summary.md"))?;
        assert!(!String::from_utf8(bytes.clone())?.contains("obsolete-synthetic-canary"));
        assert_eq!(
            load_publication(&root)?
                .unwrap()
                .readiness_for_summary(&bytes),
            PublicationReadiness::Ready
        );
        assert_eq!(
            std::fs::read_to_string(note)?,
            "Forget obsolete-synthetic-canary"
        );
        Ok(())
    }

    #[tokio::test]
    async fn stale_decisions_source_mutation_and_wrong_change_decisions_are_rejected()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "before")?;
        let note = crate::add_user_memory_note(&root, "synthetic correction").await?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&root)?);
        let publication = ConsolidationPublication::begin(
            &root,
            "+correction",
            MemoryVersion::V2,
            Arc::clone(&lock),
        )
        .await?;
        std::fs::write(
            root.join(DECISION_FILENAME),
            br#"{"generation":999,"input_digest":"other","decision":"unchanged"}"#,
        )?;
        assert!(publication.validate_decision(&root, &root).await.is_err());
        write_decision(&root, &publication, "updated")?;
        assert!(
            publication.validate_decision(&root, &root).await.is_err(),
            "no changed fact can be falsely acknowledged as updated"
        );
        write_decision(&root, &publication, "unchanged")?;
        publication.validate_decision(&root, &root).await?;
        std::fs::write(&note, "modified source")?;
        assert!(publication.validate_decision(&root, &root).await.is_err());
        publication.fail(&root).await?;
        assert_eq!(
            load_publication(&root)?.unwrap().status,
            PublicationStatus::Failed
        );
        assert_eq!(
            std::fs::read_to_string(note)?,
            "modified source",
            "failure must not silently overwrite original inputs"
        );
        Ok(())
    }

    #[tokio::test]
    async fn failed_generation_is_unavailable_and_recovery_reuses_the_pending_inputs()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories_v2");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("memory_summary.md"), "old")?;
        crate::add_user_memory_note(&root, "correction").await?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&root)?);
        let first = ConsolidationPublication::begin(
            &root,
            "+correction",
            MemoryVersion::V2,
            Arc::clone(&lock),
        )
        .await?;
        let generation = first.publication.accepted_generation;
        first.fail(&root).await?;
        assert!(has_pending_inputs(&root, MemoryVersion::V2).await?);
        let retry = ConsolidationPublication::begin(
            &root,
            "+correction",
            MemoryVersion::V2,
            Arc::clone(&lock),
        )
        .await?;
        assert_eq!(retry.publication.accepted_generation, generation);
        write_decision(&root, &retry, "unchanged")?;
        retry.validate_decision(&root, &root).await?;
        retry.publish(&root, MemoryVersion::V2).await?;
        assert!(!has_pending_inputs(&root, MemoryVersion::V2).await?);
        assert_eq!(
            load_publication(&root)?
                .unwrap()
                .readiness_for_summary(b"old"),
            PublicationReadiness::Ready
        );
        Ok(())
    }

    #[tokio::test]
    async fn v1_skill_only_correction_binds_the_complete_final_set_and_real_view()
    -> anyhow::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("memories");
        std::fs::create_dir_all(root.join("skills/obsolete"))?;
        std::fs::write(root.join("memory_summary.md"), "same supported summary")?;
        std::fs::write(root.join("MEMORY.md"), "same supported index")?;
        std::fs::write(
            root.join("skills/obsolete/SKILL.md"),
            "synthetic-forgotten-skill-canary",
        )?;
        let note =
            crate::add_user_memory_note(&root, "Forget only the obsolete synthetic skill").await?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&root)?);
        let mut publication = ConsolidationPublication::begin(
            &root,
            "+forget skill",
            MemoryVersion::V1,
            Arc::clone(&lock),
        )
        .await?;
        let stage =
            crate::workspace::MemoryStage::prepare(&root, MemoryVersion::V1, Arc::clone(&lock))
                .await?;
        publication.bind_stage(&root, stage.root()).await?;
        std::fs::remove_file(stage.root().join("skills/obsolete/SKILL.md"))?;
        write_decision(stage.root(), &publication, "updated")?;
        publication.validate_decision(&root, stage.root()).await?;
        stage.publish_outputs(&root, MemoryVersion::V1).await?;
        publication.publish(&root, MemoryVersion::V1).await?;
        stage.allow_cleanup();
        drop(publication);
        drop(stage);
        drop(lock);
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
        let view = loop {
            let view = memory_publication_snapshot(&root, MemoryVersion::V1).await?;
            if view.status != "busy" {
                break view;
            }
            anyhow::ensure!(
                tokio::time::Instant::now() < deadline,
                "successful stage cleanup did not release the root lock"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        };
        assert!(view.current);
        assert_eq!(view.summary.as_deref(), Some("same supported summary"));
        assert!(view.recovery_stage_root.is_none());
        let state = load_publication(&root)?.unwrap();
        assert!(
            !state
                .artifact_digests
                .contains_key("skills/obsolete/SKILL.md")
        );
        assert!(!root.join("skills/obsolete").exists());
        assert_eq!(
            std::fs::read_to_string(note)?,
            "Forget only the obsolete synthetic skill"
        );
        Ok(())
    }
}
