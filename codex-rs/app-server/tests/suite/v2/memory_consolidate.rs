//! Real stdio transport tests with synthetic memory workspaces and SQLite state only.
use super::*;
use codex_app_server_protocol::ThreadMemoryConsolidateResponse;
use codex_app_server_protocol::ThreadMemoryMode;
use codex_app_server_protocol::ThreadMemoryModeSetParams;
use codex_app_server_protocol::ThreadMemoryModeSetResponse;
use codex_app_server_protocol::ThreadMemoryNoteAddResponse;
use codex_memories_write::MemoryWorkspaceLock;
use codex_protocol::MemoryVersion;
use codex_protocol::ThreadId;
use codex_state::Phase2JobClaimOutcome;
use codex_utils_absolute_path::test_support::PathExt;
use pretty_assertions::assert_eq;
use std::path::Path;
use std::path::PathBuf;

#[tokio::test]
async fn memory_consolidation_note_worker_patch_is_visible_to_fresh_memory_api_and_context()
-> Result<()> {
    use codex_protocol::openai_models::ApplyPatchToolType;
    use core_test_support::responses::ev_apply_patch_custom_tool_call;
    use core_test_support::responses::ev_function_call;
    use core_test_support::responses::mount_sse_sequence;
    use tokio::time::{Duration, Instant};

    // Synthetic input only. The test and mock responder never write either output artifact.
    let marker = "T09_SYNTHETIC_README_CURRENT_83c4";
    let note = format!("Explicit fixture preference: keep README current. Marker: {marker}.");
    let summary = format!(
        "v1\n\n## User Profile\n\n## User preferences\n- Keep README current: {marker}.\n\n## General Tips\n\n## What's in Memory\n- Synthetic explicit note.\n"
    );
    let server = start_mock_server().await;
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    // Exercise phase-2's supported preservation of a parent's disabled sandbox.
    // Managed Windows sandbox enforcement is separate; this fixture needs no native setup.
    MockResponsesConfig::new(&server.uri())
        .with_model("gpt-5.5")
        .with_sandbox_mode("danger-full-access")
        .enable_feature(Feature::MemoryTool)
        .enable_feature(Feature::Sqlite)
        .enable_feature(Feature::UnifiedExec)
        .disable_feature(Feature::CodeMode)
        .disable_feature(Feature::CodeModeOnly)
        .write(home.path())?;
    // Use the same explicit tool-capability fixture as request_permissions tests.
    let mut model = codex_models_manager::model_info::model_info_from_slug("gpt-5.5");
    model.apply_patch_tool_type = Some(ApplyPatchToolType::Freeform);
    model.shell_type = codex_protocol::openai_models::ConfigShellToolType::UnifiedExec;
    app_test_support::write_models_cache_with_models(home.path(), vec![model]).await?;
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .build_initialized()
        .await?;
    let start = app
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            cwd: Some(project.path().to_string_lossy().into_owned()),
            config: Some(std::collections::HashMap::from([
                ("memories.scope".into(), json!("project")),
                ("memories.version".into(), json!("v1")),
                ("memories.generate_memories".into(), json!(true)),
                ("memories.consolidation_model".into(), json!("gpt-5.5")),
            ])),
            ..Default::default()
        })
        .await?;
    let started: ThreadStartResponse = app.read_response(start).await?;
    let before = read_memory(&mut app, &started.thread.id, true).await?;
    let root = PathBuf::from(before.root.expect("project memory root"));
    assert!(root.starts_with(home.path().join("project_memories")));
    assert_eq!(before.scope.as_deref(), Some("project"));
    assert_eq!(before.status.as_deref(), Some("missing"));
    let request = app
        .send_raw_request(
            "thread/memory/note/add",
            Some(json!({
                "threadId": started.thread.id, "note": note
            })),
        )
        .await?;
    let receipt: ThreadMemoryNoteAddResponse = app.read_response(request).await?;
    assert!(receipt.pending_consolidation);
    assert_eq!(receipt.root.as_deref(), root.to_str());
    assert!(!root.join("MEMORY.md").exists());
    assert!(!root.join("memory_summary.md").exists());
    let notes = std::fs::read_dir(root.join("extensions/ad_hoc/notes"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    assert_eq!(notes.len(), 1);
    let quoted_note = notes[0].to_string_lossy();
    let read_command = if cfg!(windows) {
        format!(
            "Get-Content -Raw -LiteralPath '{}'",
            quoted_note.replace('\'', "''")
        )
    } else {
        format!("cat -- '{}'", quoted_note.replace('\'', "'\\''"))
    };
    let patch = format!(
        "*** Begin Patch\n*** Add File: MEMORY.md\n+# Synthetic fixture memory\n+- Keep README current: {marker}.\n*** Add File: memory_summary.md\n{}\n*** End Patch",
        summary
            .lines()
            .map(|line| format!("+{line}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    let worker = mount_sse_sequence(
        &server,
        vec![
            sse(vec![
                ev_response_created("t09-read-note"),
                ev_function_call(
                    "t09-read-note-tool",
                    "exec_command",
                    &json!({
                        "cmd": read_command, "yield_time_ms": 10_000, "max_output_tokens": 2_000
                    })
                    .to_string(),
                ),
                ev_completed("t09-read-note"),
            ]),
            sse(vec![
                ev_response_created("t09-apply-memory"),
                ev_apply_patch_custom_tool_call("t09-apply-memory-tool", &patch),
                ev_completed("t09-apply-memory"),
            ]),
            sse(vec![
                ev_response_created("t09-worker-complete"),
                ev_assistant_message(
                    "t09-worker-complete-message",
                    "Synthetic consolidation finished.",
                ),
                ev_completed("t09-worker-complete"),
            ]),
        ],
    )
    .await;
    assert_eq!(
        consolidate_wire(&mut app, &started.thread.id, None)
            .await?
            .status,
        "accepted"
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(state) = read_memory(&mut app, &started.thread.id, true)
            .await?
            .consolidation
        {
            anyhow::ensure!(
                state.status != "error",
                "consolidation failed: {:?}",
                state.last_error
            );
            if state.status == "done" {
                assert!(state.finished_at.is_some());
                break;
            }
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "worker did not persist completion"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let requests = worker.requests();
    assert_eq!(requests.len(), 3);
    assert!(
        !requests[0].body_contains_text(marker),
        "the note must be read through the real tool"
    );
    let note_result = requests[1]
        .function_call_output_content_and_success("t09-read-note-tool")
        .expect("worker's actual note-read tool result");
    let note_output = note_result.0.expect("note-read output text");
    assert!(note_output.contains(marker));
    assert!(note_output.contains("Process exited with code 0"));
    assert_ne!(note_result.1, Some(false));
    let patch_output = requests[2]
        .custom_tool_call_output_content_and_success("t09-apply-memory-tool")
        .expect("worker's actual patch tool result");
    let patch_text = patch_output.0.expect("patch output text");
    assert!(patch_text.contains("Success. Updated the following files:"));
    assert!(patch_text.contains("MEMORY.md"));
    assert!(patch_text.contains("memory_summary.md"));
    assert_ne!(patch_output.1, Some(false));
    assert!(std::fs::read_to_string(root.join("MEMORY.md"))?.contains(marker));
    assert_eq!(
        std::fs::read_to_string(root.join("memory_summary.md"))?,
        summary
    );

    // A fresh thread exercises the real memory API and extension, without another startup job.
    let request = app
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            cwd: Some(project.path().to_string_lossy().into_owned()),
            ephemeral: Some(true),
            config: Some(std::collections::HashMap::from([
                ("memories.scope".into(), json!("project")),
                ("memories.version".into(), json!("v1")),
                ("memories.generate_memories".into(), json!(false)),
                ("memories.use_memories".into(), json!(true)),
            ])),
            ..Default::default()
        })
        .await?;
    let fresh: ThreadStartResponse = app.read_response(request).await?;
    assert_ne!(fresh.thread.id, started.thread.id);
    let reread = read_memory(&mut app, &fresh.thread.id, false).await?;
    assert_eq!(reread.root.as_deref(), root.to_str());
    assert_eq!(reread.status.as_deref(), Some("ready"));
    assert_eq!(reread.summary.as_deref(), Some(summary.as_str()));
    let foreground = mount_sse_once(
        &server,
        sse(vec![
            ev_response_created("t09-fresh-context"),
            ev_assistant_message("t09-fresh-context-message", "Fresh context observed."),
            ev_completed("t09-fresh-context"),
        ]),
    )
    .await;
    memory_fixture_turn(&mut app, &fresh.thread.id).await?;
    assert!(
        foreground
            .single_request()
            .message_input_texts("developer")
            .iter()
            .any(|text| text.contains(marker)),
        "the production memories extension must inject the worker-produced summary"
    );
    app.shutdown_gracefully().await?;
    Ok(())
}

#[tokio::test]
async fn memory_note_scope_write_target_preserves_pinned_global_and_config() -> Result<()> {
    use codex_app_server_protocol::MemoryNoteScope;
    let server = start_mock_server().await;
    let home = TempDir::new()?;
    let project = TempDir::new()?;
    let nested = project.path().join("nested");
    std::fs::create_dir(&nested)?;
    std::fs::write(project.path().join(".fixture-project"), "synthetic marker")?;
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::MemoryTool)
        .enable_feature(Feature::Sqlite)
        .write(home.path())?;
    let configuration_before = std::fs::read(home.path().join("config.toml"))?;
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .build_initialized()
        .await?;
    let mut ids = Vec::new();
    let mut roots = Vec::new();
    for (scope, cwd) in [("global", nested.as_path()), ("project", project.path())] {
        // A global-to-project note must use the actual loaded local project's cwd.
        // The auto-env helper otherwise supplies its independent temporary cwd.
        let mut environment = app.auto_env_params()?;
        environment.cwd = cwd.abs().into();
        let request = app
            .send_thread_start_request(ThreadStartParams {
                cwd: Some(cwd.to_string_lossy().into_owned()),
                environments: Some(vec![environment]),
                config: Some(std::collections::HashMap::from([
                    ("memories.scope".into(), json!(scope)),
                    ("memories.version".into(), json!("v2")),
                    ("memories.generate_memories".into(), json!(false)),
                    ("project_root_markers".into(), json!([".fixture-project"])),
                ])),
                ..Default::default()
            })
            .await?;
        let started: ThreadStartResponse = app.read_response(request).await?;
        let state = read_memory(&mut app, &started.thread.id, true).await?;
        assert_eq!(state.scope.as_deref(), Some(scope));
        ids.push(started.thread.id);
        roots.push(PathBuf::from(state.root.expect("resolved scope")));
    }
    assert_ne!(roots[0], roots[1]);
    std::fs::create_dir_all(&roots[0])?;
    std::fs::write(
        roots[0].join("memory_summary.md"),
        "global summary preserved",
    )?;
    let request = app
        .send_raw_request(
            "thread/memory/note/add",
            Some(json!({
                "threadId":ids[0],"note":"README 최신화를 프로젝트 기준으로 유지","scope":"project"
            })),
        )
        .await?;
    let receipt: ThreadMemoryNoteAddResponse = app.read_response(request).await?;
    assert_eq!(receipt.scope, Some(MemoryNoteScope::Project));
    assert_eq!(receipt.root.as_deref(), roots[1].to_str());
    assert!(receipt.pending_consolidation);
    assert_eq!(
        std::fs::read_dir(roots[1].join("extensions/ad_hoc/notes"))?.count(),
        1
    );
    assert!(!roots[0].join("extensions/ad_hoc/notes").exists());
    let current = read_memory(&mut app, &ids[0], true).await?;
    assert_eq!(current.scope.as_deref(), Some("global"));
    assert_eq!(current.root.as_deref(), roots[0].to_str());
    assert_eq!(current.summary.as_deref(), Some("global summary preserved"));
    for (thread_id, scope) in [(&ids[0], None), (&ids[1], Some("global"))] {
        let mut params = json!({"threadId":thread_id,"note":"explicit or legacy global note"});
        if let Some(scope) = scope {
            params["scope"] = json!(scope);
        }
        let request = app
            .send_raw_request("thread/memory/note/add", Some(params))
            .await?;
        let receipt: ThreadMemoryNoteAddResponse = app.read_response(request).await?;
        assert_eq!(receipt.scope, Some(MemoryNoteScope::Global));
        assert_eq!(receipt.root.as_deref(), roots[0].to_str());
    }
    assert_eq!(
        std::fs::read_dir(roots[0].join("extensions/ad_hoc/notes"))?.count(),
        2
    );
    assert_eq!(
        std::fs::read_dir(roots[1].join("extensions/ad_hoc/notes"))?.count(),
        1
    );
    assert_eq!(
        read_memory(&mut app, &ids[1], true).await?.root.as_deref(),
        roots[1].to_str()
    );
    assert_eq!(
        std::fs::read(home.path().join("config.toml"))?,
        configuration_before
    );
    assert!(
        !server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.url.path().ends_with("/responses"))
    );
    app.shutdown_gracefully().await?;
    Ok(())
}

async fn start_scoped_thread(
    app: &mut TestAppServer,
    cwd: &Path,
    scope: &str,
) -> Result<(String, PathBuf)> {
    let request = app
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            cwd: Some(cwd.to_string_lossy().into_owned()),
            config: Some(std::collections::HashMap::from([
                ("memories.scope".to_string(), json!(scope)),
                ("memories.version".to_string(), json!("v2")),
                ("memories.use_memories".to_string(), json!(false)),
            ])),
            ..Default::default()
        })
        .await?;
    let started: ThreadStartResponse = app.read_response(request).await?;
    let memory = read_memory(app, &started.thread.id, true).await?;
    Ok((
        started.thread.id,
        PathBuf::from(memory.root.expect("selected namespace")),
    ))
}

