// Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md.
use super::*;
use crate::backend_id::BackendId;
use crate::session_control;
use codex_app_server_protocol::{DynamicToolCallParams, DynamicToolCallResponse};
use codex_features::Feature;
use codex_utils_absolute_path::test_support::PathExt;
use std::sync::{Arc, atomic::AtomicU8};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn memory_consolidation_dispatch_is_explicit_and_acceptance_is_not_completion()
-> color_eyre::Result<()> {
    let (mut app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
    let thread = ThreadId::new();
    let (mut server, mut requests, worker) =
        control_server(json!({"status":"accepted","reason":"worker_dispatched"})).await?;
    let result = app
        .session_control_command(
            &mut server,
            thread,
            session_control::Command::MemoryConsolidate { retry: false },
        )
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error))?;
    let (method, params) = next_request(&mut requests).await;
    assert_eq!(method, "thread/memory/consolidate");
    assert_eq!(params["threadId"], thread.to_string());
    assert_eq!(params["retry"], false);
    assert_eq!(result["result"]["status"], "accepted");
    assert!(!result["message"].as_str().unwrap().contains("통합을 완료"));
    server.shutdown().await?;
    worker.await??;
    Ok(())
}

fn request(
    session: &AppServerSession,
    thread: ThreadId,
    call: &str,
    arguments: Value,
) -> (
    crate::cron_tools::Request,
    oneshot::Receiver<DynamicToolCallResponse>,
) {
    let (reply, result) = oneshot::channel();
    (
        crate::cron_tools::Request {
            connection: session.cron_tool_connection(),
            params: DynamicToolCallParams {
                thread_id: thread.to_string(),
                turn_id: "control-turn".into(),
                call_id: call.into(),
                namespace: Some(crate::dynamic_tools::NAMESPACE.into()),
                tool: session_control::NAME.into(),
                arguments: arguments.clone(),
            },
            command: session_control::command(arguments).unwrap(),
            claim: Arc::new(AtomicU8::new(0)),
            cancellation: CancellationToken::new(),
            reply,
        },
        result,
    )
}

#[tokio::test]
async fn model_control_call_preserves_origin_cancellation_and_replay_boundary()
-> color_eyre::Result<()> {
    let (mut app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
    let (mut server, mut observed, worker) =
        super::super::app_server_requests::retained_reply_tests::connection().await?;
    let owner = ThreadId::new();
    app.active_thread_id = Some(owner);
    app.ensure_thread_channel(owner)
        .store
        .lock()
        .await
        .set_active_turn_id("control-turn".into());
    for case in ["cancelled", "stale", "other-screen", "valid", "valid"] {
        let (mut call, response) = request(&server, owner, case, json!({"action":"status"}));
        match case {
            "cancelled" => call.cancellation.cancel(),
            "stale" => call.connection.backend = BackendId::new(),
            "other-screen" => app.active_thread_id = Some(ThreadId::new()),
            _ => app.active_thread_id = Some(owner),
        }
        app.handle_session_control_tool_call(&mut server, call)
            .await;
        if case == "cancelled" {
            assert!(response.await.is_err());
        } else {
            assert_eq!(response.await?.success, case == "valid");
        }
    }
    let (call, response) = request(&server, owner, "valid", json!({"action":"context_reset"}));
    app.handle_session_control_tool_call(&mut server, call)
        .await;
    assert!(
        !response.await?.success,
        "같은 식별자의 다른 요청을 실행하지 않음"
    );
    assert!(
        observed.try_recv().is_err(),
        "거절·상태 조회·동일 호출 재시도는 서버 쓰기를 하지 않음"
    );
    server.shutdown().await?;
    worker.await??;

    let thread = owner;
    let (mut server, mut requests, worker) =
        control_server(json!({"summary":"x".repeat(1024 * 1024)})).await?;
    app.active_thread_id = Some(thread);
    app.ensure_thread_channel(thread)
        .store
        .lock()
        .await
        .set_active_turn_id("control-turn".into());
    for _ in 0..2 {
        let (call, response) = request(
            &server,
            thread,
            "large-read",
            json!({"action":"memory_view"}),
        );
        let key = call.key(thread);
        app.handle_session_control_tool_call(&mut server, call)
            .await;
        assert!(response.await?.success);
        assert_eq!(next_request(&mut requests).await.0, "thread/memory/read");
        assert!(
            serde_json::to_vec(&app.cron.model_calls[&key].1)?.len() < 1024,
            "읽기 재시도는 요약 전체를 호출 기록에 누적하지 않음"
        );
    }
    server.shutdown().await?;
    worker.await??;
    Ok(())
}

#[tokio::test]
async fn native_control_handlers_keep_memory_corrections_and_thread_settings_scoped()
-> color_eyre::Result<()> {
    let (mut app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
    let home = tempfile::tempdir()?;
    app.config.codex_home = home.path().to_path_buf().abs();
    app.config.sqlite = codex_state::SqliteConfig::new_for_testing(home.path().abs());
    app.config.features.enable(Feature::MemoryTool)?;
    app.config.memories.scope = codex_config::types::MemoryScope::Global;
    app.config.memories.use_memories = false;
    app.config.memories.generate_memories = false;
    std::fs::write(
        home.path().join("config.toml"),
        "[features]\nmemories = true\n[memories]\nscope = 'global'\nuse_memories = false\ngenerate_memories = false\n",
    )?;
    let mut server = crate::start_embedded_app_server_for_picker(&app.config).await?;
    let started = server.start_thread(&app.config).await?;
    let thread = started.session.thread_id;
    app.enqueue_primary_thread_session(started.session, started.turns)
        .await?;
    server.thread_inject_items(thread, vec![serde_json::from_value(json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"isolated control fixture"}]}))?]).await?;
    let stored = app
        .session_control_command(&mut server, thread, Command::MemoryView)
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error))?;
    let root = std::path::PathBuf::from(
        stored["memory"]["root"]
            .as_str()
            .expect("local memory root"),
    );
    assert!(
        root.starts_with(home.path()),
        "시험 메모리는 임시 홈 안에 있어야 함"
    );
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join("memory_summary.md"), "기존 메모리 요약\n")?;
    let configuration_before_notes = std::fs::read(home.path().join("config.toml"))?;
    for command in [
        Command::MemoryNote {
            note: "정정할 내용".into(),
            scope: None,
        },
        Command::MemoryForget {
            note: "이전 선호".into(),
            scope: None,
        },
    ] {
        let result = app
            .session_control_command(&mut server, thread, command)
            .await;
        assert!(result.is_ok(), "{result:?}");
    }
    let state_after_notes = server.memory_read(thread).await?;
    assert_eq!(state_after_notes.use_memories, Some(false));
    assert_eq!(state_after_notes.generate_memories, Some(false));
    assert!(!app.config.memories.use_memories);
    assert!(!app.config.memories.generate_memories);
    assert_eq!(
        std::fs::read(home.path().join("config.toml"))?,
        configuration_before_notes
    );
    assert_eq!(
        std::fs::read_to_string(root.join("memory_summary.md"))?,
        "기존 메모리 요약\n"
    );
    let notes = std::fs::read_dir(root.join("extensions/ad_hoc/notes"))?
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(notes.len(), 2);
    assert!(
        notes
            .iter()
            .any(|note| note.contains("Explicit user forget request: 이전 선호"))
    );
    let defaults = std::fs::read(home.path().join("config.toml"))?;
    for command in [
        Command::ContextSet { tokens: 120000 },
        Command::ContextGet,
        Command::ContextReset,
        Command::ServiceTier {
            tier: "default".into(),
        },
        Command::ReloadPlugins,
    ] {
        let result = app
            .session_control_command(&mut server, thread, command)
            .await;
        assert!(result.is_ok(), "{result:?}");
    }
    assert_eq!(
        std::fs::read(home.path().join("config.toml"))?,
        defaults,
        "문맥·속도·다시 읽기는 전역 기본값을 쓰지 않음"
    );
    server.shutdown().await?;
    Ok(())
}

