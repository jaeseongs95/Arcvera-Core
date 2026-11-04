// Modified for OpenDIY Codex R37 (2026-11-04); see MODIFICATIONS.md.
//! Memory readiness and summary reads using a loaded thread's selected pipeline.

use crate::JsonSchema;
use crate::TS;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadMemoryReadParams {
    /// The thread must already be started or resumed.
    pub thread_id: String,
    /// Explicit browsing of stored memory, even when automatic injection is disabled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub include_disabled: Option<bool>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadMemoryReadResponse {
    /// Summary from the thread's selected memory version. Null when memory use is
    /// disabled or the selected summary is missing, unreadable, or blank.
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root: Option<String>,
    /// ready, missing, disabled, unavailable, unreadable, or oversized.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub use_memories: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub generate_memories: Option<bool>,
    /// Selected namespace's persisted phase-2 job. Not proof that a particular note was included.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub consolidation: Option<MemoryConsolidationState>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct MemoryConsolidationState {
    pub status: String,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub lease_until: Option<i64>,
    pub retry_at: Option<i64>,
    pub last_error: Option<String>,
    pub input_watermark: Option<i64>,
    pub last_success_watermark: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadMemoryConsolidateParams {
    pub thread_id: String,
    /// Explicit retry of a failed or cooled-down consolidation; never steals an active lease.
    #[serde(default)]
    pub retry: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadMemoryConsolidateResponse {
    /// accepted, completed, blocked, or failed. Acceptance is not completion.
    pub status: String,
    pub reason: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadMemoryNoteAddParams {
    pub thread_id: String,
    /// An explicit user's addition, correction, or forget request; at most 16 KiB.
    pub note: String,
    /// Explicit storage target only. Omitted means the thread's pinned namespace.
    /// Does not change memory injection, consolidation, or future-thread settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub scope: Option<MemoryNoteScope>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "v2/")]
pub enum MemoryNoteScope {
    Project,
    Global,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadMemoryNoteAddResponse {
    /// Saved for later consolidation. This is not a confirmation that the summary changed.
    pub pending_consolidation: bool,
    /// Actual storage target. Optional when decoding older server responses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scope: Option<MemoryNoteScope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub root: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct MemoryStatusParams {
    /// Required distinct consolidated threads. Defaults to 20; supported range is 1..=4096.
    #[ts(optional = nullable)]
    pub min_consolidated_threads: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct MemoryStatusResponse {
    pub v2_consolidated_threads: u32,
    pub v2_ready: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn memory_read_response_preserves_optional_metadata_and_legacy_decode() {
        let response: ThreadMemoryReadResponse =
            serde_json::from_value(json!({"summary": null})).unwrap();
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            json!({"summary": null})
        );
        assert!(
            serde_json::from_value::<ThreadMemoryReadParams>(json!({"threadId":"fixture"}))
                .unwrap()
                .include_disabled
                .is_none()
        );
    }

    #[test]
    fn memory_consolidate_defaults_to_normal_admission_and_preserves_explicit_retry() {
        let normal: ThreadMemoryConsolidateParams =
            serde_json::from_value(json!({"threadId":"selected-thread"})).unwrap();
        assert!(!normal.retry);
        let retry: ThreadMemoryConsolidateParams =
            serde_json::from_value(json!({"threadId":"selected-thread","retry":true})).unwrap();
        assert_eq!(
            serde_json::to_value(retry).unwrap(),
            json!({"threadId":"selected-thread","retry":true})
        );
        for invalid in [json!(null), json!("true"), json!(1)] {
            assert!(
                serde_json::from_value::<ThreadMemoryConsolidateParams>(
                    json!({"threadId":"selected-thread","retry":invalid})
                )
                .is_err()
            );
        }
    }

    #[test]
    fn memory_consolidation_response_distinguishes_admission_and_persisted_failure() {
        let admitted: ThreadMemoryConsolidateResponse =
            serde_json::from_value(json!({"status":"accepted","reason":null})).unwrap();
        assert_eq!(admitted.status, "accepted");
        let state: ThreadMemoryReadResponse = serde_json::from_value(json!({
            "summary":null,
            "consolidation":{
                "status":"error", "startedAt":10, "finishedAt":11,
                "leaseUntil":null, "retryAt":20,
                "lastError":"failed_invalid_artifacts", "inputWatermark":42,
                "lastSuccessWatermark":41
            }
        }))
        .unwrap();
        let consolidation = state.consolidation.as_ref().unwrap();
        assert_eq!(
            consolidation.last_error.as_deref(),
            Some("failed_invalid_artifacts")
        );
        assert_eq!(consolidation.retry_at, Some(20));
        assert_eq!(
            serde_json::to_value(state).unwrap()["consolidation"]["lastSuccessWatermark"],
            41
        );
    }

    #[test]
    fn memory_note_scope_is_optional_explicit_and_backward_compatible() {
        let legacy: ThreadMemoryNoteAddParams =
            serde_json::from_value(json!({"threadId":"selected-thread","note":"legacy"})).unwrap();
        assert!(legacy.scope.is_none());
        assert_eq!(
            serde_json::to_value(legacy).unwrap(),
            json!({"threadId":"selected-thread","note":"legacy"})
        );
        let scoped: ThreadMemoryNoteAddParams = serde_json::from_value(
            json!({"threadId":"selected-thread","note":"project correction","scope":"project"}),
        )
        .unwrap();
        assert_eq!(scoped.scope, Some(MemoryNoteScope::Project));
        for invalid in [json!("other"), json!(false), json!({"root":"elsewhere"})] {
            assert!(
                serde_json::from_value::<ThreadMemoryNoteAddParams>(json!({
                    "threadId":"selected-thread","note":"correction","scope":invalid
                }))
                .is_err()
            );
        }
        let legacy: ThreadMemoryNoteAddResponse =
            serde_json::from_value(json!({"pendingConsolidation":true})).unwrap();
        assert!(legacy.scope.is_none());
        assert!(legacy.root.is_none());
        let receipt: ThreadMemoryNoteAddResponse = serde_json::from_value(json!({
            "pendingConsolidation":true,"scope":"project","root":"fixture/project_memories/key/memories_v2"
        })).unwrap();
        assert_eq!(receipt.scope, Some(MemoryNoteScope::Project));
        assert!(receipt.pending_consolidation);
    }
}
