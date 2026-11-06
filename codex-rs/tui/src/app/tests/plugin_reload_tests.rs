// Modified for OpenDIY Codex R38 (2026-11-06); see MODIFICATIONS.md.
//! Targeted reload never retargets a queued command or falls back to global config mutation.

use super::session_lifecycle_requests::{
    HistoryCapabilities, recorded_params, start_recording_app_server_with_history,
};
use super::*;
use crate::app::session_lifecycle::ThreadAttachPresentation;
use crate::app_server_session::ResumeModelSettings;
use crate::app_server_session::ThreadParamsMode;
use pretty_assertions::assert_eq;

#[tokio::test]
async fn reload_plugin_routes_captured_thread_and_unsupported_backend_fails_closed() -> Result<()> {
    for capabilities in [
        HistoryCapabilities::Current,
        HistoryCapabilities::PluginReloadUnsupported,
    ] {
        let (mut app, mut events, _) = make_test_app_with_channels().await;
        let (mut server, requests, proxy) = start_recording_app_server_with_history(
            &app.config,
            capabilities,
            None,
            None,
            ThreadParamsMode::Embedded,
            LoaderOverrides::without_managed_config_for_tests(),
        )
        .await?;
        let started = server.start_thread(&app.config).await?;
        let original = started.session.thread_id;
        app.enqueue_primary_thread_session(started.session, started.turns)
            .await?;
        let command = queued_reload_plugin(&mut app, &mut events);
        app.chat_widget
            .restore_user_message_to_composer("retained draft".into());
        requests.lock().unwrap().clear();
        let mut tui = crate::tui::test_support::make_test_tui()?;
        app.handle_event(&mut tui, &mut server, command).await?;
        let reloads = recorded_params(&requests, "thread/plugin/reload");
        assert_eq!(reloads.len(), 1);
        assert_eq!(reloads[0]["threadId"], original.to_string());
        for forbidden in [
            "config/batchWrite",
            "config/mcpServer/reload",
            "turn/start",
            "turn/steer",
            "turn/interrupt",
        ] {
            assert!(
                recorded_params(&requests, forbidden).is_empty(),
                "{forbidden}"
            );
        }
        assert_eq!(
            app.chat_widget.composer_text_with_pending(),
            "retained draft"
        );
        let history = next_history_message(&mut events);
        if capabilities == HistoryCapabilities::Current {
            assert!(history.contains("MCP reconnect requested"));
            let unwrapped = history.split_whitespace().collect::<Vec<_>>().join(" ");
            assert!(unwrapped.contains("server readiness is not yet confirmed"));
        } else {
            assert!(history.contains("Plugin reload failed"));
            assert!(history.contains("method not found"));
        }
        proxy.abort();
    }
    Ok(())
}

#[test]
fn reload_plugin_messages_localize_without_translating_server_diagnostics() {
    let thread_id = ThreadId::new();
    let diagnostic = r"ignoring additionalContextLimit for Stop hook in C:\fixture\hooks.json: this event cannot emit additionalContext";
    for (locale, requested, readiness, partial, invalid, failed, stale) in [
        (
            crate::i18n::Locale::English,
            "MCP reconnect requested",
            "server readiness is not yet confirmed",
            "Partial refresh warnings:",
            "invalid acknowledgement",
            "Plugin reload failed",
            "Stale plugin reload",
        ),
        (
            crate::i18n::Locale::Korean,
            "MCP 재연결을 요청",
            "서버 준비 완료는 아직 확인하지 않았습니다",
            "일부 새로고침 경고:",
            "원래 대화와 일치하지 않거나 재연결 요청을 확인하지 않았습니다",
            "플러그인 다시 읽기에 실패했습니다",
            "요청이므로 전송하지 않았습니다",
        ),
    ] {
        crate::i18n::with_locale(locale, || {
            let clean = App::plugin_reload_result_message(thread_id, &[]);
            let warning = App::plugin_reload_result_message(thread_id, &[diagnostic.to_string()]);
            for message in [&clean, &warning] {
                assert!(message.contains(&thread_id.to_string()));
                assert!(message.contains(requested));
                assert!(message.contains(readiness));
            }
            assert!(!clean.contains(partial));
            assert!(warning.contains(partial));
            assert!(warning.contains(diagnostic));
            let invalid_ack = crate::i18n::message(
                "cli-plugin-reload-invalid-ack",
                &[("thread", &thread_id.to_string())],
            );
            assert!(invalid_ack.contains(invalid));
            assert!(invalid_ack.contains(&thread_id.to_string()));
            let failure = crate::i18n::message(
                "cli-plugin-reload-failed",
                &[("thread", &thread_id.to_string()), ("error", diagnostic)],
            );
            assert!(failure.contains(failed));
            assert!(failure.contains(diagnostic));
            let stale_message = crate::i18n::message("cli-plugin-reload-stale", &[]);
            assert!(stale_message.contains(stale));
            assert!(stale_message.contains("/reload_plugin"));
        });
    }
}

