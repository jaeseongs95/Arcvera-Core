// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
// Keep the ACL probe independent of test-harness startup, which writes ARCVERA_HOME.
#[cfg(target_os = "windows")]
mod win;

#[cfg(target_os = "windows")]
fn main() {
    win::main();
}

#[cfg(not(target_os = "windows"))]
fn main() {
    panic!("codex-windows-managed-deny-probe is Windows-only");
}
