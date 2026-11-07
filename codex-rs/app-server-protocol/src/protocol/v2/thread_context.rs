// Modified for OpenDIY Codex R39 (2026-11-07); see MODIFICATIONS.md.
use crate::JsonSchema;
use crate::TS;
use codex_protocol::context_snapshot::ThreadContextSnapshot;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadContextReadParams {
    pub thread_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct ThreadContextReadResponse {
    pub thread_id: String,
    pub snapshot: ThreadContextSnapshot,
}
