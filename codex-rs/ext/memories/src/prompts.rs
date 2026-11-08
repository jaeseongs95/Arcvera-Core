use crate::MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_SUMMARY_TOKEN_LIMIT;
use codex_protocol::MemoryVersion;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::truncate_text;
use codex_utils_template::Template;
use std::io::Read;
use std::sync::LazyLock;

static MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    parse_embedded_template(
        include_str!("../templates/memories/read_path.md"),
        "memories/read_path.md",
    )
});

static MEMORY_V2_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    parse_embedded_template(
        include_str!("../templates/memories/read_path_v2.md"),
        "memories/read_path_v2.md",
    )
});

fn parse_embedded_template(source: &'static str, template_name: &str) -> Template {
    match Template::parse(source) {
        Ok(template) => template,
        Err(err) => panic!("embedded template {template_name} is invalid: {err}"),
    }
}

/// Build the memory read-path prompt that is added to developer instructions.
///
/// Large `memory_summary.md` files are truncated at
/// [MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_SUMMARY_TOKEN_LIMIT].
#[cfg(test)]
pub(crate) async fn build_memory_tool_developer_instructions(
    codex_home: &AbsolutePathBuf,
    version: MemoryVersion,
) -> Option<String> {
    let base_path = codex_home.join(version.directory_name());
    build_memory_instructions_from_root(&base_path, version).await
}

#[cfg(test)]
pub(crate) async fn build_memory_instructions_from_root(
    base_path: &AbsolutePathBuf,
    version: MemoryVersion,
) -> Option<String> {
    build_memory_context_from_root(base_path, version)
        .await
        .map(|(text, _)| text)
}

pub(crate) async fn build_memory_context_from_root(
    base_path: &AbsolutePathBuf,
    version: MemoryVersion,
) -> Option<(String, bool)> {
    // V2 keeps only stable policy in the initial developer context. Historical
    // data is selected for an individual query by the turn-input contributor.
    let root = base_path.clone();
    let versioned = version == MemoryVersion::V2
        || !matches!(
            tokio::task::spawn_blocking(
                move || codex_memories_read::publication::load_publication(root.as_path())
            )
            .await,
            Ok(Ok(None))
        );
    if versioned {
        let base_path = base_path.display().to_string();
        let mut policy = MEMORY_V2_TEMPLATE
            .render([("base_path", base_path.as_str())])
            .ok()?;
        if version == MemoryVersion::V1 {
            policy.push_str("\nVersioned v1 also exposes its current published MEMORY.md and final skills through the memory tools; raw inputs and user notes remain outside that publication.\n");
        }
        return Some((policy, true));
    }
    let root = base_path.clone();
    // Optional context is omitted on unreadable, invalid or oversized input; explicit browsing
    // reports the storage state. Bound the read before allocation, then apply the token budget.
    let memory_summary = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        codex_core::config::validate_scoped_memory_root(root.as_path())?;
        let _lock = codex_memories_read::publication::MemoryWorkspaceLock::try_acquire_shared(
            root.as_path(),
        )?;
        if codex_memories_read::publication::load_publication(root.as_path())?.is_some() {
            return Err(std::io::Error::other(
                "legacy memory became versioned before the summary snapshot",
            ));
        }
        let file = std::fs::File::open(root.join("memory_summary.md"))?;
        let mut bytes = Vec::new();
        file.take(codex_core::config::MEMORY_SUMMARY_MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > codex_core::config::MEMORY_SUMMARY_MAX_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "memory summary exceeds the disk input limit",
            ));
        }
        String::from_utf8(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })
    .await
    .ok()?
    .ok()?;
    let memory_summary = memory_summary.trim();
    let memory_summary = truncate_text(
        &memory_summary,
        TruncationPolicy::Tokens(MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_SUMMARY_TOKEN_LIMIT),
    );
    if memory_summary.is_empty() {
        return None;
    }
    let base_path = base_path.display().to_string();
    let template = match version {
        MemoryVersion::V1 => &MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_TEMPLATE,
        MemoryVersion::V2 => &MEMORY_V2_TEMPLATE,
    };
    template
        .render([
            ("base_path", base_path.as_str()),
            ("memory_summary", memory_summary.as_str()),
        ])
        .ok()
        .map(|text| (text, false))
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;
