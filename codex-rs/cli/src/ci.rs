//! Read-only GitHub Actions observation through the user's existing GitHub CLI.
use anyhow::Context;
use clap::Parser;
use codex_tui::i18n::message;
use codex_utils_process::background_command;
use serde::Deserialize;
use serde::Serialize;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::Instant;

const RUN_FIELDS: &str = "databaseId,headSha,headBranch,status,conclusion,url,workflowName,attempt";
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Parser)]
#[command(about = "Observe GitHub Actions runs without changing remote state")]
pub(crate) struct CiCommand {
    /// Repository in [HOST/]OWNER/REPO format (defaults to the current repository)
    #[arg(long, short = 'R', global = true, value_parser = parse_repo)]
    repo: Option<String>,
    #[command(subcommand)]
    command: CiAction,
}

#[derive(Debug, clap::Subcommand)]
enum CiAction {
    /// Query CI for the current HEAD or --commit once
    Status(CiList),
    /// Query recent CI runs once
    List(CiList),
    /// Watch one run until completion or timeout without rerunning it
    Watch {
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        run_id: u64,
        /// Maximum observation time in seconds
        #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(1..=86400))]
        timeout: u64,
        /// Observation interval in seconds
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u64).range(1..=300))]
        interval: u64,
        /// Print each observation as one JSON line
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, clap::Args)]
struct CiList {
    /// Full commit SHA (defaults to local HEAD for status)
    #[arg(long, value_parser = parse_sha)]
    commit: Option<String>,
    /// Additionally filter by branch
    #[arg(long)]
    branch: Option<String>,
    /// Maximum runs to return (does not enumerate required checks)
    #[arg(long, default_value_t = 20, value_parser = clap::value_parser!(u32).range(1..=100))]
    limit: u32,
    /// Emit JSON instead of plain text.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CiRun {
    database_id: u64,
    head_sha: String,
    head_branch: String,
    status: String,
    conclusion: Option<String>,
    url: String,
    workflow_name: String,
    attempt: u64,
}

pub(crate) async fn run(command: CiCommand) -> anyhow::Result<()> {
    observe_until_cancelled(observe(command), cancellation_signal()).await
}

async fn observe_until_cancelled(
    observation: impl std::future::Future<Output = anyhow::Result<()>>,
    cancellation: impl std::future::Future<Output = anyhow::Result<()>>,
) -> anyhow::Result<()> {
    tokio::select! {
        // Register listeners before starting git/gh, and drop the observation
        // (including its kill_on_drop child) when cancellation wins.
        biased;
        result = cancellation => {
            result?;
            anyhow::bail!("{}", message("ci-cancelled", &[]));
        }
        result = observation => result,
    }
}

async fn cancellation_signal() -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        // CtrlBreak otherwise invokes Windows' abrupt default termination,
        // which cannot run the active query child's Rust drop handler.
        let mut ctrl_c = tokio::signal::windows::ctrl_c()
            .context(message("ci-signal-failed", &[("signal", "Ctrl+C")]))?;
        let mut ctrl_break = tokio::signal::windows::ctrl_break()
            .context(message("ci-signal-failed", &[("signal", "Ctrl+Break")]))?;
        tokio::select! {
            signal = ctrl_c.recv() => {
                signal.context(message("ci-signal-failed", &[("signal", "Ctrl+C")]))?;
            }
            signal = ctrl_break.recv() => {
                signal.context(message("ci-signal-failed", &[("signal", "Ctrl+Break")]))?;
            }
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        tokio::signal::ctrl_c()
            .await
            .context(message("ci-signal-failed", &[("signal", "Ctrl+C")]))
    }
}

