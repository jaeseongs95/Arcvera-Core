// Modified for OpenDIY Codex R36 (2026-11-03); see MODIFICATIONS.md.
use std::fs;
use std::path::Path;
use std::sync::Arc;

use codex_protocol::ThreadId;
use tempfile::TempDir;

use super::COORDINATION_LOCK_FILE;
use super::WRITER_LOCK_DIR;
use super::WriterLockCoordinator;
use pretty_assertions::assert_eq;
use std::io::ErrorKind;

#[test]
fn writer_locks_reject_competing_owners_and_release_their_files() {
    let home = TempDir::new().expect("temp dir");
    let primary = Arc::new(WriterLockCoordinator::new(home.path()));
    let secondary = Arc::new(WriterLockCoordinator::new(home.path()));
    let thread_id = ThreadId::default();
    let other_thread_id = ThreadId::default();

    let owner = primary.acquire(thread_id).expect("acquire writer lock");
    let lock_path = home
        .path()
        .join(WRITER_LOCK_DIR)
        .join(format!("{thread_id}.lock"));
    assert!(lock_path.exists());

    let err = match secondary.acquire(thread_id) {
        Ok(_) => panic!("competing owner should fail"),
        Err(err) => err,
    };
    assert_eq!(err.kind(), ErrorKind::WouldBlock);
    let other_owner = secondary
        .acquire(other_thread_id)
        .expect("other thread should acquire its own lock");

    drop(owner);
    assert!(!lock_path.exists());
    let next_owner = secondary
        .acquire(thread_id)
        .expect("released thread should accept another owner");
    drop(next_owner);
    drop(other_owner);

    let entries = fs::read_dir(home.path().join(WRITER_LOCK_DIR))
        .expect("read lock directory")
        .map(|entry| entry.expect("lock directory entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(entries, vec![COORDINATION_LOCK_FILE]);
}

#[test]
fn first_acquisition_removes_stale_locks_without_removing_active_locks() {
    let home = TempDir::new().expect("temp dir");
    let primary = Arc::new(WriterLockCoordinator::new(home.path()));
    let active_thread_id = ThreadId::default();
    let active_owner = primary
        .acquire(active_thread_id)
        .expect("acquire active writer lock");

    let stale_thread_id = ThreadId::default();
    let stale_path = home
        .path()
        .join(WRITER_LOCK_DIR)
        .join(format!("{stale_thread_id}.lock"));
    fs::File::create(&stale_path).expect("create stale writer lock");

    let secondary = Arc::new(WriterLockCoordinator::new(home.path()));
    let secondary_owner = secondary
        .acquire(ThreadId::default())
        .expect("acquire writer lock after cleanup");

    assert!(!stale_path.exists());
    let err = match secondary.acquire(active_thread_id) {
        Ok(_) => panic!("active writer should survive cleanup"),
        Err(err) => err,
    };
    assert_eq!(err.kind(), ErrorKind::WouldBlock);

    drop(secondary_owner);
    drop(active_owner);
}

#[test]
fn publication_skips_live_writers_and_keeps_coordination_locked() {
    let home = TempDir::new().expect("temp dir");
    let publisher = Arc::new(WriterLockCoordinator::new(home.path()));
    let writer = Arc::new(WriterLockCoordinator::new(home.path()));
    let thread_id = ThreadId::default();
    let owner = writer.acquire(thread_id).expect("writer owns thread");
    assert!(
        publisher
            .try_acquire_for_publication(thread_id)
            .unwrap()
            .is_none()
    );
    drop(owner);

    let publication = publisher
        .try_acquire_for_publication(thread_id)
        .unwrap()
        .unwrap();
    let coordination = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(writer.directory.join(COORDINATION_LOCK_FILE))
        .unwrap();
    assert!(matches!(
        coordination.try_lock(),
        Err(fs::TryLockError::WouldBlock)
    ));
    let error = writer
        .acquire(ThreadId::default())
        .err()
        .expect("coordination contention must return an error instead of hanging");
    assert_eq!(error.kind(), ErrorKind::TimedOut);
    drop(publication);
    coordination
        .try_lock()
        .expect("publication releases coordination");
    drop(coordination);
    // An existing but unlocked file is also idle; file existence is not ownership.
    fs::File::create(writer.directory.join(format!("{thread_id}.lock"))).unwrap();
    drop(
        publisher
            .try_acquire_for_publication(thread_id)
            .unwrap()
            .expect("stale lock is idle"),
    );
    assert!(writer.acquire(thread_id).is_ok());
}

#[test]
fn reference_leases_share_with_active_writers_and_never_split_the_namespace() {
    let home = TempDir::new().expect("isolated home");
    let first = Arc::new(WriterLockCoordinator::new(home.path()));
    let second = Arc::new(WriterLockCoordinator::new(home.path()));
    let parent = ThreadId::new();
    let writer = first.acquire(parent).expect("active parent writer");
    let prepared = first
        .reserve_reference_publication()
        .expect("prepare during active writer");
    let publishing = second
        .reserve_reference_publication()
        .expect("concurrent reference publication");
    assert_eq!(
        second.reserve_reference_deletion().unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    // Writer acquisition also performs stale-file cleanup. It must leave the graph inode alone.
    let third = Arc::new(WriterLockCoordinator::new(home.path()));
    let unrelated_writer = third
        .acquire(ThreadId::new())
        .expect("unrelated writer remains available");
    assert_eq!(
        third.reserve_reference_deletion().unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    drop(publishing);
    drop(prepared);
    let deleting = second
        .reserve_reference_deletion()
        .expect("released reference reservations");
    assert_eq!(
        first.reserve_reference_publication().unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    assert!(
        first
            .try_acquire_for_publication(ThreadId::new())
            .unwrap()
            .is_none(),
        "compression must not move a reference during deletion's scan"
    );
    drop(deleting);
    drop(unrelated_writer);
    drop(writer);
    let shared = first
        .reserve_reference_publication()
        .expect("namespace remains usable");
    assert_eq!(
        second.reserve_reference_deletion().unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    drop(shared);
    second
        .reserve_reference_deletion()
        .expect("all guards release without coordination deadlock");
}

#[test]
fn reference_leases_cross_process_probe() {
    let Some(home) = std::env::var_os("CODEX_R36_REFERENCE_FIXTURE_HOME") else {
        return;
    };
    let coordinator = WriterLockCoordinator::new(Path::new(&home));
    if std::env::var_os("CODEX_R36_REFERENCE_FIXTURE_BLOCKED").is_some() {
        assert_eq!(
            coordinator.reserve_reference_deletion().unwrap_err().kind(),
            ErrorKind::WouldBlock
        );
    } else {
        coordinator
            .reserve_reference_deletion()
            .expect("parent released its lease");
    }
}

#[test]
fn reference_leases_are_enforced_across_processes() {
    let home = TempDir::new().expect("isolated home");
    let coordinator = WriterLockCoordinator::new(home.path());
    let shared = coordinator
        .reserve_reference_publication()
        .expect("parent publication lease");
    let run_probe = |blocked| {
        let mut command =
            std::process::Command::new(std::env::current_exe().expect("test executable"));
        command
            .args([
                "--exact",
                "writer_lock::tests::reference_leases_cross_process_probe",
                "--nocapture",
            ])
            .env("CODEX_R36_REFERENCE_FIXTURE_HOME", home.path());
        if blocked {
            command.env("CODEX_R36_REFERENCE_FIXTURE_BLOCKED", "1");
        } else {
            command.env_remove("CODEX_R36_REFERENCE_FIXTURE_BLOCKED");
        }
        let output = command.output().expect("run isolated child lock probe");
        assert!(
            output.status.success(),
            "child failed: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "probe must actually run"
        );
    };
    run_probe(true);
    drop(shared);
    run_probe(false);
}