#[test]
fn memory_note_live_call_replay_preserves_independent_requests_in_real_storage()
-> color_eyre::Result<()> {
    const SELECTOR: &str = "app::session_control::tests::memory_note_live_call_replay_preserves_independent_requests_in_real_storage";
    const CHILD: &str = "CODEX_T10_MEMORY_CHILD";
    const FIXTURE_HOME: &str = "CODEX_T10_MEMORY_HOME";
    const TOKEN: &str = "CODEX_T10_MEMORY_TOKEN";
    const OWNER: &str = "t10-process-fixture.owner";
    const PROOF: &str = "t10-process-fixture.result.json";
    const CONFIGURATION: &[u8] = b"[features]\nmemories = true\n[memories]\nscope = 'global'\nuse_memories = false\ngenerate_memories = false\n";

    if std::env::var_os(CHILD).is_none() {
        let home = tempfile::Builder::new()
            .prefix("codex-t10-memory-")
            .tempdir()?;
        let body_result: color_eyre::Result<()> = (|| {
            let token = uuid::Uuid::new_v4().to_string();
            std::fs::write(
                home.path().join(OWNER),
                format!("T10 process fixture v1\n{token}\n"),
            )?;
            std::fs::write(home.path().join("config.toml"), CONFIGURATION)?;
            let mut command = std::process::Command::new(std::env::current_exe()?);
            command
                .args(["--exact", SELECTOR, "--nocapture", "--test-threads=1"])
                .env(CHILD, "child-v1")
                .env(FIXTURE_HOME, home.path())
                .env(TOKEN, token)
                .env("CODEX_HOME", home.path())
                .env("HOME", home.path());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000); // CREATE_NO_WINDOW
            }
            let output = command.output()?;
            for (stream, bytes) in [("stdout", &output.stdout), ("stderr", &output.stderr)] {
                for line in String::from_utf8_lossy(bytes).lines() {
                    eprintln!(
                        "T10 child {stream}: {}",
                        line.replace("test result:", "child libtest result:")
                    );
                }
            }
            color_eyre::eyre::ensure!(
                output.status.success(),
                "T10 child exited abnormally: {}\nstdout:\n{}\nstderr:\n{}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let proof: Value = serde_json::from_slice(&std::fs::read(home.path().join(PROOF))?)?;
            let relative_root: std::path::PathBuf =
                serde_json::from_value(proof["memoryRoot"].clone())?;
            color_eyre::eyre::ensure!(
                !relative_root.as_os_str().is_empty()
                    && relative_root
                        .components()
                        .all(|component| matches!(component, std::path::Component::Normal(_))),
                "invalid fixture memory root"
            );
            let root = home.path().join(relative_root);
            let canonical_home = home.path().canonicalize()?;
            let canonical_root = root.canonicalize()?;
            color_eyre::eyre::ensure!(
                canonical_root != canonical_home && canonical_root.starts_with(&canonical_home),
                "fixture memory root escaped its home"
            );
            let notes = std::fs::read_dir(root.join("extensions/ad_hoc/notes"))?
                .map(|entry| {
                    let path = entry?.path();
                    let bytes = std::fs::read(&path)?;
                    Ok((path, bytes))
                })
                .collect::<std::io::Result<std::collections::BTreeMap<_, _>>>()?;
            color_eyre::eyre::ensure!(
                notes.len() == 3,
                "parent observed {} notes instead of 3",
                notes.len()
            );
            color_eyre::eyre::ensure!(
                notes
                    .values()
                    .filter(|note| note.as_slice()
                        == b"T10 synthetic note A: preserve independent requests.")
                    .count()
                    == 2,
                "parent must observe two separate A files"
            );
            color_eyre::eyre::ensure!(
                notes
                    .values()
                    .filter(|note| note.as_slice()
                        == b"T10 synthetic note B: preserve the later correction.")
                    .count()
                    == 1,
                "parent must observe one B file"
            );
            color_eyre::eyre::ensure!(
                std::fs::read(home.path().join("config.toml"))? == CONFIGURATION,
                "parent observed changed configuration"
            );
            color_eyre::eyre::ensure!(
                !root.join("memory_summary.md").exists(),
                "note receipt must not create a summary"
            );
            color_eyre::eyre::ensure!(
                proof["useMemories"] == false && proof["generateMemories"] == false,
                "read/generate flags changed"
            );
            let responses: Vec<DynamicToolCallResponse> =
                serde_json::from_value(proof["responses"].clone())?;
            color_eyre::eyre::ensure!(
                responses.len() == 4 && proof["responses"][0] == proof["responses"][1],
                "replay receipts differ"
            );
            for response in &responses {
                color_eyre::eyre::ensure!(response.success, "child returned failed note receipt");
                let [
                    codex_app_server_protocol::DynamicToolCallOutputContentItem::InputText { text },
                ] = response.content_items.as_slice()
                else {
                    color_eyre::eyre::bail!("expected actual note receipt text");
                };
                let payload: Value = serde_json::from_str(text)?;
                color_eyre::eyre::ensure!(
                    payload["result"]["pendingConsolidation"] == true
                        && payload["result"]["scope"] == "global"
                        && payload["result"]["root"]
                            .as_str()
                            .is_some_and(|receipt_root| {
                                std::path::Path::new(receipt_root) == root.as_path()
                            }),
                    "parent observed invalid note receipt"
                );
            }
            let conflict: DynamicToolCallResponse =
                serde_json::from_value(proof["conflict"].clone())?;
            color_eyre::eyre::ensure!(!conflict.success, "conflicting call was accepted");
            Ok(())
        })();
        let cleanup_result = home.close().map_err(color_eyre::eyre::Report::from);
        let failures = [
            ("parent/child verification", body_result),
            ("TempDir close", cleanup_result),
        ]
        .into_iter()
        .filter_map(|(phase, result)| result.err().map(|error| format!("{phase}: {error:#}")))
        .collect::<Vec<_>>();
        if !failures.is_empty() {
            color_eyre::eyre::bail!("{}", failures.join("\n"));
        }
        return Ok(());
    }

    color_eyre::eyre::ensure!(
        matches!(std::env::var(CHILD).as_deref(), Ok("child-v1")),
        "invalid child fixture mode"
    );
    let token = std::env::var(TOKEN)?;
    uuid::Uuid::parse_str(&token)?;
    let home_path = std::path::PathBuf::from(
        std::env::var_os(FIXTURE_HOME)
            .ok_or_else(|| color_eyre::eyre::eyre!("missing child fixture home"))?,
    );
    let temporary_base = std::env::temp_dir();
    color_eyre::eyre::ensure!(
        home_path.parent() == Some(temporary_base.as_path())
            && home_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("codex-t10-memory-")),
        "child home is outside the parent temporary fixture boundary"
    );
    color_eyre::eyre::ensure!(
        home_path.canonicalize()?.parent() == Some(temporary_base.canonicalize()?.as_path()),
        "redirected child fixture home"
    );
    color_eyre::eyre::ensure!(
        std::env::var_os("CODEX_HOME").as_deref() == Some(home_path.as_os_str()),
        "child CODEX_HOME does not match the fixture"
    );
    color_eyre::eyre::ensure!(
        std::fs::symlink_metadata(home_path.join(OWNER))?.is_file(),
        "invalid fixture owner marker"
    );
    color_eyre::eyre::ensure!(
        std::fs::read(home_path.join(OWNER))?
            == format!("T10 process fixture v1\n{token}\n").as_bytes(),
        "fixture owner marker mismatch"
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let body_result = runtime.block_on(async {
        let (mut app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
        app.config.codex_home = home_path.as_path().abs();
        app.config.sqlite = codex_state::SqliteConfig::new_for_testing(home_path.as_path().abs());
        app.config.cwd = home_path.as_path().abs();
        app.config.features.enable(Feature::MemoryTool)?;
        app.config.memories.scope = codex_config::types::MemoryScope::Global;
        app.config.memories.project = None;
        app.config.memories.use_memories = false;
        app.config.memories.generate_memories = false;
        let configuration = b"[features]\nmemories = true\n[memories]\nscope = 'global'\nuse_memories = false\ngenerate_memories = false\n";
        assert_eq!(std::fs::read(home_path.join("config.toml"))?, configuration);
        // Retain the same DB owner passed to the server so cleanup can await pool closure.
        let state_db =
            crate::init_state_db_for_app_server_target(&app.config, &crate::AppServerTarget::Embedded)
                .await?
                .expect("embedded fixture state database");
        let mut server = Box::pin(crate::start_app_server_for_picker(
            BackendId::new(),
            &app.config,
            &crate::AppServerTarget::Embedded,
            Vec::new(),
            codex_config::LoaderOverrides::without_managed_config_for_tests(),
            Some(Arc::clone(&state_db)),
            Arc::new(codex_exec_server::EnvironmentManager::default_for_tests()),
        ))
        .await?;
        let started = server.start_thread(&app.config).await?;
        let thread = started.session.thread_id;
        app.enqueue_primary_thread_session(started.session, started.turns)
            .await?;
        server.thread_inject_items(thread, vec![serde_json::from_value(json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"synthetic T10 control fixture"}]}))?]).await?;
        // Only the client-side caller fixture is active; no model turn is started.
        app.active_thread_id = Some(thread);
        app.ensure_thread_channel(thread)
            .store
            .lock()
            .await
            .set_active_turn_id("control-turn".into());
        let before = server.memory_read(thread).await?;
        let root = std::path::PathBuf::from(before.root.expect("actual memory root"));
        assert!(root.starts_with(home_path.as_path()));
        let notes_dir = root.join("extensions/ad_hoc/notes");
        assert!(!notes_dir.exists());
        let read_notes =
            || -> std::io::Result<std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>> {
                std::fs::read_dir(&notes_dir)?
                    .map(|entry| {
                        let path = entry?.path();
                        let bytes = std::fs::read(&path)?;
                        Ok((path, bytes))
                    })
                    .collect()
            };
        let original = "T10 synthetic note A: preserve independent requests.";
        let different = "T10 synthetic note B: preserve the later correction.";
        let mut prior_notes = std::collections::BTreeMap::new();
        let mut responses = Vec::new();
        for (call_id, note, expected_count) in [
            ("note-original", original, 1),
            ("note-original", original, 1),
            ("note-same-body-new", original, 2),
            ("note-different-body-new", different, 3),
        ] {
            // Each delivery owns a fresh Request/claim/reply, including the replay.
            let (call, response) = request(
                &server,
                thread,
                call_id,
                json!({"action":"memory_note","note":note}),
            );
            app.handle_session_control_tool_call(&mut server, call)
                .await;
            let response = response.await?;
            assert!(response.success, "{response:?}");
            let [codex_app_server_protocol::DynamicToolCallOutputContentItem::InputText { text }] =
                response.content_items.as_slice()
            else {
                panic!("expected actual memory note receipt");
            };
            let payload: Value = serde_json::from_str(text)?;
            assert_eq!(payload["result"]["pendingConsolidation"], true);
            assert_eq!(payload["result"]["scope"], "global");
            assert_eq!(payload["result"]["root"].as_str(), root.to_str());
            responses.push(serde_json::to_value(&response)?);
            let notes = read_notes()?;
            assert_eq!(notes.len(), expected_count);
            for (path, bytes) in &prior_notes {
                assert_eq!(notes.get(path), Some(bytes), "an existing note changed");
            }
            assert_eq!(
                notes
                    .values()
                    .filter(|bytes| bytes.as_slice() == original.as_bytes())
                    .count(),
                expected_count.min(2)
            );
            assert_eq!(
                notes
                    .values()
                    .filter(|bytes| bytes.as_slice() == different.as_bytes())
                    .count(),
                usize::from(expected_count == 3)
            );
            prior_notes = notes;
        }
        assert_eq!(
            responses[0], responses[1],
            "replay must retain the original receipt"
        );
        let (conflicting_call, response) = request(
            &server,
            thread,
            "note-original",
            json!({"action":"memory_note","note":"T10 conflicting payload must not be saved."}),
        );
        app.handle_session_control_tool_call(&mut server, conflicting_call)
            .await;
        let conflict = response.await?;
        assert!(
            !conflict.success,
            "same call with different arguments must fail"
        );
        assert_eq!(read_notes()?, prior_notes);
        let after = server.memory_read(thread).await?;
        assert_eq!(after.use_memories, Some(false));
        assert_eq!(after.generate_memories, Some(false));
        assert_eq!(
            std::fs::read(home_path.as_path().join("config.toml"))?,
            configuration
        );
        assert!(!root.join("memory_summary.md").exists());
        app.cron.stop();
        server.shutdown().await?;
        drop(app);
        // Close the retained state DB owner before the child process exits.
        state_db.close().await;
        drop(state_db);
        let proof = json!({
            "memoryRoot": root.strip_prefix(&home_path)?,
            "responses": responses,
            "conflict": conflict,
            "useMemories": after.use_memories,
            "generateMemories": after.generate_memories,
        });
        std::fs::write(home_path.join(PROOF), serde_json::to_vec(&proof)?)?;
        Ok(())
    });
    drop(runtime);
    body_result
}