async fn observe(command: CiCommand) -> anyhow::Result<()> {
    let repo = command.repo.as_deref();
    match command.command {
        CiAction::Status(mut options) => {
            if options.commit.is_none() {
                let output = output(
                    "git",
                    &["rev-parse".into(), "--verify".into(), "HEAD".into()],
                )
                .await
                .context(message("ci-head-unavailable", &[]))?;
                let sha = std::str::from_utf8(&output).context(message("ci-head-encoding", &[]))?;
                options.commit = Some(parse_sha(sha.trim()).map_err(anyhow::Error::msg)?);
            }
            list(repo, options).await
        }
        CiAction::List(options) => list(repo, options).await,
        CiAction::Watch {
            run_id,
            timeout,
            interval,
            json,
        } => {
            // One monotonic deadline covers network calls and sleeps together.
            tokio::time::timeout(
                Duration::from_secs(timeout),
                watch(repo, run_id, Duration::from_secs(interval), json),
            )
            .await
            .context(message("ci-watch-timeout", &[]))?
        }
    }
}

async fn list(repo: Option<&str>, options: CiList) -> anyhow::Result<()> {
    let bytes = output("gh", &list_args(repo, &options)).await?;
    let runs = parse_runs(&bytes, options.commit.as_deref())?;
    if options.json {
        println!("{}", serde_json::to_string_pretty(&runs)?);
    } else if runs.is_empty() {
        println!("{}", message("ci-empty", &[]));
    } else {
        for run in &runs {
            print_run(run);
        }
        if runs.len() == options.limit as usize {
            eprintln!("{}", message("ci-limit", &[]));
        }
    }
    Ok(())
}

fn list_args(repo: Option<&str>, options: &CiList) -> Vec<String> {
    let mut args = vec![
        "run".into(),
        "list".into(),
        "--json".into(),
        RUN_FIELDS.into(),
        "--limit".into(),
        options.limit.to_string(),
    ];
    if let Some(commit) = &options.commit {
        args.extend(["--commit".into(), commit.clone()]);
    }
    if let Some(branch) = &options.branch {
        args.push(format!("--branch={branch}"));
    }
    add_repo(&mut args, repo);
    args
}

fn add_repo(args: &mut Vec<String>, repo: Option<&str>) {
    if let Some(repo) = repo {
        args.extend(["--repo".into(), repo.into()]);
    }
}

fn parse_runs(bytes: &[u8], commit: Option<&str>) -> anyhow::Result<Vec<CiRun>> {
    let runs: Vec<CiRun> = serde_json::from_slice(bytes)
        .map_err(|_| anyhow::anyhow!("{}", message("ci-list-invalid", &[])))?;
    if let Some(commit) = commit
        && runs
            .iter()
            .any(|run| !run.head_sha.eq_ignore_ascii_case(commit))
    {
        anyhow::bail!("{}", message("ci-sha-mismatch", &[]));
    }
    Ok(runs)
}

async fn watch(
    repo: Option<&str>,
    run_id: u64,
    interval: Duration,
    json: bool,
) -> anyhow::Result<()> {
    let mut args = vec![
        "run".into(),
        "view".into(),
        run_id.to_string(),
        "--json".into(),
        RUN_FIELDS.into(),
    ];
    add_repo(&mut args, repo);
    loop {
        let bytes = output("gh", &args).await?;
        let run: CiRun = serde_json::from_slice(&bytes)
            .map_err(|_| anyhow::anyhow!("{}", message("ci-run-invalid", &[])))?;
        let completed = watch_completed(&run, run_id)?;
        if json {
            println!("{}", serde_json::to_string(&run)?);
        } else {
            print_run(&run);
        }
        if completed {
            if run.conclusion.as_deref() == Some("success") {
                return Ok(());
            }
            anyhow::bail!(
                "{}",
                message("ci-not-successful", &[("run", &run_id.to_string())])
            );
        }
        tokio::time::sleep_until(Instant::now() + interval).await;
    }
}

fn watch_completed(run: &CiRun, run_id: u64) -> anyhow::Result<bool> {
    if run.database_id != run_id {
        anyhow::bail!("{}", message("ci-run-mismatch", &[]));
    }
    Ok(run.status == "completed")
}

