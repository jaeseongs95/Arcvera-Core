// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R32 (2026-11-01); see MODIFICATIONS.md.
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
        include_str!("../templates/memories/read_path_publication.md"),
        "memories/read_path_publication.md",
    )
});

static MEMORY_V2_TEMPLATE: LazyLock<Template> = LazyLock::new(|| {
    parse_embedded_template(
        include_str!("../templates/memories/read_path_v2_publication.md"),
        "memories/read_path_v2_publication.md",
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

pub(crate) async fn build_memory_instructions_from_root(
    base_path: &AbsolutePathBuf,
    version: MemoryVersion,
) -> Option<String> {
    build_memory_instructions_from_snapshot(base_path, base_path, version, None, None, None).await.ok().flatten()
}

/// Caller supplies the host-captured generation and the separate note root.
/// Errors are explicit so the caller can emit an unavailable warning.
pub(crate) async fn build_memory_instructions_from_snapshot(
    derived_path: &AbsolutePathBuf,
    source_path: &AbsolutePathBuf,
    version: MemoryVersion,
    current_for_notes: Option<bool>,
    provenance: Option<&str>,
    artifact_manifest: Option<std::collections::BTreeMap<String, String>>,
) -> std::io::Result<Option<String>> {
    let root = derived_path.clone();
    // Optional context is omitted on unreadable, invalid or oversized input; explicit browsing
    // reports the storage state. Bound the read before allocation, then apply the token budget.
    let memory_summary = tokio::task::spawn_blocking(move || -> std::io::Result<String> {
        codex_core::config::validate_scoped_memory_root(root.as_path())?;
        let path = root.join("memory_summary.md");
        let bytes = if let Some(manifest) = artifact_manifest {
            codex_memories_read::publication_snapshot::read_verified_artifact(root.as_path(), &path, &manifest, codex_core::config::MEMORY_SUMMARY_MAX_BYTES)?
        } else {
            let file = codex_memories_read::publication_notes::open_regular_without_links(&path)?;
            let mut bytes = Vec::new();
            file.take(codex_core::config::MEMORY_SUMMARY_MAX_BYTES + 1).read_to_end(&mut bytes)?;
            bytes
        };
        if bytes.len() as u64 > codex_core::config::MEMORY_SUMMARY_MAX_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "memory summary exceeds the disk input limit",
            ));
        }
        String::from_utf8(bytes)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })
    .await.map_err(std::io::Error::other)??;
    let memory_summary = memory_summary.trim();
    let memory_summary = truncate_text(
        &memory_summary,
        TruncationPolicy::Tokens(MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_SUMMARY_TOKEN_LIMIT),
    );
    if memory_summary.is_empty() {
        return Ok(None);
    }
    let base_path = derived_path.display().to_string();
    let source_path = source_path.display().to_string();
    let template = match version {
        MemoryVersion::V1 => &MEMORY_TOOL_DEVELOPER_INSTRUCTIONS_TEMPLATE,
        MemoryVersion::V2 => &MEMORY_V2_TEMPLATE,
    };
    let rendered = template
        .render([
            ("base_path", base_path.as_str()),
            ("source_path", source_path.as_str()),
            ("memory_summary", memory_summary.as_str()),
        ])
        .map_err(std::io::Error::other)?;
    let note_status = match (provenance, current_for_notes) {
        (Some("legacy_import"), _) => "This is an imported legacy snapshot; original completeness and correction application are unverified.",
        (_, Some(false)) => "Notes changed after this snapshot; do not assume their corrections are reflected here.",
        (_, None) => "Whether current notes are reflected in this snapshot is unknown.",
        (_, Some(true)) => "The note input set matches this snapshot; this does not verify semantic correction application.",
    };
    Ok(Some(format!("{note_status}\nHost memory tools verify captured artifact bytes; direct filesystem tools use their own access and do not provide that verification.\n\n{rendered}")))
}

#[cfg(test)]
#[path = "prompts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "prompt_snapshot_tests.rs"]
mod snapshot_tests;
