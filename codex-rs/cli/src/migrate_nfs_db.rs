//! Confirms explicit database migration and reports the shared migrator's results.
//! Other SQLite clients must remain stopped during replacement, and NFS must enforce
//! cross-client POSIX locks.

use anyhow::Context;
use anyhow::ensure;
use clap::Parser;
use codex_config::LoaderOverrides;
use codex_core::config::ConfigBuilder;
use codex_state::NfsDbMigration;
use codex_state::NfsDbMigrationOutcome;
use codex_tui::i18n::message;
use codex_utils_cli::CliConfigOverrides;
use owo_colors::OwoColorize;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    after_help = "Stop SQLite clients on every host and keep them stopped through cutover.\nThe NFS mount must support cross-client POSIX locks (no nolock/local_lock=all).\nThis command replaces configured databases in place and retains their WAL backups."
)]
pub(crate) struct MigrateNfsDbCommand {
    /// Use a local directory with space for the database and WAL files.
    /// The default is the system temporary directory.
    #[arg(long, default_value_os_t = std::env::temp_dir())]
    staging_dir: PathBuf,
}

pub(crate) async fn run(
    command: MigrateNfsDbCommand,
    config_overrides: CliConfigOverrides,
    loader_overrides: LoaderOverrides,
) -> anyhow::Result<()> {
    ensure!(cfg!(unix), "{}", message("r40-cli-nfs-unix-required", &[]));
    let warning = message("r40-cli-nfs-concurrency-warning", &[]);
    if supports_color::on(supports_color::Stream::Stderr).is_some() {
        eprintln!("{}", warning.red().bold());
    } else {
        eprintln!("{warning}");
    }
    eprintln!("{}", message("r40-cli-nfs-stop-clients", &[]));
    let config = ConfigBuilder::default()
        .cli_overrides(
            config_overrides
                .parse_overrides()
                .map_err(anyhow::Error::msg)?,
        )
        .loader_overrides(loader_overrides)
        .build()
        .await?;
    // Check the merged config for an explicit log directory.
    let effective_config = config.config_layer_stack.effective_config();
    let skip_logs = effective_config.get("log_dir").is_some();
    let sqlite = config.sqlite_config();
    if !sqlite.home().try_exists()? {
        println!("{}", message("r40-cli-nfs-no-databases", &[]));
        return Ok(());
    }
    let mut needs_permission_confirmation = false;
    for database in sqlite.runtime_db_paths() {
        if skip_logs && database.path == sqlite.logs_db_path() {
            continue;
        }
        needs_permission_confirmation |=
            NfsDbMigration::needs_permission_confirmation(&database.path)?;
    }
    if needs_permission_confirmation {
        eprintln!("{}", message("r40-cli-nfs-nonprivate-permissions", &[]));
    }
    eprintln!("{}", message("r40-cli-nfs-ownership-warning", &[]));
    let prompt = if needs_permission_confirmation {
        message("r40-cli-nfs-confirm-permissions", &[])
    } else {
        message("r40-cli-nfs-confirm", &[])
    };
    ensure!(
        crate::confirm(&prompt)?,
        "{}",
        message("r40-cli-nfs-cancelled", &[])
    );
    let mut migration = NfsDbMigration::lock(sqlite.home())?;
    let mut databases_found = 0;
    for database in sqlite.runtime_db_paths() {
        if skip_logs && database.path == sqlite.logs_db_path() {
            println!("{}", message("r40-cli-nfs-skipped-logs", &[]));
            continue;
        }
        let outcome = migration
            .migrate(&database.path, &command.staging_dir)
            .await
            .with_context(|| {
                message(
                    "r40-cli-nfs-failed",
                    &[("path", &database.path.display().to_string())],
                )
            })?;
        match outcome {
            NfsDbMigrationOutcome::Missing => continue,
            NfsDbMigrationOutcome::AlreadyDelete => {
                println!(
                    "{}",
                    message(
                        "r40-cli-nfs-already-delete",
                        &[("path", &database.path.display().to_string())]
                    )
                )
            }
            NfsDbMigrationOutcome::Migrated { backup_directory } => println!(
                "{}",
                message(
                    "r40-cli-nfs-migrated",
                    &[
                        ("path", &database.path.display().to_string()),
                        ("backup", &backup_directory.display().to_string())
                    ]
                )
            ),
        }
        databases_found += 1;
    }
    if databases_found == 0 {
        println!("{}", message("r40-cli-nfs-no-databases", &[]));
        return Ok(());
    }
    println!("{}", message("r40-cli-nfs-complete", &[]));
    Ok(())
}
