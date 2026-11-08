//! Request lifetime tests using the existing channel-backed transport and virtual time.
use super::*;
use crate::app_event::AppEvent;
use crate::chatwidget::ConnectorScopeGeneration;
use tokio::sync::mpsc;

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
    assert!(
        matches!(&outcome, ContextInspectionOutcome::Completed(Err(message)) if message.contains("controlled wire failure"))
    );
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
    let outcome = ContextInspectionOutcome::Completed(Err("stale".into()));
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