async fn consolidate_wire(
    app: &mut TestAppServer,
    id: &str,
    retry: Option<bool>,
) -> Result<ThreadMemoryConsolidateResponse> {
    let mut params = json!({"threadId":id});
    if let Some(retry) = retry {
        params["retry"] = json!(retry);
    }
    let request = app
        .send_raw_request("thread/memory/consolidate", Some(params))
        .await?;
    app.read_response(request).await
}

#[tokio::test]
async fn memory_consolidation_wire_preserves_disabled_feature_and_generation_gates() -> Result<()> {
    let server = start_mock_server().await;
    let home = TempDir::new()?;
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::MemoryTool)
        .enable_feature(Feature::Sqlite)
        .write(home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .build_initialized()
        .await?;
    let request = app
        .send_thread_start_request_with_auto_env(ThreadStartParams {
            config: Some(std::collections::HashMap::from([(
                "features.memories".to_string(),
                json!(false),
            )])),
            ..Default::default()
        })
        .await?;
    let disabled: ThreadStartResponse = app.read_response(request).await?;
    let result = consolidate_wire(&mut app, &disabled.thread.id, Some(true)).await?;
    assert_eq!(result.status, "blocked");
    assert_eq!(result.reason.as_deref(), Some("memory_disabled"));
    let state = read_memory(&mut app, &disabled.thread.id, true).await?;
    assert_eq!(state.status.as_deref(), Some("disabled"));
    assert_eq!(state.use_memories, Some(false));
    assert!(state.consolidation.is_none());
    let configuration_before_notes = std::fs::read(home.path().join("config.toml"))?;
    for scope in [None, Some("project")] {
        let mut params = json!({"threadId":disabled.thread.id,"note":"must not enable memory"});
        if let Some(scope) = scope {
            params["scope"] = json!(scope);
        }
        let request = app
            .send_raw_request("thread/memory/note/add", Some(params))
            .await?;
        let rejected = app
            .read_stream_until_error_message(codex_app_server_protocol::RequestId::Integer(request))
            .await?;
        assert_eq!(rejected.error.code, -32602);
        assert!(rejected.error.message.contains("memory is disabled"));
    }
    assert_eq!(
        std::fs::read(home.path().join("config.toml"))?,
        configuration_before_notes
    );
    assert!(
        !PathBuf::from(state.root.expect("pinned namespace"))
            .join("extensions/ad_hoc/notes")
            .exists()
    );
    assert!(!home.path().join("project_memories").exists());
    let after_rejection = read_memory(&mut app, &disabled.thread.id, true).await?;
    assert_eq!(after_rejection.use_memories, Some(false));
    assert_eq!(after_rejection.generate_memories, Some(false));

    let (id, root) = start_scoped_thread(&mut app, home.path(), "global").await?;
    let request = app
        .send_thread_memory_mode_set_request(ThreadMemoryModeSetParams {
            thread_id: id.clone(),
            mode: ThreadMemoryMode::Disabled,
        })
        .await?;
    let _: ThreadMemoryModeSetResponse = app.read_response(request).await?;
    let result = consolidate_wire(&mut app, &id, Some(true)).await?;
    assert_eq!(result.status, "blocked");
    assert_eq!(result.reason.as_deref(), Some("generation_disabled"));
    assert_eq!(
        read_memory(&mut app, &id, false).await?.generate_memories,
        Some(false)
    );
    assert!(!root.exists());
    assert!(
        !server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.url.path().ends_with("/responses"))
    );
    app.shutdown_gracefully().await?;
    Ok(())
}

