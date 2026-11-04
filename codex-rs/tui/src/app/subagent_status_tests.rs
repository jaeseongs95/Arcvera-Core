// Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md.
//! Event-cache-to-terminal regression tests for the root-scoped subagent status row.

use super::*;
use crate::app::test_support::make_test_app;
use crate::render::renderable::Renderable;
use codex_app_server_protocol::CommandExecutionRequestApprovalParams;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ServerRequestResolvedNotification;
use codex_app_server_protocol::ToolRequestUserInputParams;
use codex_app_server_protocol::TurnCompletedNotification;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[tokio::test]
async fn expanded_agent_footer_keeps_draft_and_cursor_during_scroll_and_small_layouts() {
    for count in [15, 100] {
        let mut app = make_test_app().await;
        let root = ThreadId::new();
        app.primary_thread_id = Some(root);
        app.active_thread_id = Some(root);
        app.agent_navigation.bind_status_root(root);
        app.chat_widget.insert_str("draft must survive");
        for index in 0..count {
            app.upsert_agent_picker_thread(
                ThreadId::new(),
                Some(format!("worker-{index:03}")),
                None,
                false,
            );
        }
        app.chat_widget
            .handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        app.chat_widget
            .handle_key_event(KeyEvent::new(KeyCode::PageDown, KeyModifiers::ALT));
        for (width, budget) in [(100, 16), (40, 8), (100, 8)] {
            let view =
                app.chat_widget
                    .bottom_pane_renderable(crate::bottom_pane::ComposerRenderOptions {
                        max_height: Some(budget),
                        ..Default::default()
                    });
            let area = Rect::new(0, 0, width, view.desired_height(width));
            let mut buffer = Buffer::empty(area);
            view.render(area, &mut buffer);
            let text: String = buffer.content().iter().map(|cell| cell.symbol()).collect();
            assert!(
                text.contains("draft must survive"),
                "{count} agents, {width} columns, budget {budget}"
            );
            assert!(
                view.cursor_pos(area)
                    .is_some_and(|(x, y)| x < area.width && y < area.height)
            );
            if width >= 48 && budget >= 12 {
                assert!(text.contains("worker-001"));
                assert!(!text.contains("worker-000"));
                assert!(!text.contains("worker-004"));
            } else {
                assert!(
                    !text.contains("worker-"),
                    "details collapse to preserve a small composer budget"
                );
            }
        }
        app.active_thread_id = app.agent_navigation.ordered_thread_ids().first().copied();
        app.sync_active_agent_label();
        assert!(
            render_bottom(&app).1.contains("worker-001"),
            "same-root agent navigation retains footer scroll"
        );
        app.primary_thread_id = Some(ThreadId::new());
        app.agent_navigation
            .bind_status_root(app.primary_thread_id.unwrap());
        app.sync_active_agent_label();
        assert!(!render_bottom(&app).1.contains("worker-"));
    }
}

fn render_bottom(app: &App) -> (u16, String) {
    let view = app.chat_widget.bottom_pane_renderable(Default::default());
    let width = 200;
    let height = view.desired_height(width);
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    view.render(area, &mut buffer);
    (
        height,
        buffer.content().iter().map(|cell| cell.symbol()).collect(),
    )
}

fn turn(status: TurnStatus) -> Turn {
    Turn {
        id: "turn-status".to_string(),
        root_turn_id: None,
        items: Vec::new(),
        items_view: Default::default(),
        status,
        error: None,
        started_at: None,
        completed_at: None,
        duration_ms: None,
    }
}

