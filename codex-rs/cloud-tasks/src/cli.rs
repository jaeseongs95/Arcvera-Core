// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
use clap::Args;
use clap::Parser;
use codex_tui::i18n::message;
use codex_utils_cli::CliConfigOverrides;

#[derive(Parser, Debug, Default)]
#[command(version)]
pub struct Cli {
    #[clap(skip)]
    pub config_overrides: CliConfigOverrides,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    /// Submit a new Codex Cloud task without launching the TUI.
    #[command(about = message("cloud-cli-help-exec", &[]))]
    Exec(ExecCommand),
    /// Show the status of a Codex Cloud task.
    #[command(about = message("cloud-cli-help-status", &[]))]
    Status(StatusCommand),
    /// List Codex Cloud tasks.
    #[command(about = message("cloud-cli-help-list", &[]))]
    List(ListCommand),
    /// List accessible Codex Cloud environments.
    #[command(about = message("cloud-cli-help-environments", &[]))]
    Environments(EnvironmentsCommand),
    /// Apply the diff for a Codex Cloud task locally.
    #[command(about = message("cloud-cli-help-apply", &[]))]
    Apply(ApplyCommand),
    /// Show the unified diff for a Codex Cloud task.
    #[command(about = message("cloud-cli-help-diff", &[]))]
    Diff(DiffCommand),
}

#[derive(Debug, Args)]
pub struct ExecCommand {
    /// Task prompt to run in Codex Cloud.
    #[arg(help = message("cloud-cli-help-query", &[]))]
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,

    /// Target environment identifier (see `codex cloud` to browse).
    #[arg(help = message("cloud-cli-help-environment", &[]))]
    #[arg(long = "env", value_name = "ENV_ID")]
    pub environment: String,

    /// Number of assistant attempts (best-of-N).
    #[arg(help = message("cloud-cli-help-attempts", &[]))]
    #[arg(
        long = "attempts",
        default_value_t = 1usize,
        value_parser = parse_attempts
    )]
    pub attempts: usize,

    /// Git branch to run in Codex Cloud (defaults to current branch).
    #[arg(help = message("cloud-cli-help-branch", &[]))]
    #[arg(long = "branch", value_name = "BRANCH")]
    pub branch: Option<String>,
}

fn parse_attempts(input: &str) -> Result<usize, String> {
    let value: usize = input
        .parse()
        .map_err(|_| message("cloud-attempts-integer", &[]))?;
    if (1..=4).contains(&value) {
        Ok(value)
    } else {
        Err(message("cloud-attempts-range", &[]))
    }
}

fn parse_limit(input: &str) -> Result<i64, String> {
    let value: i64 = input
        .parse()
        .map_err(|_| message("cloud-limit-integer", &[]))?;
    if (1..=20).contains(&value) {
        Ok(value)
    } else {
        Err(message("cloud-limit-range", &[]))
    }
}

#[derive(Debug, Args)]
pub struct StatusCommand {
    /// Codex Cloud task identifier to inspect.
    #[arg(help = message("cloud-cli-help-status-id", &[]))]
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,
}

#[derive(Debug, Args)]
pub struct ListCommand {
    /// Filter tasks by environment identifier.
    #[arg(help = message("cloud-cli-help-filter-environment", &[]))]
    #[arg(long = "env", value_name = "ENV_ID")]
    pub environment: Option<String>,

    /// Maximum number of tasks to return (1-20).
    #[arg(help = message("cloud-cli-help-limit", &[]))]
    #[arg(long = "limit", default_value_t = 20, value_parser = parse_limit, value_name = "N")]
    pub limit: i64,

    /// Pagination cursor returned by a previous call.
    #[arg(help = message("cloud-cli-help-cursor", &[]))]
    #[arg(long = "cursor", value_name = "CURSOR")]
    pub cursor: Option<String>,

    /// Emit JSON instead of plain text.
    #[arg(help = message("cloud-cli-help-json", &[]))]
    #[arg(long = "json", default_value_t = false)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct EnvironmentsCommand {
    /// Emit JSON instead of plain text.
    #[arg(help = message("cloud-cli-help-json", &[]))]
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct ApplyCommand {
    /// Codex Cloud task identifier to apply.
    #[arg(help = message("cloud-cli-help-apply-id", &[]))]
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Attempt number to apply (1-based).
    #[arg(help = message("cloud-cli-help-apply-attempt", &[]))]
    #[arg(long = "attempt", value_parser = parse_attempts, value_name = "N")]
    pub attempt: Option<usize>,
}

#[derive(Debug, Args)]
pub struct DiffCommand {
    /// Codex Cloud task identifier to display.
    #[arg(help = message("cloud-cli-help-diff-id", &[]))]
    #[arg(value_name = "TASK_ID")]
    pub task_id: String,

    /// Attempt number to display (1-based).
    #[arg(help = message("cloud-cli-help-diff-attempt", &[]))]
    #[arg(long = "attempt", value_parser = parse_attempts, value_name = "N")]
    pub attempt: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_help_matches_derive_doc_comment_normalization() {
        use clap::CommandFactory;
        let command = Cli::command();
        let exec = command.find_subcommand("exec").unwrap();
        assert_eq!(
            exec.get_about().unwrap().to_string(),
            "Submit a new Codex Cloud task without launching the TUI"
        );
        let query = exec
            .get_arguments()
            .find(|arg| arg.get_id() == "query")
            .unwrap();
        assert_eq!(
            query.get_help().unwrap().to_string(),
            "Task prompt to run in Codex Cloud"
        );
    }

    #[test]
    fn environments_supports_machine_readable_output() {
        let cli = Cli::try_parse_from(["codex cloud", "environments", "--json"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Environments(EnvironmentsCommand { json: true }))
        ));
    }
}
