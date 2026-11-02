//! Cleanup commands share the same backend selection as other session commands.

use crate::app_server_session::AppServerSession;
use crate::i18n::Locale;
use crate::session_archive_commands::SessionArchiveCommandOptions;
use crate::session_archive_commands::start_app_server_for_session_command;
use codex_app_server_client::TypedRequestError;
use codex_app_server_protocol::ClientRequest;
use codex_app_server_protocol::ThreadCleanupCompletedParams;
use codex_app_server_protocol::ThreadCleanupCompletedResponse;
use codex_app_server_protocol::experimental_required_message;
use color_eyre::Report;
use color_eyre::eyre::Result;
use color_eyre::eyre::eyre;
use std::io::IsTerminal;
use std::io::Write;

const CLEANUP_METHOD: &str = "thread/cleanupCompleted";

pub struct CleanupAgentsCommandOptions {
    pub current: String,
    pub parent: Option<String>,
    pub threads: Vec<String>,
    pub apply: bool,
    pub confirm: bool,
}

pub async fn run_cleanup_agents_command(
    command: CleanupAgentsCommandOptions,
    options: SessionArchiveCommandOptions,
) -> Result<()> {
    let home = codex_utils_home_dir::find_codex_home()?;
    let mut app_server = start_app_server_for_session_command(options, home.to_path_buf()).await?;
    // Shutdown the owned connection on every request/confirmation outcome. A failed
    // request keeps its original error even when connection teardown also fails.
    let result = run_cleanup_agents_action(&mut app_server, command).await;
    let shutdown = app_server.shutdown().await;
    result?;
    shutdown?;
    Ok(())
}

async fn request_cleanup(
    app_server: &mut AppServerSession,
    params: ThreadCleanupCompletedParams,
) -> Result<ThreadCleanupCompletedResponse> {
    let request_id = app_server.next_request_id();
    app_server
        .request_handle()
        .request_typed(ClientRequest::ThreadCleanupCompleted { request_id, params })
        .await
        .map_err(|error| cleanup_request_error(error, crate::i18n::locale()))
}

fn cleanup_request_error(error: TypedRequestError, locale: Locale) -> Report {
    if matches!(
        &error,
        TypedRequestError::Server { method, source }
            if method == CLEANUP_METHOD
                && (source.code == -32601
                    || (source.code == -32600
                        && (source.message.starts_with(
                            "Invalid request: unknown variant `thread/cleanupCompleted`",
                        ) || source.message == experimental_required_message(CLEANUP_METHOD))))
    ) {
        // Legacy parsers list every supported RPC in this rejection. Replace that
        // known compatibility failure without concealing unrelated server errors.
        eyre!(match locale {
            Locale::Korean =>
                "선택한 서버는 서브에이전트 정리 API를 지원하지 않습니다. 삭제한 대화는 0개입니다. 지원하는 서버를 --remote로 명시하거나, 지원하는 서버로 전환한 뒤 다시 사용하세요.",
            Locale::English =>
                "The selected server does not support the subagent cleanup API. No conversations were deleted. Specify a supporting server with --remote, or switch to a supporting server before retrying.",
        })
    } else {
        error.into()
    }
}

