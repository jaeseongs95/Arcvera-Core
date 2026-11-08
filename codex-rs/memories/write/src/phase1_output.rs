//! Version-specific extraction contracts; v2 never decodes raw memory.

use codex_protocol::MemoryVersion;
use codex_secrets::redact_secrets;
use codex_utils_output_truncation::TruncationPolicy;
use codex_utils_output_truncation::truncate_text;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;

pub(crate) struct StageOneOutput {
    pub(crate) raw_memory: Option<String>,
    pub(crate) rollout_summary: String,
    pub(crate) rollout_slug: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyOutput {
    raw_memory: String,
    rollout_summary: String,
    #[serde(default)]
    rollout_slug: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SummaryOutput {
    rollout_summary: String,
    rollout_slug: String,
}

impl StageOneOutput {
    pub(crate) fn parse(source: &str, version: MemoryVersion) -> serde_json::Result<Self> {
        let (raw_memory, rollout_summary, rollout_slug) = match version {
            MemoryVersion::V1 => {
                let output: LegacyOutput = serde_json::from_str(source)?;
                (
                    Some(output.raw_memory),
                    output.rollout_summary,
                    output.rollout_slug,
                )
            }
            MemoryVersion::V2 => {
                let output: SummaryOutput = serde_json::from_str(source)?;
                (None, output.rollout_summary, Some(output.rollout_slug))
            }
        };
        let mut rollout_summary = redact_secrets(rollout_summary);
        if version == MemoryVersion::V2 {
            rollout_summary = truncate_text(&rollout_summary, TruncationPolicy::Bytes(9_000));
            // Whitespace is not a useful memory. Preserve the existing empty
            // output contract so no-output jobs do not accumulate empty rows.
            if rollout_summary.trim().is_empty() {
                rollout_summary.clear();
            }
        }
        Ok(Self {
            raw_memory: raw_memory.map(redact_secrets),
            rollout_summary,
            rollout_slug: rollout_slug.map(redact_secrets),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v2_whitespace_is_no_memory_and_useful_utf8_is_preserved() {
        let empty = StageOneOutput::parse(
            r#"{"rollout_summary":" \n\t","rollout_slug":"routine-update"}"#,
            MemoryVersion::V2,
        )
        .unwrap();
        assert!(empty.rollout_summary.is_empty());
        let useful = StageOneOutput::parse(
            r#"{"rollout_summary":"A 프로젝트에서만 재시도 전에 원인 확인","rollout_slug":"a-retry"}"#,
            MemoryVersion::V2,
        ).unwrap();
        assert_eq!(
            useful.rollout_summary,
            "A 프로젝트에서만 재시도 전에 원인 확인"
        );
        assert!(useful.raw_memory.is_none());
    }
}

pub(crate) fn output_schema(version: MemoryVersion) -> Value {
    match version {
        MemoryVersion::V1 => json!({
            "type": "object",
            "properties": {
                "rollout_summary": { "type": "string" },
                "rollout_slug": { "type": ["string", "null"] },
                "raw_memory": { "type": "string" }
            },
            "required": ["rollout_summary", "rollout_slug", "raw_memory"],
            "additionalProperties": false
        }),
        MemoryVersion::V2 => json!({
            "type": "object",
            "properties": {
                "rollout_summary": { "type": "string" },
                "rollout_slug": { "type": "string" }
            },
            "required": ["rollout_summary", "rollout_slug"],
            "additionalProperties": false
        }),
    }
}
