// Modified for OpenDIY Codex R36 (2026-11-03); see MODIFICATIONS.md.
use clap::Parser;

pub(crate) const LONG_ABOUT: &str = "미리보기가 기본이며 각 항목의 reason에 정리·보존 사유를 표시합니다. --apply는 미리보기 뒤 명시 확인을 받습니다. 메모리에 없는 과거 종료 항목은 참조 안전성을 입증할 수 없어 reference_participants_unverified로 보존합니다. 해당 항목의 운영 정리 지원은 미완입니다. 실행 중이거나 상태가 불명확한 항목, 다른 세대·원격 소유·이전 방식 항목은 보존합니다. --thread를 반복하면 정확한 선택 집합만 정리하며 선택하지 않은 후손은 보존합니다.";

#[derive(Debug, Parser)]
#[command(
    about = "현재 부모 아래 로컬 2세대 서브에이전트의 종료 대화를 미리 보고 정리합니다.",
    long_about = LONG_ABOUT
)]
pub(crate) struct CleanupAgentsCommand {
    /// 현재 대화 UUID. 생략하면 CODEX_THREAD_ID를 사용합니다.
    #[arg(long, env = "CODEX_THREAD_ID")]
    current: String,
    /// 정리 범위의 부모 UUID. 생략하면 현재 대화입니다.
    #[arg(long)]
    parent: Option<String>,
    /// 정리할 정확한 UUID. 반복할 수 있으며 생략하면 미리보기의 모든 종료 후보입니다.
    #[arg(long = "thread")]
    threads: Vec<String>,
    /// 미리보기 후 확인을 받아 실제로 정리합니다. 생략하면 읽기 전용입니다.
    #[arg(long)]
    apply: bool,
    /// 미리보기의 선택 항목 삭제를 명시적으로 승인합니다.
    #[arg(long, requires = "apply")]
    confirm: bool,
    #[command(flatten)]
    remote: crate::InteractiveRemoteOptions,
}

pub(crate) async fn run(
    command: CleanupAgentsCommand,
    interactive: codex_tui::Cli,
    root_config_overrides: codex_utils_cli::CliConfigOverrides,
    root_remote: Option<String>,
    root_auth: Option<String>,
    arg0_paths: codex_arg0::Arg0DispatchPaths,
) -> anyhow::Result<()> {
    let options = session_options(
        &command,
        interactive,
        root_config_overrides,
        root_remote,
        root_auth,
        arg0_paths,
    )?;
    codex_tui::run_cleanup_agents_command(
        codex_tui::CleanupAgentsCommandOptions {
            current: command.current,
            parent: command.parent,
            threads: command.threads,
            apply: command.apply,
            confirm: command.confirm,
        },
        options,
    )
    .await
    .map_err(|error| anyhow::anyhow!("{error}"))
}

fn session_options(
    command: &CleanupAgentsCommand,
    mut interactive: codex_tui::Cli,
    root_config_overrides: codex_utils_cli::CliConfigOverrides,
    root_remote: Option<String>,
    root_auth: Option<String>,
    arg0_paths: codex_arg0::Arg0DispatchPaths,
) -> anyhow::Result<codex_tui::SessionArchiveCommandOptions> {
    crate::prepend_config_flags(&mut interactive.config_overrides, root_config_overrides);
    Ok(codex_tui::SessionArchiveCommandOptions {
        cli: interactive,
        arg0_paths,
        explicit_remote_endpoint: crate::resolve_remote_endpoint(
            command.remote.remote.clone().or(root_remote),
            command.remote.remote_auth_token_env.clone().or(root_auth),
        )?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_is_default_and_confirmation_requires_apply() {
        let command =
            CleanupAgentsCommand::try_parse_from(["cleanup-agents", "--current", "current"])
                .unwrap();
        assert!(!command.apply);
        assert!(!command.confirm);
        assert!(
            CleanupAgentsCommand::try_parse_from([
                "cleanup-agents",
                "--current",
                "current",
                "--confirm"
            ])
            .is_err()
        );
        let command = CleanupAgentsCommand::try_parse_from([
            "cleanup-agents",
            "--current",
            "current",
            "--apply",
            "--confirm",
            "--thread",
            "one",
            "--thread",
            "two",
        ])
        .unwrap();
        assert_eq!(command.threads, ["one", "two"]);
    }

    #[test]
    fn cleanup_preserves_no_daemon_and_root_configuration() {
        let command =
            CleanupAgentsCommand::try_parse_from(["cleanup-agents", "--current", "current"])
                .unwrap();
        let interactive = codex_tui::Cli::try_parse_from(["codex", "--no-daemon"]).unwrap();
        let arg0_paths = codex_arg0::Arg0DispatchPaths {
            codex_self_exe: Some("matching-cli.exe".into()),
            ..Default::default()
        };
        let options = session_options(
            &command,
            interactive,
            codex_utils_cli::CliConfigOverrides {
                raw_overrides: vec!["model=\"selected\"".into()],
            },
            None,
            None,
            arg0_paths.clone(),
        )
        .unwrap();
        assert!(options.cli.no_daemon);
        assert_eq!(
            options.cli.config_overrides.raw_overrides,
            ["model=\"selected\""]
        );
        assert_eq!(options.arg0_paths, arg0_paths);
        assert!(options.explicit_remote_endpoint.is_none());
    }

    #[test]
    fn cleanup_keeps_the_explicit_remote_instead_of_the_root_endpoint() {
        let command = CleanupAgentsCommand::try_parse_from([
            "cleanup-agents",
            "--current",
            "current",
            "--remote",
            "ws://127.0.0.1:12345",
        ])
        .unwrap();
        let options = session_options(
            &command,
            codex_tui::Cli::try_parse_from(["codex"]).unwrap(),
            codex_utils_cli::CliConfigOverrides::default(),
            Some("ws://127.0.0.1:23456".into()),
            None,
            codex_arg0::Arg0DispatchPaths::default(),
        )
        .unwrap();
        assert_eq!(
            options.explicit_remote_endpoint,
            Some(
                codex_app_server_client::RemoteAppServerEndpoint::WebSocket {
                    websocket_url: "ws://127.0.0.1:12345/".into(),
                    auth_token: None,
                }
            ),
        );
    }
}
