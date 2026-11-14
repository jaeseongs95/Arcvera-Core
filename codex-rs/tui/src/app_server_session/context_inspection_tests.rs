// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Request lifetime tests using the existing channel-backed transport and virtual time.
use super::*;
use crate::app_event::AppEvent;
use crate::chatwidget::ConnectorScopeGeneration;
use tokio::sync::mpsc;

fn page(thread: ThreadId, snapshot: &str, next: Option<usize>) -> serde_json::Value {
    serde_json::json!({
        "threadId":thread.to_string(),"snapshotId":snapshot,"contextIndexRevision":1,"historyVersion":2,"asOf":"2026-01-01T00:00:00Z",
        "sizes":{"residentSerializedBytes":0,"selectedResidentSerializedBytes":0,"excludedResidentSerializedBytes":0,"archiveSerializedBytes":null,
            "selectedBodyEstimatedTokens":0,"estimateProvenance":"heuristic","navigationEstimatedTokens":0,"baseEstimatedTokens":null,"toolEstimatedTokens":null,"totalEstimatedTokens":null},
        "limits":{"requestedContextWindowOverride":null,"effectiveContextWindow":null,"provenance":"unknown"},"fragments":[],
        "cache":{"scope":"response","provenance":"not_reported","status":"unknown","revisionStatus":"unknown","ttlSupport":"unknown","ttlApplication":"unknown",
            "minimumValidUntilProvenance":"unknown","minimumValidUntilScope":"unknown"},
        "requestSizes":null,"nextCursor":next
    })
}

#[tokio::test]
async fn context_inspection_returns_without_waiting_and_bounds_one_wire_request() {
    let (client, mut commands, _events) = codex_app_server_client::CloudClient::channel(1);
    let mut session = AppServerSession::new(
        crate::backend_id::BackendId::new(),
        AppServerClient::Cloud(client),
        ThreadParamsMode::Remote,
    );
    let thread = ThreadId::new();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sender = AppEventSender::new(tx);
    assert!(session.start_context_inspection(
        thread,
        ConnectorScopeGeneration::default(),
        sender.clone()
    ));
    let command = commands.recv().await.expect("one context wire request");
    assert!(matches!(&command.operation,
        codex_app_server_client::CloudOperation::Request(request)
            if matches!(request.as_ref(), ClientRequest::ThreadContextRead { params, .. } if params.thread_id == thread.to_string())
    ));
    // The response sender remains unresolved while an unrelated app event progresses.
    sender.interrupt();
    assert!(matches!(rx.recv().await, Some(AppEvent::CodexOp(_))));
    assert!(!session.start_context_inspection(
        ThreadId::new(),
        ConnectorScopeGeneration::default(),
        sender
    ));
    assert!(commands.try_recv().is_err());
    command
        .response
        .send(Err(std::io::Error::other("controlled wire failure")))
        .unwrap();
    let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
        panic!("expected context completion");
    };
    assert!(matches!(
        &outcome,
        ContextInspectionOutcome::Completed(Err(ContextInspectionFailure::Failed))
    ));
    assert!(session.accept_context_inspection(key, &outcome));
    assert!(session.pending_context_inspection.is_none());
}

#[tokio::test(start_paused = true)]
async fn context_inspection_timeout_keeps_slot_until_original_response_drains() {
    let (client, mut commands, _events) = codex_app_server_client::CloudClient::channel(1);
    let mut session = AppServerSession::new(
        crate::backend_id::BackendId::new(),
        AppServerClient::Cloud(client),
        ThreadParamsMode::Remote,
    );
    let thread = ThreadId::new();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sender = AppEventSender::new(tx);
    assert!(session.start_context_inspection(
        thread,
        ConnectorScopeGeneration::default(),
        sender.clone()
    ));
    let command = commands.recv().await.unwrap();
    tokio::time::advance(CONTEXT_INSPECTION_UI_DEADLINE).await;
    let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
        panic!("expected context timeout");
    };
    assert!(matches!(outcome, ContextInspectionOutcome::TimedOut));
    assert!(session.accept_context_inspection(key, &outcome));
    assert!(!session.start_context_inspection(
        thread,
        ConnectorScopeGeneration::default(),
        sender.clone()
    ));
    assert!(commands.try_recv().is_err());
    command
        .response
        .send(Err(std::io::Error::other("late reply")))
        .unwrap();
    let Some(AppEvent::ContextCompositionLoaded {
        key: drained_key,
        outcome,
    }) = rx.recv().await
    else {
        panic!("expected original request drain");
    };
    assert_eq!(drained_key, key);
    assert!(matches!(outcome, ContextInspectionOutcome::Drained));
    assert!(session.accept_context_inspection(drained_key, &outcome));
    assert!(
        rx.try_recv().is_err(),
        "late reply must not also render as a completed result"
    );
    assert!(session.start_context_inspection(thread, ConnectorScopeGeneration::default(), sender));
    session.pending_context_inspection = None;
}