fn print_run(run: &CiRun) {
    println!(
        "run={} attempt={} workflow={} branch={} head={} status={} conclusion={}\n{}",
        run.database_id,
        run.attempt,
        run.workflow_name,
        run.head_branch,
        run.head_sha,
        run.status,
        run.conclusion
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("pending"),
        run.url,
    );
}

async fn output(program: &str, args: &[String]) -> anyhow::Result<Vec<u8>> {
    let mut command = Command::from(background_command(program));
    command
        .args(args)
        .stdin(Stdio::null())
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .kill_on_drop(true);
    query_output(program, command, COMMAND_TIMEOUT).await
}

async fn query_output(
    program: &str,
    mut command: Command,
    timeout: Duration,
) -> anyhow::Result<Vec<u8>> {
    let result = tokio::time::timeout(timeout, command.output())
        .await
        .with_context(|| message("ci-command-timeout", &[("program", program)]))?
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                anyhow::anyhow!("{}", message("ci-program-missing", &[("program", program)]))
            } else {
                anyhow::anyhow!(
                    "{}",
                    message(
                        "ci-start-failed",
                        &[("program", program), ("error", &error.to_string())]
                    )
                )
            }
        })?;
    if !result.status.success() {
        anyhow::bail!(
            "{}",
            message(
                "ci-command-failed",
                &[
                    ("program", program),
                    ("status", &result.status.to_string()),
                    ("hint", &message(error_hint(&result.stderr), &[]))
                ]
            )
        );
    }
    Ok(result.stdout)
}

// Classify stderr without echoing backend response bodies or credentials.
fn error_hint(stderr: &[u8]) -> &'static str {
    let text = String::from_utf8_lossy(stderr).to_ascii_lowercase();
    if text.contains("401") || text.contains("authentication") || text.contains("gh auth login") {
        "ci-hint-auth"
    } else if text.contains("429") || text.contains("rate limit") {
        "ci-hint-rate-limit"
    } else if text.contains("403")
        || text.contains("forbidden")
        || text.contains("resource not accessible")
    {
        "ci-hint-permission"
    } else if text.contains("404")
        || text.contains("not found")
        || text.contains("could not resolve")
    {
        "ci-hint-not-found"
    } else {
        "ci-hint-other"
    }
}

fn parse_sha(value: &str) -> Result<String, String> {
    if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value.to_ascii_lowercase())
    } else {
        Err(message("ci-invalid-sha", &[]))
    }
}