#[tokio::test]
async fn reload_plugin_reports_unsupported_stop_setting_without_removing_the_hook() -> Result<()> {
    let (mut app, mut events, _) = make_test_app_with_channels().await;
    let owned_home = app._test_codex_home.as_ref().expect("owned fixture home");
    assert_eq!(app.config.codex_home.as_path(), owned_home.path());
    let config_path = app.config.codex_home.join("config.toml");
    let config_bytes = b"[features]\ncodex_hooks = true\n";
    std::fs::write(&config_path, config_bytes)?;
    app.config.features.enable(Feature::CodexHooks)?;
    app.config.config_layer_stack = app.config.config_layer_stack.with_user_config(
        &config_path,
        toml::from_str(std::str::from_utf8(config_bytes)?)?,
    )?;
    let (mut server, requests, proxy) = start_recording_app_server_with_history(
        &app.config,
        HistoryCapabilities::Current,
        None,
        None,
        ThreadParamsMode::Embedded,
        LoaderOverrides::without_managed_config_for_tests(),
    )
    .await?;
    let started = server.start_thread(&app.config).await?;
    let thread_id = started.session.thread_id;
    app.enqueue_primary_thread_session(started.session, started.turns)
        .await?;
    let hooks_path = app.config.codex_home.join("hooks.json");
    let original = serde_json::json!({
        "description": "owned Stop reload fixture",
        "hooks": {
            "Stop": [{"hooks": [{
                "type": "command", "command": "echo stop-fixture",
                "timeout": 17, "statusMessage": "retained status",
                "env": {"FIXTURE_VALUE": "retained"}, "futureField": "retained",
                "additionalContextLimit": 8192,
            }]}],
            "PreToolUse": [{"matcher": "fixture-tool", "hooks": [{
                "type": "command", "command": "echo other-event",
                "additionalContextLimit": 4096,
            }]}],
        },
    });
    let mut corrected = original.clone();
    corrected["hooks"]["Stop"][0]["hooks"][0]
        .as_object_mut()
        .expect("fixture handler")
        .remove("additionalContextLimit");
    let mut first_stop = None;
    let mut tui = crate::tui::test_support::make_test_tui()?;
    requests.lock().unwrap().clear();
    for (document, expect_warning) in [(&original, true), (&corrected, false)] {
        // The test explicitly edits only its fixture; reload must not edit either file.
        let bytes = serde_json::to_vec_pretty(document)?;
        std::fs::write(&hooks_path, &bytes)?;
        let response = server.reload_plugins(thread_id).await?;
        assert_eq!(response.thread_id, thread_id.to_string());
        assert!(response.mcp_reconnect_requested);
        let unsupported = response
            .warnings
            .iter()
            .find(|warning| warning.contains("ignoring additionalContextLimit for Stop hook"));
        assert_eq!(unsupported.is_some(), expect_warning);
        if let Some(warning) = unsupported {
            assert!(warning.contains(&hooks_path.display().to_string()));
            assert!(warning.contains("the Stop hook remains configured"));
            assert!(
                warning.contains("Remove only additionalContextLimit from this Stop hook handler")
            );
        }
        let inventory = crate::hooks_rpc::fetch_hooks_list(
            server.request_handle(),
            &crate::hooks_rpc::HooksListTarget::Thread(thread_id),
        )
        .await?;
        let stop = inventory
            .hooks
            .iter()
            .find(|hook| {
                hook.event_name == codex_app_server_protocol::HookEventName::Stop
                    && hook.source_path.as_path() == hooks_path.as_path()
            })
            .expect("Stop handler is retained after reload");
        assert!(matches!(
            &stop.handler,
            codex_app_server_protocol::HookHandlerMetadata::Command { command, .. }
                if command == "echo stop-fixture"
        ));
        assert!(stop.enabled);
        assert_eq!(stop.timeout_sec, 17);
        assert_eq!(stop.additional_context_limit, None);
        if let Some(first) = &first_stop {
            assert_eq!(
                stop, first,
                "removing an ignored field preserves hook identity"
            );
        } else {
            first_stop = Some(stop.clone());
        }
        let command = queued_reload_plugin(&mut app, &mut events);
        app.handle_event(&mut tui, &mut server, command).await?;
        let cell = std::iter::from_fn(|| events.try_recv().ok())
            .find_map(|event| match event {
                AppEvent::InsertHistoryCell(cell) => Some(cell),
                _ => None,
            })
            .expect("reload result history message");
        // Unlike the shared 80-column helper, this preserves the diagnostic's
        // uninterrupted text while still exercising the actual history cell.
        let history = lines_to_single_string(&cell.display_lines(/*width*/ 4096));
        assert!(history.contains("server readiness is not yet confirmed"));
        assert_eq!(
            history.contains("Partial refresh warnings:"),
            expect_warning
        );
        if let Some(warning) = unsupported {
            assert!(
                history.contains(warning),
                "server diagnostic is not rewritten"
            );
        }
        assert_eq!(std::fs::read(&hooks_path)?, bytes);
        assert_eq!(
            std::fs::read(&config_path)?.as_slice(),
            config_bytes.as_slice()
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes)?,
            *document
        );
    }
    let reloads = recorded_params(&requests, "thread/plugin/reload");
    assert_eq!(reloads.len(), 4);
    assert!(
        reloads
            .iter()
            .all(|params| params["threadId"] == thread_id.to_string())
    );
    for forbidden in ["config/batchWrite", "config/mcpServer/reload", "turn/start"] {
        assert!(
            recorded_params(&requests, forbidden).is_empty(),
            "{forbidden}"
        );
    }
    proxy.abort();
    Ok(())
}

