// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R38 (2026-11-06); see MODIFICATIONS.md.
//! Exit output distinguishes a disconnected client from a stopped embedded server.
//! Persisted sessions offer a direct UUID command and an optional named picker hint.

use super::*;
use crate::RemoteAppServerEndpoint;
use crate::exec_command::escape_command;
use crate::status::remote_connection::sanitized_websocket_url;

/// A persisted thread that can be resumed after the TUI exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumableThread {
    pub thread_id: ThreadId,
    pub thread_name: Option<String>,
}

/// Reconnection and stop guidance for a task owned by a persistent app server.
#[derive(Debug, Clone)]
pub struct DisconnectInfo {
    /// CLI arguments identifying the server, without credential values.
    pub command: Vec<String>,
    pub stop_hint: String,
}

impl App {
    pub(super) fn exit_info(&self, exit_reason: ExitReason) -> AppExitInfo {
        let thread_id = self.chat_widget.thread_id().or(self.primary_thread_id);
        let disconnect_info = thread_id.and_then(|_| {
            let command = match &self.app_server_target {
                AppServerTarget::Embedded => return None,
                AppServerTarget::LocalDaemon { .. } => vec!["codex".to_string()],
                AppServerTarget::Remote { endpoint } => {
                    let address = match endpoint {
                        RemoteAppServerEndpoint::WebSocket { websocket_url, .. } => {
                            sanitized_websocket_url(websocket_url)
                                .and_then(|url| {
                                    let host = url.host()?;
                                    let port = url.port_or_known_default()?;
                                    // The CLI requires an explicit port, including 80/443.
                                    Some(format!("{}://{host}:{port}{}", url.scheme(), url.path()))
                                })
                                .unwrap_or_else(|| "<server-address>".to_string())
                        }
                        RemoteAppServerEndpoint::UnixSocket { socket_path } => {
                            format!("unix://{}", socket_path.display())
                        }
                    };
                    vec!["codex".to_string(), "--remote".to_string(), address]
                }
            };
            let stop_hint = self
                .keymap
                .agents
                .primary_hint("stop", &self.keymap.agents.stop)
                .map_or_else(
                    || crate::i18n::static_message("controls-exit-stop-shortcut").to_string(),
                    |key| {
                        crate::i18n::message(
                            "controls-exit-press-key",
                            &[("value", key.display_label().as_str())],
                        )
                    },
                );
            Some(DisconnectInfo { command, stop_hint })
        });
        AppExitInfo {
            token_usage: self.token_usage(),
            thread_id,
            resume_hint: resumable_thread(
                thread_id,
                self.chat_widget.thread_name(),
                self.chat_widget.rollout_path().as_deref(),
            ),
            disconnect_info,
            update_action: self.pending_update_action,
            exit_reason,
        }
    }
}

impl AppExitInfo {
    /// Format the terminal summary for both the CLI and standalone TUI binaries.
    pub fn format_exit_messages(self, color_enabled: bool) -> Vec<String> {
        let color_command = |command: String| {
            if color_enabled {
                format!("\u{1b}[36m{command}\u{1b}[39m")
            } else {
                command
            }
        };
        let mut lines = Vec::new();
        if let Some(disconnect) = self.disconnect_info
            && let Some(thread_id) = self.thread_id
        {
            let turn_interrupted = matches!(self.exit_reason, ExitReason::TurnInterrupted);
            let message = match self.exit_reason {
                ExitReason::UserRequested => {
                    crate::i18n::static_message("controls-exit-disconnected")
                }
                ExitReason::Fatal(_) => {
                    crate::i18n::static_message("controls-exit-disconnected-fatal")
                }
                ExitReason::TurnInterrupted => {
                    crate::i18n::static_message("controls-exit-disconnected-stopped")
                }
            };
            lines.push(message.to_string());
            let mut resume_command = disconnect.command.clone();
            resume_command.extend(["resume".to_string(), thread_id.to_string()]);
            lines.push(crate::i18n::static_message("controls-exit-reconnect").to_string());
            lines.push(format!(
                "  {}",
                color_command(escape_command(&resume_command)),
            ));
            if !turn_interrupted {
                let mut agents_command = disconnect.command;
                agents_command.push("agents".to_string());
                lines.push(crate::i18n::message(
                    "controls-exit-stop-turn",
                    &[
                        (
                            "command",
                            color_command(escape_command(&agents_command)).as_str(),
                        ),
                        ("hint", disconnect.stop_hint.as_str()),
                    ],
                ));
            }
            if !self.token_usage.is_zero() {
                lines.push(self.token_usage.ui_summary(!turn_interrupted));
            }
            return lines;
        }

        if !self.token_usage.is_zero() {
            lines.push(self.token_usage.ui_summary(false));
        }
        if let Some(thread) = self.resume_hint {
            lines.push(crate::i18n::static_message("controls-exit-continue").to_string());
            lines.push(format!(
                "  {}",
                color_command(format!("arcvera resume {}", thread.thread_id)),
            ));
            if let Some(thread_name) = thread.thread_name.filter(|name| !name.is_empty()) {
                lines.push(crate::i18n::message(
                    "controls-exit-select",
                    &[
                        (
                            "command",
                            color_command("arcvera resume".to_string()).as_str(),
                        ),
                        ("value", color_command(thread_name).as_str()),
                    ],
                ));
            }
        } else if let Some(thread_id) = self.thread_id {
            lines.push(crate::i18n::message(
                "controls-exit-session-id",
                &[("value", thread_id.to_string().as_str())],
            ));
        }
        lines
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;

    #[test]
    fn exit_summary_localizes_guidance_preserving_commands_and_user_title() {
        let thread_id = ThreadId::new();
        let name = "Original English { $value } 한국어 /goal";
        for (locale, heading, named_hint) in [
            (
                crate::i18n::Locale::English,
                "To continue this session, run:",
                format!("Or run codex resume and select {name}."),
            ),
            (
                crate::i18n::Locale::Korean,
                "이 세션을 계속하려면 다음을 실행하세요:",
                format!("또는 codex resume 명령을 실행하고 {name} 항목을 선택하세요."),
            ),
        ] {
            crate::i18n::with_locale(locale, || {
                let info = AppExitInfo {
                    token_usage: Default::default(),
                    thread_id: Some(thread_id),
                    resume_hint: Some(ResumableThread {
                        thread_id,
                        thread_name: Some(name.to_string()),
                    }),
                    disconnect_info: None,
                    update_action: None,
                    exit_reason: ExitReason::UserRequested,
                };
                assert_eq!(
                    info.format_exit_messages(false),
                    vec![
                        heading.to_string(),
                        format!("  codex resume {thread_id}"),
                        named_hint,
                    ]
                );
            });
        }
    }
}