async fn run_cleanup_agents_action(
    app_server: &mut AppServerSession,
    command: CleanupAgentsCommandOptions,
) -> Result<()> {
    let parent = command.parent.unwrap_or_else(|| command.current.clone());
    let preview = request_cleanup(
        app_server,
        ThreadCleanupCompletedParams {
            parent_thread_id: parent.clone(),
            current_thread_id: command.current.clone(),
            confirmed: false,
            selection: Vec::new(),
        },
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&preview)?);
    let mut selected = Vec::new();
    for result in preview.results {
        if command.threads.is_empty() || command.threads.contains(&result.thread_id) {
            if let Some(snapshot) = result.snapshot {
                selected.push(snapshot);
            }
        }
    }
    for id in &command.threads {
        if !selected.iter().any(|item| &item.thread_id == id) {
            color_eyre::eyre::bail!("선택한 항목은 종료 상태를 확정할 수 없어 보존됩니다: {id}");
        }
    }
    if !command.apply || selected.is_empty() {
        return Ok(());
    }
    if !command.confirm {
        if !std::io::stdin().is_terminal() {
            color_eyre::eyre::bail!(
                "실제 정리에는 터미널 확인 또는 --apply --confirm이 필요합니다."
            );
        }
        eprint!(
            "위 미리보기에서 선택한 {}개 대화를 영구 삭제하려면 '정리'를 입력하세요: ",
            selected.len()
        );
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if answer.trim() != "정리" {
            return Ok(());
        }
    }
    let result = request_cleanup(
        app_server,
        ThreadCleanupCompletedParams {
            parent_thread_id: parent,
            current_thread_id: command.current,
            confirmed: true,
            selection: selected,
        },
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    if result
        .results
        .iter()
        .any(|entry| entry.outcome != "deleted")
    {
        color_eyre::eyre::bail!(
            "일부 항목을 보존했거나 정리 결과를 확정하지 못했습니다. 출력된 항목별 결과를 확인하세요."
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use codex_app_server_protocol::JSONRPCErrorError;

    fn server_error(method: &str, code: i64, message: &str) -> TypedRequestError {
        TypedRequestError::Server {
            method: method.into(),
            source: JSONRPCErrorError {
                code,
                message: message.into(),
                data: None,
            },
        }
    }

    #[test]
    fn unsupported_cleanup_has_a_concise_message_in_the_selected_language() {
        let experimental = experimental_required_message(CLEANUP_METHOD);
        for locale in [Locale::English, Locale::Korean] {
            for (code, message) in [
                (-32601, "Method not found"),
                (
                    -32600,
                    "Invalid request: unknown variant `thread/cleanupCompleted`, expected `initialize`, `thread/start`, `thread/read`",
                ),
                (-32600, experimental.as_str()),
            ] {
                let error =
                    cleanup_request_error(server_error(CLEANUP_METHOD, code, message), locale);
                let message = error.to_string();
                assert!(message.contains("--remote"));
                assert!(!message.contains("thread/start"));
                assert!(error.downcast_ref::<TypedRequestError>().is_none());
                assert!(message.contains(match locale {
                    Locale::English => "No conversations were deleted",
                    Locale::Korean => "삭제한 대화는 0개",
                }));
            }
        }
    }

    #[test]
    fn cleanup_preserves_other_typed_failures() {
        for (method, code, message) in [
            (CLEANUP_METHOD, -32600, "permission denied"),
            (CLEANUP_METHOD, -32603, "internal error"),
            (
                CLEANUP_METHOD,
                -32600,
                "cleanup requires loaded, locally owned parent and current runtimes",
            ),
            (
                CLEANUP_METHOD,
                -32600,
                "Invalid request: unknown variant `somethingElse`, expected ...",
            ),
            ("thread/read", -32601, "Method not found"),
        ] {
            let error = cleanup_request_error(server_error(method, code, message), Locale::English);
            assert!(matches!(
                error.downcast_ref::<TypedRequestError>(),
                Some(TypedRequestError::Server { method: actual_method, source })
                    if actual_method == method && source.code == code && source.message == message
            ));
        }
        let transport = cleanup_request_error(
            TypedRequestError::Transport {
                method: CLEANUP_METHOD.into(),
                source: std::io::Error::new(
                    std::io::ErrorKind::ConnectionRefused,
                    "connection refused",
                ),
            },
            Locale::Korean,
        );
        assert!(matches!(
            transport.downcast_ref::<TypedRequestError>(),
            Some(TypedRequestError::Transport { .. })
        ));
        let decode = cleanup_request_error(
            TypedRequestError::Deserialize {
                method: CLEANUP_METHOD.into(),
                source: serde_json::from_str::<ThreadCleanupCompletedResponse>("not json")
                    .unwrap_err(),
            },
            Locale::English,
        );
        assert!(matches!(
            decode.downcast_ref::<TypedRequestError>(),
            Some(TypedRequestError::Deserialize { .. })
        ));
    }

    #[tokio::test]
    async fn cleanup_does_not_override_explicit_remote_with_no_daemon() {
        let home = tempfile::tempdir().unwrap();
        let options = SessionArchiveCommandOptions {
            cli: crate::Cli::try_parse_from(["codex", "--no-daemon"]).unwrap(),
            arg0_paths: codex_arg0::Arg0DispatchPaths::default(),
            explicit_remote_endpoint: Some(crate::RemoteAppServerEndpoint::WebSocket {
                websocket_url: "ws://127.0.0.1:12345".into(),
                auth_token: None,
            }),
        };
        let error = match start_app_server_for_session_command(options, home.path().to_path_buf())
            .await
        {
            Ok(_) => panic!("conflicting explicit backend selection must fail before connecting"),
            Err(error) => error,
        };
        assert_eq!(
            error.to_string(),
            "--no-daemon cannot be used with --remote."
        );
    }
}
