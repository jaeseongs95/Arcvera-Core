use super::Process;
use pretty_assertions::assert_eq;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::FromRawHandle;
use std::os::windows::io::OwnedHandle;
use std::os::windows::process::CommandExt;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use windows_sys::Win32::Foundation::ERROR_ACCESS_DENIED;
use windows_sys::Win32::Foundation::GetHandleInformation;
use windows_sys::Win32::Foundation::HANDLE_FLAG_INHERIT;
use windows_sys::Win32::Foundation::SetHandleInformation;
use windows_sys::Win32::System::JobObjects::IsProcessInJob;
use windows_sys::Win32::System::Threading::CREATE_NO_WINDOW;
use windows_sys::Win32::System::Threading::DETACHED_PROCESS;
use windows_sys::Win32::System::Threading::TerminateProcess;

#[tokio::test]
async fn captured_stdio_closes_while_child_is_alive() {
    const TEST: &str = "backend::windows::tests::captured_stdio_closes_while_child_is_alive";
    const ROLE: &str = "CODEX_TEST_STDIO_ROLE";
    const HOME: &str = "CODEX_TEST_STDIO_HOME";
    const REMOVED: &str = "CODEX_TEST_STDIO_REMOVED";
    const CHANGED: &str = "CODEX_TEST_STDIO_CHANGED";
    const SKIPS: &[&str] = &["한글 space", "a\"b", "ends in slash \\", "\\\"quoted"];
    let executable = std::env::current_exe().expect("test executable");

    if let Ok(role) = std::env::var(ROLE) {
        let home = std::path::PathBuf::from(std::env::var_os(HOME).expect("test home"));
        if role == "child" {
            assert_eq!(std::env::current_dir().unwrap(), home);
            assert!(std::env::var_os(REMOVED).is_none());
            assert_eq!(std::env::var(CHANGED).unwrap(), "변경된 value");
            let args = std::env::args().collect::<Vec<_>>();
            for skip in SKIPS {
                assert!(args.windows(2).any(|a| a[0] == "--skip" && a[1] == *skip));
            }
            eprintln!("child stderr");
            std::fs::write(home.join("ready"), []).unwrap();
            // A file models the daemon's normal shutdown request. Keep a fallback
            // deadline so even a broken fixture cannot leave an unbounded child.
            let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
            while !home.join("shutdown").exists() && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            eprintln!("child shutdown");
            return;
        }

        // Deliberately retain the original captured pipe handles as inheritable.
        // Rust's nested launcher duplicates stdio, so the originals are then
        // invisible to GetStdHandle in the inner launcher.
        let handles = [
            std::io::stdout().as_raw_handle(),
            std::io::stderr().as_raw_handle(),
        ];
        for handle in handles {
            assert_ne!(
                unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) },
                0
            );
        }
        if role == "wrapper" {
            let status = tokio::process::Command::new(executable)
                .args(["--exact", TEST, "--nocapture"])
                .env(ROLE, "launcher")
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .await
                .unwrap();
            assert!(status.success());
            return;
        }

        let unrelated = std::fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(home.join("unrelated"))
            .unwrap();
        assert_ne!(
            unsafe {
                SetHandleInformation(
                    unrelated.as_raw_handle(),
                    HANDLE_FLAG_INHERIT,
                    HANDLE_FLAG_INHERIT,
                )
            },
            0
        );
        let mut command = tokio::process::Command::new(executable);
        let stderr_log = std::fs::File::create(home.join("child.log")).unwrap();
        command
            .args(["--exact", TEST, "--nocapture"])
            .env(ROLE, "child")
            .env_remove(REMOVED)
            .env(CHANGED, "변경된 value")
            .current_dir(&home)
            .env(CHANGED.to_ascii_lowercase(), "변경된 value");
        for skip in SKIPS {
            command.args(["--skip", skip]);
        }
        // The CI job may forbid breakaway; it does not affect inheritance.
        let child =
            super::spawn_without_inheriting_stdio(command.as_std(), &stderr_log, DETACHED_PROCESS)
                .unwrap();
        let pid = child.id().unwrap();
        let started = Process::open(pid).unwrap().unwrap().start_time().unwrap();
        std::fs::write(
            home.join("pid"),
            serde_json::to_vec(&(pid, started)).unwrap(),
        )
        .unwrap();
        let unchanged = handles.into_iter().all(|handle| {
            let mut flags = 0;
            (unsafe { GetHandleInformation(handle, &mut flags) }) != 0
                && flags & HANDLE_FLAG_INHERIT != 0
        });
        std::fs::write(home.join("inheritance-preserved"), [u8::from(unchanged)]).unwrap();
        println!("launcher stdout");
        eprintln!("launcher stderr");
        return;
    }

    for role in ["wrapper", "launcher"] {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("경로 with spaces");
        std::fs::create_dir(&home).unwrap();
        std::fs::write(home.join("unrelated"), []).unwrap();
        let mut launcher = tokio::process::Command::new(&executable)
            .args(["--exact", TEST, "--nocapture"])
            .env(ROLE, role)
            .env(HOME, &home)
            .env(REMOVED, "must be removed")
            .env(CHANGED, "must be changed")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let status = tokio::time::timeout(Duration::from_secs(10), launcher.wait())
            .await
            .expect("launcher exit")
            .unwrap();
        let (pid, started): (u32, String) =
            serde_json::from_slice(&std::fs::read(home.join("pid")).unwrap()).unwrap();
        let child = Process::open(pid).unwrap().expect("live child");
        assert_eq!(
            child.start_time().unwrap(),
            started,
            "fixture PID was reused"
        );
        let mut stdout = String::new();
        let mut stderr = String::new();
        let captured = tokio::time::timeout(Duration::from_secs(5), async {
            while !home.join("ready").exists() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            tokio::try_join!(
                launcher
                    .stdout
                    .as_mut()
                    .unwrap()
                    .read_to_string(&mut stdout),
                launcher
                    .stderr
                    .as_mut()
                    .unwrap()
                    .read_to_string(&mut stderr),
            )
        })
        .await;
        let still_running = child.is_running();
        let unrelated = std::fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(home.join("unrelated"));
        std::fs::write(home.join("shutdown"), []).expect("request normal child shutdown");
        let stopped = tokio::time::timeout(Duration::from_secs(5), async {
            while child.is_running().unwrap() {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await;
        // Cleanup only this identity if the normal shutdown fixture failed.
        if stopped.is_err() {
            child
                .terminate()
                .expect("cleanup child after shutdown timeout");
        }

        captured
            .expect("launcher output must close while child is alive")
            .unwrap();
        stopped.expect("child must exit normally after shutdown request");
        assert!(status.success(), "{role}: {stderr}");
        assert!(
            still_running.unwrap(),
            "{role}: child exited before its launcher output closed"
        );
        assert!(
            unrelated.is_ok(),
            "{role}: child inherited an unrelated handle"
        );
        assert_eq!(
            std::fs::read(home.join("inheritance-preserved")).unwrap(),
            [1]
        );
        assert!(stdout.contains("launcher stdout"));
        assert!(stderr.contains("launcher stderr"));
        let child_log = std::fs::read_to_string(home.join("child.log")).unwrap();
        assert!(child_log.contains("child stderr"));
        assert!(child_log.contains("child shutdown"));
    }
}

#[test]
fn failed_detached_spawn_releases_owned_handles() {
    const ROLE: &str = "CODEX_TEST_FAILED_DETACHED_SPAWN";
    const TEST: &str = "backend::windows::tests::failed_detached_spawn_releases_owned_handles";
    if std::env::var_os(ROLE).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(ROLE, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let home = tempfile::tempdir().unwrap();
    let stderr = std::fs::File::create(home.path().join("child.log")).unwrap();
    let command = std::process::Command::new(home.path().join("missing.exe"));
    let count = || {
        let mut count = 0;
        assert_ne!(
            unsafe {
                windows_sys::Win32::System::Threading::GetProcessHandleCount(
                    super::GetCurrentProcess(),
                    &mut count,
                )
            },
            0
        );
        count
    };
    // Diagnostic snapshots are limited to this isolated process. Record numeric
    // handle types/access only; never query object names or another process.
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct HandleEntry {
        handle: super::HANDLE,
        handle_count: usize,
        pointer_count: usize,
        granted_access: u32,
        object_type: u32,
        attributes: u32,
        reserved: u32,
    }
    #[link(name = "ntdll")]
    unsafe extern "system" {
        #[link_name = "NtQueryInformationProcess"]
        fn query_process_handles(
            process: super::HANDLE,
            class: u32,
            information: *mut std::ffi::c_void,
            size: u32,
            returned: *mut u32,
        ) -> i32;
    }
    fn handles() -> Result<Vec<(usize, u32, u32)>, i32> {
        let mut storage = vec![0usize; 8192];
        let mut returned = 0;
        // ProcessHandleInformation returns two ULONG_PTR header fields followed
        // by PROCESS_HANDLE_TABLE_ENTRY_INFO entries. Aligned storage is bounded.
        let status = unsafe {
            query_process_handles(
                super::GetCurrentProcess(),
                51,
                storage.as_mut_ptr().cast(),
                std::mem::size_of_val(storage.as_slice()) as u32,
                &mut returned,
            )
        };
        if status < 0 {
            return Err(status);
        }
        let length = storage[0];
        let bytes = length
            .checked_mul(std::mem::size_of::<HandleEntry>())
            .and_then(|size| size.checked_add(2 * std::mem::size_of::<usize>()));
        if bytes.is_none_or(|size| size > std::mem::size_of_val(storage.as_slice())) {
            return Err(0xC0000004u32 as i32);
        }
        let entries = unsafe {
            std::slice::from_raw_parts(storage.as_ptr().add(2).cast::<HandleEntry>(), length)
        };
        Ok(entries
            .iter()
            .map(|entry| {
                (
                    entry.handle as usize,
                    entry.object_type,
                    entry.granted_access,
                )
            })
            .collect())
    }

    // Keep the original pre-first-call baseline and 32-call assertion. Additional
    // observations do not warm up the helper or alter the expected handle count.
    let before = count();
    let baseline_handles = handles();
    let after_snapshot = count();
    let mut counts = Vec::new();
    let mut snapshots = Vec::new();
    let mut after_32 = None;
    for iteration in 1..=64 {
        let error = match super::spawn_without_inheriting_stdio(&command, &stderr, DETACHED_PROCESS)
        {
            Ok(_) => panic!("missing executable unexpectedly launched"),
            Err(error) => error,
        };
        let after_spawn = count();
        assert!(error.root_cause().is::<std::io::Error>(), "{error:#}");
        let after_error_chain = count();
        drop(error);
        let after_error_drop = count();
        if matches!(iteration, 1 | 2 | 32 | 64) {
            counts.push((iteration, after_spawn, after_error_chain, after_error_drop));
            snapshots.push((iteration, handles()));
        }
        if iteration == 32 {
            after_32 = Some(after_error_drop);
        }
    }
    eprintln!(
        "failed spawn handle counts: baseline={before}, after_baseline_snapshot={after_snapshot}, (iteration, after_spawn, after_error_chain, after_error_drop)={counts:?}"
    );
    match &baseline_handles {
        Ok(baseline) => {
            let file_type = baseline
                .iter()
                .find(|(handle, _, _)| *handle == stderr.as_raw_handle() as usize)
                .map(|(_, kind, _)| *kind);
            for (iteration, snapshot) in snapshots {
                match snapshot {
                    Ok(current) => {
                        let added = current
                            .iter()
                            .filter(|entry| !baseline.contains(entry))
                            .map(|(handle, kind, access)| {
                                let file_class = (Some(*kind) == file_type).then(|| unsafe {
                                    windows_sys::Win32::Storage::FileSystem::GetFileType(
                                        *handle as super::HANDLE,
                                    )
                                });
                                (*handle, *kind, *access, file_class)
                            })
                            .collect::<Vec<_>>();
                        eprintln!(
                            "handle snapshot iteration={iteration} total={} baseline_file_type={file_type:?} added_or_changed=(value,type,access,file_class){added:?}",
                            current.len()
                        );
                    }
                    Err(status) => {
                        eprintln!("handle snapshot iteration={iteration} status={status:#x}")
                    }
                }
            }
        }
        Err(status) => eprintln!("baseline handle snapshot status={status:#x}"),
    }
    assert_eq!(
        after_32.expect("observed the original 32-call boundary"),
        before,
        "failed detached spawns leaked owned handles"
    );
}

#[test]
fn detached_launch_preflight_rejects_restrictive_job() {
    const CHILD: &str = "CODEX_TEST_RESTRICTIVE_LAUNCH_JOB";
    let executable = std::env::current_exe().expect("test executable");
    if std::env::var_os(CHILD).is_some() {
        let job = unsafe { super::CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        assert!(!job.is_null());
        let job = unsafe {
            <std::os::windows::io::OwnedHandle as std::os::windows::io::FromRawHandle>::from_raw_handle(job)
        };
        assert_ne!(
            unsafe {
                super::AssignProcessToJobObject(job.as_raw_handle(), super::GetCurrentProcess())
            },
            0
        );
        // A new job does not permit breakaway. Reject before any lifecycle mutation.
        let err = super::ensure_detached_launch(&executable).expect_err("breakaway forbidden");
        assert!(
            err.is::<super::DetachedLaunchRestricted>(),
            "expected a classified job restriction: {err:#}"
        );
        // File access failures must still fail normally, even in a restricted
        // job: a CLI must not hide corrupt or inaccessible installations.
        let temp = tempfile::tempdir().expect("test directory");
        let err =
            super::ensure_detached_launch(temp.path()).expect_err("directory is not an executable");
        assert!(!err.is::<super::DetachedLaunchRestricted>(), "{err:#}");
        return;
    }
    let output = std::process::Command::new(executable)
        .args([
            "--exact",
            "backend::windows::tests::detached_launch_preflight_rejects_restrictive_job",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .expect("isolated job test");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[test]
fn detached_launch_preflight_allows_residual_job() {
    const CHILD: &str = "CODEX_TEST_RESIDUAL_LAUNCH_JOB";
    let executable = std::env::current_exe().expect("test executable");
    if std::env::var_os(CHILD).is_some() {
        let mut jobs = Vec::new();
        for inner in [false, true] {
            let job = unsafe { super::CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            assert!(!job.is_null());
            let job = unsafe { OwnedHandle::from_raw_handle(job) };
            if inner {
                let mut limits: super::JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
                    unsafe { std::mem::zeroed() };
                limits.BasicLimitInformation.LimitFlags = super::JOB_OBJECT_LIMIT_BREAKAWAY_OK;
                assert_ne!(
                    unsafe {
                        super::SetInformationJobObject(
                            job.as_raw_handle(),
                            super::JobObjectExtendedLimitInformation,
                            (&limits as *const super::JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                            std::mem::size_of_val(&limits) as u32,
                        )
                    },
                    0
                );
            }
            assert_ne!(
                unsafe {
                    super::AssignProcessToJobObject(job.as_raw_handle(), super::GetCurrentProcess())
                },
                0
            );
            jobs.push(job);
        }

        let mut probe = std::process::Command::new(&executable)
            .creation_flags(
                super::CREATE_SUSPENDED
                    | super::DETACHED_PROCESS
                    | super::CREATE_BREAKAWAY_FROM_JOB,
            )
            .spawn()
            .expect("breakaway probe");
        let mut in_outer = 0;
        let mut in_inner = 0;
        let outer_checked = unsafe {
            IsProcessInJob(
                probe.as_raw_handle(),
                jobs[0].as_raw_handle(),
                &mut in_outer,
            )
        };
        let inner_checked = unsafe {
            IsProcessInJob(
                probe.as_raw_handle(),
                jobs[1].as_raw_handle(),
                &mut in_inner,
            )
        };
        probe.kill().expect("terminate probe");
        probe.wait().expect("reap probe");
        assert_ne!(outer_checked, 0);
        assert_ne!(inner_checked, 0);
        assert_ne!(in_outer, 0, "probe must retain its outer job");
        assert_eq!(in_inner, 0, "probe must leave its inner job");
        super::ensure_detached_launch(&executable).expect("residual job must be accepted");

        if let Err(err) = super::ensure_not_elevated() {
            assert!(err.to_string().contains("non-elevated terminal"));
            return;
        }
        let temp = tempfile::tempdir().expect("temp dir");
        let mut backend = crate::backend::pid::PidBackend::new(
            executable.clone(),
            temp.path().join("state").join("daemon.pid"),
            /*remote_control_enabled*/ false,
        );
        backend.test_command = Some("backend::pid::tests::managed_child_fixture");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let pid = runtime
            .block_on(backend.start())
            .expect("daemon launch")
            .expect("new daemon pid");
        let process = Process::open(pid)
            .expect("open daemon")
            .expect("live daemon");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !temp.path().join("state/workdir/launched.cwd").exists()
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let ready = temp.path().join("state/workdir/launched.cwd").exists();
        let outer_checked = unsafe {
            IsProcessInJob(
                process.0.as_raw_handle(),
                jobs[0].as_raw_handle(),
                &mut in_outer,
            )
        };
        let inner_checked = unsafe {
            IsProcessInJob(
                process.0.as_raw_handle(),
                jobs[1].as_raw_handle(),
                &mut in_inner,
            )
        };
        process.terminate().expect("terminate daemon fixture");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while process.is_running().expect("daemon liveness") && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(!process.is_running().expect("daemon liveness"));
        assert!(ready, "daemon fixture did not start");
        assert_ne!(outer_checked, 0);
        assert_ne!(inner_checked, 0);
        assert_ne!(in_outer, 0, "daemon must retain its outer job");
        assert_eq!(in_inner, 0, "daemon must leave its inner job");
        return;
    }
    let output = std::process::Command::new(executable)
        .args([
            "--exact",
            "backend::windows::tests::detached_launch_preflight_allows_residual_job",
            "--nocapture",
        ])
        .env(CHILD, "1")
        .output()
        .expect("isolated job test");
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
}

#[tokio::test]
async fn identity_queries_do_not_require_termination_access() {
    let mut child = tokio::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep 60",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(true)
        .spawn()
        .expect("child");
    let process = Process::open(child.id().expect("pid"))
        .expect("query handle")
        .expect("live process");
    assert!(!process.start_time().expect("creation time").is_empty());
    assert!(process.is_running().expect("liveness"));
    // Check the rights on the actual query handle, independent of privileges
    // that could let the caller reopen the process with termination access.
    assert_eq!(
        unsafe {
            TerminateProcess(process.0.as_raw_handle(), /*uexitcode*/ 1)
        },
        0
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(ERROR_ACCESS_DENIED as i32)
    );
    assert!(
        process
            .is_running()
            .expect("query must not terminate child")
    );
    child
        .kill()
        .await
        .expect("cleanup through original spawn handle");
}
