use super::*;

#[test]
fn readers_only_observe_complete_settings_during_atomic_replacement() {
    let home = tempfile::tempdir().unwrap();
    update(home.path(), Update::Logging { enabled: false }).unwrap();
    let done = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let mut reads = 0;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !done.load(std::sync::atomic::Ordering::Acquire) || reads == 0 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "bounded settings reader"
                );
                let settings: batch_log::Settings = toml::from_str(
                    &fs::read_to_string(home.path().join("cli-logging.toml")).unwrap(),
                )
                .unwrap();
                assert_eq!(settings.flush_seconds, 60);
                assert_eq!(settings.sample_seconds, 5);
                assert_eq!(settings.max_batch_bytes, 16 * 1024 * 1024);
                reads += 1;
            }
            reads
        });
        let writer_result = [true, false]
            .into_iter()
            .cycle()
            .take(20)
            .try_for_each(|enabled| update(home.path(), Update::Logging { enabled }).map(|_| ()));
        // Report the writer's original failure without leaving its reader spinning.
        done.store(true, std::sync::atomic::Ordering::Release);
        let reader_result = reader.join();
        writer_result.expect("settings publication must succeed with concurrent readers");
        assert!(reader_result.unwrap() > 0);
    });
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 2);
}

#[test]
fn open_reader_retains_old_settings_while_new_readers_see_replacement() {
    use std::io::Read;

    let home = tempfile::tempdir().unwrap();
    let logging = home.path().join("cli-logging.toml");
    update(home.path(), Update::Logging { enabled: false }).unwrap();
    let original = fs::read_to_string(&logging).unwrap();
    let mut old_reader = fs::File::open(&logging).unwrap();

    update(home.path(), Update::Logging { enabled: true }).unwrap();
    assert!(status(home.path()).unwrap().saved_logging.enabled);
    assert_eq!(
        fs::read_to_string(&logging).unwrap(),
        original.replace("false", "true")
    );
    let mut old_contents = String::new();
    old_reader.read_to_string(&mut old_contents).unwrap();
    assert_eq!(old_contents, original);
    drop(old_reader);
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 2);
}

#[cfg(windows)]
#[test]
fn reader_without_delete_sharing_rejects_publication_and_preserves_settings() {
    use std::os::windows::fs::OpenOptionsExt;

    let home = tempfile::tempdir().unwrap();
    let logging = home.path().join("cli-logging.toml");
    update(home.path(), Update::Logging { enabled: false }).unwrap();
    let original = fs::read(&logging).unwrap();
    // FILE_SHARE_READ | FILE_SHARE_WRITE deliberately excludes FILE_SHARE_DELETE.
    let reader = OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2)
        .open(&logging)
        .unwrap();
    let error = update(home.path(), Update::Logging { enabled: true }).unwrap_err();
    assert!(matches!(error.raw_os_error(), Some(5 | 32)), "{error}");
    assert_eq!(fs::read(&logging).unwrap(), original);
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 2);
    drop(reader);
    assert!(
        update(home.path(), Update::Logging { enabled: true })
            .unwrap()
            .saved_logging
            .enabled
    );
}

#[test]
fn edits_preserve_existing_logging_policy_comments_and_crlf() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    fs::create_dir(&home).unwrap();
    let original = format!(
        "# existing policy\r\nenabled = false # operator switch\r\ndestination = '{}'\r\nflush_seconds = 73\r\nsample_seconds = 11\r\nmax_batch_bytes = 65536\r\nrpc = false\r\nevents = false\r\nprocess = false\r\n",
        root.path().join("drive").display(),
    );
    let logging = home.join("cli-logging.toml");
    fs::write(&logging, &original).unwrap();
    let result = update(&home, Update::Logging { enabled: true }).unwrap();
    assert!(result.saved_logging.enabled);
    assert_eq!(result.saved_logging.flush_seconds, 73);
    assert_eq!(result.saved_logging.sample_seconds, 11);
    assert_eq!(result.saved_logging.max_batch_bytes, 65536);
    assert_eq!(
        result.saved_logging.destination,
        Some(root.path().join("drive"))
    );
    assert!(
        !result.saved_logging.rpc && !result.saved_logging.events && !result.saved_logging.process
    );
    assert!(!result.saved_cache.response && !result.saved_cache.wire);
    assert!(
        !result
            .next_process_mode_if_environment_unchanged
            .response_enabled
    );
    assert_eq!(
        fs::read_to_string(&logging).unwrap(),
        original.replace("enabled = false", "enabled = true")
    );
    assert!(!home.join("cli-cache-diagnostics.toml").exists());

    let cache = home.join("cli-cache-diagnostics.toml");
    let cache_original =
        "# cache only\r\nresponse = false # response choice\r\nwire = false # wire choice\r\n";
    fs::write(&cache, cache_original).unwrap();
    let logging_before = fs::read(&logging).unwrap();
    let result = update(
        &home,
        Update::Cache {
            response: true,
            wire: true,
        },
    )
    .unwrap();
    assert!(result.saved_cache.response && result.saved_cache.wire);
    assert!(
        result
            .next_process_mode_if_environment_unchanged
            .response_enabled
    );
    assert!(
        result
            .next_process_mode_if_environment_unchanged
            .wire_enabled
    );
    assert_eq!(fs::read(&logging).unwrap(), logging_before);
    assert_eq!(
        fs::read_to_string(cache).unwrap(),
        cache_original.replace("= false", "= true")
    );
    let disabled = update(&home, Update::Logging { enabled: false }).unwrap();
    assert!(disabled.saved_cache.response && disabled.saved_cache.wire);
    assert!(
        !disabled
            .next_process_mode_if_environment_unchanged
            .response_enabled
    );
    assert!(
        !disabled
            .next_process_mode_if_environment_unchanged
            .wire_enabled
    );
}

