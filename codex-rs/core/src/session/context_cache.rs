// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! Response-level cache observations. They never determine context relevance, authorization,
//! retention of original history, or whether an individual source was cached.
use super::session::Session;
use crate::state::SessionState;
use chrono::DateTime;
use chrono::SecondsFormat;
use chrono::Utc;
use codex_model_provider_info::PromptCachePolicy;
use codex_protocol::ResponseUsageMetadata;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CacheStatus {
    Hit,
    Miss,
    Unknown,
    Invalid,
}

/// Stores only an opaque host observation identity, numeric usage, time and request revision. Raw usage,
/// cache keys, response bodies, source identities and tokens are never copied into this state.
pub(crate) struct ContextCacheObservation {
    /// Host observation identity, never a provider cache key or cache-entry locator.
    observation_id: String,
    status: CacheStatus,
    cached_input_tokens: Option<u64>,
    write_input_tokens: Option<u64>,
    uncached_input_tokens: Option<u64>,
    total_input_tokens: Option<u64>,
    counter_provenance: &'static str,
    requested_ttl_seconds: Option<u64>,
    request_started_at: Option<String>,
    ttl_support: &'static str,
    ttl_application: &'static str,
    observed_at: String,
    request_context_index_revision: Option<u64>,
}

impl ContextCacheObservation {
    fn from_usage(
        usage: Option<&ResponseUsageMetadata>,
        request_context_index_revision: Option<u64>,
        observed_at: String,
    ) -> Self {
        let counters = cache_counters(usage.and_then(|usage| usage.metadata.as_ref()));
        Self {
            observation_id: uuid::Uuid::new_v4().to_string(),
            status: counters.status,
            cached_input_tokens: counters.read,
            write_input_tokens: counters.write,
            uncached_input_tokens: counters.uncached,
            total_input_tokens: counters.total,
            counter_provenance: counters.provenance,
            requested_ttl_seconds: None,
            request_started_at: None,
            ttl_support: "unknown",
            ttl_application: "unknown",
            observed_at,
            request_context_index_revision,
        }
    }

    fn bind_request_policy(&mut self, policy: Option<PromptCachePolicy>, started: DateTime<Utc>) {
        self.request_started_at = Some(started.to_rfc3339_opts(SecondsFormat::Millis, true));
        let Some(policy) = policy else {
            return;
        };
        self.ttl_support = "configured_provider_policy";
        self.ttl_application = "requested_unconfirmed";
        self.requested_ttl_seconds = policy.requested_ttl().map(|duration| duration.as_secs());
        // Counters report cache activity, not whether a provider honored the
        // requested TTL or guaranteed retention for any entry.
        if self.status != CacheStatus::Invalid
            && (self.cached_input_tokens.is_some_and(|tokens| tokens > 0)
                || self.write_input_tokens.is_some_and(|tokens| tokens > 0))
        {
            self.ttl_application = "request_cache_activity_observed";
        }
    }
}

struct CacheCounters {
    status: CacheStatus,
    read: Option<u64>,
    write: Option<u64>,
    uncached: Option<u64>,
    total: Option<u64>,
    provenance: &'static str,
}

impl CacheCounters {
    fn unknown(status: CacheStatus) -> Self {
        Self {
            status,
            read: None,
            write: None,
            uncached: None,
            total: None,
            provenance: "not_reported",
        }
    }
}