fn parse_repo(value: &str) -> Result<String, String> {
    let parts: Vec<_> = value.split('/').collect();
    if matches!(parts.len(), 2 | 3)
        && parts.iter().all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && *part != "."
                && *part != ".."
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
    {
        Ok(value.into())
    } else {
        Err(message("ci-invalid-repo", &[]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    #[test]
    fn observation_requires_explicit_watch_and_bounded_arguments() {
        for action in ["status", "list"] {
            let command = CiCommand::try_parse_from(["ci", action]).unwrap();
            assert!(!matches!(command.command, CiAction::Watch { .. }));
        }
        for args in [
            vec!["ci", "watch", "0"],
            vec!["ci", "watch", "123", "--timeout", "0"],
            vec!["ci", "watch", "123", "--interval", "0"],
            vec!["ci", "list", "--limit", "101"],
            vec!["ci", "status", "--commit", "main"],
            vec!["ci", "--repo", "https://github.com/a/b", "list"],
            vec!["ci", "rerun", "123"],
        ] {
            assert!(CiCommand::try_parse_from(args).is_err());
        }
    }

    #[test]
    fn list_uses_read_only_command_and_exact_commit_selector() {
        let command = CiCommand::try_parse_from([
            "ci",
            "status",
            "--repo",
            "openai/codex",
            "--commit",
            SHA,
            "--branch",
            "--odd",
        ]);
        assert!(command.is_err());
        let command = CiCommand::try_parse_from([
            "ci",
            "status",
            "--repo",
            "openai/codex",
            "--commit",
            SHA,
            "--branch=-odd",
        ])
        .unwrap();
        let CiAction::Status(options) = command.command else {
            panic!("status expected")
        };
        let args = list_args(command.repo.as_deref(), &options);
        assert_eq!(&args[..2], &["run", "list"]);
        assert!(args.windows(2).any(|pair| pair == ["--commit", SHA]));
        assert!(args.contains(&"--branch=-odd".to_string()));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--repo", "openai/codex"])
        );
    }

    #[test]
    fn json_preserves_identity_and_rejects_wrong_commit_or_malformed_results() {
        let bytes = format!(
            r#"[{{"databaseId":12,"headSha":"{SHA}","headBranch":"topic","status":"completed","conclusion":"failure","url":"https://github.com/o/r/actions/runs/12","workflowName":"build","attempt":2}}]"#
        );
        let runs = parse_runs(bytes.as_bytes(), Some(SHA)).unwrap();
        assert_eq!(runs[0].database_id, 12);
        assert_eq!(runs[0].attempt, 2);
        assert_eq!(runs[0].conclusion.as_deref(), Some("failure"));
        assert_eq!(runs[0].head_branch, "topic");
        assert!(parse_runs(bytes.as_bytes(), Some(&"a".repeat(40))).is_err());
        assert!(parse_runs(br#"[{"status":"completed"}]"#, None).is_err());
        let error =
            parse_runs(br#"[{"databaseId":"sensitive-response-value"}]"#, None).unwrap_err();
        assert!(!format!("{error:#}").contains("sensitive-response-value"));
        assert!(parse_runs(b"[]", Some(SHA)).unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_executable_is_distinct_from_an_empty_successful_query() {
        let error = output("codex-r36-missing-ci-executable-6c8d7fae", &[])
            .await
            .unwrap_err();
        assert!(error.to_string().contains(&message(
            "ci-program-missing",
            &[("program", "codex-r36-missing-ci-executable-6c8d7fae")]
        )));
    }

    #[tokio::test]
    async fn cancellation_before_query_does_not_start_observation() {
        let queried = std::cell::Cell::new(false);
        let observation = async {
            queried.set(true);
            Ok(())
        };
        let error = observe_until_cancelled(observation, async { Ok(()) })
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), message("ci-cancelled", &[]));
        assert!(!queried.get(), "query started after cancellation was ready");
    }

    #[tokio::test]
    async fn cancellation_preserves_observation_success_and_errors() {
        observe_until_cancelled(async { Ok(()) }, std::future::pending())
            .await
            .unwrap();
        let error = observe_until_cancelled(
            async { anyhow::bail!("synthetic query failure") },
            std::future::pending(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "synthetic query failure");
    }

    #[tokio::test]
    async fn cancellation_listener_failure_is_not_user_cancellation() {
        let error = observe_until_cancelled(std::future::pending(), async {
            Err(anyhow::anyhow!("synthetic listener failure"))
        })
        .await
        .unwrap_err();
        assert_eq!(error.to_string(), "synthetic listener failure");
        assert_ne!(error.to_string(), message("ci-cancelled", &[]));
    }

    // Exercise the same drop boundary as run(), with a real owned child. OS
    // signal delivery is covered separately in an isolated CLI process group.
    #[tokio::test]
    async fn cancellation_during_query_terminates_its_child() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("child-outlived-cancellation");
        let mut command = Command::from(background_command(std::env::current_exe().unwrap()));
        command
            .args(["--ignored", "--exact", "ci::tests::timeout_child_fixture"])
            .env("CODEX_R36_CI_TIMEOUT_MARKER", &marker)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let observation = async {
            query_output("fixture", command, COMMAND_TIMEOUT).await?;
            Ok(())
        };
        let cancellation = async {
            while !marker.with_extension("started").exists() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Ok(())
        };
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            observe_until_cancelled(observation, cancellation),
        )
        .await
        .expect("query cancellation did not complete")
        .unwrap_err();
        assert_eq!(error.to_string(), message("ci-cancelled", &[]));
        assert!(marker.with_extension("started").exists());
        tokio::time::sleep(Duration::from_millis(2200)).await;
        assert!(!marker.exists(), "child continued after cancellation");
    }

    // The child is this test executable, so this checks cancellation on either
    // supported host without depending on a shell, gh installation, or account.
    #[tokio::test]
    async fn query_timeout_terminates_its_child() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("child-outlived-timeout");
        let mut command = Command::from(background_command(std::env::current_exe().unwrap()));
        command
            .args(["--ignored", "--exact", "ci::tests::timeout_child_fixture"])
            .env("CODEX_R36_CI_TIMEOUT_MARKER", &marker)
            .stdin(Stdio::null())
            .kill_on_drop(true);
        let error = query_output("fixture", command, Duration::from_secs(1))
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            message("ci-command-timeout", &[("program", "fixture")])
        );
        assert!(
            marker.with_extension("started").exists(),
            "fixture did not start"
        );
        tokio::time::sleep(Duration::from_millis(2200)).await;
        assert!(!marker.exists(), "child continued after timeout");
    }

    #[test]
    #[ignore = "subprocess fixture for query_timeout_terminates_its_child"]
    fn timeout_child_fixture() {
        let marker = std::env::var_os("CODEX_R36_CI_TIMEOUT_MARKER").expect("fixture marker");
        std::fs::write(
            std::path::Path::new(&marker).with_extension("started"),
            "started",
        )
        .unwrap();
        std::thread::sleep(Duration::from_secs(2));
        std::fs::write(marker, "child survived").unwrap();
    }

    #[test]
    fn errors_distinguish_auth_permissions_missing_target_and_rate_limit_without_body() {
        for (raw, expected) in [
            ("HTTP 401 secret-body", "ci-hint-auth"),
            ("HTTP 403 secret-body", "ci-hint-permission"),
            ("HTTP 404 secret-body", "ci-hint-not-found"),
            ("HTTP 429 secret-body", "ci-hint-rate-limit"),
        ] {
            let hint = error_hint(raw.as_bytes());
            assert!(hint.contains(expected));
            assert!(!hint.contains("secret-body"));
        }
    }

    #[test]
    fn watch_preserves_pending_failed_and_different_run_id() {
        let mut run: CiRun = serde_json::from_str(&format!(r#"{{"databaseId":12,"headSha":"{SHA}","headBranch":"topic","status":"in_progress","conclusion":null,"url":"https://github.com/o/r/actions/runs/12","workflowName":"build","attempt":1}}"#)).unwrap();
        assert!(!watch_completed(&run, 12).unwrap());
        assert!(watch_completed(&run, 13).is_err());
        run.status = "completed".into();
        run.conclusion = Some("failure".into());
        assert!(watch_completed(&run, 12).unwrap());
        assert_ne!(run.conclusion.as_deref(), Some("success"));
    }

    #[test]
    fn ui_languages_keep_identifiers_opaque() {
        use codex_tui::i18n::Locale;
        use codex_tui::i18n::message_for;
        for (locale, expected) in [
            (Locale::English, "Cannot find"),
            (Locale::Korean, "찾을 수 없습니다"),
        ] {
            let result = message_for(
                locale,
                "ci-program-missing",
                &[("program", "synthetic-gh-id")],
            );
            assert!(result.contains(expected), "{result}");
            assert!(result.contains("synthetic-gh-id"), "{result}");
            for (key, expected) in match locale {
                Locale::English => [
                    ("ci-cancelled", "observation cancelled"),
                    ("ci-signal-failed", "Cannot listen for"),
                ],
                Locale::Korean => [
                    ("ci-cancelled", "관측을 취소했습니다"),
                    ("ci-signal-failed", "신호를 수신할 수 없습니다"),
                ],
            } {
                let result = message_for(locale, key, &[("signal", "Ctrl+Break")]);
                assert!(result.contains(expected), "{result}");
                if key == "ci-signal-failed" {
                    assert!(result.contains("Ctrl+Break"), "{result}");
                }
            }
        }
    }
}