#[tokio::test]
async fn context_inspection_rejects_wrong_owner_request_and_view() {
    let (client, mut commands, _events) = codex_app_server_client::CloudClient::channel(1);
    let mut session = AppServerSession::new(
        crate::backend_id::BackendId::new(),
        AppServerClient::Cloud(client),
        ThreadParamsMode::Remote,
    );
    let mut app = crate::app::test_support::make_test_app().await;
    let generation = app.chat_widget.connector_scope_generation();
    let thread = ThreadId::new();
    let (tx, _rx) = mpsc::unbounded_channel();
    assert!(session.start_context_inspection(thread, generation, AppEventSender::new(tx)));
    let command = commands.recv().await.unwrap();
    let key = session.pending_context_inspection.as_ref().unwrap().key;
    let outcome = ContextInspectionOutcome::Completed(Err(ContextInspectionFailure::Stale));
    let wrong_keys = [
        ContextInspectionKey {
            backend_id: crate::backend_id::BackendId::new(),
            ..key
        },
        ContextInspectionKey {
            connection_id: uuid::Uuid::new_v4(),
            ..key
        },
        ContextInspectionKey {
            request_generation: uuid::Uuid::new_v4(),
            ..key
        },
    ];
    for wrong in wrong_keys {
        assert!(!session.accept_context_inspection(wrong, &outcome));
        assert_eq!(
            session.pending_context_inspection.as_ref().unwrap().key,
            key
        );
    }
    assert!(key.matches_view(Some(thread), generation));
    assert!(!key.matches_view(None, generation));
    assert!(!key.matches_view(Some(ThreadId::new()), generation));
    app.chat_widget.invalidate_connector_scope();
    assert!(!key.matches_view(Some(thread), app.chat_widget.connector_scope_generation()));
    assert!(session.accept_context_inspection(key, &outcome));
    tokio::task::yield_now().await;
    assert!(command.response.is_closed());
}

#[tokio::test]
async fn context_inspection_disconnect_aborts_only_its_pending_task() {
    let (client, mut commands, events) = codex_app_server_client::CloudClient::channel(1);
    let mut session = AppServerSession::new(
        crate::backend_id::BackendId::new(),
        AppServerClient::Cloud(client),
        ThreadParamsMode::Remote,
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    assert!(session.start_context_inspection(
        ThreadId::new(),
        ConnectorScopeGeneration::default(),
        AppEventSender::new(tx)
    ));
    let command = commands.recv().await.unwrap();
    events
        .send(codex_app_server_client::AppServerEvent::Disconnected {
            message: "controlled disconnect".into(),
        })
        .unwrap();
    assert!(session.next_event().await.is_some());
    assert!(session.pending_context_inspection.is_none());
    tokio::task::yield_now().await;
    assert!(command.response.is_closed());
    assert!(rx.try_recv().is_err());
    assert!(commands.try_recv().is_err());
}

#[tokio::test]
async fn cancelled_inspection_keeps_wire_slot_until_response_drains() {
    let (client, mut commands, _events) = codex_app_server_client::CloudClient::channel(1);
    let mut session = AppServerSession::new(
        crate::backend_id::BackendId::new(),
        AppServerClient::Cloud(client),
        ThreadParamsMode::Remote,
    );
    let thread = ThreadId::new();
    let view = ConnectorScopeGeneration::default();
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sender = AppEventSender::new(tx);
    assert!(session.start_context_page(thread, view, sender.clone(), false, 25));
    let command = commands.recv().await.unwrap();
    assert!(session.cancel_context_inspection(thread, view));
    assert!(!session.start_context_page(ThreadId::new(), view, sender, false, 25));
    assert!(!command.response.is_closed());
    command
        .response
        .send(Err(std::io::Error::other("cancelled response")))
        .unwrap();
    let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
        panic!("expected drain");
    };
    assert!(!session.accept_context_inspection(key, &outcome));
    assert!(session.pending_context_inspection.is_none());
}