/// Normalize documented usage shapes without coupling selection to a provider/model name.
/// `uncached` excludes both cache reads and cache writes; unknown writes stay unknown.
fn cache_counters(raw_usage: Option<&Value>) -> CacheCounters {
    let parse = || -> Result<CacheCounters, ()> {
        let raw_usage = match raw_usage {
            None | Some(Value::Null) => return Ok(CacheCounters::unknown(CacheStatus::Unknown)),
            Some(Value::Object(usage)) => usage,
            Some(_) => return Err(()),
        };
        let number = |value: Option<&Value>| -> Result<Option<u64>, ()> {
            match value {
                None | Some(Value::Null) => Ok(None),
                Some(value) => value.as_u64().map(Some).ok_or(()),
            }
        };
        let input = number(raw_usage.get("input_tokens"))?;
        // Additive wire protocols report ordinary input separately from cache reads/writes.
        if raw_usage.contains_key("cache_read_input_tokens")
            || raw_usage.contains_key("cache_creation_input_tokens")
        {
            if raw_usage
                .get("input_tokens_details")
                .is_some_and(|v| !v.is_null())
            {
                return Err(()); // Ambiguous mixtures must not silently change accounting semantics.
            }
            let read = number(raw_usage.get("cache_read_input_tokens"))?;
            let write = number(raw_usage.get("cache_creation_input_tokens"))?;
            let total = match (input, read, write) {
                (Some(input), Some(read), Some(write)) => Some(
                    input
                        .checked_add(read)
                        .and_then(|v| v.checked_add(write))
                        .ok_or(())?,
                ),
                _ => None,
            };
            return Ok(CacheCounters {
                status: counter_status(read),
                read,
                write,
                uncached: input,
                total,
                provenance: "additive_response_input_usage",
            });
        }
        let details = match raw_usage.get("input_tokens_details") {
            None | Some(Value::Null) => {
                return Ok(CacheCounters {
                    total: input,
                    ..CacheCounters::unknown(CacheStatus::Unknown)
                });
            }
            Some(Value::Object(details)) => details,
            Some(_) => return Err(()),
        };
        let read = number(details.get("cached_tokens"))?;
        let write = number(details.get("cache_write_tokens"))?;
        if let Some(input) = input {
            if read.is_some_and(|read| read > input) || write.is_some_and(|write| write > input) {
                return Err(());
            }
        }
        let accounted = match (read, write) {
            (Some(read), Some(write)) => Some(read.checked_add(write).ok_or(())?),
            _ => None,
        };
        let uncached = match (input, accounted) {
            (Some(input), Some(accounted)) => Some(input.checked_sub(accounted).ok_or(())?),
            _ => None,
        };
        Ok(CacheCounters {
            status: counter_status(read),
            read,
            write,
            uncached,
            total: input,
            provenance: "inclusive_response_input_usage",
        })
    };
    parse().unwrap_or_else(|()| CacheCounters::unknown(CacheStatus::Invalid))
}

fn counter_status(read: Option<u64>) -> CacheStatus {
    match read {
        Some(0) => CacheStatus::Miss,
        Some(_) => CacheStatus::Hit,
        None => CacheStatus::Unknown,
    }
}

impl SessionState {
    /// The caller can include this in the same locked snapshot as context_index list metadata.
    pub(crate) fn context_cache_metadata(&self) -> Value {
        let current_revision = self.history.context_index_revision();
        let observation = self.latest_context_cache.as_ref();
        let request_revision = observation.and_then(|value| value.request_context_index_revision);
        let revision_status = match request_revision {
            Some(revision) if revision == current_revision => "current",
            Some(_) => "stale",
            None => "unknown",
        };
        json!({
            "scope": "response",
            "observation_id": observation.map(|value| value.observation_id.as_str()),
            "request_kind": "sampling",
            "status": observation.map_or(CacheStatus::Unknown, |value| value.status),
            "cached_input_tokens": observation.and_then(|value| value.cached_input_tokens),
            "write_input_tokens": observation.and_then(|value| value.write_input_tokens),
            "uncached_input_tokens": observation.and_then(|value| value.uncached_input_tokens),
            "total_input_tokens": observation.and_then(|value| value.total_input_tokens),
            "counter_provenance": observation.map_or("not_reported", |value| value.counter_provenance),
            "request_started_at": observation.and_then(|value| value.request_started_at.as_deref()),
            "requested_ttl_seconds": observation.and_then(|value| value.requested_ttl_seconds),
            "effective_ttl_seconds": null,
            "ttl_support": observation.map_or("unknown", |value| value.ttl_support),
            "ttl_application": observation.map_or("unknown", |value| value.ttl_application),
            "minimum_valid_until": null,
            "minimum_valid_until_provenance": "unknown",
            "minimum_valid_until_scope": "unknown",
            "estimated_expiry": null,
            "server_confirmed_expiry": null,
            "evidence": "reported_response_usage_counter_only",
            "observed_at": observation.map(|value| value.observed_at.as_str()),
            "observation_time_basis": "host_sampling_completed_event",
            "request_context_index_revision": request_revision,
            "current_context_index_revision": current_revision,
            "revision_status": revision_status,
            "source_cache_attribution": "not_reported",
            "retention": {"status": "unknown", "provenance": "not_provided", "duration_seconds": null},
            "expiry": {"status": "unknown", "provenance": "not_provided", "expires_at": null},
            "coverage": {
                "sampling": if observation.is_some() { "latest_completed_response" } else { "not_observed" },
                "compaction": "not_observed",
                "prewarm": "not_observed"
            }
        })
    }
}

