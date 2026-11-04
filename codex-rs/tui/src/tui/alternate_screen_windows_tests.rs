//! Exercise Windows console mouse capture in a separate process with its own console.

use super::AlternateScreen;
use pretty_assertions::assert_eq;
use std::fs::File;
use std::fs::OpenOptions;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows_sys::Win32::System::Console::ENABLE_MOUSE_INPUT;
use windows_sys::Win32::System::Console::GetConsoleMode;
use windows_sys::Win32::System::Console::SetConsoleMode;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;

const CHILD_ENV: &str = "CODEX_TUI_MOUSE_CAPTURE_TEST_CHILD";

fn console_mode(input: &File) -> u32 {
    let mut mode = 0;
    // SAFETY: input is an open console handle and mode points to writable storage.
    let result = unsafe { GetConsoleMode(input.as_raw_handle(), &mut mode) };
    assert_ne!(result, 0, "{}", std::io::Error::last_os_error());
    mode
}

#[tokio::test(flavor = "current_thread")]
async fn mouse_capture_restores_console_mode_and_encoding() {
    if std::env::var_os(CHILD_ENV).is_none() {
        let capture = tempfile::tempdir().expect("isolated console capture directory");
        let stdout = capture.path().join("stdout.txt");
        let stderr = capture.path().join("stderr.txt");
        let powershell_root = std::path::PathBuf::from(
            std::env::var_os("SystemRoot").expect("SystemRoot for built-in Windows PowerShell"),
        )
        .join("System32/WindowsPowerShell/v1.0");
        // The shell has no window; Start-Process creates the child's own hidden console.
        // Paths are data in the environment, never interpolated into shell source.
        let script = r#"
$ErrorActionPreference = 'Stop'
$consoleChild = $null
$consoleExit = 1
try {
    $consoleChild = Start-Process -FilePath $env:CODEX_TUI_CONSOLE_TEST_EXE -ArgumentList @('--exact', 'tui::alternate_screen::windows_tests::mouse_capture_restores_console_mode_and_encoding', '--nocapture') -WorkingDirectory $env:CODEX_TUI_CONSOLE_TEST_DIR -RedirectStandardOutput $env:CODEX_TUI_CONSOLE_TEST_STDOUT -RedirectStandardError $env:CODEX_TUI_CONSOLE_TEST_STDERR -WindowStyle Hidden -PassThru
    $null = $consoleChild.Handle
    if (-not $consoleChild.WaitForExit(20000)) { throw 'isolated console child timed out after 20 seconds' }
    $consoleChild.WaitForExit()
    $consoleExit = $consoleChild.ExitCode
    if ($null -eq $consoleExit) { throw 'isolated console child exit code is unavailable' }
} catch {
    [Console]::Error.WriteLine($_.Exception.Message)
    $consoleExit = 1
} finally {
    if ($null -ne $consoleChild) {
        try {
            if (-not $consoleChild.HasExited) {
                $consoleChild.Kill()
                if (-not $consoleChild.WaitForExit(5000)) { throw 'isolated console child did not exit after Kill' }
            }
        } catch {
            [Console]::Error.WriteLine($_.Exception.Message)
            $consoleExit = 1
        } finally { $consoleChild.Dispose() }
    }
}
exit $consoleExit
"#;
        let output = Command::new(powershell_root.join("powershell.exe"))
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env(CHILD_ENV, "1")
            .env("PSModulePath", powershell_root.join("Modules"))
            .env(
                "CODEX_TUI_CONSOLE_TEST_EXE",
                std::env::current_exe().expect("test binary"),
            )
            .env("CODEX_TUI_CONSOLE_TEST_DIR", capture.path())
            .env("CODEX_TUI_CONSOLE_TEST_STDOUT", &stdout)
            .env("CODEX_TUI_CONSOLE_TEST_STDERR", &stderr)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .expect("run built-in Windows PowerShell for hidden isolated console test");
        let child_stdout = std::fs::read(stdout);
        let child_stderr = std::fs::read(stderr);
        assert!(
            output.status.success()
                && child_stdout
                    .as_ref()
                    .is_ok_and(|bytes| String::from_utf8_lossy(bytes).contains("1 passed"))
                && child_stderr.is_ok(),
            "PowerShell stdout: {}\nPowerShell stderr: {}\nchild stdout: {:?}\nchild stderr: {:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
            child_stdout
                .as_ref()
                .map(|bytes| String::from_utf8_lossy(bytes)),
            child_stderr
                .as_ref()
                .map(|bytes| String::from_utf8_lossy(bytes)),
        );
        return;
    }

    let input = OpenOptions::new()
        .read(true)
        .write(true)
        .open("CONIN$")
        .expect("open console input");
    let initial_mode = console_mode(&input) & !ENABLE_MOUSE_INPUT;
    // SAFETY: input is an open console handle and initial_mode is a valid console input mode.
    let result = unsafe { SetConsoleMode(input.as_raw_handle(), initial_mode) };
    assert_ne!(result, 0, "{}", std::io::Error::last_os_error());
    let original_mode = console_mode(&input);
    assert_eq!(original_mode & ENABLE_MOUSE_INPUT, 0);
    let screen = AlternateScreen::default();
    let mut output = Vec::new();
    let mut terminal = vt100::Parser::new(
        /*rows*/ 24, /*cols*/ 80, /*scrollback_len*/ 0,
    );

    screen
        .configure_input(&mut output, /*capture_mouse*/ true)
        .unwrap();
    terminal.process(&std::mem::take(&mut output));
    assert_ne!(console_mode(&input) & ENABLE_MOUSE_INPUT, 0);
    assert_eq!(
        terminal.screen().mouse_protocol_encoding(),
        vt100::MouseProtocolEncoding::Sgr,
    );

    screen
        .configure_input(&mut output, /*capture_mouse*/ false)
        .unwrap();
    terminal.process(&output);
    assert_eq!(
        (
            console_mode(&input),
            terminal.screen().mouse_protocol_encoding()
        ),
        (original_mode, vt100::MouseProtocolEncoding::Default),
    );

    // Exercise the actual Tui API, rather than injecting MouseEvent or calling its low-level
    // screen helper. This console belongs only to this child process.
    let mut tui = super::super::test_support::make_test_tui().expect("isolated TUI");
    tui.set_alt_screen_enabled(false);
    for alt_allowed in [false, true] {
        tui.set_alt_screen_enabled(alt_allowed);
        tui.set_mouse_capture(super::super::MouseCapture::Enabled)
            .unwrap();
        assert!(!tui.is_alt_screen_active());
        assert_ne!(console_mode(&input) & ENABLE_MOUSE_INPUT, 0);
        super::super::restore_keep_raw(super::super::TerminalHandoff::Restore).unwrap();
        assert_eq!(console_mode(&input) & ENABLE_MOUSE_INPUT, 0);
        tui.set_mouse_capture(super::super::MouseCapture::Enabled)
            .unwrap();
        assert_ne!(
            console_mode(&input) & ENABLE_MOUSE_INPUT,
            0,
            "same requested policy must be reapplied after handoff"
        );
        tui.with_restored(super::super::TerminalHandoff::Restore, || async {
            assert_eq!(
                console_mode(&input) & ENABLE_MOUSE_INPUT,
                0,
                "editor owns an uncaptured console"
            );
        })
        .await;
        assert_ne!(
            console_mode(&input) & ENABLE_MOUSE_INPUT,
            0,
            "inline capture resumes after editor"
        );
        assert!(!tui.is_alt_screen_active());
        super::super::restore_after_exit().unwrap();
        assert_eq!(console_mode(&input) & ENABLE_MOUSE_INPUT, 0);
        tui.recover_after_caught_panic().unwrap();
        assert_ne!(console_mode(&input) & ENABLE_MOUSE_INPUT, 0);
        assert!(
            !tui.is_alt_screen_active(),
            "panic recovery must preserve inline/no-alt mode"
        );
        tui.set_mouse_capture(super::super::MouseCapture::Inherit)
            .unwrap();
        assert_eq!(
            console_mode(&input) & ENABLE_MOUSE_INPUT,
            0,
            "hidden footer restores shell mouse input"
        );
    }
    super::super::restore_after_exit().unwrap();
    assert_eq!(console_mode(&input), original_mode);
}