#[tokio::test]
async fn next_page_requires_matching_view_and_binds_snapshot() {
    let (client, mut commands, _events) = codex_app_server_client::CloudClient::channel(1);
    let mut session = AppServerSession::new(
        crate::backend_id::BackendId::new(),
        AppServerClient::Cloud(client),
        ThreadParamsMode::Remote,
    );
    let thread = ThreadId::new();
    let view = ConnectorScopeGeneration::default();
    session.context_inspection_cursor = Some(ContextInspectionCursor {
        thread_id: thread,
        view_generation: view,
        snapshot_id: "synthetic-snapshot".into(),
        cursor: 25,
        page_limit: 25,
    });
    let (tx, _rx) = mpsc::unbounded_channel();
    assert!(!session.has_context_inspection_next(ThreadId::new(), view));
    assert!(session.has_context_inspection_next(thread, view));
    assert!(session.start_context_page(thread, view, AppEventSender::new(tx), true, 25));
    let command = commands.recv().await.unwrap();
    assert!(
        matches!(&command.operation, codex_app_server_client::CloudOperation::Request(request)
        if matches!(request.as_ref(), ClientRequest::ThreadContextRead { params, .. }
            if params.cursor == 25 && params.limit == Some(25) && params.snapshot_id.as_deref() == Some("synthetic-snapshot")))
    );
}

#[tokio::test]
async fn older_server_method_not_found_is_explicitly_unsupported() {
    let (client, mut commands, _events) = codex_app_server_client::CloudClient::channel(1);
    let mut session = AppServerSession::new(
        crate::backend_id::BackendId::new(),
        AppServerClient::Cloud(client),
        ThreadParamsMode::Remote,
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    assert!(session.start_context_inspection(
        ThreadId::new(),
        ConnectorScopeGeneration::default(),
        AppEventSender::new(tx)
    ));
    let command = commands.recv().await.unwrap();
    command
        .response
        .send(Ok(Err(JSONRPCErrorError {
            code: JSONRPC_METHOD_NOT_FOUND,
            message: "method not found".into(),
            data: None,
        })))
        .unwrap();
    let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
        panic!("expected result")
    };
    assert!(matches!(
        outcome,
        ContextInspectionOutcome::Completed(Err(ContextInspectionFailure::Unsupported))
    ));
    assert!(session.accept_context_inspection(key, &outcome));
}

#[tokio::test]
async fn actual_first_page_sets_cursor_and_mismatched_next_snapshot_is_rejected() {
    let (client, mut commands, _events) = codex_app_server_client::CloudClient::channel(1);
    let mut session = AppServerSession::new(
        crate::backend_id::BackendId::new(),
        AppServerClient::Cloud(client),
        ThreadParamsMode::Remote,
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sender = AppEventSender::new(tx);
    let thread = ThreadId::new();
    let view = ConnectorScopeGeneration::default();
    assert!(session.start_context_inspection(thread, view, sender.clone()));
    commands
        .recv()
        .await
        .unwrap()
        .response
        .send(Ok(Ok(page(thread, "first", Some(25)))))
        .unwrap();
    let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
        panic!("first page")
    };
    assert!(session.accept_context_inspection(key, &outcome));
    assert!(session.has_context_inspection_next(thread, view));
    assert!(session.start_context_page(thread, view, sender, true, 25));
    commands
        .recv()
        .await
        .unwrap()
        .response
        .send(Ok(Ok(page(thread, "wrong", None))))
        .unwrap();
    let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
        panic!("next page")
    };
    assert!(matches!(
        outcome,
        ContextInspectionOutcome::Completed(Err(ContextInspectionFailure::Stale))
    ));
    assert!(session.accept_context_inspection(key, &outcome));
    assert!(!session.has_context_inspection_next(thread, view));
}

