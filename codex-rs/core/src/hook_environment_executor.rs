// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Dispatches admitted hooks on the ready environments captured for a lifecycle event.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use codex_exec_server::ExecOutputStream;
use codex_exec_server::ExecParams;
use codex_exec_server::ExecProcess;
use codex_exec_server::ExecProcessEvent;
use codex_exec_server::ProcessId;
use codex_hooks::EnvironmentHookOutput;
use codex_hooks::EnvironmentHookSource;
use codex_hooks::HookEnvironmentExecutor;
use codex_hooks::Hooks;
use codex_network_proxy::NetworkProxy;
use codex_protocol::models::PermissionProfile;
use futures::FutureExt;
use futures::future::BoxFuture;

use crate::environment_selection::TurnEnvironmentSnapshot;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;
use crate::session::turn_context::TurnEnvironment;
use crate::shell::ShellType;
use crate::tools::network_approval::execution_network_proxy;
use crate::tools::network_approval::remote_network_proxy_launch;
use crate::tools::sandboxing::ToolError;
use crate::unified_exec::exec_env_policy_from_shell_policy;

const MAX_HOOK_OUTPUT_BYTES: usize = 1024 * 1024;

pub(crate) fn with_environment_hooks(
    session: &Arc<Session>,
    turn: &Arc<TurnContext>,
    environments: &TurnEnvironmentSnapshot,
) -> Hooks {
    with_tool_environment_hooks(
        session,
        turn,
        environments,
        /*target_environments*/ &[],
    )
}

pub(crate) fn with_tool_environment_hooks(
    session: &Arc<Session>,
    turn: &Arc<TurnContext>,
    environments: &TurnEnvironmentSnapshot,
    target_environments: &[&str],
) -> Hooks {
    session.hooks().with_environment_hooks(
        environment_hook_sources(environments, target_environments),
        Arc::new(EnvironmentCommandExecutor {
            environments: environments.clone(),
            session: Arc::clone(session),
            turn: Arc::clone(turn),
        }),
    )
}

pub(crate) fn environment_hook_sources(
    environments: &TurnEnvironmentSnapshot,
    target_environments: &[&str],
) -> Vec<EnvironmentHookSource> {
    environments
        .turn_environments()
        .filter(|environment| {
            !environment.config().environment_hooks.is_empty()
                && (target_environments.is_empty()
                    || target_environments.contains(&environment.selection.environment_id.as_str()))
        })
        .map(|environment| EnvironmentHookSource {
            environment_id: environment.selection.environment_id.clone(),
            cwd: environment.cwd().clone(),
            hooks: environment.config().environment_hooks.clone(),
        })
        .collect()
}

struct EnvironmentCommandExecutor {
    environments: TurnEnvironmentSnapshot,
    session: Arc<Session>,
    turn: Arc<TurnContext>,
}

impl EnvironmentCommandExecutor {
    async fn launch_params(
        &self,
        environment: &TurnEnvironment,
        argv: Vec<String>,
        env: HashMap<String, String>,
    ) -> Result<(ExecParams, Option<NetworkProxy>), ToolError> {
        let process_id = ProcessId::from(format!("hook-{}", uuid::Uuid::new_v4()));
        let network = execution_network_proxy(
            &self.session,
            &self.turn,
            Some(environment),
            self.turn.network.clone(),
            &environment.selection.environment_id,
            environment.permission_profile(),
            environment.config().network_policy.as_ref(),
            process_id.as_str(),
            /*fallback_policy_decider*/ None,
        )
        .await?;
        let network_proxy = if let Some(network) = &network {
            let (launch, _) = remote_network_proxy_launch(environment, network, &self.turn).await?;
            launch.proxy.enabled.then_some(launch)
        } else {
            None
        };
        let sandbox = network_proxy.as_ref().map(|_| {
            // Enforce managed network policy with the hook owner's filesystem access.
            let mut sandbox = environment.sandbox_context(/*additional_permissions*/ None);
            sandbox.permissions = PermissionProfile::Disabled;
            sandbox
        });
        let params = ExecParams {
            process_id,
            metadata: None,
            argv,
            cwd: environment.cwd().clone(),
            env_policy: Some(exec_env_policy_from_shell_policy(
                environment.shell_environment_policy(),
            )),
            shell_snapshot: None,
            env,
            tty: false,
            pipe_stdin: true,
            arg0: None,
            enforce_managed_network: sandbox.is_some(),
            sandbox,
            managed_network: None,
            network_proxy,
        };
        Ok((params, network))
    }
}

struct HookProcessGuard(Option<Arc<dyn ExecProcess>>);

impl Drop for HookProcessGuard {
    fn drop(&mut self) {
        // Don't let waiting for termination delay a hook timeout or error.
        if let Some(process) = self.0.take() {
            tokio::spawn(async move {
                let _ = process.terminate().await;
            });
        }
    }
}