async fn control_server(
    response: Value,
) -> color_eyre::Result<(
    AppServerSession,
    tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
    tokio::task::JoinHandle<color_eyre::Result<()>>,
)> {
    let (server, requests, worker, _) = control_server_with_notifications(response).await?;
    Ok((server, requests, worker))
}

async fn control_server_with_notifications(
    response: Value,
) -> color_eyre::Result<(
    AppServerSession,
    tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
    tokio::task::JoinHandle<color_eyre::Result<()>>,
    tokio::sync::mpsc::UnboundedSender<Value>,
)> {
    control_server_with_metadata(response, json!({})).await
}

async fn control_server_with_metadata(
    response: Value,
    initialize_metadata: Value,
) -> color_eyre::Result<(
    AppServerSession,
    tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
    tokio::task::JoinHandle<color_eyre::Result<()>>,
    tokio::sync::mpsc::UnboundedSender<Value>,
)> {
    use codex_app_server_protocol::JSONRPCMessage;
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let endpoint = crate::resolve_remote_addr(&format!("ws://{}", listener.local_addr()?))?;
    let (observed, requests) = tokio::sync::mpsc::unbounded_channel();
    let (notifications_tx, mut notifications_rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
    let worker = tokio::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let mut socket = tokio_tungstenite::accept_async(stream).await?;
        loop {
            let frame = tokio::select! {
                frame = socket.next() => match frame { Some(frame) => frame, None => break },
                Some(notification) = notifications_rx.recv() => {
                    socket.send(Message::Text(notification.to_string().into())).await?;
                    continue;
                }
            };
            let Message::Text(text) = frame? else {
                continue;
            };
            let JSONRPCMessage::Request(request) = serde_json::from_str(&text)? else {
                continue;
            };
            let result = if request.method == "initialize" {
                let mut metadata = json!({"userAgent":"control-test/9999","codexHome":std::env::temp_dir(),"platformFamily":std::env::consts::FAMILY,"platformOs":std::env::consts::OS});
                metadata
                    .as_object_mut()
                    .unwrap()
                    .extend(initialize_metadata.as_object().unwrap().clone());
                metadata
            } else {
                let result = response
                    .get(&request.method)
                    .cloned()
                    .unwrap_or_else(|| response.clone());
                observed.send((request.method, request.params.unwrap_or_default()))?;
                result
            };
            socket
                .send(Message::Text(
                    json!({"id":request.id,"result":result}).to_string().into(),
                ))
                .await?;
        }
        Ok(())
    });
    let server = AppServerSession::new(
        BackendId::new(),
        crate::connect_remote_app_server(endpoint).await?,
        crate::app_server_session::ThreadParamsMode::Remote,
    );
    Ok((server, requests, worker, notifications_tx))
}

