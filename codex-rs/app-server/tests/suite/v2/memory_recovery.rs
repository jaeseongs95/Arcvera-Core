// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Recovery with real stdio, worker tools, SQLite and synthetic inputs only.
use super::*;
use core_test_support::responses::{ev_function_call, mount_sse_sequence};
use std::collections::BTreeMap;
use tokio::time::{Duration, Instant};

async fn recovery_thread(app: &mut TestAppServer, project: &Path) -> Result<String> {
    let request = app
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            cwd: Some(project.to_string_lossy().into_owned()),
            config: Some(std::collections::HashMap::from([
                ("memories.scope".into(), json!("project")),
                ("memories.version".into(), json!("v1")),
                ("memories.dual_write".into(), json!(false)),
                ("memories.generate_memories".into(), json!(true)),
                ("memories.use_memories".into(), json!(false)),
                ("memories.consolidation_model".into(), json!("gpt-5.5")),
            ])),
            ..Default::default()
        })
        .await?;
    let started: ThreadStartResponse = app.read_response(request).await?;
    Ok(started.thread.id)
}

async fn recovery_state(
    app: &mut TestAppServer,
    id: &str,
    expected: &str,
) -> Result<ThreadMemoryReadResponse> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let read = read_memory(app, id, true).await?;
        if let Some(state) = &read.consolidation {
            if state.status == expected {
                return Ok(read);
            }
            anyhow::ensure!(
                state.status != "error" || expected == "error",
                "unexpected worker failure: {:?}",
                state.last_error
            );
        }
        anyhow::ensure!(Instant::now() < deadline, "worker did not reach {expected}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn recovery_idle(root: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match MemoryWorkspaceLock::try_acquire(root) {
            Ok(lock) => {
                drop(lock);
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "worker retained workspace ownership"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn recovery_bytes(root: &Path) -> Result<BTreeMap<PathBuf, Vec<u8>>> {
    fn visit(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
        let metadata = std::fs::symlink_metadata(path)?;
        anyhow::ensure!(
            !metadata.file_type().is_symlink(),
            "fixture contains redirected path"
        );
        if metadata.is_dir() {
            for entry in std::fs::read_dir(path)? {
                visit(root, &entry?.path(), out)?;
            }
        } else {
            out.insert(path.strip_prefix(root)?.to_path_buf(), std::fs::read(path)?);
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result)?;
    Ok(result)
}

fn recovery_stages(root: &Path) -> Result<Vec<PathBuf>> {
    let identity = codex_memories_read::publication_snapshot::root_identity(root)?;
    let prefix = format!("codex-memory-stage-{identity}-");
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(root.parent().expect("fixture root parent"))? {
        let entry = entry?;
        let name = entry.file_name();
        if let Some(id) = name.to_string_lossy().strip_prefix(&prefix) {
            // Exact root identity and a canonical UUID prove fixture ownership.
            let parsed = uuid::Uuid::parse_str(id)?;
            assert_eq!(parsed.to_string(), id);
            assert_eq!(
                std::fs::read_to_string(entry.path().join(".codex-memory-stage"))?,
                format!("{identity}\n{id}\n")
            );
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

#[tokio::test]
async fn memory_recovery_archives_failed_stages_after_shutdown_and_retries() -> Result<()> {
    let server = start_mock_server().await;
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .with_model("gpt-5.5")
        .with_sandbox_mode("danger-full-access")
        .enable_feature(Feature::MemoryTool)
        .enable_feature(Feature::Sqlite)
        .disable_feature(Feature::CodeMode)
        .disable_feature(Feature::CodeModeOnly)
        .write(home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .build_initialized()
        .await?;
    let id = recovery_thread(&mut app, project.path()).await?;
    let root = PathBuf::from(
        read_memory(&mut app, &id, true)
            .await?
            .root
            .expect("project root"),
    );
    std::fs::create_dir_all(&root)?;
    let legacy = "v1\n\n## User Profile\n\n## User preferences\n- Synthetic legacy preference.\n\n## General Tips\n\n## What's in Memory\n- Legacy input.\n";
    std::fs::write(root.join("memory_summary.md"), legacy)?;
    std::fs::write(
        root.join("MEMORY.md"),
        "# Original synthetic legacy memory\n",
    )?;
    let add = app
        .send_raw_request(
            "thread/memory/note/add",
            Some(json!({"threadId":id,"note":"Synthetic recovery note: preserve original bytes."})),
        )
        .await?;
    let note: ThreadMemoryNoteAddResponse = app.read_response(add).await?;
    assert!(note.pending_consolidation);
    let originals = recovery_bytes(&root.join("extensions"))?;
    let mut events = Vec::new();
    for attempt in 0..2 {
        events.push(sse(vec![ev_function_call(&format!("invalid-{attempt}"), "memory_stage", &json!({"operation":"write","path":"memory_summary.md","content":"invalid summary without version header\n"}).to_string()), ev_completed(&format!("invalid-write-{attempt}"))]));
        events.push(sse(vec![
            ev_assistant_message(&format!("invalid-done-{attempt}"), "Done."),
            ev_completed(&format!("invalid-finished-{attempt}")),
        ]));
    }
    let valid = legacy.replace("Legacy input.", "Recovered publication.");
    events.push(sse(vec![
        ev_function_call(
            "recovered-write",
            "memory_stage",
            &json!({"operation":"write","path":"memory_summary.md","content":valid}).to_string(),
        ),
        ev_completed("recovered-tools"),
    ]));
    events.push(sse(vec![
        ev_assistant_message("recovered-done", "Done."),
        ev_completed("recovered-finished"),
    ]));
    let model = mount_sse_sequence(&server, events).await;
    let mut baseline_id = None;
    let mut baseline_bytes = None;
    for attempt in 0..2 {
        assert_eq!(
            consolidate_wire(&mut app, &id, Some(true)).await?.status,
            "accepted"
        );
        let read = recovery_state(&mut app, &id, "error").await?;
        assert!(
            read.consolidation
                .as_ref()
                .and_then(|state| state.last_error.as_deref())
                .is_some_and(
                    |error| error.contains("memory summary artifact does not start with v1")
                )
        );
        recovery_idle(&root).await?;
        assert_eq!(read.summary.as_deref(), Some(legacy));
        assert_eq!(
            read.publication_provenance.as_deref(),
            Some("legacy_import")
        );
        let generation = read.generation_id.expect("retained legacy generation");
        if let Some(previous) = &baseline_id {
            assert_eq!(previous, &generation);
        }
        let path = codex_memories_read::publication_snapshot::generation_root(&root, &generation)?;
        let bytes = recovery_bytes(&path)?;
        if let Some(previous) = &baseline_bytes {
            assert_eq!(previous, &bytes);
        }
        baseline_id = Some(generation);
        baseline_bytes = Some(bytes);
        assert_eq!(recovery_stages(&root)?.len(), attempt + 1);
        assert_eq!(recovery_bytes(&root.join("extensions"))?, originals);
        let requests = model.requests();
        let call = format!("invalid-{attempt}");
        let output = requests[attempt * 2 + 1]
            .function_call_output_content_and_success(&call)
            .expect("actual stage tool result");
        assert_ne!(output.1, Some(false));
        assert!(output.0.expect("stage tool output").contains("written"));
    }
    assert_eq!(model.requests().len(), 4);
    let blocked = consolidate_wire(&mut app, &id, Some(true)).await?;
    assert_eq!(blocked.status, "blocked");
    assert!(
        blocked
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("retained memory stages require recovery"))
    );
    assert_eq!(
        model.requests().len(),
        4,
        "capacity block must precede model work"
    );
    let stages = recovery_stages(&root)?;
    assert_eq!(stages.len(), 2);
    let marker = std::fs::read(root.join(".codex-memory-managed"))?;
    // Shutdown joins this fixture's workers/readers before any archive operation.
    app.shutdown_gracefully().await?;
    let lock = MemoryWorkspaceLock::try_acquire(&root)?;
    let archive = home.path().join("recovery-archive");
    std::fs::create_dir(&archive)?;
    let mut archived = Vec::new();
    for stage in stages {
        let bytes = recovery_bytes(&stage)?;
        let destination = archive.join(stage.file_name().expect("owned stage name"));
        assert!(!destination.exists());
        std::fs::rename(&stage, &destination)?;
        assert_eq!(recovery_bytes(&destination)?, bytes);
        archived.push((destination, bytes));
    }
    assert!(recovery_stages(&root)?.is_empty());
    drop(lock);
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .build_initialized()
        .await?;
    let id = recovery_thread(&mut app, project.path()).await?;
    let before = read_memory(&mut app, &id, true).await?;
    assert_eq!(before.generation_id, baseline_id);
    assert_eq!(before.root.as_deref(), root.to_str());
    assert_eq!(
        consolidate_wire(&mut app, &id, Some(true)).await?.status,
        "accepted"
    );
    let recovered = recovery_state(&mut app, &id, "done").await?;
    assert_eq!(recovered.summary.as_deref(), Some(valid.as_str()));
    assert_eq!(
        recovered.publication_provenance.as_deref(),
        Some("consolidated")
    );
    assert_ne!(recovered.generation_id, baseline_id);
    assert_eq!(model.requests().len(), 6);
    assert_eq!(std::fs::read(root.join(".codex-memory-managed"))?, marker);
    assert_eq!(
        std::fs::read_to_string(root.join("memory_summary.md"))?,
        legacy
    );
    assert_eq!(
        std::fs::read_to_string(root.join("MEMORY.md"))?,
        "# Original synthetic legacy memory\n"
    );
    assert_eq!(recovery_bytes(&root.join("extensions"))?, originals);
    let baseline = codex_memories_read::publication_snapshot::generation_root(
        &root,
        baseline_id.as_deref().expect("baseline"),
    )?;
    assert_eq!(
        recovery_bytes(&baseline)?,
        baseline_bytes.expect("baseline bytes")
    );
    for (path, bytes) in archived {
        assert_eq!(recovery_bytes(&path)?, bytes);
    }
    assert!(recovery_stages(&root)?.is_empty());
    app.shutdown_gracefully().await?;
    Ok(())
}
