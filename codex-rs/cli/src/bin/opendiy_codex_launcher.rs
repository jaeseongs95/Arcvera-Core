fn main() {
    match codex_cli::opendiy_update::launch() {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("OpenDIY launcher: {error:#}");
            std::process::exit(1);
        }
    }
}