#[tokio::test]
async fn subagent_status_events_update_real_bottom_buffer_and_root_switch_clears_it() {
    let mut app = make_test_app().await;
    let root = ThreadId::new();
    let child = ThreadId::new();
    app.primary_thread_id = Some(root);
    app.active_thread_id = Some(root);
    app.agent_navigation.bind_status_root(root);
    app.sync_active_agent_label();
    let (empty_height, empty) = render_bottom(&app);
    assert!(!empty.contains("/subagents"));

    app.upsert_agent_picker_thread(child, None, None, false);
    assert!(render_bottom(&app).1.contains("idle 1"));
    let channel = app.ensure_thread_channel(child);
    let store = Arc::clone(&channel.store);
    store
        .lock()
        .await
        .set_active_turn_id("turn-status".to_string());
    app.refresh_pending_thread_approvals().await;
    assert!(render_bottom(&app).1.contains("running 1"));

    store
        .lock()
        .await
        .push_request(ServerRequest::CommandExecutionRequestApproval {
            request_id: RequestId::Integer(1),
            params: CommandExecutionRequestApprovalParams {
                kind: Default::default(),
                thread_id: child.to_string(),
                turn_id: "turn-status".to_string(),
                item_id: "call".to_string(),
                started_at_ms: 0,
                approval_id: None,
                environment_id: None,
                reason: None,
                network_approval_context: None,
                command: Some("echo status".to_string()),
                cwd: None,
                command_actions: None,
                additional_permissions: None,
                proposed_execpolicy_amendment: None,
                proposed_network_policy_amendments: None,
                available_decisions: None,
            },
        });
    app.refresh_pending_thread_approvals().await;
    assert!(render_bottom(&app).1.contains("approval needed 1"));
    // Viewing the child hides its inactive-approval preview, but must retain its status row.
    app.active_thread_id = Some(child);
    app.refresh_pending_thread_approvals().await;
    assert!(render_bottom(&app).1.contains("approval needed 1"));
    assert!(app.chat_widget.pending_thread_approvals().is_empty());
    store
        .lock()
        .await
        .push_notification(ServerNotification::ServerRequestResolved(
            ServerRequestResolvedNotification {
                thread_id: child.to_string(),
                request_id: RequestId::Integer(1),
            },
        ));
    store
        .lock()
        .await
        .push_request(ServerRequest::ToolRequestUserInput {
            request_id: RequestId::Integer(2),
            params: ToolRequestUserInputParams {
                thread_id: child.to_string(),
                turn_id: "turn-status".to_string(),
                item_id: "question".to_string(),
                questions: Vec::new(),
                is_blocking: true,
                auto_resolution_ms: None,
            },
        });
    app.refresh_pending_thread_approvals().await;
    assert!(render_bottom(&app).1.contains("input needed 1"));
    store
        .lock()
        .await
        .push_notification(ServerNotification::ServerRequestResolved(
            ServerRequestResolvedNotification {
                thread_id: child.to_string(),
                request_id: RequestId::Integer(2),
            },
        ));
    app.refresh_pending_thread_approvals().await;
    assert!(render_bottom(&app).1.contains("running 1"));
    for (status, label) in [
        (TurnStatus::Completed, "completed 1"),
        (TurnStatus::Failed, "failed 1"),
        (TurnStatus::Interrupted, "stopped 1"),
    ] {
        app.enqueue_thread_notification(
            child,
            ServerNotification::TurnCompleted(TurnCompletedNotification {
                thread_id: child.to_string(),
                turn: turn(status),
            }),
        )
        .await
        .expect("route terminal event from the existing event feed");
        assert!(render_bottom(&app).1.contains(label), "{label}");
    }

    let other_root = ThreadId::new();
    app.agent_navigation.bind_status_root(other_root);
    app.primary_thread_id = Some(other_root);
    app.active_thread_id = Some(other_root);
    app.refresh_pending_thread_approvals().await;
    let (new_height, rendered) = render_bottom(&app);
    assert!(!rendered.contains("/subagents"));
    assert_eq!(
        new_height, empty_height,
        "zero-agent root reserves no status-row height"
    );
    // A retained old store can change without reappearing under the new conversation root.
    store
        .lock()
        .await
        .set_active_turn_id("old-root-late-turn".to_string());
    app.refresh_pending_thread_approvals().await;
    assert!(!render_bottom(&app).1.contains("/subagents"));
}

fn selection_summary(
    root: ThreadId,
    ids: &[ThreadId],
) -> crate::bottom_pane::SubagentStatusSummary {
    let mut summary = crate::bottom_pane::SubagentStatusSummary::default();
    summary.root = Some(root);
    for (index, thread_id) in ids.iter().enumerate() {
        summary.add_entry(crate::bottom_pane::SubagentStatusEntry {
            thread_id: *thread_id,
            name: format!("select-worker-{index:02}"),
            state: crate::bottom_pane::SubagentState::Running,
            task: None,
            started_at: None,
            duration_ms: None,
            finished_at: None,
        });
    }
    summary
}

