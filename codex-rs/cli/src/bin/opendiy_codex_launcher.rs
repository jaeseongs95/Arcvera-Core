// Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md.
fn main() {
    match codex_cli::opendiy_update::launch() {
        Ok(Some(status)) => std::process::exit(status.code().unwrap_or(1)),
        Ok(None) => {}
        Err(error) => {
            eprintln!("OpenDIY launcher: {error:#}");
            std::process::exit(1);
        }
    }
}
