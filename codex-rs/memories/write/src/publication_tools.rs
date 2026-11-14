// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Stage-scoped filesystem control for isolated consolidation workers.
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Mutex;

use codex_extension_api::{ExtensionData, FunctionCallError, JsonToolOutput, ResponsesApiTool, ToolCall, ToolContributor, ToolExecutor, ToolName, ToolSpec, parse_tool_input_schema};
use codex_memories_read::publication_notes::{open_regular_for_update_without_links, open_regular_without_links};
use codex_protocol::MemoryVersion;
use serde::Deserialize;
use serde_json::{Value, json};

const TOOL_NAME: &str = "memory_stage";
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_FILES: usize = 10_000;
const MAX_PAGE: usize = 32;
const MAX_FILE_BYTES: usize = 1024 * 1024;
const MAX_READ_BYTES: usize = 8 * 1024;

#[derive(Clone)]
pub(crate) struct StageTools {
    root: PathBuf,
    version: MemoryVersion,
    gate: Arc<Mutex<()>>,
    lock: Arc<crate::MemoryWorkspaceLock>,
    failed: Arc<AtomicBool>,
}

impl StageTools {
    pub(crate) fn new(root: PathBuf, version: MemoryVersion, gate: Arc<Mutex<()>>, lock: Arc<crate::MemoryWorkspaceLock>, failed: Arc<AtomicBool>) -> Self {
        Self { root, version, gate, lock, failed }
    }

    pub(crate) fn policy() -> codex_extension_api::ToolPolicy {
        codex_extension_api::ToolPolicy { allowed_tools: Some(vec![ToolName::plain(TOOL_NAME)]), expose_additional_permissions: false, ..Default::default() }
    }
}

impl ToolContributor for StageTools {
    fn tools(&self, _session: &ExtensionData, _thread: &ExtensionData) -> Vec<Arc<dyn for<'call> ToolExecutor<ToolCall<'call>>>> {
        vec![Arc::new(self.clone())]
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request { operation: String, path: String, content: Option<String>, offset: Option<usize>, limit: Option<usize> }

impl<'call> ToolExecutor<ToolCall<'call>> for StageTools {
    fn tool_name(&self) -> ToolName { ToolName::plain(TOOL_NAME) }

    fn spec(&self) -> ToolSpec {
        ToolSpec::Function(ResponsesApiTool {
            name: TOOL_NAME.into(),
            description: "List or read selected memory stage files, or write/delete generated stage outputs. Paths are relative to the stage; no shell or other filesystem is available. List offsets count entries; read offsets count bytes. Use next_offset to continue. Writes replace one complete output file.".into(),
            strict: false,
            defer_loading: None,
            parameters: parse_tool_input_schema(&json!({
                "type":"object", "additionalProperties":false,
                "properties":{
                    "operation":{"type":"string","enum":["list","read","write","delete"]},
                    "path":{"type":"string","description":"Relative path; use empty string to list the stage root."},
                    "content":{"type":"string"}, "offset":{"type":"integer","minimum":0},
                    "limit":{"type":"integer","minimum":1,"maximum":32}
                }, "required":["operation","path"]
            })).expect("static stage schema"),
            output_schema: None,
        })
    }

    fn handle<'a>(&'a self, call: ToolCall<'call>) -> codex_extension_api::ToolExecutorFuture<'a> where 'call: 'a {
        Box::pin(async move {
            let arguments = call.function_arguments()?;
            if arguments.len() > MAX_FILE_BYTES * 6 + 16 * 1024 {
                return Err(FunctionCallError::RespondToModel("stage request is too large".to_string()));
            }
            let args: Request = serde_json::from_str(arguments)
                .map_err(|error| FunctionCallError::RespondToModel(format!("invalid stage request: {error}")))?;
            let mutates = matches!(args.operation.as_str(), "write" | "delete");
            let tool = self.clone();
            // Claim the gate before dispatching the blocking job. If the caller
            // is cancelled after dispatch, finish waits for that job to release it.
            let gate = Arc::clone(&tool.gate).lock_owned().await;
            let result = tokio::task::spawn_blocking(move || {
                let _lock = Arc::clone(&tool.lock);
                let _gate = gate;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tool.execute(args)))
                    .unwrap_or_else(|_| Err(io::Error::other("stage operation panicked")));
                if result.is_err() && mutates { tool.failed.store(true, Ordering::Release); }
                result
            }).await.map_err(|error| FunctionCallError::Fatal(format!("stage operation failed: {error}")))?
                .map_err(|error| FunctionCallError::RespondToModel(error.to_string()))?;
            Ok(Box::new(JsonToolOutput::new(result)) as Box<dyn codex_extension_api::ToolOutput>)
        })
    }
}

