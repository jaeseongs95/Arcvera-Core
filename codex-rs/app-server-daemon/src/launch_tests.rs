// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md.
use std::collections::BTreeMap;
use std::path::Path;

use super::CliDaemonMismatch;
use crate::Daemon;
use crate::settings::DaemonSettings;

async fn stage<T>(name: &'static str, future: impl std::future::Future<Output = T>) -> T {
    eprintln!("daemon-fixture stage begin: {name}");
    let result = tokio::time::timeout(std::time::Duration::from_secs(15), future)
        .await
        .unwrap_or_else(|_| panic!("daemon-fixture stage timed out: {name}"));
    eprintln!("daemon-fixture stage completed: {name}");
    result
}

struct ControlServer(tokio::task::JoinHandle<()>);

impl Drop for ControlServer {
    fn drop(&mut self) {
        // Own only this synthetic listener/task, including on assertion failure.
        self.0.abort();
    }
}

async fn daemon(home: &Path) -> Daemon {
    let state = home.join("app-server-daemon");
    // Create with the production owner-only ACL; precreating this directory
    // with inherited Windows permissions is deliberately rejected by the API.
    codex_uds::prepare_private_socket_directory(&state)
        .await
        .expect("private synthetic daemon state directory");
    Daemon {
        log_diagnostics: false,
        socket_path: state.join("app-server.sock"),
        pid_file: state.join(crate::DAEMON_PID_FILE_NAME),
        update_pid_file: state.join(crate::DAEMON_UPDATE_PID_FILE_NAME),
        operation_lock_file: state.join(crate::OPERATION_LOCK_FILE_NAME),
        settings_file: state.join(crate::SETTINGS_FILE_NAME),
        managed_codex_bin: crate::managed_install::managed_codex_bin(home),
    }
}

fn selected(daemon: &Daemon, bytes: &[u8]) {
    std::fs::create_dir_all(daemon.managed_codex_bin.parent().unwrap()).unwrap();
    std::fs::write(&daemon.managed_codex_bin, bytes).unwrap();
}

fn copy_test_executable(path: &Path, overlay: &[u8]) {
    // Even a regressed guard may probe this fixture on Windows. Keep an actual
    // native executable and vary only its trailing overlay for identity tests.
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::copy(std::env::current_exe().unwrap(), path).unwrap();
    if !overlay.is_empty() {
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        std::io::Write::write_all(&mut file, overlay).unwrap();
    }
}

#[tokio::test]
async fn stopped_mismatched_selection_is_not_launched_or_replaced() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path()).await;
    copy_test_executable(
        &daemon.managed_codex_bin,
        b"selected older-generation overlay",
    );
    let selected_before = crate::managed_install::executable_identity(&daemon.managed_codex_bin)
        .await
        .unwrap();
    let cli = home.path().join("new-cli.exe");
    copy_test_executable(&cli, b"");
    let cli_before = crate::managed_install::executable_identity(&cli)
        .await
        .unwrap();
    assert!(!selected_before.same_contents(&cli_before));
    let recovery = daemon.recovery_file().unwrap();
    std::fs::create_dir_all(recovery.parent().unwrap()).unwrap();
    std::fs::write(&recovery, b"preserve recovery").unwrap();
    let settings = DaemonSettings::default();
    settings.save(&daemon.settings_file).await.unwrap();
    let settings_bytes = std::fs::read(&daemon.settings_file).unwrap();
    let _operation_lock = daemon.acquire_operation_lock().await.unwrap();
    let error = stage(
        "stopped CLI start",
        daemon.start_for_cli(&BTreeMap::new(), &cli),
    )
    .await
    .unwrap_err();
    let mismatch = error.downcast_ref::<CliDaemonMismatch>().unwrap();
    assert_eq!(mismatch.cli_path, cli);
    assert_eq!(mismatch.managed_path, daemon.managed_codex_bin);
    assert!(error.to_string().contains("--from-cli --yes --no-force"));
    let selected_after = crate::managed_install::executable_identity(&daemon.managed_codex_bin)
        .await
        .unwrap();
    assert!(selected_before.same_contents(&selected_after));
    assert!(
        cli_before.same_contents(
            &crate::managed_install::executable_identity(&cli)
                .await
                .unwrap()
        )
    );
    assert_eq!(
        std::fs::read(&daemon.settings_file).unwrap(),
        settings_bytes
    );
    assert_eq!(std::fs::read(recovery).unwrap(), b"preserve recovery");
    assert!(!daemon.pid_file.exists());
    assert!(!daemon.update_pid_file.exists());
}

