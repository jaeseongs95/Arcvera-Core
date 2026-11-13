// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
use clap::Parser;
use std::path::PathBuf;

#[derive(Debug, Parser)]
pub struct AppCommand {
    /// Legacy workspace argument (Desktop integration is disabled).
    #[arg(value_name = "PATH", default_value = ".")]
    pub path: PathBuf,

    /// Legacy installer URL argument (ignored; installation is disabled).
    #[arg(long = "download-url")]
    pub download_url_override: Option<String>,
}

pub async fn run_app(_cmd: AppCommand) -> anyhow::Result<()> {
    anyhow::bail!(
        "Desktop app launch and installation are disabled in Arcvera Core. Download releases from https://github.com/jaeseongs95/Arcvera-Core/releases"
    )
}