fn paint_widget_footer(widget: &crate::chatwidget::ChatWidget, width: u16) -> Buffer {
    let view = widget.bottom_pane_renderable(Default::default());
    let area = Rect::new(5, 9, width, view.desired_height(width));
    let mut buffer = Buffer::empty(area);
    view.render(area, &mut buffer);
    // Cursor/layout queries after rendering must retain the last painted hit map.
    let _ = widget.cursor_pos(area);
    let _ = widget.desired_height(width);
    buffer
}

#[tokio::test]
async fn footer_selection_public_keys_preserve_draft_and_escape_never_interrupts_running_turn() {
    let (mut widget, _sender, mut events, mut ops) =
        crate::chatwidget::tests::make_chatwidget_manual_with_sender().await;
    let root = ThreadId::new();
    let ids = [ThreadId::new(), ThreadId::new()];
    widget.handle_server_notification(
        ServerNotification::TurnStarted(codex_app_server_protocol::TurnStartedNotification {
            thread_id: root.to_string(),
            turn: turn(TurnStatus::InProgress),
        }),
        None,
    );
    widget.restore_user_message_to_composer("keep selection draft".into());
    widget.set_subagent_status(selection_summary(root, &ids));
    while events.try_recv().is_ok() {}
    while ops.try_recv().is_ok() {}
    widget.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
    paint_widget_footer(&widget, 120);
    widget.handle_key_event(KeyCode::Down.into());
    paint_widget_footer(&widget, 120);
    widget.handle_key_event(KeyCode::Enter.into());
    widget.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Enter,
        KeyModifiers::NONE,
        crossterm::event::KeyEventKind::Repeat,
    ));
    assert_eq!(widget.composer_text_with_pending(), "keep selection draft");
    assert!(
        ops.try_recv().is_err(),
        "selection must not submit or interrupt the foreground turn"
    );
    let selections: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            AppEvent::SelectFooterAgent {
                root_thread_id,
                thread_id,
            } => Some((root_thread_id, thread_id)),
            _ => None,
        })
        .collect();
    assert_eq!(selections, vec![(root, ids[1])]);
    for _ in 0..2 {
        widget.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
    }
    paint_widget_footer(&widget, 120);
    widget.handle_key_event(KeyCode::Esc.into());
    widget.handle_key_event(KeyEvent::new_with_kind(
        KeyCode::Esc,
        KeyModifiers::NONE,
        crossterm::event::KeyEventKind::Repeat,
    ));
    assert_eq!(widget.composer_text_with_pending(), "keep selection draft");
    assert!(
        widget.is_task_running_for_test(),
        "Escape only returns footer focus to input"
    );
    assert!(ops.try_recv().is_err(), "Escape must not enqueue Interrupt");
}

#[tokio::test]
async fn footer_mouse_capture_and_raw_terminal_coordinates_follow_painted_visibility() {
    use crate::tui::MouseCapture;
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    let (mut widget, _sender, mut events, _ops) =
        crate::chatwidget::tests::make_chatwidget_manual_with_sender().await;
    let root = ThreadId::new();
    let ids: Vec<_> = (0..15).map(|_| ThreadId::new()).collect();
    widget.set_subagent_status(selection_summary(root, &ids));
    assert!(matches!(widget.mouse_capture(false), MouseCapture::Inherit));
    paint_widget_footer(&widget, 120);
    assert!(matches!(widget.mouse_capture(false), MouseCapture::Enabled));
    widget.handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
    widget.handle_key_event(KeyEvent::new(KeyCode::PageDown, KeyModifiers::ALT));
    let buffer = paint_widget_footer(&widget, 120);
    let area = *buffer.area();
    let row = (area.y..area.bottom())
        .find(|y| {
            (area.x..area.right())
                .map(|x| buffer[(x, *y)].symbol())
                .collect::<String>()
                .contains("select-worker-02")
        })
        .expect("second visible agent row has actual terminal coordinates");
    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: area.x + 2,
        row,
        modifiers: KeyModifiers::NONE,
    };
    assert!(widget.prepare_composer_mouse(mouse));
    assert!(widget.handle_composer_mouse(mouse));
    let selections: Vec<_> = std::iter::from_fn(|| events.try_recv().ok())
        .filter_map(|event| match event {
            AppEvent::SelectFooterAgent {
                root_thread_id,
                thread_id,
            } => Some((root_thread_id, thread_id)),
            _ => None,
        })
        .collect();
    assert_eq!(selections, vec![(root, ids[2])]);
    widget.show_selection_view(crate::bottom_pane::SelectionViewParams {
        title: Some("Popup keeps input ownership".into()),
        ..Default::default()
    });
    paint_widget_footer(&widget, 120);
    assert!(matches!(widget.mouse_capture(false), MouseCapture::Inherit));
    assert!(!widget.handle_subagent_footer_mouse(mouse));
    widget.set_subagent_status(Default::default());
    assert!(matches!(widget.mouse_capture(false), MouseCapture::Inherit));
    assert!(
        !widget.handle_subagent_footer_mouse(mouse),
        "old footer coordinates cannot select after root teardown"
    );
}