#[tokio::test]
async fn copied_matching_binary_and_missing_selection_keep_start_policy() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path()).await;
    let settings = DaemonSettings::default();
    let cli = home.path().join("cli");
    std::fs::write(&cli, b"same executable contents").unwrap();
    let _operation_lock = daemon.acquire_operation_lock().await.unwrap();
    daemon
        .ensure_cli_matches_stopped_installation(&settings, &cli)
        .await
        .unwrap();
    assert!(!daemon.managed_codex_bin.exists());
    selected(&daemon, b"same executable contents");
    daemon
        .ensure_cli_matches_stopped_installation(&settings, &cli)
        .await
        .unwrap();
    assert!(!daemon.pid_file.exists());
}

#[tokio::test]
async fn identity_io_failure_is_not_reported_as_mismatch() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path()).await;
    selected(&daemon, b"selected executable");
    let _operation_lock = daemon.acquire_operation_lock().await.unwrap();
    let error = daemon
        .ensure_cli_matches_stopped_installation(
            &DaemonSettings::default(),
            &home.path().join("missing-cli"),
        )
        .await
        .unwrap_err();
    assert!(!error.is::<CliDaemonMismatch>());
    assert!(error.to_string().contains("failed to read executable"));
    assert!(!daemon.pid_file.exists());
}

#[tokio::test]
async fn cli_start_waits_for_installer_and_checks_its_new_selection() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path()).await;
    copy_test_executable(&daemon.managed_codex_bin, b"");
    let cli = home.path().join("cli.exe");
    copy_test_executable(&cli, b"");
    let cli_before = crate::managed_install::executable_identity(&cli)
        .await
        .unwrap();
    let initially_selected = crate::managed_install::executable_identity(&daemon.managed_codex_bin)
        .await
        .unwrap();
    assert!(initially_selected.same_contents(&cli_before));
    let recovery = daemon.recovery_file().unwrap();
    std::fs::create_dir_all(recovery.parent().unwrap()).unwrap();
    std::fs::write(&recovery, b"preserve recovery").unwrap();
    let _operation_lock = daemon.acquire_operation_lock().await.unwrap();
    let installer =
        crate::install_lock::acquire_install_lock(&home.path().join("packages/app-server-daemon"))
            .await
            .unwrap();
    let features = BTreeMap::new();
    let start = daemon.start_for_cli(&features, &cli);
    tokio::pin!(start);
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), &mut start)
            .await
            .is_err(),
        "start must wait for installer quiescence"
    );
    // The installer completes a different selection while owning its lock.
    copy_test_executable(
        &daemon.managed_codex_bin,
        b"installer-selected different generation overlay",
    );
    let selected_by_installer =
        crate::managed_install::executable_identity(&daemon.managed_codex_bin)
            .await
            .unwrap();
    assert!(!selected_by_installer.same_contents(&cli_before));
    drop(installer);
    let error = tokio::time::timeout(std::time::Duration::from_secs(5), &mut start)
        .await
        .unwrap()
        .unwrap_err();
    assert!(error.is::<CliDaemonMismatch>());
    let selected_after = crate::managed_install::executable_identity(&daemon.managed_codex_bin)
        .await
        .unwrap();
    assert!(selected_by_installer.same_contents(&selected_after));
    assert!(
        cli_before.same_contents(
            &crate::managed_install::executable_identity(&cli)
                .await
                .unwrap()
        )
    );
    assert!(!daemon.pid_file.exists());
    assert_eq!(std::fs::read(recovery).unwrap(), b"preserve recovery");
}

#[tokio::test]
async fn starting_reservation_is_preserved_without_comparing_cli() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path()).await;
    selected(&daemon, b"old daemon");
    let reservation = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(daemon.pid_file.with_extension("pid.lock"))
        .unwrap();
    reservation.try_lock().unwrap();
    let _operation_lock = daemon.acquire_operation_lock().await.unwrap();
    // A missing CLI proves that the starting branch bypasses identity I/O.
    daemon
        .ensure_cli_matches_stopped_installation(
            &DaemonSettings::default(),
            &home.path().join("missing-cli"),
        )
        .await
        .unwrap();
    assert!(!daemon.pid_file.exists());
    assert_eq!(
        std::fs::read(&daemon.managed_codex_bin).unwrap(),
        b"old daemon"
    );
}

#[tokio::test]
async fn existing_socket_is_preserved_without_comparing_cli() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path()).await;
    selected(&daemon, b"old daemon");
    let mut server = control_server(&daemon, "0.152.0").await;
    let _operation_lock = daemon.acquire_operation_lock().await.unwrap();
    daemon
        .ensure_cli_matches_stopped_installation(
            &DaemonSettings::default(),
            &home.path().join("missing-cli"),
        )
        .await
        .unwrap();
    stage("control server completion", &mut server.0)
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(&daemon.managed_codex_bin).unwrap(),
        b"old daemon"
    );
    assert!(!daemon.pid_file.exists());
}

