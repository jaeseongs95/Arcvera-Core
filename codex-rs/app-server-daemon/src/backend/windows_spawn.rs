//! Native detached spawn with an explicit inherited-handle allowlist.
//! This consumes the daemon builder's program, regular arguments, environment
//! changes and cwd; stdio and creation flags are deliberately explicit.

use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs::File;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::io::FromRawHandle;
use std::os::windows::io::OwnedHandle;
use std::process::Command;

use anyhow::Context;
use anyhow::Result;
use windows_sys::Win32::Foundation::DUPLICATE_SAME_ACCESS;
use windows_sys::Win32::Foundation::DuplicateHandle;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Globalization::CompareStringOrdinal;
use windows_sys::Win32::System::Threading::CREATE_UNICODE_ENVIRONMENT;
use windows_sys::Win32::System::Threading::CreateProcessW;
use windows_sys::Win32::System::Threading::DeleteProcThreadAttributeList;
use windows_sys::Win32::System::Threading::EXTENDED_STARTUPINFO_PRESENT;
use windows_sys::Win32::System::Threading::GetCurrentProcess;
use windows_sys::Win32::System::Threading::InitializeProcThreadAttributeList;
use windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST;
use windows_sys::Win32::System::Threading::PROC_THREAD_ATTRIBUTE_HANDLE_LIST;
use windows_sys::Win32::System::Threading::STARTF_USESTDHANDLES;
use windows_sys::Win32::System::Threading::STARTUPINFOEXW;
use windows_sys::Win32::System::Threading::UpdateProcThreadAttribute;

/// The PID-managed builder uses inherited environment plus explicit deltas and
/// regular arguments, never env_clear/raw_arg or implicit stdio inheritance.
pub(in crate::backend) fn spawn_without_inheriting_stdio(
    command: &Command,
    stderr: &File,
    creation_flags: u32,
) -> Result<DetachedChild> {
    let mut application = utf16(command.get_program())?;
    application.push(0);
    let mut command_line = Vec::new();
    append_argument(command.get_program(), &mut command_line)?;
    for arg in command.get_args() {
        command_line.push(b' ' as u16);
        append_argument(arg, &mut command_line)?;
    }
    command_line.push(0);
    let environment = environment_block(command)?;
    let directory = command
        .get_current_dir()
        .map(|path| {
            let mut wide = utf16(path.as_os_str())?;
            wide.push(0);
            Ok::<_, io::Error>(wide)
        })
        .transpose()?;

    let null = File::options()
        .read(true)
        .write(true)
        .open("NUL")
        .context("failed to open detached daemon null stdio")?;
    let stdio = [
        inheritable_duplicate(null.as_raw_handle())?,
        inheritable_duplicate(null.as_raw_handle())?,
        inheritable_duplicate(stderr.as_raw_handle())?,
    ];
    let handles = stdio.each_ref().map(AsRawHandle::as_raw_handle);
    let mut attributes = HandleList::new(&handles)
        .context("failed to restrict detached daemon handle inheritance")?;
    let mut startup: STARTUPINFOEXW = unsafe { std::mem::zeroed() };
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = handles[0];
    startup.StartupInfo.hStdOutput = handles[1];
    startup.StartupInfo.hStdError = handles[2];
    startup.lpAttributeList = attributes.as_mut_ptr();
    let mut process = unsafe { std::mem::zeroed() };
    // SAFETY: all UTF16 buffers are terminated and remain alive through this call.
    // The initialized attribute list references only our live inheritable
    // duplicates, excluding every pre-existing launcher handle. No global flags
    // are changed. The requested job/console flags are never relaxed on failure.
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            /*binherithandles*/ 1,
            creation_flags | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_ptr().cast(),
            directory
                .as_ref()
                .map_or(std::ptr::null(), |wide| wide.as_ptr()),
            &startup.StartupInfo,
            &mut process,
        )
    } == 0
    {
        return Err(io::Error::last_os_error()).context("failed to create detached daemon process");
    }
    let process_handle = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
    let _thread = unsafe { OwnedHandle::from_raw_handle(process.hThread) };
    Ok(DetachedChild {
        _process: process_handle,
        pid: process.dwProcessId,
    })
}

/// Retain the original process identity until PID/start-time publication finishes,
/// just as tokio::process::Child did. Closing it later does not stop the daemon.
pub(in crate::backend) struct DetachedChild {
    _process: OwnedHandle,
    pid: u32,
}

impl DetachedChild {
    pub(in crate::backend) fn id(&self) -> Option<u32> {
        Some(self.pid)
    }
}

fn inheritable_duplicate(handle: HANDLE) -> io::Result<OwnedHandle> {
    let mut duplicate = std::ptr::null_mut();
    // SAFETY: the borrowed source remains live; only the new owned handle gets
    // the inheritance bit. No caller-owned handle or global state is modified.
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle,
            GetCurrentProcess(),
            &mut duplicate,
            0,
            /*binherithandle*/ 1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { OwnedHandle::from_raw_handle(duplicate) })
}