#[tokio::test]
async fn footer_selection_uses_owned_terminal_routing_and_existing_thread_switch_with_stale_guards()
-> color_eyre::Result<()> {
    use crate::app::agent_navigation::AgentPickerThreadVisibility;
    let (mut app, mut events, mut ops) = crate::app::tests::make_test_app_with_channels().await;
    let mut server = crate::start_embedded_app_server_for_picker(&app.config).await?;
    let root = server.start_thread(&app.config).await?;
    let child = server.start_thread(&app.config).await?;
    let root_id = root.session.thread_id;
    let child_id = child.session.thread_id;
    app.enqueue_primary_thread_session(root.session, root.turns)
        .await?;
    app.upsert_agent_picker_thread(child_id, Some("footer-child".into()), None, false);
    app.chat_widget
        .restore_user_message_to_composer("root draft survives visit".into());
    while events.try_recv().is_ok() {}
    while ops.try_recv().is_ok() {}
    let mut tui = crate::tui::test_support::make_test_tui()?;
    tui.set_owned_screen(true)?;
    app.chat_widget
        .handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
    app.render_owned_transcript(&mut tui, ratatui::layout::Size::new(80, 24))?;
    let enter = crate::tui::TuiEvent::Key(KeyCode::Enter.into());
    assert!(
        !app.handle_owned_transcript_event(&mut tui, &mut server, &enter)?,
        "focused footer must receive Enter before history navigation"
    );
    app.handle_tui_event(&mut tui, &mut server, enter).await?;
    let selection = std::iter::from_fn(|| events.try_recv().ok())
        .find(|event| matches!(event, AppEvent::SelectFooterAgent { .. }))
        .expect("real footer selected event");
    assert!(
        matches!(&selection, AppEvent::SelectFooterAgent { root_thread_id, thread_id } if *root_thread_id == root_id && *thread_id == child_id)
    );
    assert!(
        ops.try_recv().is_err(),
        "selection did not submit the composer draft"
    );
    app.handle_event(&mut tui, &mut server, selection).await?;
    assert_eq!(app.active_thread_id, Some(child_id));
    app.select_agent_thread(&mut tui, &mut server, root_id)
        .await?;
    assert_eq!(
        app.chat_widget.composer_text_with_pending(),
        "root draft survives visit"
    );
    for owned in [true, false] {
        use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
        tui.set_owned_screen(owned)?;
        app.chat_widget
            .handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        let buffer = paint_widget_footer(&app.chat_widget, 120);
        let area = *buffer.area();
        let row = (area.y..area.bottom())
            .find(|y| {
                (area.x..area.right())
                    .map(|x| buffer[(x, *y)].symbol())
                    .collect::<String>()
                    .contains("footer-child")
            })
            .expect("painted child row");
        // Owned routing refreshes its terminal geometry before hit testing.
        let (column, row) = if owned {
            app.render_owned_transcript(&mut tui, ratatui::layout::Size::new(80, 24))?;
            let screen = crate::custom_terminal::test_support::last_rendered_buffer(&tui.terminal);
            let screen_area = *screen.area();
            let row = (screen_area.y..screen_area.bottom())
                .find(|y| {
                    (screen_area.x..screen_area.right())
                        .map(|x| screen[(x, *y)].symbol())
                        .collect::<String>()
                        .contains("footer-child")
                })
                .expect("actual owned terminal child row");
            (screen_area.x + 2, row)
        } else {
            (area.x + 2, row)
        };
        while events.try_recv().is_ok() {}
        app.handle_tui_event(
            &mut tui,
            &mut server,
            crate::tui::TuiEvent::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column,
                row,
                modifiers: KeyModifiers::NONE,
            }),
        )
        .await?;
        let selected = std::iter::from_fn(|| events.try_recv().ok())
            .find(|event| matches!(event, AppEvent::SelectFooterAgent { .. }))
            .expect("public terminal mouse route selected a footer row");
        assert!(
            matches!(&selected, AppEvent::SelectFooterAgent { root_thread_id, thread_id } if *root_thread_id == root_id && *thread_id == child_id)
        );
        app.handle_event(&mut tui, &mut server, selected).await?;
        assert_eq!(app.active_thread_id, Some(child_id));
        app.select_agent_thread(&mut tui, &mut server, root_id)
            .await?;
        assert_eq!(
            app.chat_widget.composer_text_with_pending(),
            "root draft survives visit"
        );
    }
    app.chat_widget.apply_external_edit(String::new());
    for owned in [true, false] {
        tui.set_owned_screen(owned)?;
        // Each new pane starts collapsed; the second pass explicitly collapses/re-expands it.
        if !owned {
            app.chat_widget
                .handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        }
        app.chat_widget
            .handle_key_event(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        if owned {
            app.render_owned_transcript(&mut tui, ratatui::layout::Size::new(80, 24))?;
        } else {
            paint_widget_footer(&app.chat_widget, 120);
        }
        assert!(
            app.chat_widget
                .subagent_selection_owns_key(KeyCode::Esc.into())
        );
        app.handle_tui_event(
            &mut tui,
            &mut server,
            crate::tui::TuiEvent::Key(KeyCode::Esc.into()),
        )
        .await?;
        app.handle_tui_event(
            &mut tui,
            &mut server,
            crate::tui::TuiEvent::Key(KeyEvent::new_with_kind(
                KeyCode::Esc,
                KeyModifiers::NONE,
                crossterm::event::KeyEventKind::Repeat,
            )),
        )
        .await?;
        assert!(
            !app.backtrack.primed,
            "footer Escape cannot prime history rewind"
        );
        assert_eq!(app.chat_widget.composer_text_with_pending(), "");
        assert!(
            !app.chat_widget
                .subagent_selection_owns_key(KeyCode::Esc.into())
        );
    }
    app.agent_navigation
        .set_picker_thread_visibility(child_id, AgentPickerThreadVisibility::Hidden);
    app.sync_active_agent_label();
    app.handle_event(
        &mut tui,
        &mut server,
        AppEvent::SelectFooterAgent {
            root_thread_id: root_id,
            thread_id: child_id,
        },
    )
    .await?;
    assert_eq!(
        app.active_thread_id,
        Some(root_id),
        "queued click must not reopen a hidden row"
    );
    app.handle_event(
        &mut tui,
        &mut server,
        AppEvent::SelectFooterAgent {
            root_thread_id: ThreadId::new(),
            thread_id: child_id,
        },
    )
    .await?;
    assert_eq!(
        app.active_thread_id,
        Some(root_id),
        "another root's stale click must be ignored"
    );
    server.shutdown().await?;
    tui.set_owned_screen(false)?;
    Ok(())
}