#[tokio::test]
async fn memory_note_scope_old_server_rejects_different_target_before_any_write()
-> color_eyre::Result<()> {
    for (metadata, pinned_scope) in [
        (json!({}), Some("global")),
        (
            json!({"opendiyCapabilities":{"memoryNoteScope":false}}),
            Some("global"),
        ),
        (
            json!({"opendiyCapabilities":{"memoryNoteScope":"true"}}),
            Some("global"),
        ),
        (json!({}), None),
    ] {
        let (mut app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
        let thread = ThreadId::new();
        let mut read_response = json!({"summary":null,"status":"missing"});
        if let Some(scope) = pinned_scope {
            read_response["scope"] = json!(scope);
        }
        let (mut server, mut requests, worker, _) =
            control_server_with_metadata(read_response, metadata).await?;
        let result = app
            .session_control_command(
                &mut server,
                thread,
                Command::MemoryNote {
                    note: "README 최신화".into(),
                    scope: Some(crate::session_control::Scope::Project),
                },
            )
            .await;
        assert!(result.is_err());
        assert_eq!(next_request(&mut requests).await.0, "thread/memory/read");
        assert!(
            requests.try_recv().is_err(),
            "unsupported scoped note must send zero writes"
        );
        server.shutdown().await?;
        worker.await??;
    }
    Ok(())
}

#[tokio::test]
async fn memory_note_scope_same_legacy_namespace_omits_scope_and_new_support_forwards_target()
-> color_eyre::Result<()> {
    for scoped in [false, true] {
        let (mut app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
        let thread = ThreadId::new();
        let target = if scoped { "project" } else { "global" };
        let metadata = if scoped {
            json!({"opendiyCapabilities":{"memoryNoteScope":true}})
        } else {
            json!({})
        };
        let (mut server, mut requests, worker, _) = control_server_with_metadata(json!({
            "thread/memory/read":{"summary":null,"status":"missing","scope":"global"},
            "thread/memory/note/add":{"pendingConsolidation":true,"scope":target,"root":"fixture-only"}
        }),metadata).await?;
        let result = app
            .session_control_command(
                &mut server,
                thread,
                Command::MemoryNote {
                    note: "explicit note".into(),
                    scope: Some(if scoped {
                        crate::session_control::Scope::Project
                    } else {
                        crate::session_control::Scope::Global
                    }),
                },
            )
            .await
            .map_err(|error| color_eyre::eyre::eyre!(error))?;
        assert_eq!(next_request(&mut requests).await.0, "thread/memory/read");
        let (method, params) = next_request(&mut requests).await;
        assert_eq!(method, "thread/memory/note/add");
        if scoped {
            assert_eq!(params["scope"], "project");
        } else {
            assert!(params.get("scope").is_none());
        }
        assert_eq!(result["result"]["scope"], target);
        assert_eq!(result["result"]["pendingConsolidation"], true);
        assert!(requests.try_recv().is_err());
        server.shutdown().await?;
        worker.await??;
    }
    Ok(())
}

#[tokio::test]
async fn memory_note_scope_control_forwards_project_without_rebinding_global_thread()
-> color_eyre::Result<()> {
    let (mut app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
    let home = tempfile::tempdir()?;
    let project = tempfile::tempdir()?;
    app.config.codex_home = home.path().abs();
    app.config.sqlite = codex_state::SqliteConfig::new_for_testing(home.path().abs());
    app.config.cwd = project.path().abs();
    app.config.active_project.trust_level = Some(codex_protocol::config_types::TrustLevel::Trusted);
    app.config.features.enable(Feature::MemoryTool)?;
    app.config.memories.scope = codex_config::types::MemoryScope::Global;
    app.config.memories.project = None;
    app.config.memories.use_memories = false;
    app.config.memories.generate_memories = false;
    let configuration =
        b"[features]\nmemories = true\n[memories]\nscope = 'global'\nuse_memories = false\ngenerate_memories = false\n";
    std::fs::write(home.path().join("config.toml"), configuration)?;
    let mut server = crate::start_embedded_app_server_for_picker(&app.config).await?;
    let started = server.start_thread(&app.config).await?;
    let thread = started.session.thread_id;
    let before = server.memory_read(thread).await?;
    assert_eq!(before.scope.as_deref(), Some("global"));
    let command = session_control::from_command(
        &session_control::command(json!({
            "action":"memory_note","note":"README 최신화를 잊지 않는다","scope":"project"
        }))
        .unwrap(),
    )
    .unwrap();
    let response = app
        .session_control_command(&mut server, thread, command)
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error))?;
    assert_eq!(response["result"]["scope"], "project");
    assert_eq!(response["result"]["pendingConsolidation"], true);
    let target =
        std::path::PathBuf::from(response["result"]["root"].as_str().expect("actual target"));
    assert!(target.starts_with(home.path().join("project_memories")));
    assert_ne!(
        Some(target.to_string_lossy().as_ref()),
        before.root.as_deref()
    );
    let notes = std::fs::read_dir(target.join("extensions/ad_hoc/notes"))?
        .map(|entry| std::fs::read_to_string(entry.unwrap().path()).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(notes, vec!["README 최신화를 잊지 않는다"]);
    let after = server.memory_read(thread).await?;
    assert_eq!(after.scope.as_deref(), Some("global"));
    assert_eq!(after.root, before.root);
    assert_eq!(after.use_memories, Some(false));
    assert_eq!(after.generate_memories, Some(false));
    assert!(!app.config.memories.use_memories);
    assert!(!app.config.memories.generate_memories);
    assert_eq!(
        app.config.memories.scope,
        codex_config::types::MemoryScope::Global
    );
    assert_eq!(
        std::fs::read(home.path().join("config.toml"))?,
        configuration
    );
    assert!(
        !std::path::PathBuf::from(after.root.unwrap())
            .join("extensions/ad_hoc/notes")
            .exists()
    );
    server.shutdown().await?;
    Ok(())
}

#[test]
fn memory_note_scope_errors_have_english_and_korean_catalog_messages() {
    use crate::i18n::{Locale, message_for};
    for (key, english, korean) in [
        (
            "controls-memory-note-scope-unsupported",
            "This server has not declared support for saving a memory note to a different scope. The note was not saved.",
            "이 서버는 다른 범위를 지정한 메모리 저장을 지원한다고 확인되지 않았습니다. 노트는 저장하지 않았습니다.",
        ),
        (
            "controls-memory-note-scope-unconfirmed",
            "The actual scope of the saved memory note could not be verified. Check the result and do not resend automatically.",
            "메모리 저장 응답의 실제 범위를 확인할 수 없습니다. 결과를 조회하고 자동 재전송하지 마세요.",
        ),
    ] {
        assert_eq!(message_for(Locale::English, key, &[]), english);
        assert_eq!(message_for(Locale::Korean, key, &[]), korean);
    }
}

#[tokio::test]
async fn natural_compact_waits_for_original_turn_and_uses_one_idle_admission()
-> color_eyre::Result<()> {
    for outcome in ["started", "skippedBusy", "notStarted"] {
        let (mut app, mut events, _) = crate::app::tests::make_test_app_with_channels().await;
        let thread = ThreadId::new();
        let (mut server, mut requests, worker, notifications) = control_server_with_notifications(
            json!({"status":outcome,"reason":"fixture rejection","turn":{"id":"compact-turn","items":[],"status":"inProgress","error":null}}),
        )
        .await?;
        app.active_thread_id = Some(thread);
        app.ensure_thread_channel(thread)
            .store
            .lock()
            .await
            .set_active_turn_id("control-turn".into());
        let (call, reply) = request(&server, thread, "compact", json!({"action":"compact"}));
        app.handle_session_control_tool_call(&mut server, call)
            .await;
        let response = reply.await?;
        assert!(response.success);
        let codex_app_server_protocol::DynamicToolCallOutputContentItem::InputText { text } =
            &response.content_items[0]
        else {
            panic!("pending response text")
        };
        assert_eq!(serde_json::from_str::<Value>(text)?["status"], "pending");
        while events.try_recv().is_ok() {}
        assert!(
            requests.try_recv().is_err(),
            "tool acceptance must not interrupt its own turn"
        );
        for (turn, matches_origin) in [
            ("other-turn", false),
            ("control-turn", true),
            ("control-turn", false),
        ] {
            notifications.send(json!({"method":"turn/completed","params":{"threadId":thread.to_string(),"turn":{"id":turn,"items":[],"status":"completed","error":null}}}))?;
            tokio::time::timeout(std::time::Duration::from_secs(5), server.next_event())
                .await?
                .expect("completed notification");
            if matches_origin {
                let (method, params) = next_request(&mut requests).await;
                assert_eq!(method, "thread/compact/startIfIdle");
                assert_eq!(params, json!({"threadId":thread.to_string()}));
                let notice = tokio::time::timeout(std::time::Duration::from_secs(5), events.recv())
                    .await?
                    .expect("admission notice");
                assert!(matches!(
                    notice,
                    crate::app_event::AppEvent::InsertHistoryCell(_)
                ));
            } else {
                assert!(requests.try_recv().is_err());
            }
        }
        assert!(
            requests.try_recv().is_err(),
            "no automatic retry or user turn"
        );
        server.shutdown().await?;
        worker.await??;
    }
    Ok(())
}

async fn next_request(
    requests: &mut tokio::sync::mpsc::UnboundedReceiver<(String, Value)>,
) -> (String, Value) {
    tokio::time::timeout(std::time::Duration::from_secs(5), requests.recv())
        .await
        .expect("제어 요청 관측 시간초과")
        .expect("제어 요청 관측 채널 종료")
}

#[tokio::test]
async fn natural_compact_does_not_run_after_interruption_or_runtime_replacement()
-> color_eyre::Result<()> {
    for status in ["failed", "interrupted", "stale-runtime"] {
        let (app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
        let thread = ThreadId::new();
        let (mut server, mut requests, worker, notifications) =
            control_server_with_notifications(json!({"status":"skippedBusy"})).await?;
        server.compact_after_turn(thread, "control-turn".into(), app.app_event_tx.clone())?;
        assert!(
            server
                .compact_after_turn(thread, "control-turn".into(), app.app_event_tx.clone())
                .is_err()
        );
        if status == "stale-runtime" {
            server.invalidate_runtime_tool_registration().await;
        }
        let completion_status = if status == "stale-runtime" {
            "completed"
        } else {
            status
        };
        notifications.send(json!({"method":"turn/completed","params":{"threadId":thread.to_string(),"turn":{"id":"control-turn","items":[],"status":completion_status,"error":null}}}))?;
        tokio::time::timeout(std::time::Duration::from_secs(5), server.next_event())
            .await?
            .expect("terminal notification");
        assert!(
            requests.try_recv().is_err(),
            "failed, interrupted, or replaced turns must not start compaction"
        );
        server.shutdown().await?;
        worker.await??;
    }
    Ok(())
}

#[tokio::test]
async fn explicit_memory_enable_confirms_runtime_and_preserves_partial_off()
-> color_eyre::Result<()> {
    let (mut app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
    let home = tempfile::tempdir()?;
    app.config.codex_home = home.path().to_path_buf().abs();
    app.config.sqlite = codex_state::SqliteConfig::new_for_testing(home.path().abs());
    app.config.features.disable(Feature::MemoryTool)?;
    app.config.memories.scope = codex_config::types::MemoryScope::Global;
    app.config.memories.use_memories = false;
    app.config.memories.generate_memories = false;
    let file = home.path().join("config.toml");
    let original = "[features]\nmemories = false\n[memories]\nscope = 'global'\nuse_memories = false\ngenerate_memories = false\n";
    std::fs::write(&file, original)?;
    let mut server = crate::start_embedded_app_server_for_picker(&app.config).await?;
    let started = server.start_thread(&app.config).await?;
    let thread = started.session.thread_id;
    server.thread_inject_items(thread, vec![serde_json::from_value(json!({"type":"message","role":"assistant","content":[{"type":"output_text","text":"isolated activation fixture"}]}))?]).await?;
    assert!(
        app.session_control_command(
            &mut server,
            thread,
            Command::MemoryNote {
                note: "must not enable".into(),
                scope: None,
            }
        )
        .await
        .is_err()
    );
    assert_eq!(std::fs::read(&file)?, original.as_bytes());
    let enabled = app
        .session_control_command(&mut server, thread, Command::MemoryEnable { enabled: true })
        .await;
    assert!(enabled.is_ok(), "{enabled:?}");
    let state = server.memory_read(thread).await?;
    assert_eq!(state.use_memories, Some(true));
    assert_eq!(state.generate_memories, Some(true));
    assert_ne!(state.status.as_deref(), Some("disabled"));
    assert!(
        app.chat_widget
            .config_ref()
            .features
            .enabled(Feature::MemoryTool)
    );
    assert!(
        app.session_control_command(
            &mut server,
            thread,
            Command::MemoryNote {
                note: "explicitly retained note".into(),
                scope: None,
            }
        )
        .await
        .is_ok()
    );
    let root = std::path::PathBuf::from(state.root.expect("isolated memory root"));
    assert!(root.starts_with(home.path()));
    assert_eq!(
        std::fs::read_dir(root.join("extensions/ad_hoc/notes"))?.count(),
        1
    );
    assert!(
        !root.join("memory_summary.md").exists(),
        "activation and note acceptance do not claim summary generation"
    );
    server
        .thread_memory_mode_set(
            thread,
            codex_app_server_protocol::ThreadMemoryMode::Disabled,
        )
        .await?;
    assert!(
        app.config.memories.generate_memories,
        "global generation remains enabled"
    );
    let reading = app
        .session_control_command(&mut server, thread, Command::MemoryRead { enabled: true })
        .await;
    assert!(reading.is_ok(), "{reading:?}");
    assert_eq!(
        server.memory_read(thread).await?.generate_memories,
        Some(false),
        "reading changes preserve the thread's explicit generation opt-out"
    );
    for command in [
        Command::MemoryGenerate { enabled: false },
        Command::MemoryRead { enabled: true },
    ] {
        let result = app
            .session_control_command(&mut server, thread, command)
            .await;
        assert!(result.is_ok(), "{result:?}");
    }
    let state = server.memory_read(thread).await?;
    assert_eq!(state.use_memories, Some(true));
    assert_eq!(state.generate_memories, Some(false));
    assert!(!app.config.memories.generate_memories);
    let disabled = app
        .session_control_command(
            &mut server,
            thread,
            Command::MemoryEnable { enabled: false },
        )
        .await;
    assert!(disabled.is_ok(), "{disabled:?}");
    let saved = std::fs::read(&file)?;
    assert!(
        server
            .memory_note_add(thread, "must stay off".into())
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&file)?, saved);
    assert_eq!(
        server.memory_read(thread).await?.status.as_deref(),
        Some("disabled")
    );
    server.shutdown().await?;
    Ok(())
}

#[tokio::test]
async fn control_preview_never_deletes_and_queue_only_settings_do_not_report_success()
-> color_eyre::Result<()> {
    let (mut app, _events, _) = crate::app::tests::make_test_app_with_channels().await;
    let thread = ThreadId::new();
    let (mut server, mut requests, worker) = control_server(json!({"results":[]})).await?;
    let preview = app
        .session_control_command(&mut server, thread, Command::CleanupAgentsPreview)
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error))?;
    assert_eq!(preview["deleted"], false);
    let (method, params) = next_request(&mut requests).await;
    assert_eq!(method, "thread/cleanupCompleted");
    assert_eq!(params["confirmed"], false);
    assert_eq!(params["currentThreadId"], thread.to_string());
    assert_eq!(params["parentThreadId"], thread.to_string());
    assert_eq!(params["selection"], json!([]));
    assert!(requests.try_recv().is_err());
    server.shutdown().await?;
    worker.await??;

    let before = app.chat_widget.configured_service_tier();
    let (mut server, mut requests, worker) = control_server(json!({})).await?;
    for command in [
        Command::ContextSet { tokens: 120000 },
        Command::ServiceTier {
            tier: "default".into(),
        },
    ] {
        assert!(
            app.session_control_command(&mut server, thread, command)
                .await
                .is_err()
        );
        let (method, params) = next_request(&mut requests).await;
        assert_eq!(method, "thread/settings/update");
        assert_eq!(params["threadId"], thread.to_string());
        assert_eq!(app.chat_widget.configured_service_tier(), before);
    }
    server.shutdown().await?;
    worker.await??;
    Ok(())
}