impl HookEnvironmentExecutor for EnvironmentCommandExecutor {
    fn execute(
        &self,
        environment_id: &str,
        command: &str,
        env: &HashMap<String, String>,
        input_json: &str,
        timeout: Duration,
    ) -> BoxFuture<'_, anyhow::Result<EnvironmentHookOutput>> {
        let environment_id = environment_id.to_owned();
        let command = command.to_owned();
        let env = env.clone();
        let input_json = input_json.to_owned();
        async move {
            let environment = self
                .environments
                .turn_environments()
                .find(|environment| environment.selection.environment_id == environment_id)
                .context("hook environment was not ready for this event")?;
            let shell = environment
                .shell
                .as_ref()
                .context("hook environment did not report a shell")?;

            // Exec-server keeps pipe stdin open and older executors have no close-stdin RPC.
            // JSON is one line: read precisely that line, then give the hook a pipe that reaches EOF.
            let input_json = format!("{input_json}\n").into_bytes();
            let script = match shell.shell_type {
                // Start the hook on a new line so a leading `(` doesn't become Bash's arithmetic `((`.
                ShellType::Sh | ShellType::Bash | ShellType::Zsh => format!(
                    "IFS= read -r codex_hook_input; printf '%s' \"$codex_hook_input\" | (\n{command}\n)"
                ),
                // A script block can reset `$?`. Run the hook at the top level and preserve its
                // exit code before PowerShell's -Command reduces it to 0 or 1.
                ShellType::PowerShell => format!(
                    "$global:LASTEXITCODE = 0\n\
                     [Console]::In.ReadLine() | {command}\n\
                     if ($?) {{ exit 0 }}\n\
                     if ($LASTEXITCODE) {{ exit $LASTEXITCODE }}\n\
                     exit 1"
                ),
                ShellType::Cmd => bail!("environment command hooks require PowerShell on Windows"),
            };
            let argv = shell.derive_exec_args(&script, environment.config().allow_login_shell);
            // Keep network attribution registered until the hook and its output are complete.
            let (params, _execution_network) = self
                .launch_params(environment, argv, env)
                .await
                .map_err(|error| match error {
                    ToolError::Rejected(message) => anyhow::anyhow!("{message}"),
                    ToolError::Codex(error) => anyhow::Error::new(error),
                })?;
            let deadline = tokio::time::Instant::now() + timeout;
            let backend = environment.environment.get_exec_backend();
            let process = tokio::time::timeout_at(deadline, backend.start(params))
                .await
                .context("environment command hook timed out during launch")??
                .process;
            let mut guard = HookProcessGuard(Some(Arc::clone(&process)));
            let output = tokio::time::timeout_at(deadline, async {
                let mut events = process.subscribe_events();
                let write = async {
                    let written = process.write(input_json).await?;
                    if written.status != codex_exec_server::WriteStatus::Accepted {
                        bail!("hook process did not accept its input: {:?}", written.status);
                    }
                    anyhow::Ok(())
                };
                let read = async {
                    let mut output = HookEventOutput::default();
                    loop {
                        let event = events.recv().await.context("hook output event stream lost events")?;
                        if let Some(result) = output.accept(event)? {
                            return Ok(result);
                        }
                    }
                };
                let (_, output) = tokio::try_join!(write, read)?;
                anyhow::Ok(output)
            })
            .await
            .context("environment command hook timed out")??;
            guard.0 = None;
            Ok(output)
        }
        .boxed()
    }
}

#[derive(Default)]
struct HookEventOutput {
    last_seq: u64,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl HookEventOutput {
    fn accept(&mut self, event: ExecProcessEvent) -> anyhow::Result<Option<EnvironmentHookOutput>> {
        let seq = match &event {
            ExecProcessEvent::Output(chunk) => chunk.seq,
            ExecProcessEvent::Exited { seq, .. } | ExecProcessEvent::Closed { seq } => *seq,
            ExecProcessEvent::Failed(message) => bail!("hook process failed: {message}"),
        };
        if self.last_seq.checked_add(1) != Some(seq) {
            bail!(
                "hook output event sequence lost: expected after {}, received {seq}",
                self.last_seq
            );
        }
        self.last_seq = seq;
        match event {
            ExecProcessEvent::Output(chunk) => {
                let target = match chunk.stream {
                    ExecOutputStream::Stdout | ExecOutputStream::Pty => &mut self.stdout,
                    ExecOutputStream::Stderr => &mut self.stderr,
                };
                append_hook_output(target, &chunk.chunk.0, chunk.stream)?;
                Ok(None)
            }
            ExecProcessEvent::Exited { exit_code, .. } => {
                self.exit_code = Some(exit_code);
                Ok(None)
            }
            ExecProcessEvent::Closed { .. } => Ok(Some(EnvironmentHookOutput {
                exit_code: self
                    .exit_code
                    .context("hook process closed without exit status")?,
                stdout: String::from_utf8_lossy(&self.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&self.stderr).into_owned(),
            })),
            ExecProcessEvent::Failed(_) => unreachable!(),
        }
    }
}

fn append_hook_output(
    target: &mut Vec<u8>,
    chunk: &[u8],
    stream: ExecOutputStream,
) -> anyhow::Result<()> {
    if chunk.len() > MAX_HOOK_OUTPUT_BYTES.saturating_sub(target.len()) {
        let name = match stream {
            ExecOutputStream::Stdout | ExecOutputStream::Pty => "stdout",
            ExecOutputStream::Stderr => "stderr",
        };
        bail!("hook {name} exceeded the {MAX_HOOK_OUTPUT_BYTES}-byte output limit");
    }
    target.extend_from_slice(chunk);
    Ok(())
}

#[cfg(test)]
mod output_limit_tests {
    use super::*;