#[tokio::test]
async fn footer_hover_public_inline_owned_routes_preserve_interaction_and_modal_priority()
-> color_eyre::Result<()> {
    use crossterm::event::{MouseEvent, MouseEventKind};
    use ratatui::style::Modifier;

    fn paint(app: &mut App, tui: &mut crate::tui::Tui, owned: bool) -> color_eyre::Result<Buffer> {
        if owned {
            app.render_owned_transcript(tui, ratatui::layout::Size::new(80, 24))?;
            Ok(crate::custom_terminal::test_support::last_rendered_buffer(&tui.terminal).clone())
        } else {
            Ok(paint_widget_footer(&app.chat_widget, 120))
        }
    }

    fn summary_mouse(buffer: &Buffer) -> MouseEvent {
        let area = *buffer.area();
        let row = (area.y..area.bottom())
            .find(|y| {
                (area.x..area.right())
                    .map(|x| buffer[(x, *y)].symbol())
                    .collect::<String>()
                    .contains("/subagents")
            })
            .expect("actual painted summary row");
        MouseEvent {
            kind: MouseEventKind::Moved,
            column: area.x + 2,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    let (mut app, mut events, mut ops) = crate::app::tests::make_test_app_with_channels().await;
    let mut server = crate::start_embedded_app_server_for_picker(&app.config).await?;
    let root = ThreadId::new();
    let child = ThreadId::new();
    app.transcript_cells
        .push(Arc::new(crate::history_cell::new_user_prompt(
            "hover selection history".into(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )));
    app.chat_widget
        .set_subagent_status(selection_summary(root, &[child]));
    app.chat_widget
        .restore_user_message_to_composer("hover keeps draft".into());
    app.chat_widget.handle_server_notification(
        ServerNotification::TurnStarted(codex_app_server_protocol::TurnStartedNotification {
            thread_id: root.to_string(),
            turn: turn(TurnStatus::InProgress),
        }),
        None,
    );
    while events.try_recv().is_ok() {}
    while ops.try_recv().is_ok() {}
    let mut tui = crate::tui::test_support::make_test_tui()?;
    let mut last_mouse = None;
    for owned in [true, false] {
        tui.set_owned_screen(owned)?;
        app.transcript_view.begin_search();
        let mouse = summary_mouse(&paint(&mut app, &mut tui, owned)?);
        app.handle_tui_event(&mut tui, &mut server, crate::tui::TuiEvent::Mouse(mouse))
            .await?;
        assert!(
            app.transcript_view.is_search_editing(),
            "hover must preserve transcript search"
        );
        assert_eq!(
            app.chat_widget.composer_text_with_pending(),
            "hover keeps draft"
        );
        assert!(
            !app.chat_widget
                .subagent_selection_owns_key(KeyCode::Enter.into())
        );
        let hovered = paint(&mut app, &mut tui, owned)?;
        assert!(
            hovered[(mouse.column, mouse.row)]
                .modifier
                .contains(Modifier::REVERSED)
        );
        app.transcript_view.cancel_search();
        assert!(app.transcript_view.begin_select_mode(&app.transcript_cells));
        let mouse = summary_mouse(&paint(&mut app, &mut tui, owned)?);
        app.handle_tui_event(&mut tui, &mut server, crate::tui::TuiEvent::Mouse(mouse))
            .await?;
        assert!(
            app.transcript_view.is_select_mode(),
            "hover must preserve transcript selection"
        );
        app.transcript_view.end_select_mode();
        app.handle_tui_event(
            &mut tui,
            &mut server,
            crate::tui::TuiEvent::Mouse(MouseEvent {
                row: mouse.row.saturating_sub(1),
                ..mouse
            }),
        )
        .await?;
        let unhovered = paint(&mut app, &mut tui, owned)?;
        assert!(
            !unhovered[(mouse.column, mouse.row)]
                .modifier
                .contains(Modifier::REVERSED)
        );
        last_mouse = Some(mouse);
    }
    app.chat_widget
        .show_selection_view(crate::bottom_pane::SelectionViewParams {
            title: Some("Modal retains mouse priority".into()),
            ..Default::default()
        });
    paint_widget_footer(&app.chat_widget, 120);
    assert!(
        !app.chat_widget
            .handle_subagent_footer_mouse(last_mouse.unwrap())
    );
    assert_eq!(
        app.chat_widget.composer_text_with_pending(),
        "hover keeps draft"
    );
    assert!(
        app.chat_widget.is_task_running_for_test(),
        "hover must not interrupt a turn"
    );
    assert!(ops.try_recv().is_err());
    assert!(
        !std::iter::from_fn(|| events.try_recv().ok())
            .any(|event| matches!(event, AppEvent::SelectFooterAgent { .. }))
    );
    server.shutdown().await?;
    tui.set_owned_screen(false)?;
    Ok(())
}
