// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R32 (2026-11-01); see MODIFICATIONS.md.
// Test-only executable for packaging and Windows argument/exit-code handling.
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args == ["--version"] {
        let executable = std::env::current_exe().expect("fixture executable path");
        match executable
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
        {
            "arcvera.exe" => println!("arcvera 0.168.0"),
            "codex-app-server.exe" => println!("codex-app-server 0.168.0"),
            "codex-code-mode-host.exe" => println!("codex-code-mode-host 0.168.0"),
            "wrong-server.exe" => println!("codex-app-server 0.167.0"),
            "failing-version.exe" => std::process::exit(23),
            _ => println!("arcvera 0.165.0+test"),
        }
        return;
    }
    let output = std::env::var_os("PACKAGE_FIXTURE_ARGV").expect("fixture output path");
    std::fs::write(output, args.join("\n")).expect("write fixture argv");
    std::process::exit(17);
}