    fn output(seq: u64, bytes: &[u8]) -> ExecProcessEvent {
        ExecProcessEvent::Output(codex_exec_server::ProcessOutputChunk {
            seq,
            stream: ExecOutputStream::Stdout,
            chunk: bytes.to_vec().into(),
        })
    }

    #[test]
    fn event_sequence_retains_output_after_exit_and_rejects_loss() {
        let mut collector = HookEventOutput::default();
        assert!(collector.accept(output(1, b"before")).unwrap().is_none());
        assert!(
            collector
                .accept(ExecProcessEvent::Exited {
                    seq: 2,
                    exit_code: 0,
                    sandbox_denied: None,
                })
                .unwrap()
                .is_none()
        );
        assert!(collector.accept(output(3, b"after")).unwrap().is_none());
        let result = collector
            .accept(ExecProcessEvent::Closed { seq: 4 })
            .unwrap()
            .unwrap();
        assert_eq!(result.stdout, "beforeafter");
        assert_eq!(result.exit_code, 0);

        let mut missing_first = HookEventOutput::default();
        assert!(missing_first.accept(output(2, b"partial")).is_err());
        let mut missing_middle = HookEventOutput::default();
        missing_middle.accept(output(1, b"partial")).unwrap();
        assert!(
            missing_middle
                .accept(ExecProcessEvent::Exited {
                    seq: 3,
                    exit_code: 0,
                    sandbox_denied: None,
                })
                .is_err()
        );
        let mut missing_exit = HookEventOutput::default();
        assert!(
            missing_exit
                .accept(ExecProcessEvent::Closed { seq: 1 })
                .is_err()
        );
    }

    #[test]
    fn exact_limit_per_stream_is_retained_and_overflow_is_rejected() {
        for stream in [
            ExecOutputStream::Stdout,
            ExecOutputStream::Stderr,
            ExecOutputStream::Pty,
        ] {
            let mut output = Vec::new();
            append_hook_output(&mut output, &vec![b'x'; MAX_HOOK_OUTPUT_BYTES - 1], stream)
                .unwrap();
            append_hook_output(&mut output, b"x", stream).unwrap();
            assert_eq!(output.len(), MAX_HOOK_OUTPUT_BYTES);
            let error = append_hook_output(&mut output, b"x", stream).unwrap_err();
            assert!(error.to_string().contains("output limit"));
            assert_eq!(output.len(), MAX_HOOK_OUTPUT_BYTES);
        }
    }

    #[test]
    fn valid_json_prefix_cannot_turn_truncated_output_into_success() {
        let mut output = br#"{"continue":true}"#.to_vec();
        let padding = vec![b' '; MAX_HOOK_OUTPUT_BYTES - output.len()];
        append_hook_output(&mut output, &padding, ExecOutputStream::Stdout).unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&output).is_ok());
        let error = append_hook_output(&mut output, b" ", ExecOutputStream::Stdout).unwrap_err();
        assert!(error.to_string().contains("stdout"));
    }

    #[test]
    fn separate_streams_have_independent_budgets_across_utf8_chunks() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let bytes = "한".as_bytes();
        append_hook_output(&mut stdout, &bytes[..1], ExecOutputStream::Stdout).unwrap();
        append_hook_output(
            &mut stderr,
            &vec![b'e'; MAX_HOOK_OUTPUT_BYTES],
            ExecOutputStream::Stderr,
        )
        .unwrap();
        append_hook_output(&mut stdout, &bytes[1..], ExecOutputStream::Stdout).unwrap();
        assert_eq!(std::str::from_utf8(&stdout).unwrap(), "한");
        assert!(append_hook_output(&mut stderr, b"e", ExecOutputStream::Stderr).is_err());
    }
}
