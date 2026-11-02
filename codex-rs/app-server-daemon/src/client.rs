use std::path::Path;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use codex_app_server_protocol::ClientInfo;
use codex_app_server_protocol::InitializeCapabilities;
use codex_app_server_protocol::InitializeParams;
use codex_app_server_protocol::InitializeResponse;
use codex_app_server_protocol::JSONRPCMessage;
use codex_app_server_protocol::JSONRPCNotification;
use codex_app_server_protocol::JSONRPCRequest;
use codex_app_server_protocol::RequestId;
use codex_uds::UnixStream;
use futures::SinkExt;
use futures::StreamExt;
use tokio::io::AsyncRead;
use tokio::io::AsyncWrite;
use tokio::time::timeout;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::client_async;
use tokio_tungstenite::tungstenite::Message;

pub(crate) const CONTROL_SOCKET_RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const CLIENT_NAME: &str = "codex_app_server_daemon";
const INITIALIZE_REQUEST_ID: RequestId = RequestId::Integer(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeInfo {
    pub(crate) app_server_version: String,
}

pub(crate) async fn probe(socket_path: &Path) -> Result<ProbeInfo> {
    timeout(CONTROL_SOCKET_RESPONSE_TIMEOUT, probe_inner(socket_path))
        .await
        .with_context(|| {
            format!(
                "timed out probing app-server control socket {}",
                socket_path.display()
            )
        })?
}

async fn probe_inner(socket_path: &Path) -> Result<ProbeInfo> {
    let mut websocket = connect(socket_path).await?;

    let initialize_response = initialize(&mut websocket, /*experimental_api*/ false).await?;
    let initialized = JSONRPCMessage::Notification(JSONRPCNotification {
        method: "initialized".to_string(),
        params: None,
    });
    send_message(&mut websocket, &initialized)
        .await
        .context("failed to send initialized notification")?;
    websocket.close(None).await.ok();

    Ok(ProbeInfo {
        app_server_version: parse_version_from_user_agent(&initialize_response.user_agent)?,
    })
}

pub(crate) async fn connect(socket_path: &Path) -> Result<WebSocketStream<UnixStream>> {
    connect_at(socket_path, "ws://localhost/").await
}

async fn connect_at(socket_path: &Path, url: &str) -> Result<WebSocketStream<UnixStream>> {
    let stream = UnixStream::connect(socket_path)
        .await
        .with_context(|| format!("failed to connect to {}", socket_path.display()))?;
    let (websocket, _response) = client_async(url, stream)
        .await
        .with_context(|| format!("failed to upgrade {}", socket_path.display()))?;
    Ok(websocket)
}

#[cfg(any(windows, test))]
pub(crate) async fn request_shutdown(
    socket_path: &Path,
    pid: u32,
    graceful_only: bool,
) -> Result<()> {
    // Older servers require exactly the PID and reject this extended frame
    // without a shutdown event. Never fall back to their forceable request.
    let request = if graceful_only {
        format!("graceful:{pid}")
    } else {
        pid.to_string()
    };
    timeout(CONTROL_SOCKET_RESPONSE_TIMEOUT, async {
        let mut websocket = connect_at(socket_path, "ws://localhost/daemon/shutdown").await?;
        websocket
            .send(Message::Text(request.clone().into()))
            .await?;
        let reply = websocket
            .next()
            .await
            .context("shutdown socket closed without acknowledgment")??;
        anyhow::ensure!(
            matches!(reply, Message::Text(ack) if ack == request),
            "shutdown acknowledgment did not match the requested shutdown mode and managed process {pid}"
        );
        websocket.close(None).await?;
        Ok::<(), anyhow::Error>(())
    })
    .await
    .context("timed out waiting for managed app-server shutdown acknowledgment")?
    .with_context(|| if graceful_only {
        "graceful-only shutdown was not acknowledged; the server may not support it; no forceable request was sent"
    } else {
        "managed app-server shutdown was not acknowledged"
    })
}

pub(crate) async fn initialize<S>(
    websocket: &mut WebSocketStream<S>,
    experimental_api: bool,
) -> Result<InitializeResponse>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let initialize = JSONRPCMessage::Request(JSONRPCRequest {
        id: INITIALIZE_REQUEST_ID,
        method: "initialize".to_string(),
        params: Some(serde_json::to_value(InitializeParams {
            client_info: ClientInfo {
                name: CLIENT_NAME.to_string(),
                title: Some("Codex App Server Daemon".to_string()),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            capabilities: if experimental_api {
                Some(InitializeCapabilities {
                    experimental_api: true,
                    ..Default::default()
                })
            } else {
                None
            },
        })?),
        trace: None,
    });
    send_message(websocket, &initialize)
        .await
        .context("failed to send initialize request")?;

    let response = loop {
        let message = timeout(CONTROL_SOCKET_RESPONSE_TIMEOUT, read_message(websocket))
            .await
            .context("timed out waiting for initialize response")??;
        if let JSONRPCMessage::Response(response) = message
            && response.id == INITIALIZE_REQUEST_ID
        {
            break response;
        }
    };
    serde_json::from_value::<InitializeResponse>(response.result)
        .context("failed to parse initialize response")
}

pub(crate) async fn send_message<S>(
    websocket: &mut WebSocketStream<S>,
    message: &JSONRPCMessage,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    websocket
        .send(Message::Text(serde_json::to_string(message)?.into()))
        .await?;
    Ok(())
}

pub(crate) async fn read_message<S>(websocket: &mut WebSocketStream<S>) -> Result<JSONRPCMessage>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    loop {
        let frame = websocket
            .next()
            .await
            .ok_or_else(|| anyhow!("app-server closed the control socket"))??;
        let Message::Text(payload) = frame else {
            continue;
        };
        return serde_json::from_str::<JSONRPCMessage>(&payload)
            .context("failed to parse app-server JSON-RPC message");
    }
}

fn parse_version_from_user_agent(user_agent: &str) -> Result<String> {
    let (_originator, rest) = user_agent
        .split_once('/')
        .ok_or_else(|| anyhow!("app-server user-agent omitted version separator"))?;
    let version = rest
        .split_whitespace()
        .next()
        .filter(|version| !version.is_empty())
        .ok_or_else(|| anyhow!("app-server user-agent omitted version"))?;
    Ok(version.to_string())
}

#[cfg(test)]
mod tests {
    use futures::SinkExt;
    use futures::StreamExt;
    use pretty_assertions::assert_eq;
    use tokio_tungstenite::tungstenite::Message;

    use super::parse_version_from_user_agent;

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
}