struct HandleList(Vec<usize>);

impl HandleList {
    fn new(handles: &[HANDLE]) -> io::Result<Self> {
        let mut size = 0;
        unsafe { InitializeProcThreadAttributeList(std::ptr::null_mut(), 1, 0, &mut size) };
        if size == 0 {
            return Err(io::Error::last_os_error());
        }
        // Unlike a byte vector, usize storage guarantees pointer alignment.
        let mut storage = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
        let list = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(list, 1, 0, &mut size) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let mut list = Self(storage);
        // SAFETY: handles and their owned duplicates remain live until after
        // process creation and DeleteProcThreadAttributeList.
        if unsafe {
            UpdateProcThreadAttribute(
                list.as_mut_ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                handles.as_ptr().cast_mut().cast(),
                std::mem::size_of_val(handles),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(list)
    }

    fn as_mut_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        self.0.as_mut_ptr().cast()
    }
}

impl Drop for HandleList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()) };
    }
}

fn utf16(value: &OsStr) -> io::Result<Vec<u16>> {
    let wide: Vec<_> = value.encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "embedded NUL in daemon launch",
        ));
    }
    Ok(wide)
}

// Quote regular Windows CRT arguments, including empty values, embedded quotes
// and trailing backslashes. Preserve UTF16 directly instead of lossy UTF8.
fn append_argument(value: &OsStr, line: &mut Vec<u16>) -> io::Result<()> {
    line.push(b'"' as u16);
    let mut slashes = 0;
    for unit in utf16(value)? {
        if unit == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        let count = if unit == b'"' as u16 {
            slashes * 2 + 1
        } else {
            slashes
        };
        line.extend(std::iter::repeat_n(b'\\' as u16, count));
        line.push(unit);
        slashes = 0;
    }
    line.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    line.push(b'"' as u16);
    Ok(())
}

fn compare_keys(left: &[u16], right: &[u16]) -> io::Result<Ordering> {
    let length = |key: &[u16]| {
        i32::try_from(key.len())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "environment key too long"))
    };
    match unsafe {
        CompareStringOrdinal(
            left.as_ptr(),
            length(left)?,
            right.as_ptr(),
            length(right)?,
            1,
        )
    } {
        1 => Ok(Ordering::Less),
        2 => Ok(Ordering::Equal),
        3 => Ok(Ordering::Greater),
        _ => Err(io::Error::last_os_error()),
    }
}

fn update_environment(
    entries: &mut Vec<(Vec<u16>, Vec<u16>)>,
    key: Vec<u16>,
    value: Option<Vec<u16>>,
) -> io::Result<()> {
    let mut index = 0;
    while index < entries.len() {
        match compare_keys(&key, &entries[index].0)? {
            Ordering::Less => break,
            Ordering::Equal => {
                entries.remove(index);
                break;
            }
            Ordering::Greater => index += 1,
        }
    }
    if let Some(value) = value {
        entries.insert(index, (key, value));
    }
    Ok(())
}

fn environment_block(command: &Command) -> io::Result<Vec<u16>> {
    let mut entries = Vec::new();
    for (key, value) in std::env::vars_os() {
        update_environment(&mut entries, utf16(&key)?, Some(utf16(&value)?))?;
    }
    for (key, value) in command.get_envs() {
        let key = utf16(key)?;
        if key.is_empty() || key.contains(&(b'=' as u16)) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid daemon environment key",
            ));
        }
        update_environment(&mut entries, key, value.map(utf16).transpose()?)?;
    }
    let mut block = Vec::new();
    for (key, value) in entries {
        block.extend(key);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_empty_unicode_quotes_and_trailing_backslashes() {
        for (input, expected) in [
            ("", "\"\""),
            ("한글 space", "\"한글 space\""),
            ("a\"b", "\"a\\\"b\""),
            ("tail \\", "\"tail \\\\\""),
            ("\\\"quoted", "\"\\\\\\\"quoted\""),
        ] {
            let mut actual = Vec::new();
            append_argument(OsStr::new(input), &mut actual).unwrap();
            assert_eq!(String::from_utf16(&actual).unwrap(), expected);
        }
        assert!(append_argument(OsStr::new("a\0b"), &mut Vec::new()).is_err());
    }

    #[test]
    fn environment_merges_windows_ordinal_case_and_removal() {
        let mut entries = Vec::new();
        let wide = |value: &str| utf16(OsStr::new(value)).unwrap();
        update_environment(&mut entries, wide("Z"), Some(wide("last"))).unwrap();
        update_environment(&mut entries, wide("Ä_NAME"), Some(wide("before"))).unwrap();
        update_environment(&mut entries, wide("ä_name"), Some(wide("변경"))).unwrap();
        assert_eq!(entries.len(), 2);
        assert!(
            entries
                .iter()
                .any(|(k, v)| *k == wide("ä_name") && *v == wide("변경"))
        );
        update_environment(&mut entries, wide("Ä_NAME"), None).unwrap();
        assert_eq!(entries, vec![(wide("Z"), wide("last"))]);
    }
}