fn relative(path: &str, allow_empty: bool) -> io::Result<PathBuf> {
    if path.is_empty() && allow_empty { return Ok(PathBuf::new()); }
    if path.is_empty() || path.len() > 1024 || path.contains(['\\', ':']) { return Err(io::Error::other("invalid relative stage path")); }
    let mut result = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            Component::CurDir if result.as_os_str().is_empty() => {},
            Component::Normal(name) if !name.to_string_lossy().starts_with('.') => result.push(name),
            _ => return Err(io::Error::other("stage path must contain only normal, non-hidden components")),
        }
    }
    if result.as_os_str().is_empty() && !allow_empty { return Err(io::Error::other("empty stage file path")); }
    Ok(result)
}

fn allowed_output(path: &Path, version: MemoryVersion) -> bool {
    let components = path.components().collect::<Vec<_>>();
    if components.len() == 1 && components[0].as_os_str() == "memory_summary.md" { return true; }
    if version != MemoryVersion::V1 { return false; }
    (components.len() == 1 && components[0].as_os_str() == "MEMORY.md")
        || (components.len() >= 2 && components[0].as_os_str() == "skills")
}

fn checked_dir(path: &Path, create: bool) -> io::Result<()> {
    if create {
        match std::fs::create_dir(path) {
            Ok(()) => {}, Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}, Err(error) => return Err(error),
        }
    }
    crate::publication_stage::validate_root(path)
}

fn checked_path(root: &Path, path: &Path, create_parents: bool) -> io::Result<PathBuf> {
    checked_dir(root, false)?;
    let mut result = root.to_path_buf();
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        result.push(component.as_os_str());
        if index + 1 < components.len() { checked_dir(&result, create_parents)?; }
    }
    Ok(result)
}

fn measure(root: &Path) -> io::Result<(usize, u64)> {
    // ceiling: each write scans at most MAX_FILES entries; if this becomes a
    // bottleneck, retain a verified per-stage accounting index under the gate.
    let mut pending = vec![root.to_path_buf()];
    let (mut count, mut bytes) = (0usize, 0u64);
    while let Some(path) = pending.pop() {
        let metadata = std::fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            checked_dir(&path, false)?;
            for entry in std::fs::read_dir(path)? {
                if count + pending.len() >= MAX_FILES { return Err(io::Error::other("stage entry limit exceeded")); }
                pending.push(entry?.path());
            }
        } else if metadata.is_file() {
            let file = open_regular_without_links(&path)?;
            bytes = bytes.checked_add(file.metadata()?.len()).ok_or_else(|| io::Error::other("stage size overflow"))?;
        } else { return Err(io::Error::other("invalid stage entry")); }
        count += 1;
        if count > MAX_FILES || bytes > MAX_BYTES { return Err(io::Error::other("stage budget exceeded")); }
    }
    Ok((count, bytes))
}

pub(crate) fn validate_stage_budget(root: &Path) -> io::Result<()> {
    measure(root).map(|_| ())
}