/// Exercise the actual slash parser and event queue, retaining the command across a UI handoff.
fn queued_reload_plugin(
    app: &mut App,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
) -> AppEvent {
    queued_runtime_command(app, events, "/reload_plugin")
}

fn queued_runtime_command(
    app: &mut App,
    events: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>,
    command: &str,
) -> AppEvent {
    while events.try_recv().is_ok() {}
    app.chat_widget.insert_str(command);
    app.chat_widget.handle_key_event(KeyCode::Esc.into());
    app.chat_widget.handle_key_event(KeyCode::Enter.into());
    std::iter::from_fn(|| events.try_recv().ok())
        .find(|event| {
            matches!(
                event,
                AppEvent::SubmitThreadOp {
                    op: Op::ReloadPlugins { .. } | Op::SelectServiceTier { .. },
                    ..
                }
            )
        })
        .expect("slash command enqueues a captured runtime owner")
}

#[tokio::test]
async fn reload_plugin_and_fast_queued_before_screen_or_backend_replacement_sends_zero_rpcs()
-> Result<()> {
    // Same-backend navigation is intentionally rejected too; users can issue a new command.
    for (switch_backend, command, stale_error) in [
        (false, "/reload_plugin", "Stale plugin reload"),
        (true, "/reload_plugin", "Stale plugin reload"),
        (false, "/fast", "Stale service tier selection"),
        (true, "/fast", "Stale service tier selection"),
    ] {
        let (mut app, mut events, _) = make_test_app_with_channels().await;
        let timestamp = "2026-01-01T00-00-00";
        let saved = app_test_support::create_fake_rollout(
            app.config.codex_home.as_path(),
            timestamp,
            "2026-01-01T00:00:00Z",
            "reload owner fixture",
            Some(&app.config.model_provider_id),
            None,
        )
        .expect("saved rollout");
        let id = ThreadId::from_string(&saved)?;
        let other_home = tempdir()?;
        let mut other_config = app.config.clone();
        other_config.codex_home = other_home.path().to_path_buf().abs();
        other_config.sqlite = codex_state::SqliteConfig::new_for_testing(other_home.path().abs());
        // Independent homes avoid shared writer leases while deliberately reusing the native UUID.
        let source =
            app_test_support::rollout_path(app.config.codex_home.as_path(), timestamp, &saved);
        let destination = app_test_support::rollout_path(other_home.path(), timestamp, &saved);
        std::fs::create_dir_all(destination.parent().unwrap())?;
        std::fs::copy(source, destination)?;
        let (mut origin, origin_requests, origin_proxy) = start_recording_app_server_with_history(
            &app.config,
            HistoryCapabilities::Current,
            None,
            None,
            ThreadParamsMode::Embedded,
            LoaderOverrides::without_managed_config_for_tests(),
        )
        .await?;
        let (mut other, other_requests, other_proxy) = start_recording_app_server_with_history(
            &other_config,
            HistoryCapabilities::Current,
            None,
            None,
            ThreadParamsMode::Embedded,
            LoaderOverrides::without_managed_config_for_tests(),
        )
        .await?;
        assert_ne!(origin.backend_id(), other.backend_id());
        let original = origin
            .resume_thread(
                &app.local_settings,
                app.config.clone(),
                id,
                ResumeModelSettings::PreserveExistingThread,
            )
            .await?;
        let mut tui = crate::tui::test_support::make_test_tui()?;
        app.replace_chat_widget_with_app_server_thread(
            &mut tui,
            original,
            ThreadAttachPresentation::SessionLineage,
            None,
        )
        .await?;
        app.chat_widget.set_model("gpt-5.4");
        set_fast_mode_test_catalog(&mut app.chat_widget);
        app.chat_widget.set_feature_enabled(Feature::FastMode, true);
        let owner = app.chat_widget.thread_runtime_generation;
        let queued = queued_runtime_command(&mut app, &mut events, command);
        let replacement = if switch_backend {
            other
                .resume_thread(
                    &crate::local_settings::LocalSettings::from(&other_config),
                    other_config,
                    id,
                    ResumeModelSettings::PreserveExistingThread,
                )
                .await?
        } else {
            origin.start_thread(&app.config).await?
        };
        if switch_backend {
            assert!(
                !replacement.blocks_direct_input,
                "replacement runtime must be writable"
            );
            assert_eq!(
                replacement.session.thread_id, id,
                "both native backends loaded the same UUID"
            );
        }
        app.replace_chat_widget_with_app_server_thread(
            &mut tui,
            replacement,
            ThreadAttachPresentation::SessionLineage,
            None,
        )
        .await?;
        assert_ne!(app.chat_widget.thread_runtime_generation, owner);
        app.chat_widget
            .restore_user_message_to_composer("keep replacement draft".into());
        origin_requests.lock().unwrap().clear();
        other_requests.lock().unwrap().clear();
        while events.try_recv().is_ok() {}
        let displayed_server = if switch_backend {
            &mut other
        } else {
            &mut origin
        };
        app.handle_event(&mut tui, displayed_server, queued).await?;
        assert!(origin_requests.lock().unwrap().is_empty());
        assert!(other_requests.lock().unwrap().is_empty());
        assert_eq!(
            app.chat_widget.composer_text_with_pending(),
            "keep replacement draft"
        );
        let history = next_history_message(&mut events);
        assert!(history.contains(stale_error));
        assert!(!history.contains("Service tier set"));
        assert!(!history.contains("Reloaded plugin configuration"));
        origin_proxy.abort();
        other_proxy.abort();
    }
    Ok(())
}