#[tokio::test]
async fn oversized_page_is_explicit_and_small_restart_keeps_its_page_limit() {
    let (client, mut commands, _events) = codex_app_server_client::CloudClient::channel(1);
    let mut session = AppServerSession::new(
        crate::backend_id::BackendId::new(),
        AppServerClient::Cloud(client),
        ThreadParamsMode::Remote,
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    let sender = AppEventSender::new(tx);
    let thread = ThreadId::new();
    let view = ConnectorScopeGeneration::default();
    assert!(session.start_context_inspection(thread, view, sender.clone()));
    commands
        .recv()
        .await
        .unwrap()
        .response
        .send(Ok(Err(JSONRPCErrorError {
            code: JSONRPC_INVALID_REQUEST,
            message: "context page exceeds 65536 bytes; request a smaller limit".into(),
            data: None,
        })))
        .unwrap();
    let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
        panic!("oversize")
    };
    assert!(matches!(
        outcome,
        ContextInspectionOutcome::Completed(Err(ContextInspectionFailure::PageTooLarge))
    ));
    assert!(session.accept_context_inspection(key, &outcome));
    assert!(session.start_context_page(thread, view, sender.clone(), false, 1));
    let first = commands.recv().await.unwrap();
    assert!(
        matches!(&first.operation, codex_app_server_client::CloudOperation::Request(request)
        if matches!(request.as_ref(), ClientRequest::ThreadContextRead { params, .. } if params.limit == Some(1) && params.cursor == 0))
    );
    first
        .response
        .send(Ok(Ok(page(thread, "small", Some(1)))))
        .unwrap();
    let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
        panic!("small page")
    };
    assert!(session.accept_context_inspection(key, &outcome));
    assert!(session.start_context_page(thread, view, sender, true, 25));
    let next = commands.recv().await.unwrap();
    assert!(
        matches!(&next.operation, codex_app_server_client::CloudOperation::Request(request)
        if matches!(request.as_ref(), ClientRequest::ThreadContextRead { params, .. }
            if params.limit == Some(1) && params.cursor == 1 && params.snapshot_id.as_deref() == Some("small")))
    );
}

#[tokio::test]
async fn oversize_continuation_does_not_advance_cursor_or_cross_thread() {
    for limit in [25, 1] {
        let (client, mut commands, _events) = codex_app_server_client::CloudClient::channel(1);
        let mut session = AppServerSession::new(
            crate::backend_id::BackendId::new(),
            AppServerClient::Cloud(client),
            ThreadParamsMode::Remote,
        );
        let (tx, mut rx) = mpsc::unbounded_channel();
        let sender = AppEventSender::new(tx);
        let thread = ThreadId::new();
        let view = ConnectorScopeGeneration::default();
        assert!(session.start_context_page(thread, view, sender.clone(), false, limit));
        commands
            .recv()
            .await
            .unwrap()
            .response
            .send(Ok(Ok(page(thread, "same-snapshot", Some(limit)))))
            .unwrap();
        let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
            panic!("initial page")
        };
        assert!(session.accept_context_inspection(key, &outcome));
        assert!(session.start_context_page(thread, view, sender, true, 25));
        let command = commands.recv().await.unwrap();
        assert!(
            matches!(&command.operation, codex_app_server_client::CloudOperation::Request(request)
            if matches!(request.as_ref(), ClientRequest::ThreadContextRead { params, .. } if params.limit == Some(limit) && params.cursor == limit))
        );
        command
            .response
            .send(Ok(Err(JSONRPCErrorError {
                code: JSONRPC_INVALID_REQUEST,
                message: "context page exceeds 65536 bytes; request a smaller limit".into(),
                data: None,
            })))
            .unwrap();
        let Some(AppEvent::ContextCompositionLoaded { key, outcome }) = rx.recv().await else {
            panic!("oversize page")
        };
        if limit == 1 {
            assert!(matches!(
                outcome,
                ContextInspectionOutcome::Completed(Err(
                    ContextInspectionFailure::SingleFragmentTooLarge
                ))
            ));
        } else {
            assert!(matches!(
                outcome,
                ContextInspectionOutcome::Completed(Err(ContextInspectionFailure::PageTooLarge))
            ));
        }
        assert!(session.accept_context_inspection(key, &outcome));
        let cursor = session.context_inspection_cursor.as_ref().unwrap();
        assert_eq!(cursor.cursor, limit);
        assert_eq!(cursor.snapshot_id, "same-snapshot");
        assert!(!session.has_context_inspection_next(ThreadId::new(), view));
        assert!(commands.try_recv().is_err());
    }
}
