// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
//! Serializable capability overrides for custom Responses-compatible providers.
//!
//! Unspecified capabilities inherit the provider's existing defaults. Model and
//! turn settings may further restrict the capabilities declared here.

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

/// Overrides for the optional API features supported by a custom provider.
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ModelProviderCapabilities {
    /// Whether the provider accepts explicit `access_programs.cyber` selections.
    /// Defaults to enabled for the built-in OpenAI provider and disabled otherwise.
    /// This does not grant program access or change authentication.
    pub cyber_access_programs: Option<bool>,
    /// Whether hosted web search may access the live web.
    pub external_web_access: Option<bool>,
    /// Remote context-compaction protocol; omission preserves provider defaults.
    pub remote_compaction: Option<RemoteCompactionSupport>,
    /// Explicit request policy; unset sends no cache retention fields.
    pub prompt_cache: Option<PromptCachePolicy>,
}

/// A provider's explicitly selected Responses request cache policy.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PromptCachePolicy {
    Implicit { ttl: PromptCacheTtl },
    Legacy { retention: PromptCacheRetention },
}

impl PromptCachePolicy {
    /// Requested implicit TTL, without evidence of provider support or retention.
    pub fn requested_ttl(self) -> Option<std::time::Duration> {
        match self {
            Self::Implicit {
                ttl: PromptCacheTtl::ThirtyMinutes,
            } => Some(std::time::Duration::from_secs(1800)),
            Self::Implicit {
                ttl: PromptCacheTtl::TwelveHours,
            } => Some(std::time::Duration::from_secs(43200)),
            Self::Legacy { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub enum PromptCacheTtl {
    #[serde(rename = "30m")]
    ThirtyMinutes,
    #[serde(rename = "12h")]
    TwelveHours,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
pub enum PromptCacheRetention {
    #[serde(rename = "in_memory")]
    InMemory,
    #[serde(rename = "24h")]
    TwentyFourHours,
}

/// Remote context-compaction protocols supported by a model provider.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemoteCompactionSupport {
    /// Use local compaction instead of a provider compaction protocol.
    Unsupported,
    /// Send `compaction_trigger` items over the Responses endpoint.
    V2,
}