impl StageTools {
    fn execute(&self, request: Request) -> io::Result<Value> {
        let valid_fields = match request.operation.as_str() {
            "list" => request.content.is_none(),
            "read" => request.content.is_none() && request.limit.is_none(),
            "write" => request.content.is_some() && request.offset.is_none() && request.limit.is_none(),
            "delete" => request.content.is_none() && request.offset.is_none() && request.limit.is_none(),
            _ => false,
        };
        if !valid_fields { return Err(io::Error::other("invalid stage operation or arguments")); }
        if request.limit.is_some_and(|limit| !(1..=MAX_PAGE).contains(&limit)) {
            return Err(io::Error::other("invalid stage page limit"));
        }
        let path = relative(&request.path, request.operation == "list")?;
        if request.operation == "write" {
            if !allowed_output(&path, self.version) { return Err(io::Error::other("stage input is read-only")); }
            let content = request.content.as_ref().ok_or_else(|| io::Error::other("write requires content"))?;
            if content.len() > MAX_FILE_BYTES { return Err(io::Error::other("stage output exceeds per-file limit")); }
            let (count, _) = measure(&self.root)?;
            if count.saturating_add(path.components().count()) > MAX_FILES { return Err(io::Error::other("stage entry limit exceeded")); }
        }
        let target = checked_path(&self.root, &path, request.operation == "write")?;
        match request.operation.as_str() {
            "list" => {
                checked_dir(&target, false)?;
                let mut names = Vec::new();
                for entry in std::fs::read_dir(target)? {
                    if names.len() >= MAX_FILES { return Err(io::Error::other("stage entry limit exceeded")); }
                    let entry = entry?;
                    let name = entry.file_name().into_string().map_err(|_| io::Error::other("stage filename is not UTF-8"))?;
                    if name.starts_with('.') { continue; }
                    let metadata = std::fs::symlink_metadata(entry.path())?;
                    let is_dir = if metadata.is_dir() { checked_dir(&entry.path(), false)?; true }
                    else if metadata.is_file() { open_regular_without_links(&entry.path())?; false }
                    else { return Err(io::Error::other("linked or invalid stage entry")); };
                    names.push((name, is_dir));
                }
                names.sort_unstable();
                let start = request.offset.unwrap_or(0);
                if start > names.len() { return Err(io::Error::other("stage list offset is out of range")); }
                let end = start.saturating_add(request.limit.unwrap_or(100).clamp(1, MAX_PAGE)).min(names.len());
                let entries = names[start..end].iter().map(|(name, directory)| json!({"name":name,"type":if *directory { "directory" } else { "file" }})).collect::<Vec<_>>();
                Ok(json!({"entries":entries, "next_offset": (end < names.len()).then_some(end)}))
            }
            "read" => {
                let file = open_regular_without_links(&target)?;
                if file.metadata()?.len() > MAX_BYTES { return Err(io::Error::other("stage file exceeds limit")); }
                let mut file = file;
                let offset = request.offset.unwrap_or(0) as u64;
                file.seek(SeekFrom::Start(offset))?;
                let mut bytes = Vec::new();
                file.take(MAX_READ_BYTES as u64 + 1).read_to_end(&mut bytes)?;
                let truncated = bytes.len() > MAX_READ_BYTES;
                if truncated { bytes.truncate(MAX_READ_BYTES); }
                let text = loop {
                    match std::str::from_utf8(&bytes) {
                        Ok(text) => break text.to_owned(),
                        Err(error) if truncated && error.error_len().is_none() => { bytes.truncate(error.valid_up_to()); },
                        Err(error) => return Err(io::Error::other(error)),
                    }
                };
                let next_offset = truncated.then_some(offset.saturating_add(bytes.len() as u64));
                Ok(json!({"content":text,"next_offset":next_offset}))
            }
            "write" => {
                if !allowed_output(&path, self.version) { return Err(io::Error::other("stage input is read-only")); }
                let bytes = request.content.ok_or_else(|| io::Error::other("write requires content"))?.into_bytes();
                if bytes.len() > MAX_FILE_BYTES { return Err(io::Error::other("stage output exceeds per-file limit")); }
                let (count, total) = measure(&self.root)?;
                let old = match open_regular_without_links(&target) { Ok(file) => Some(file.metadata()?.len()), Err(error) if error.kind() == io::ErrorKind::NotFound => None, Err(error) => return Err(error) };
                if old.is_none() && count >= MAX_FILES || total.saturating_sub(old.unwrap_or(0)).saturating_add(bytes.len() as u64) > MAX_BYTES { return Err(io::Error::other("stage budget exceeded")); }
                let mut file = open_regular_for_update_without_links(&target, old.is_none())?;
                file.set_len(0)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                Ok(json!({"written":bytes.len()}))
            }
            "delete" => {
                if !allowed_output(&path, self.version) { return Err(io::Error::other("stage input is read-only")); }
                delete_output(&target)?;
                Ok(json!({"deleted":true}))
            }
            _ => Err(io::Error::other("unsupported stage operation")),
        }
    }
}