#[tokio::test]
async fn memory_consolidation_wire_reports_cross_process_workspace_busy_without_other_scope_changes()
-> Result<()> {
    let server = start_mock_server().await;
    let home = TempDir::new()?;
    let projects = TempDir::new()?;
    let a = projects.path().join("a");
    let b = projects.path().join("b");
    std::fs::create_dir_all(&a)?;
    std::fs::create_dir_all(&b)?;
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::MemoryTool)
        .enable_feature(Feature::Sqlite)
        .write(home.path())?;
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .build_initialized()
        .await?;
    let (id_a, root_a) = start_scoped_thread(&mut app, &a, "project").await?;
    let (id_b, root_b) = start_scoped_thread(&mut app, &b, "project").await?;
    assert_ne!(root_a, root_b);
    for (id, root, marker) in [(&id_a, &root_a, "a"), (&id_b, &root_b, "b")] {
        let request = app
            .send_raw_request(
                "thread/memory/note/add",
                Some(json!({"threadId":id,"note":format!("synthetic correction {marker}")})),
            )
            .await?;
        let response: ThreadMemoryNoteAddResponse = app.read_response(request).await?;
        assert!(response.pending_consolidation);
        tokio::fs::write(root.join("memory_summary.md"), marker).await?;
    }
    let lock = MemoryWorkspaceLock::try_acquire(&root_a)?;
    let response = consolidate_wire(&mut app, &id_a, Some(true)).await?;
    assert_eq!(response.status, "blocked");
    assert_eq!(response.reason.as_deref(), Some("skipped_workspace_busy"));
    assert!(
        read_memory(&mut app, &id_a, true)
            .await?
            .consolidation
            .is_none()
    );
    assert!(
        read_memory(&mut app, &id_b, true)
            .await?
            .consolidation
            .is_none()
    );
    assert_eq!(
        tokio::fs::read_to_string(root_a.join("memory_summary.md")).await?,
        "a"
    );
    assert_eq!(
        tokio::fs::read_to_string(root_b.join("memory_summary.md")).await?,
        "b"
    );
    assert!(!home.path().join("memories_v2").exists());
    assert!(
        !server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.url.path().ends_with("/responses"))
    );
    drop(lock);
    app.shutdown_gracefully().await?;
    Ok(())
}