async fn control_server(daemon: &Daemon, version: &str) -> ControlServer {
    use futures::SinkExt;
    use futures::StreamExt;

    let mut listener = stage(
        "UDS bind",
        codex_uds::UnixListener::bind(&daemon.socket_path),
    )
    .await
    .unwrap();
    let response = serde_json::json!({
        "userAgent": format!("codex_app_server_daemon/{version}"),
        "codexHome": daemon.settings_file.parent().unwrap().parent().unwrap(),
        "platformFamily": std::env::consts::FAMILY,
        "platformOs": std::env::consts::OS,
        "supportsSubagentManagement": false
    });
    // Validate the actual public DTO before opening a protocol exchange. Missing
    // optional capabilities remain absent, rather than falsely claiming support.
    let response: codex_app_server_protocol::InitializeResponse =
        serde_json::from_value(response).expect("valid synthetic initialize response");
    let response = serde_json::json!({"id": 1, "result": response}).to_string();
    ControlServer(tokio::spawn(async move {
        let stream = stage("UDS accept", listener.accept()).await.unwrap();
        let mut socket = stage(
            "WebSocket handshake",
            tokio_tungstenite::accept_async(stream),
        )
        .await
        .unwrap();
        stage("initialize request", socket.next())
            .await
            .unwrap()
            .unwrap();
        stage(
            "initialize response",
            socket.send(tokio_tungstenite::tungstenite::Message::Text(
                response.into(),
            )),
        )
        .await
        .unwrap();
        let initialized = stage("initialized notification", socket.next())
            .await
            .unwrap()
            .unwrap();
        let tokio_tungstenite::tungstenite::Message::Text(initialized) = initialized else {
            panic!("expected initialized notification");
        };
        let initialized: serde_json::Value = serde_json::from_str(&initialized).unwrap();
        assert_eq!(initialized["method"], "initialized");
    }))
}

#[test]
fn automatic_old_or_unknown_servers_do_not_authorize_updater_changes() {
    for actual in [
        None,
        Some("0.166.0+native-r28.20261029"),
        Some(env!("CARGO_PKG_VERSION")),
    ] {
        assert_eq!(
            super::should_manage_updater(true, actual),
            actual == Some(env!("CARGO_PKG_VERSION"))
        );
        assert!(super::should_manage_updater(false, actual));
    }
}

#[tokio::test]
async fn automatic_old_server_preserves_updater_state_for_both_update_settings() {
    for auto_update_enabled in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let daemon = daemon(home.path()).await;
        // The lifecycle output probes this file with --version. Use an actual
        // PE/libtest executable; the old advertised socket runtime stays R28.
        std::fs::create_dir_all(daemon.managed_codex_bin.parent().unwrap()).unwrap();
        std::fs::copy(std::env::current_exe().unwrap(), &daemon.managed_codex_bin).unwrap();
        let selected_before =
            crate::managed_install::executable_identity(&daemon.managed_codex_bin)
                .await
                .unwrap();
        std::fs::write(
            &daemon.settings_file,
            serde_json::json!({
                "updater": {"autoUpdateEnabled": auto_update_enabled}
            })
            .to_string(),
        )
        .unwrap();
        // The persisted value is preserved below, but Arcvera always disables
        // inherited automatic updates when applying settings.
        assert!(!daemon.load_settings().await.unwrap().auto_update_enabled);
        let settings_bytes = std::fs::read(&daemon.settings_file).unwrap();
        // This is a synthetic managed starting reservation, never a user process.
        let reservation = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(daemon.pid_file.with_extension("pid.lock"))
            .unwrap();
        reservation.try_lock().unwrap();
        std::fs::write(&daemon.update_pid_file, b"").unwrap();
        let updater_log = daemon.update_pid_file.with_extension("stderr.log");
        std::fs::write(&updater_log, b"preserve synthetic updater state").unwrap();
        let mut server = control_server(&daemon, "0.166.0+native-r28.20261029").await;
        let _operation_lock = daemon.acquire_operation_lock().await.unwrap();
        let output = stage(
            "existing old CLI start",
            daemon.start_for_cli(&BTreeMap::new(), &home.path().join("missing-cli")),
        )
        .await
        .unwrap();
        stage("control server completion", &mut server.0)
            .await
            .unwrap();
        assert_eq!(output.status, crate::LifecycleStatus::AlreadyRunning);
        assert_eq!(output.backend, Some(crate::BackendKind::Pid));
        assert_eq!(
            output.app_server_version.as_deref(),
            Some("0.166.0+native-r28.20261029")
        );
        assert_eq!(std::fs::read(&daemon.update_pid_file).unwrap(), b"");
        assert_eq!(
            std::fs::read(updater_log).unwrap(),
            b"preserve synthetic updater state"
        );
        assert_eq!(
            std::fs::read(&daemon.settings_file).unwrap(),
            settings_bytes
        );
        let selected_after = crate::managed_install::executable_identity(&daemon.managed_codex_bin)
            .await
            .unwrap();
        assert!(selected_before.same_contents(&selected_after));
        assert!(!daemon.pid_file.exists());
    }
}
