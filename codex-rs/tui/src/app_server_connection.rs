// Modified for OpenDIY Codex R38 (2026-11-06); see MODIFICATIONS.md.
//! Applies the selected server's socket policy on initial connections and reconnects.

use crate::AppServerTarget;
#[cfg(windows)]
use crate::DEFAULT_IN_PROCESS_CHANNEL_CAPACITY;
use crate::connect_remote_app_server;
use codex_app_server_client::AppServerClient;
#[cfg(windows)]
use codex_app_server_client::RemoteAppServerClient;
#[cfg(windows)]
use codex_app_server_client::RemoteAppServerConnectArgs;
#[cfg(windows)]
use codex_app_server_client::RemoteAppServerEndpoint;
#[cfg(windows)]
use codex_utils_absolute_path::AbsolutePathBuf;

pub(crate) async fn connect(target: &AppServerTarget) -> color_eyre::Result<AppServerClient> {
    let client = match target {
        AppServerTarget::Embedded => {
            color_eyre::eyre::bail!("embedded sessions have no remote connection")
        }
        #[cfg(windows)]
        AppServerTarget::LocalDaemon {
            endpoint: RemoteAppServerEndpoint::UnixSocket { socket_path },
            ..
        } => {
            // Revalidate at the real connection, not just the earlier discovery probe.
            let (socket_path, _directory) =
                codex_uds::validate_private_socket_path(socket_path.as_path())?;
            let app_server =
                RemoteAppServerClient::connect_local_daemon(RemoteAppServerConnectArgs {
                    endpoint: RemoteAppServerEndpoint::UnixSocket {
                        socket_path: AbsolutePathBuf::from_absolute_path_checked(socket_path)?,
                    },
                    client_name: "codex-tui".to_string(),
                    client_version: env!("CARGO_PKG_VERSION").to_string(),
                    experimental_api: true,
                    mcp_server_openai_form_elicitation: false,
                    opt_out_notification_methods: Vec::new(),
                    channel_capacity: DEFAULT_IN_PROCESS_CHANNEL_CAPACITY,
                })
                .await?;
            Ok(AppServerClient::Remote(app_server))
        }
        AppServerTarget::LocalDaemon { endpoint, .. } | AppServerTarget::Remote { endpoint } => {
            connect_remote_app_server(endpoint.clone()).await
        }
    }?;
    if matches!(
        target,
        AppServerTarget::LocalDaemon {
            allow_embedded_fallback: true,
            ..
        }
    ) {
        let AppServerClient::Remote(remote) = &client else {
            unreachable!("local daemon connections are remote clients");
        };
        // An implicit choice must use this CLI's full build. Check the actual
        // handshake on every connection; a prior probe/start may race a restart.
        // Explicit endpoints and backend-ID restoration retain their own policy.
        if remote.server_version() != Some(env!("CARGO_PKG_VERSION")) {
            let status = crate::status::remote_connection::remote_connection_status_value(
                target,
                remote.server_version(),
            )
            .expect("local daemon has a connection status");
            let warning = crate::i18n::daemon_connection_mismatch_warning(
                env!("CARGO_PKG_VERSION"),
                remote.server_version(),
                &status.address,
            );
            // Shutdown releases only this client's transport; it sends no
            // app-server lifecycle or thread mutation request.
            if let Err(error) = client.shutdown().await {
                tracing::warn!(%error, "could not close rejected shared server connection");
            }
            color_eyre::eyre::bail!(warning);
        }
    }
    Ok(client)
}
