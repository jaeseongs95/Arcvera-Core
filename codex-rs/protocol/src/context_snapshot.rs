//! Content-free inventory of one loaded thread's locally retained model history.
use crate::protocol::TokenUsage;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ContextRoleCounts {
    #[ts(type = "number")]
    pub system: u64,
    #[ts(type = "number")]
    pub developer: u64,
    #[ts(type = "number")]
    pub user: u64,
    #[ts(type = "number")]
    pub assistant: u64,
    #[ts(type = "number")]
    pub other: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ContextItemCounts {
    #[ts(type = "number")]
    pub messages: u64,
    #[ts(type = "number")]
    pub agent_messages: u64,
    #[ts(type = "number")]
    pub tool_calls: u64,
    #[ts(type = "number")]
    pub tool_outputs: u64,
    #[ts(type = "number")]
    pub tool_declarations: u64,
    #[ts(type = "number")]
    pub reasoning: u64,
    #[ts(type = "number")]
    pub compaction: u64,
    #[ts(type = "number")]
    pub configuration: u64,
    #[ts(type = "number")]
    pub other: u64,
}

/// Positional sources of Message/AgentMessage parts; roles do not identify sources.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ContextMessageSourceCounts {
    #[ts(type = "number")]
    pub unknown: u64,
    #[ts(type = "number")]
    pub developer_instructions: u64,
    #[ts(type = "number")]
    pub skills: u64,
    #[ts(type = "number")]
    pub agents_md: u64,
    #[ts(type = "number")]
    pub harness: u64,
    #[ts(type = "number")]
    pub hooks: u64,
    #[ts(type = "number")]
    pub tool: u64,
    #[ts(type = "number")]
    pub user: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum ContextSnapshotScope {
    RetainedLocalHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum ContextBasePlacement {
    History,
    OutsideHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub enum ContextMeasurementStatus {
    Unverified,
    NotCaptured,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ContextBaseInstructions {
    /// Configured instructions length; not a count of a normalized request field.
    #[ts(type = "number")]
    pub utf8_bytes: u64,
    pub placement: ContextBasePlacement,
}

/// Never contains prompt text, tool arguments, paths or arbitrary metadata strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct ThreadContextSnapshot {
    pub scope: ContextSnapshotScope,
    /// Destructive-history revision; this is not an append sequence or a timestamp.
    #[ts(type = "number")]
    pub history_revision: u64,
    #[ts(type = "number")]
    pub window_number: u64,
    #[ts(type = "number")]
    pub item_count: u64,
    pub roles: ContextRoleCounts,
    pub items: ContextItemCounts,
    pub message_sources: ContextMessageSourceCounts,
    pub base_instructions: ContextBaseInstructions,
    /// Existing tracking can contain synthetic counts; not certified server usage.
    pub tracked_usage: Option<TokenUsage>,
    pub tracked_usage_provenance: ContextMeasurementStatus,
    pub server_measured_usage: Option<TokenUsage>,
    pub next_request_tools: ContextMeasurementStatus,
}
