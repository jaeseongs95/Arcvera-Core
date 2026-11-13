// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
use super::*;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ThreadBackgroundTerminalsListResponse;
use pretty_assertions::assert_eq;

#[test]
fn old_local_daemon_worktree_error_suggests_update() -> Result<()> {
    let target = AppServerTarget::LocalDaemon {
        allow_embedded_fallback: true,
        endpoint: crate::RemoteAppServerEndpoint::UnixSocket {
            socket_path: AbsolutePathBuf::relative_to_current_dir("codex.sock")?,
        },
    };
    let missing_method = |code, message: &str| {
        Err(TypedRequestError::Server {
            method: "thread/backgroundTerminals/list".to_string(),
            source: JSONRPCErrorError {
                code,
                message: message.to_string(),
                data: None,
            },
        })
    };
    let message = managed_worktree_creation::background_terminals_blocker(
        missing_method(-32601, "method not found"),
        &target,
    )
    .expect("unsupported daemon must block worktree creation");
    assert_eq!(
        message,
        "The local Arcvera service cannot check background terminals. After active work finishes, download a complete Arcvera release and run its CLI with `arcvera app-server daemon update --from-cli --yes`, then restart Arcvera."
    );
    assert_eq!(
        managed_worktree_creation::background_terminals_blocker(
            missing_method(
                -32600,
                "Invalid request: unknown variant `thread/backgroundTerminals/list`"
            ),
            &target,
        ),
        Some(message),
    );
    assert_eq!(
        managed_worktree_creation::background_terminals_blocker(
            missing_method(-32600, "Invalid request: unknown variant `another/method`"),
            &target,
        ),
        Some("Active background terminals block /cd."),
    );
    assert_eq!(
        managed_worktree_creation::background_terminals_blocker(
            missing_method(-32601, "method not found"),
            &AppServerTarget::Embedded,
        ),
        Some("Active background terminals block /cd."),
    );
    assert_eq!(
        managed_worktree_creation::background_terminals_blocker(
            Ok(ThreadBackgroundTerminalsListResponse {
                data: Vec::new(),
                next_cursor: None,
            }),
            &target,
        ),
        None,
    );
    Ok(())
}