impl Session {
    pub(crate) async fn record_context_cache_metadata(
        &self,
        usage: Option<&ResponseUsageMetadata>,
        request_context_index_revision: Option<u64>,
    ) {
        let observation = ContextCacheObservation::from_usage(
            usage,
            request_context_index_revision,
            Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        );
        self.state.lock().await.latest_context_cache = Some(observation);
    }

    pub(crate) async fn record_context_cache_response(
        &self,
        usage: Option<&ResponseUsageMetadata>,
        request_context_index_revision: Option<u64>,
        policy: Option<PromptCachePolicy>,
        request_started_at: DateTime<Utc>,
    ) {
        let mut observation = ContextCacheObservation::from_usage(
            usage,
            request_context_index_revision,
            Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        );
        observation.bind_request_policy(policy, request_started_at);
        self.state.lock().await.latest_context_cache = Some(observation);
    }

    pub(crate) async fn context_cache_metadata(&self) -> Value {
        self.state.lock().await.context_cache_metadata()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tests::make_session_and_context;
    use codex_attachment_store::InlineAttachmentStore;
    use codex_history::context_index::ContextIndexOperation;
    use codex_history::context_index::ContextTaskState;
    use codex_protocol::ResponseItemId;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    use codex_protocol::openai_models::InputModality;

    fn usage(raw: Value) -> ResponseUsageMetadata {
        ResponseUsageMetadata {
            amount: None,
            metadata: Some(raw),
        }
    }

    fn observation(raw: Option<Value>) -> ContextCacheObservation {
        ContextCacheObservation::from_usage(
            raw.map(usage).as_ref(),
            Some(0),
            "2026-11-10T00:00:00.000Z".to_string(),
        )
    }

    #[test]
    fn context_cache_missing_and_null_are_unknown_but_explicit_zero_is_miss() {
        for raw in [
            None,
            Some(Value::Null),
            Some(json!({})),
            Some(json!({"input_tokens_details":null})),
            Some(json!({"input_tokens_details":{}})),
            Some(json!({"input_tokens_details":{"cached_tokens":null}})),
        ] {
            let result = observation(raw);
            assert_eq!(result.status, CacheStatus::Unknown);
            assert_eq!(result.cached_input_tokens, None);
        }
        let result = observation(Some(
            json!({"input_tokens":100,"input_tokens_details":{"cached_tokens":0}}),
        ));
        assert_eq!(result.status, CacheStatus::Miss);
        assert_eq!(result.cached_input_tokens, Some(0));
    }

    #[test]
    fn context_cache_positive_counter_is_hit_with_exact_reported_count() {
        let result = observation(Some(
            json!({"input_tokens":100,"input_tokens_details":{"cached_tokens":64}}),
        ));
        assert_eq!(result.status, CacheStatus::Hit);
        assert_eq!(result.cached_input_tokens, Some(64));
    }

    #[test]
    fn context_cache_normalizes_disjoint_read_write_and_uncached_input() {
        let inclusive = cache_counters(Some(&json!({
            "input_tokens": 100,
            "input_tokens_details": {"cached_tokens": 60, "cache_write_tokens": 30}
        })));
        assert_eq!(inclusive.status, CacheStatus::Hit);
        assert_eq!(
            (
                inclusive.read,
                inclusive.write,
                inclusive.uncached,
                inclusive.total
            ),
            (Some(60), Some(30), Some(10), Some(100))
        );
        assert_eq!(inclusive.provenance, "inclusive_response_input_usage");
        let additive = cache_counters(Some(&json!({
            "input_tokens": 10, "cache_read_input_tokens": 60,
            "cache_creation_input_tokens": 30
        })));
        assert_eq!(
            (
                additive.read,
                additive.write,
                additive.uncached,
                additive.total
            ),
            (
                inclusive.read,
                inclusive.write,
                inclusive.uncached,
                inclusive.total
            )
        );
        assert_eq!(additive.provenance, "additive_response_input_usage");
    }

    #[test]
    fn context_cache_unreported_writes_are_unknown_instead_of_zero() {
        let counters = cache_counters(Some(&json!({
            "input_tokens":100,"input_tokens_details":{"cached_tokens":60}
        })));
        assert_eq!(counters.read, Some(60));
        assert_eq!(counters.total, Some(100));
        assert_eq!(counters.write, None);
        assert_eq!(counters.uncached, None);
        let missing_read = cache_counters(Some(&json!({
            "input_tokens":10,"cache_creation_input_tokens":30
        })));
        assert_eq!(missing_read.status, CacheStatus::Unknown);
        assert_eq!(missing_read.total, None);
    }

    #[test]
    fn context_cache_rejects_overflow_inconsistent_and_ambiguous_counters() {
        for raw in [
            json!({"input_tokens":10,"input_tokens_details":{"cached_tokens":6,"cache_write_tokens":5}}),
            json!({"input_tokens":10,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":-1}}),
            json!({"input_tokens":u64::MAX,"input_tokens_details":{"cached_tokens":u64::MAX,"cache_write_tokens":1}}),
            json!({"input_tokens":u64::MAX,"cache_read_input_tokens":1,"cache_creation_input_tokens":0}),
            json!({"input_tokens":10,"cache_read_input_tokens":"5","cache_creation_input_tokens":0}),
            json!({"input_tokens":10,"cache_read_input_tokens":0,"input_tokens_details":{"cached_tokens":0}}),
        ] {
            let counters = cache_counters(Some(&raw));
            assert_eq!(counters.status, CacheStatus::Invalid, "{raw}");
            assert_eq!(
                (
                    counters.read,
                    counters.write,
                    counters.uncached,
                    counters.total
                ),
                (None, None, None, None),
                "{raw}"
            );
        }
    }

    #[tokio::test]
    async fn context_cache_requested_ttl_and_counters_do_not_establish_retention() {
        use codex_model_provider_info::PromptCacheTtl;
        let start = DateTime::parse_from_rfc3339("2026-11-14T00:00:00.000Z")
            .expect("fixture clock")
            .with_timezone(&Utc);
        let (session, _turn) = make_session_and_context().await;
        let empty = session.context_cache_metadata().await;
        assert_eq!(empty["minimum_valid_until"], Value::Null);
        assert_eq!(empty["minimum_valid_until_provenance"], "unknown");
        assert_eq!(empty["minimum_valid_until_scope"], "unknown");
        for (ttl, seconds) in [
            (PromptCacheTtl::ThirtyMinutes, 1800),
            (PromptCacheTtl::TwelveHours, 43200),
        ] {
            let policy = Some(PromptCachePolicy::Implicit { ttl });
            for (raw, expected_status, expected_application, expected_read, expected_write) in [
                (
                    Some(
                        json!({"input_tokens":100,"input_tokens_details":{"cached_tokens":64,"cache_write_tokens":16}}),
                    ),
                    "hit",
                    "request_cache_activity_observed",
                    Some(64),
                    Some(16),
                ),
                (
                    Some(
                        json!({"input_tokens":100,"input_tokens_details":{"cached_tokens":64,"cache_write_tokens":0}}),
                    ),
                    "hit",
                    "request_cache_activity_observed",
                    Some(64),
                    Some(0),
                ),
                (
                    Some(
                        json!({"input_tokens":100,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0}}),
                    ),
                    "miss",
                    "requested_unconfirmed",
                    Some(0),
                    Some(0),
                ),
                (None, "unknown", "requested_unconfirmed", None, None),
                (
                    Some(json!({})),
                    "unknown",
                    "requested_unconfirmed",
                    None,
                    None,
                ),
                (
                    Some(json!({"input_tokens_details":{"cached_tokens":-1}})),
                    "invalid",
                    "requested_unconfirmed",
                    None,
                    None,
                ),
            ] {
                let raw = raw.map(usage);
                session
                    .record_context_cache_response(raw.as_ref(), Some(0), policy, start)
                    .await;
                let result = session.context_cache_metadata().await;
                assert_eq!(result["status"], expected_status);
                assert_eq!(result["ttl_application"], expected_application);
                assert_eq!(result["requested_ttl_seconds"], seconds);
                assert_eq!(result["request_started_at"], "2026-11-14T00:00:00.000Z");
                assert_eq!(result["cached_input_tokens"], json!(expected_read));
                assert_eq!(result["write_input_tokens"], json!(expected_write));
                let expected_total = expected_read.map(|_| 100_u64);
                let expected_uncached = expected_read
                    .zip(expected_write)
                    .map(|(read, write)| 100_u64 - read - write);
                assert_eq!(result["total_input_tokens"], json!(expected_total));
                assert_eq!(result["uncached_input_tokens"], json!(expected_uncached));
                assert_eq!(result["minimum_valid_until"], Value::Null);
                assert_eq!(result["minimum_valid_until_provenance"], "unknown");
                assert_eq!(result["minimum_valid_until_scope"], "unknown");
                assert_eq!(result["effective_ttl_seconds"], Value::Null);
                assert_eq!(result["server_confirmed_expiry"], Value::Null);
            }
        }
        let unconfigured_usage = usage(
            json!({"input_tokens":100,"input_tokens_details":{"cached_tokens":64,"cache_write_tokens":16}}),
        );
        session
            .record_context_cache_response(Some(&unconfigured_usage), None, None, start)
            .await;
        let no_policy = session.context_cache_metadata().await;
        assert_eq!(no_policy["requested_ttl_seconds"], Value::Null);
        assert_eq!(no_policy["write_input_tokens"], 16);
        assert_eq!(no_policy["ttl_support"], "unknown");
        assert_eq!(no_policy["minimum_valid_until"], Value::Null);
    }

    #[tokio::test]
    async fn context_cache_legacy_retention_is_never_reported_as_a_guaranteed_expiry() {
        use codex_model_provider_info::PromptCacheRetention;
        let start = DateTime::parse_from_rfc3339("2026-11-14T00:00:00Z")
            .expect("fixture clock")
            .with_timezone(&Utc);
        let (session, _turn) = make_session_and_context().await;
        for retention in [
            PromptCacheRetention::InMemory,
            PromptCacheRetention::TwentyFourHours,
        ] {
            let raw = usage(
                json!({"input_tokens":10,"input_tokens_details":{"cached_tokens":5,"cache_write_tokens":3}}),
            );
            session
                .record_context_cache_response(
                    Some(&raw),
                    Some(0),
                    Some(PromptCachePolicy::Legacy { retention }),
                    start,
                )
                .await;
            let metadata = session.context_cache_metadata().await;
            assert_eq!(metadata["requested_ttl_seconds"], Value::Null);
            assert_eq!(metadata["ttl_support"], "configured_provider_policy");
            assert_eq!(
                metadata["ttl_application"],
                "request_cache_activity_observed"
            );
            assert_eq!(metadata["write_input_tokens"], 3);
            assert_eq!(metadata["minimum_valid_until"], Value::Null);
            assert_eq!(metadata["minimum_valid_until_provenance"], "unknown");
            assert_eq!(metadata["minimum_valid_until_scope"], "unknown");
            assert_eq!(metadata["effective_ttl_seconds"], Value::Null);
            assert_eq!(metadata["server_confirmed_expiry"], Value::Null);
        }
    }

    #[tokio::test]
    async fn context_cache_metadata_reports_exact_counters_without_usage_payloads() {
        let (session, _turn) = make_session_and_context().await;
        let raw = usage(json!({
            "input_tokens":100,"input_tokens_details":{"cached_tokens":60,"cache_write_tokens":30},
            "private_extra":"PRIVATE_RAW_USAGE_1ac8"
        }));
        session
            .record_context_cache_metadata(Some(&raw), Some(0))
            .await;
        let metadata = session.context_cache_metadata().await;
        assert_eq!(metadata["cached_input_tokens"], json!(60));
        assert_eq!(metadata["write_input_tokens"], json!(30));
        assert_eq!(metadata["uncached_input_tokens"], json!(10));
        assert_eq!(metadata["total_input_tokens"], json!(100));
        assert_eq!(
            metadata["counter_provenance"],
            json!("inclusive_response_input_usage")
        );
        assert!(!metadata.to_string().contains("PRIVATE_RAW_USAGE_1ac8"));
    }

    #[test]
    fn identical_same_millisecond_observations_have_distinct_host_identity() {
        let usage = json!({"input_tokens":100,"input_tokens_details":{"cached_tokens":64}});
        let first = observation(Some(usage.clone()));
        let second = observation(Some(usage));
        assert_eq!(first.observed_at, second.observed_at);
        assert_eq!(first.cached_input_tokens, second.cached_input_tokens);
        assert_ne!(first.observation_id, second.observation_id);
        assert!(uuid::Uuid::parse_str(&first.observation_id).is_ok());
    }

    #[test]
    fn context_cache_invalid_containers_and_counters_are_not_hits() {
        for raw in [
            json!([]),
            json!("body"),
            json!({"input_tokens_details":[]}),
            json!({"input_tokens_details":{"cached_tokens":-1}}),
            json!({"input_tokens_details":{"cached_tokens":1.5}}),
            json!({"input_tokens_details":{"cached_tokens":"64"}}),
            json!({"input_tokens":10,"input_tokens_details":{"cached_tokens":11}}),
            json!({"input_tokens":"100","input_tokens_details":{"cached_tokens":64}}),
        ] {
            let result = observation(Some(raw));
            assert_eq!(result.status, CacheStatus::Invalid);
            assert_eq!(result.cached_input_tokens, None);
        }
    }

    #[tokio::test]
    async fn context_cache_expiry_unknown_and_no_source_or_private_metadata_copied() {
        let (session, _turn) = make_session_and_context().await;
        let raw = usage(json!({
            "input_tokens":100,"input_tokens_details":{"cached_tokens":64},
            "prompt_cache_key":"PRIVATE_CACHE_KEY_d68a", "body":"PRIVATE_BODY_e573",
            "cache_ttl_seconds":3600, "expires_at":"PRIVATE_EXPIRY_c549",
            "sources":[{"id":"PRIVATE_SOURCE_b92a","cached":true}]
        }));
        session
            .record_context_cache_metadata(Some(&raw), Some(0))
            .await;
        let metadata = session.context_cache_metadata().await;
        assert_eq!(metadata["scope"], json!("response"));
        assert_eq!(metadata["status"], json!("hit"));
        assert_eq!(metadata["source_cache_attribution"], json!("not_reported"));
        assert_eq!(
            metadata["retention"],
            json!({"status":"unknown","provenance":"not_provided","duration_seconds":null})
        );
        assert_eq!(
            metadata["expiry"],
            json!({"status":"unknown","provenance":"not_provided","expires_at":null})
        );
        assert_eq!(metadata["coverage"]["compaction"], json!("not_observed"));
        assert_eq!(metadata["coverage"]["prewarm"], json!("not_observed"));
        let encoded = metadata.to_string();
        for forbidden in [
            "PRIVATE_CACHE_KEY_d68a",
            "PRIVATE_BODY_e573",
            "PRIVATE_EXPIRY_c549",
            "PRIVATE_SOURCE_b92a",
            "3600",
        ] {
            assert!(
                !encoded.contains(forbidden),
                "private or unsupported value copied: {forbidden}"
            );
        }
    }

    #[tokio::test]
    async fn context_cache_late_response_retains_request_revision_and_is_stale() {
        let (session, _turn) = make_session_and_context().await;
        // Request left with revision 0. The policy changes before its completed response arrives.
        session
            .state
            .lock()
            .await
            .history
            .apply_context_index(
                0,
                vec![ContextIndexOperation::CreateTask {
                    task_id: "new-policy".to_string(),
                    label: "New policy".to_string(),
                }],
            )
            .expect("policy change");
        let raw = usage(json!({"input_tokens_details":{"cached_tokens":5}}));
        session
            .record_context_cache_metadata(Some(&raw), Some(0))
            .await;
        let metadata = session.context_cache_metadata().await;
        assert_eq!(metadata["request_context_index_revision"], json!(0));
        assert_eq!(metadata["current_context_index_revision"], json!(1));
        assert_eq!(metadata["revision_status"], json!("stale"));
        session
            .record_context_cache_metadata(Some(&raw), None)
            .await;
        let unbound = session.context_cache_metadata().await;
        assert_eq!(unbound["request_context_index_revision"], Value::Null);
        assert_eq!(unbound["revision_status"], json!("unknown"));
    }

    #[tokio::test]
    async fn context_cache_is_thread_local_and_does_not_change_prepared_model_prompt() {
        let (session, turn) = make_session_and_context().await;
        let item = ResponseItem::Message {
            id: Some(ResponseItemId::with_suffix("msg", "cache-prompt-original")),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "EXCLUDED_CACHE_PROMPT_ORIGINAL_c02e".to_string(),
                annotations: None,
                logprobs: None,
            }],
            status: None,
            encrypted_content: None,
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        };
        session
            .record_conversation_items(&turn, turn.model_info(), &[item])
            .await;
        session
            .state
            .lock()
            .await
            .history
            .apply_context_index(
                0,
                vec![
                    ContextIndexOperation::CreateTask {
                        task_id: "completed".to_string(),
                        label: "Completed cache fixture".to_string(),
                    },
                    ContextIndexOperation::BindSources {
                        task_id: "completed".to_string(),
                        source_ids: vec!["msg_cache-prompt-original".to_string()],
                    },
                    ContextIndexOperation::SetTaskState {
                        task_id: "completed".to_string(),
                        state: ContextTaskState::Completed,
                    },
                ],
            )
            .expect("excluded source creates navigation manifest");
        let before = session
            .clone_history()
            .await
            .projected_for_prompt(&[InputModality::Text], &InlineAttachmentStore)
            .await;
        assert!(
            !before.is_empty(),
            "navigation manifest must exercise the actual prompt path"
        );
        assert!(
            !serde_json::to_string(&before)
                .expect("prepared JSON")
                .contains("EXCLUDED_CACHE_PROMPT_ORIGINAL_c02e")
        );
        let raw = usage(json!({"input_tokens_details":{"cached_tokens":5}}));
        session
            .record_context_cache_metadata(Some(&raw), Some(1))
            .await;
        let after = session
            .clone_history()
            .await
            .projected_for_prompt(&[InputModality::Text], &InlineAttachmentStore)
            .await;
        assert_eq!(before, after);
        let metadata = session.context_cache_metadata().await;
        assert_eq!(metadata["revision_status"], json!("current"));
        assert!(metadata["observed_at"].as_str().is_some());
        let (other, _turn) = make_session_and_context().await;
        let other_metadata = other.context_cache_metadata().await;
        assert_eq!(other_metadata["status"], json!("unknown"));
        assert_eq!(other_metadata["observed_at"], Value::Null);
        assert_eq!(other_metadata["cached_input_tokens"], Value::Null);
        assert_eq!(
            other_metadata["coverage"]["sampling"],
            json!("not_observed")
        );
    }

    #[tokio::test]
    async fn context_cache_missing_completed_usage_replaces_previous_hit_with_unknown() {
        let (session, _turn) = make_session_and_context().await;
        let raw = usage(json!({"input_tokens_details":{"cached_tokens":5}}));
        session
            .record_context_cache_metadata(Some(&raw), Some(0))
            .await;
        session.record_context_cache_metadata(None, Some(0)).await;
        let metadata = session.context_cache_metadata().await;
        assert_eq!(metadata["status"], json!("unknown"));
        assert_eq!(metadata["cached_input_tokens"], Value::Null);
        assert_eq!(
            metadata["coverage"]["sampling"],
            json!("latest_completed_response")
        );
    }
}
