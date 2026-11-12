// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
use futures::SinkExt;
use futures::StreamExt;
use pretty_assertions::assert_eq;
use tokio_tungstenite::tungstenite::Message;

use super::parse_version_from_user_agent;

#[tokio::test]
async fn initialize_reports_server_rejection_without_timeout() {
    use tokio_tungstenite::tungstenite::protocol::Role;

    for experimental_api in [false, true] {
        let (client, server) = tokio::io::duplex(8192);
        let mut client =
            tokio_tungstenite::WebSocketStream::from_raw_socket(client, Role::Client, None)
                .await;
        let (release, hold) = tokio::sync::oneshot::channel::<()>();
        let peer =
            tokio::spawn(async move {
                let mut server = tokio_tungstenite::WebSocketStream::from_raw_socket(
                    server,
                    Role::Server,
                    None,
                )
                .await;
                let super::JSONRPCMessage::Request(request) =
                    super::read_message(&mut server).await.unwrap()
                else {
                    panic!("expected initialize request");
                };
                assert_eq!(request.method, "initialize");
                let params: super::InitializeParams =
                    serde_json::from_value(request.params.unwrap()).unwrap();
                assert!(params.supports_arcvera_core_version(env!("CARGO_PKG_VERSION")));
                assert_eq!(
                    params.capabilities.unwrap().experimental_api,
                    experimental_api
                );
                // An unrelated response must not be mistaken for this request's rejection.
                for id in [999, 1] {
                    server.send(Message::Text(serde_json::json!({
                    "id": id,
                    "error": {"code": -32600, "message": "incompatible Core version"}
                }).to_string().into())).await.unwrap();
                }
                // Keep the transport open so EOF cannot disguise the ignored-error bug.
                let _ = hold.await;
            });
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            super::initialize(&mut client, experimental_api),
        )
        .await;
        release.send(()).unwrap();
        peer.await.unwrap();
        let error = result
            .expect("initialize must return before its two-second timeout")
            .expect_err("server rejection must not be accepted");
        assert_eq!(
            error.to_string(),
            "app-server rejected initialize (-32600): incompatible Core version"
        );
    }
}

#[tokio::test]
async fn graceful_shutdown_requires_its_own_ack_and_never_falls_back_to_legacy() {
    for mode in ["legacy", "pidOnlyAck", "graceful"] {
        let temp = tempfile::TempDir::new().unwrap();
        let socket = temp.path().join("control.sock");
        let mut listener = codex_uds::UnixListener::bind(&socket).await.unwrap();
        let pid = std::process::id();
        let peer = tokio::spawn(async move {
            let stream = listener.accept().await.unwrap();
            let mut websocket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let frame = websocket.next().await.unwrap().unwrap();
            assert_eq!(frame, Message::Text(format!("graceful:{pid}").into()));
            if mode == "legacy" {
                // R28/R32 run_daemon_shutdown accepts exactly the PID. An
                // extended request is rejected without ACK or shutdown event.
                let shutdown_event =
                    matches!(frame, Message::Text(request) if request == pid.to_string());
                assert!(!shutdown_event);
                return;
            }
            let ack = if mode == "pidOnlyAck" {
                pid.to_string()
            } else {
                format!("graceful:{pid}")
            };
            websocket.send(Message::Text(ack.into())).await.unwrap();
            let _ = websocket.next().await;
        });
        let result = super::request_shutdown(&socket, pid, true).await;
        assert_eq!(result.is_ok(), mode == "graceful");
        peer.await.unwrap();
    }
}

#[test]
fn parses_version_from_codex_user_agent() {
    assert_eq!(
        parse_version_from_user_agent(
            "codex_app_server_daemon/1.2.3 (Linux 6.8.0; x86_64) codex_cli_rs/1.2.3",
        )
        .expect("version"),
        "1.2.3"
    );
}

#[test]
fn rejects_user_agent_without_version() {
    assert!(parse_version_from_user_agent("codex_app_server_daemon").is_err());
}