#[tokio::test]
async fn memory_consolidation_wire_status_uses_selected_project_and_global_watermarks() -> Result<()>
{
    let server = start_mock_server().await;
    let home = TempDir::new()?;
    let projects = TempDir::new()?;
    let a = projects.path().join("a");
    let b = projects.path().join("b");
    std::fs::create_dir_all(&a)?;
    std::fs::create_dir_all(&b)?;
    MockResponsesConfig::new(&server.uri())
        .enable_feature(Feature::MemoryTool)
        .enable_feature(Feature::Sqlite)
        .write(home.path())?;
    let db = codex_state::StateRuntime::init(
        codex_state::SqliteConfig::new_for_testing(home.path().abs()),
        "mock_provider".into(),
    )
    .await?;
    db.mark_backfill_complete(None).await?;
    let mut app = TestAppServer::builder()
        .with_codex_home(home.path())
        .build_initialized()
        .await?;
    let (id_a, root_a) = start_scoped_thread(&mut app, &a, "project").await?;
    let (id_b, root_b) = start_scoped_thread(&mut app, &b, "project").await?;
    let (id_global, root_global) = start_scoped_thread(&mut app, &a, "global").await?;
    let key_a = root_a
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let key_b = root_b
        .parent()
        .unwrap()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap();
    let store_a = db.memories_for_project(MemoryVersion::V2, key_a).await?;
    let store_b = db.memories_for_project(MemoryVersion::V2, key_b).await?;
    let global = db.memories_for_version(MemoryVersion::V2).await?;
    for (store, watermark, status) in [
        (&store_a, 42, "error"),
        (&store_b, 88, "running"),
        (&global, 99, "done"),
    ] {
        store.enqueue_global_consolidation(watermark).await?;
        let Phase2JobClaimOutcome::Claimed {
            ownership_token,
            input_watermark,
        } = store
            .try_claim_global_phase2_job(ThreadId::new(), 3600)
            .await?
        else {
            panic!("fixture claim")
        };
        if status == "error" {
            assert!(
                store
                    .mark_global_phase2_job_failed(
                        &ownership_token,
                        "failed_invalid_artifacts",
                        3600
                    )
                    .await?
            );
        } else if status == "done" {
            assert!(
                store
                    .mark_global_phase2_job_succeeded(&ownership_token, input_watermark, &[])
                    .await?
            );
        }
    }
    for (id, root, expected_status, watermark) in [
        (&id_a, &root_a, "error", 42),
        (&id_b, &root_b, "running", 88),
        (&id_global, &root_global, "done", 99),
    ] {
        tokio::fs::create_dir_all(root).await?;
        tokio::fs::write(
            root.join("memory_summary.md"),
            format!("selected-{watermark}"),
        )
        .await?;
        let hidden = read_memory(&mut app, id, false).await?;
        assert_eq!(hidden.status.as_deref(), Some("disabled"));
        assert!(hidden.summary.is_none());
        let inspected = read_memory(&mut app, id, true).await?;
        assert_eq!(inspected.root.as_deref(), root.to_str());
        assert_eq!(inspected.summary, Some(format!("selected-{watermark}")));
        let state = inspected.consolidation.expect("selected job");
        assert_eq!(state.status, expected_status);
        assert_eq!(state.input_watermark, Some(watermark));
        assert_eq!(
            state.last_success_watermark,
            Some(if expected_status == "done" { 99 } else { 0 })
        );
        if expected_status == "error" {
            assert_eq!(
                state.last_error.as_deref(),
                Some("failed_invalid_artifacts")
            );
            assert!(state.retry_at.is_some());
        }
    }
    let backoff = consolidate_wire(&mut app, &id_a, None).await?;
    assert_eq!(backoff.status, "blocked");
    assert_eq!(backoff.reason.as_deref(), Some("skipped_retry_unavailable"));
    assert_eq!(
        read_memory(&mut app, &id_b, true)
            .await?
            .consolidation
            .unwrap()
            .input_watermark,
        Some(88)
    );
    assert_eq!(
        read_memory(&mut app, &id_global, true)
            .await?
            .consolidation
            .unwrap()
            .last_success_watermark,
        Some(99)
    );
    assert!(
        !server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.url.path().ends_with("/responses"))
    );
    app.shutdown_gracefully().await?;
    db.close().await;
    Ok(())
}