#[cfg(windows)]
fn delete_output(path: &Path) -> io::Result<()> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ATTRIBUTE_REPARSE_POINT, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FileDispositionInfo, GetFileInformationByHandle, SetFileInformationByHandle};
    let file = std::fs::OpenOptions::new().access_mode(DELETE | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT).open(path)?;
    // SAFETY: info is a writable output structure and file is an open handle.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 { return Err(io::Error::last_os_error()); }
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 || info.nNumberOfLinks != 1 || !file.metadata()?.is_file() { return Err(io::Error::other("linked or invalid stage output")); }
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    // SAFETY: disposition and file handle remain valid for the call.
    if unsafe { SetFileInformationByHandle(file.as_raw_handle(), FileDispositionInfo, (&raw const disposition).cast(), std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32) } == 0 { return Err(io::Error::last_os_error()); }
    Ok(())
}

#[cfg(not(windows))]
fn delete_output(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "host-mediated deletion is unavailable on this platform"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(operation: &str, path: &str, content: Option<&str>) -> Request {
        Request { operation: operation.into(), path: path.into(), content: content.map(str::to_owned), offset: None, limit: None }
    }

    #[test]
    fn stage_tool_reads_inputs_writes_only_outputs_and_rejects_escape() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("stage");
        std::fs::create_dir(&root)?;
        std::fs::write(root.join("raw_memories.md"), "selected input")?;
        let outside = temp.path().join("outside");
        std::fs::write(&outside, "unchanged")?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&temp.path().join("memory"))?);
        let tool = StageTools::new(root.clone(), MemoryVersion::V1, Arc::new(Mutex::new(())), lock, Arc::new(AtomicBool::new(false)));
        assert_eq!(tool.execute(request("read", "raw_memories.md", None))?["content"], "selected input");
        assert!(tool.execute(request("write", "raw_memories.md", Some("overwrite"))).is_err());
        assert!(tool.execute(request("write", "../outside", Some("escape"))).is_err());
        assert!(tool.execute(request("read", &outside.display().to_string(), None)).is_err());
        tool.execute(request("write", "memory_summary.md", Some("v1\nsummary")))?;
        tool.execute(request("write", "skills/task/SKILL.md", Some("instructions")))?;
        assert_eq!(std::fs::read_to_string(root.join("skills/task/SKILL.md"))?, "instructions");
        assert_eq!(std::fs::read_to_string(outside)?, "unchanged");
        assert_eq!(std::fs::read_to_string(root.join("raw_memories.md"))?, "selected input");
        Ok(())
    }

    #[test]
    fn policy_denies_unregistered_exec_and_mcp_names() {
        let policy = StageTools::policy();
        assert!(policy.allows(&ToolName::plain(TOOL_NAME)));
        for name in [ToolName::plain("exec_command"), ToolName::plain("shell_command"), ToolName::plain("apply_patch"), ToolName::namespaced("mcp", "write_file")] {
            assert!(!policy.allows(&name));
        }
    }

    #[test]
    fn oversized_stage_is_rejected_before_worker_dispatch() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let file = std::fs::File::create(temp.path().join("oversized"))?;
        file.set_len(MAX_BYTES + 1)?;
        assert!(validate_stage_budget(temp.path()).is_err());
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn stage_tool_rejects_symlink_output() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("stage");
        std::fs::create_dir(&root)?;
        let outside = temp.path().join("outside");
        std::fs::write(&outside, "protected")?;
        std::os::unix::fs::symlink(&outside, root.join("memory_summary.md"))?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&temp.path().join("memory"))?);
        let tool = StageTools::new(root, MemoryVersion::V1, Arc::new(Mutex::new(())), lock, Arc::new(AtomicBool::new(false)));
        assert!(tool.execute(request("write", "memory_summary.md", Some("bad"))).is_err());
        assert_eq!(std::fs::read_to_string(outside)?, "protected");
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn isolated_stage_worker_dispatch_rejects_exec_and_path_escape() -> anyhow::Result<()> {
        use core_test_support::responses::{start_mock_server, mount_sse_sequence, sse, ev_function_call, ev_completed};
        use core_test_support::test_codex::test_codex;
        use codex_protocol::protocol::EventMsg;
        use codex_protocol::user_input::UserInput;
        use codex_core::TurnInputRequest;

        let temp = tempfile::tempdir()?;
        let stage = temp.path().join("stage");
        std::fs::create_dir(&stage)?;
        let outside = temp.path().join("outside.txt");
        std::fs::write(&outside, "protected")?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&temp.path().join("memory"))?);
        let failed = Arc::new(AtomicBool::new(false));
        let tool = StageTools::new(stage.clone(), MemoryVersion::V1, Arc::new(Mutex::new(())), lock, Arc::clone(&failed));
        let server = start_mock_server().await;
        let fixture = test_codex().build_with_auto_env(&server).await?;
        let mut options = fixture.start_thread_options().await;
        let mut registry = codex_extension_api::ExtensionRegistryBuilder::<codex_core::config::Config>::new();
        registry.tool_contributor(Arc::new(tool));
        options.thread_extension_init.insert(codex_extension_api::SessionIsolation::Isolated);
        options.thread_extension_init.insert(StageTools::policy());
        options.thread_extension_init.insert(codex_extension_api::IsolatedSessionExtensions(Arc::new(registry.build())));
        let agent = fixture.thread_manager.start_thread(options).await?;
        let escape = json!({"operation":"write","path":"../outside.txt","content":"changed"}).to_string();
        let output = json!({"operation":"write","path":"memory_summary.md","content":"v1\nsummary"}).to_string();
        let responses = mount_sse_sequence(&server, vec![
            sse(vec![ev_function_call("exec", "exec_command", r#"{"cmd":"echo must-not-run"}"#), ev_function_call("escape", TOOL_NAME, &escape), ev_function_call("write", TOOL_NAME, &output), ev_completed("tools")]),
            sse(vec![ev_completed("done")]),
        ]).await;
        agent.thread.start_or_steer_turn(TurnInputRequest::user_input(vec![UserInput::Text { text: "exercise stage controls".into(), text_elements: Vec::new() }])).await?;
        core_test_support::wait_for_event(&agent.thread, |event| matches!(event, EventMsg::TurnComplete(_))).await;
        let requests = responses.requests();
        assert_eq!(requests.len(), 2);
        let tools = requests[0].body_json()["tools"].as_array().expect("tool list").iter()
            .map(|tool| tool["name"].as_str().unwrap_or_default().to_string()).collect::<Vec<_>>();
        assert_eq!(tools, vec![TOOL_NAME.to_string()]);
        assert_eq!(requests[1].function_call_output("exec")["output"], "unsupported call: exec_command");
        assert_eq!(std::fs::read_to_string(&outside)?, "protected");
        assert_eq!(std::fs::read_to_string(stage.join("memory_summary.md"))?, "v1\nsummary");
        assert!(failed.load(Ordering::Acquire));
        agent.thread.shutdown_and_wait().await?;
        fixture.codex.shutdown_and_wait().await?;
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn stage_tool_rejects_hard_links_for_write_and_delete() -> io::Result<()> {
        let temp = tempfile::tempdir()?;
        let root = temp.path().join("stage");
        std::fs::create_dir(&root)?;
        let outside = temp.path().join("outside");
        std::fs::write(&outside, "protected")?;
        std::fs::hard_link(&outside, root.join("memory_summary.md"))?;
        let lock = Arc::new(crate::MemoryWorkspaceLock::try_acquire(&temp.path().join("memory"))?);
        let tool = StageTools::new(root, MemoryVersion::V1, Arc::new(Mutex::new(())), lock, Arc::new(AtomicBool::new(false)));
        assert!(tool.execute(request("write", "memory_summary.md", Some("bad"))).is_err());
        assert!(tool.execute(request("delete", "memory_summary.md", None)).is_err());
        assert_eq!(std::fs::read_to_string(outside)?, "protected");
        Ok(())
    }
}
