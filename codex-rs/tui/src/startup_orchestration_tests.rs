#[test]
fn automatic_daemon_requires_the_full_known_cli_version() {
    let cli = "0.166.0+native-r37.20261104";
    for server in [Some(cli), Some("0.166.0+native-r28.20261029"), None] {
        let socket = std::path::Path::new("shared server/control.sock");
        let warning = super::automatic_daemon_version_warning(cli, server, socket);
        assert_eq!(warning.is_some(), server != Some(cli));
        if let Some(warning) = warning {
            assert!(warning.contains(cli));
            let unknown = crate::i18n::message("status-unknown-version", &[]);
            assert!(warning.contains(server.unwrap_or(&unknown)));
            assert!(warning.contains(&format!("unix://{}", socket.display())));
        }
    }
}

#[tokio::test]
async fn automatic_daemon_rechecks_replaced_server_after_successful_start() -> color_eyre::Result<()>
{
    use futures::SinkExt;
    use futures::StreamExt;
    use tokio_tungstenite::tungstenite::Message;

    for version in [Some("0.166.0+native-r36.20261104"), None] {
        let home = tempfile::tempdir()?;
        let directory = home.path().join("control");
        codex_uds::prepare_private_socket_directory(&directory).await?;
        let socket_path = directory.join("server.sock");
        let mut listener = codex_uds::UnixListener::bind(&socket_path).await?;
        // start reported this build, but the socket is replaced before the
        // final connection. Exercise the same automatic target construction.
        assert!(
            super::automatic_daemon_version_warning(
                env!("CARGO_PKG_VERSION"),
                Some(env!("CARGO_PKG_VERSION")),
                &socket_path,
            )
            .is_none()
        );
        let target = super::automatic_daemon_target(socket_path)?;
        let server = tokio::spawn(async move {
            let stream = listener.accept().await?;
            let mut socket = tokio_tungstenite::accept_async(stream).await?;
            let Some(Ok(Message::Text(request))) = socket.next().await else {
                panic!("initialize request missing");
            };
            let request: serde_json::Value = serde_json::from_str(&request)?;
            assert_eq!(request["method"], "initialize");
            socket
                .send(Message::Text(
                    serde_json::json!({
                        "id": request["id"],
                        "result": {"userAgent": version.map_or_else(
                            || "daemon-test".to_string(),
                            |version| format!("daemon-test/{version}"),
                        )}
                    })
                    .to_string()
                    .into(),
                ))
                .await?;
            while let Some(Ok(message)) = socket.next().await {
                if let Message::Text(text) = message {
                    let message: serde_json::Value = serde_json::from_str(&text)?;
                    assert_eq!(message["method"], "initialized");
                }
            }
            Ok::<_, color_eyre::Report>(())
        });
        let result = crate::app_server_connection::connect(&target).await;
        assert!(result.is_err(), "a replaced server must not be attached");
        let error = result.err().unwrap().to_string();
        assert!(error.contains("does not match this CLI"), "{error}");
        tokio::time::timeout(std::time::Duration::from_secs(5), server).await???;
    }
    Ok(())
}