#[test]
fn invalid_settings_and_busy_writer_preserve_existing_files() {
    let root = tempfile::tempdir().unwrap();
    let absent = root.path().join("absent");
    let error = update(
        &absent,
        Update::Cache {
            response: false,
            wire: true,
        },
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(!absent.exists());
    let home = root.path();
    let logging = home.join("cli-logging.toml");
    let malformed = "enabled = 'not a boolean'\n";
    fs::write(&logging, malformed).unwrap();
    assert_eq!(
        status(home).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        update(home, Update::Logging { enabled: true })
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read_to_string(&logging).unwrap(), malformed);

    let invalid_policy = "enabled = false\nflush_seconds = 0\n";
    fs::write(&logging, invalid_policy).unwrap();
    assert_eq!(
        update(home, Update::Logging { enabled: true })
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(fs::read_to_string(&logging).unwrap(), invalid_policy);
    // Failure released its OS lock; a valid next request can take ownership.
    fs::write(&logging, "enabled = false\n").unwrap();
    let owner = OpenOptions::new()
        .read(true)
        .write(true)
        .open(home.join(LOCK_FILE))
        .unwrap();
    owner.try_lock().unwrap();
    assert_eq!(
        update(home, Update::Logging { enabled: true })
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(fs::read_to_string(&logging).unwrap(), "enabled = false\n");
    drop(owner);
    assert!(
        update(home, Update::Logging { enabled: true })
            .unwrap()
            .saved_logging
            .enabled
    );
    assert_eq!(
        fs::read_dir(home).unwrap().count(),
        2,
        "only settings and stable lock remain"
    );
}

#[test]
fn status_and_updates_do_not_initialize_or_change_process_mode() {
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "settings_control::tests::settings_process_mode_child",
            "--ignored",
            "--test-threads=1",
        ])
        .env_remove("APP_CACHE_WIRE_DIAGNOSTICS");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    assert!(command.status().unwrap().success());
}

#[test]
#[ignore = "isolated process for immutable startup state"]
fn settings_process_mode_child() {
    let home = tempfile::tempdir().unwrap();
    assert_eq!(cache_diagnostics::initialized_process_mode(), None);
    let initial = status(home.path()).unwrap();
    assert!(initial.current_process_startup_mode.is_none());
    assert!(initial.restart_required.is_none());
    assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
    let cache = update(
        home.path(),
        Update::Cache {
            response: true,
            wire: true,
        },
    )
    .unwrap();
    assert_eq!(cache.current_process_startup_mode, None);
    assert!(
        !cache.saved_logging.enabled,
        "cache selection must not enable general logging"
    );
    assert!(
        !cache
            .next_process_mode_if_environment_unchanged
            .wire_enabled
    );
    assert_eq!(cache_diagnostics::initialized_process_mode(), None);
    update(home.path(), Update::Logging { enabled: true }).unwrap();
    let current = cache_diagnostics::initialize(home.path(), true).unwrap();
    assert!(current.logging_enabled && current.response_enabled && current.wire_enabled);
    let after = update(
        home.path(),
        Update::Cache {
            response: false,
            wire: false,
        },
    )
    .unwrap();
    assert_eq!(after.current_process_startup_mode, Some(current));
    assert_eq!(after.restart_required, Some(true));
    assert!(
        !after
            .next_process_mode_if_environment_unchanged
            .response_enabled
    );
    assert_eq!(cache_diagnostics::process_mode(), current);
}
