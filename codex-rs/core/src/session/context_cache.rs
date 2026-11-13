// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
//! Response-level cache observations. They never determine context relevance, authorization,
//! retention of original history, or whether an individual source was cached.
use super::session::Session;
use crate::state::SessionState;
use chrono::SecondsFormat;
use chrono::Utc;
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

/// Deliberately stores only a numeric usage observation, time and request revision. Raw usage,
/// cache keys, response bodies, source identities and tokens are never copied into this state.
pub(crate) struct ContextCacheObservation {
    status: CacheStatus,
    cached_input_tokens: Option<u64>,
    observed_at: String,
    request_context_index_revision: Option<u64>,
}

impl ContextCacheObservation {
    fn from_usage(
        usage: Option<&ResponseUsageMetadata>,
        request_context_index_revision: Option<u64>,
        observed_at: String,
    ) -> Self {
        let (status, cached_input_tokens) =
            cache_counter(usage.and_then(|usage| usage.metadata.as_ref()));
        Self {
            status,
            cached_input_tokens,
            observed_at,
            request_context_index_revision,
        }
    }
}

fn cache_counter(raw_usage: Option<&Value>) -> (CacheStatus, Option<u64>) {
    let raw_usage = match raw_usage {
        None | Some(Value::Null) => return (CacheStatus::Unknown, None),
        Some(Value::Object(usage)) => usage,
        Some(_) => return (CacheStatus::Invalid, None),
    };
    let details = match raw_usage.get("input_tokens_details") {
        None | Some(Value::Null) => return (CacheStatus::Unknown, None),
        Some(Value::Object(details)) => details,
        Some(_) => return (CacheStatus::Invalid, None),
    };
    let cached = match details.get("cached_tokens") {
        None | Some(Value::Null) => return (CacheStatus::Unknown, None),
        Some(value) => match value.as_u64() {
            Some(cached) => cached,
            None => return (CacheStatus::Invalid, None),
        },
    };
    if let Some(input) = raw_usage
        .get("input_tokens")
        .filter(|input| !input.is_null())
    {
        match input.as_u64() {
            Some(input) if cached <= input => {}
            _ => return (CacheStatus::Invalid, None),
        }
    }
    (
        if cached == 0 {
            CacheStatus::Miss
        } else {
            CacheStatus::Hit
        },
        Some(cached),
    )
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
            "request_kind": "sampling",
            "status": observation.map_or(CacheStatus::Unknown, |value| value.status),
            "cached_input_tokens": observation.and_then(|value| value.cached_input_tokens),
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
