// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use super::session::Session;
use crate::context_manager::{
    CompositionFragment, CompositionIdentity, CompositionSizes, HistoryComposition,
};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Write;

struct HashWriter(Sha256);
impl std::io::Write for HashWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCacheView {
    pub scope: &'static str,
    pub provenance: &'static str,
    pub status: String,
    pub cached_input_tokens: Option<u64>,
    pub write_input_tokens: Option<u64>,
    pub uncached_input_tokens: Option<u64>,
    pub total_input_tokens: Option<u64>,
    pub observed_at: Option<String>,
    pub observation_id: Option<String>,
    pub request_context_index_revision: Option<u64>,
    pub revision_status: String,
    pub requested_ttl_seconds: Option<u64>,
    pub effective_ttl_seconds: Option<u64>,
    pub ttl_support: String,
    pub ttl_application: String,
    pub request_started_at: Option<String>,
    pub minimum_valid_until: Option<String>,
    pub minimum_valid_until_provenance: String,
    pub minimum_valid_until_scope: String,
    pub estimated_expiry: Option<String>,
    pub server_confirmed_expiry: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextCompositionPage {
    pub snapshot_id: String,
    /// This is the page observation time, excluded from snapshot identity.
    pub as_of: String,
    pub context_index_revision: u64,
    pub history_version: u64,
    pub sizes: CompositionSizes,
    pub limits: ContextLimits,
    pub fragments: Vec<CompositionFragment>,
    pub cache: ContextCacheView,
    /// Latest prepared sampling prompt, which can precede the current history.
    pub request_sizes: Option<ContextRequestSizes>,
    pub next_cursor: Option<usize>,
}

/// Numeric observations only. JSON component payloads precede provider normalization,
/// transport envelopes and delta encoding; their sum is not a billed token count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRequestSizes {
    pub observation_id: String,
    pub recorded_at: String,
    pub scope: &'static str,
    pub provenance: &'static str,
    pub history_version: u64,
    pub context_index_revision: u64,
    pub input_serialized_bytes: u64,
    pub base_instructions_serialized_bytes: u64,
    /// Standalone tools field only; incremental definitions in input remain counted there.
    pub tool_definitions_serialized_bytes: u64,
    pub output_schema_serialized_bytes: u64,
    pub component_payload_bytes: u64,
}

impl ContextRequestSizes {
    fn measure(
        prompt: &crate::client_common::Prompt,
        history_version: u64,
        context_index_revision: u64,
    ) -> Result<Self, String> {
        fn bytes<T: Serialize + ?Sized>(value: &T) -> Result<u64, String> {
            let count = crate::utils::json::serialized_json_bytes(value)
                .map_err(|error| format!("request size serialization failed: {error}"))?;
            u64::try_from(count).map_err(|error| error.to_string())
        }
        let input_serialized_bytes = bytes(&prompt.input)?;
        let base_instructions_serialized_bytes = bytes(&prompt.base_instructions.text)?;
        let tool_definitions_serialized_bytes = bytes(prompt.tools.as_ref())?;
        let output_schema_serialized_bytes = prompt
            .output_schema
            .as_ref()
            .map(bytes)
            .transpose()?
            .unwrap_or(0);
        let component_payload_bytes = [
            input_serialized_bytes,
            base_instructions_serialized_bytes,
            tool_definitions_serialized_bytes,
            output_schema_serialized_bytes,
        ]
        .into_iter()
        .try_fold(0u64, |total, part| total.checked_add(part))
        .ok_or("request component size overflow")?;
        Ok(Self {
            observation_id: uuid::Uuid::new_v4().to_string(),
            recorded_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            scope: "prepared_sampling_prompt",
            provenance: "utf8_json_components_before_provider_normalization",
            history_version,
            context_index_revision,
            input_serialized_bytes,
            base_instructions_serialized_bytes,
            tool_definitions_serialized_bytes,
            output_schema_serialized_bytes,
            component_payload_bytes,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextLimits {
    pub requested_context_window_override: Option<i64>,
    pub effective_context_window: Option<i64>,
    pub provenance: &'static str,
}

pub(crate) struct CompositionCache {
    runtime_id: uuid::Uuid,
    identity: CompositionIdentity,
    observation: ContextCacheView,
    limits: ContextLimits,
    request_sizes: Option<ContextRequestSizes>,
    snapshot_id: String,
    composition: HistoryComposition,
}

fn cache_view(value: Value) -> Result<ContextCacheView, String> {
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .ok_or("missing cache status")?;
    let revision = value
        .get("revision_status")
        .and_then(Value::as_str)
        .ok_or("missing cache revision status")?;
    let provenance = match value.get("counter_provenance").and_then(Value::as_str) {
        Some("inclusive_response_input_usage") => "inclusive_response_input_usage",
        Some("additive_response_input_usage") => "additive_response_input_usage",
        Some("not_reported") => "not_reported",
        _ => return Err("invalid cache counter provenance".into()),
    };
    let optional_string = |key: &str| -> Result<Option<String>, String> {
        match value.get(key) {
            Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            _ => Err(format!("invalid cache metadata {key}")),
        }
    };
    let optional_u64 = |key: &str| -> Result<Option<u64>, String> {
        match value.get(key) {
            Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_u64()
                .map(Some)
                .ok_or_else(|| format!("invalid cache metadata {key}")),
            None => Err(format!("missing cache metadata {key}")),
        }
    };
    let required_string = |key: &str| -> Result<String, String> {
        value
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("invalid cache metadata {key}"))
    };
    let ttl_support = required_string("ttl_support")?;
    if !matches!(
        ttl_support.as_str(),
        "unknown" | "configured_provider_policy"
    ) {
        return Err("invalid cache ttl support".into());
    }
    let ttl_application = required_string("ttl_application")?;
    if !matches!(
        ttl_application.as_str(),
        "unknown" | "requested_unconfirmed" | "request_cache_activity_observed"
    ) {
        return Err("invalid cache ttl application".into());
    }
    Ok(ContextCacheView {
        scope: "response",
        provenance,
        status: status.to_owned(),
        cached_input_tokens: optional_u64("cached_input_tokens")?,
        write_input_tokens: optional_u64("write_input_tokens")?,
        uncached_input_tokens: optional_u64("uncached_input_tokens")?,
        total_input_tokens: optional_u64("total_input_tokens")?,
        observed_at: optional_string("observed_at")?,
        observation_id: optional_string("observation_id")?,
        request_context_index_revision: optional_u64("request_context_index_revision")?,
        revision_status: revision.to_owned(),
        requested_ttl_seconds: optional_u64("requested_ttl_seconds")?,
        effective_ttl_seconds: optional_u64("effective_ttl_seconds")?,
        ttl_support,
        ttl_application,
        request_started_at: optional_string("request_started_at")?,
        minimum_valid_until: optional_string("minimum_valid_until")?,
        minimum_valid_until_provenance: required_string("minimum_valid_until_provenance")?,
        minimum_valid_until_scope: required_string("minimum_valid_until_scope")?,
        estimated_expiry: optional_string("estimated_expiry")?,
        server_confirmed_expiry: optional_string("server_confirmed_expiry")?,
    })
}

impl Session {
    pub(crate) async fn record_context_request_sizes(
        &self,
        prompt: &crate::client_common::Prompt,
        history_version: u64,
        context_index_revision: u64,
    ) -> Result<(), String> {
        // Count without retaining another body or holding the session mutex.
        let sizes = ContextRequestSizes::measure(prompt, history_version, context_index_revision)?;
        self.state.lock().await.latest_context_request_sizes = Some(sizes);
        Ok(())
    }

    pub(crate) async fn context_composition_page(
        &self,
        cache: &tokio::sync::Mutex<Option<CompositionCache>>,
        runtime_id: uuid::Uuid,
        requested_snapshot: Option<&str>,
        cursor: usize,
        limit: usize,
    ) -> Result<ContextCompositionPage, String> {
        if !(1..=100).contains(&limit) {
            return Err("limit must be in 1..100".into());
        }
        // The two clones form one coherent capture under the same session mutex.
        let (history, observation, request_sizes, limits, as_of) = {
            let state = self.state.lock().await;
            if state.context_index_error.is_some() {
                return Err("context index requires cold resume".into());
            }
            (
                state.history.clone(),
                cache_view(state.context_cache_metadata())?,
                state.latest_context_request_sizes.clone(),
                ContextLimits {
                    requested_context_window_override: state
                        .session_configuration
                        .context_window_override,
                    effective_context_window: None,
                    provenance: "session_configuration_override",
                },
                Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            )
        };
        // Serialize one scan per runtime and generation. No history body is retained by the cache.
        let mut cached = cache.lock().await;
        let matches = cached.as_ref().is_some_and(|entry| {
            entry.runtime_id == runtime_id
                && entry.identity.matches(&history)
                && entry.observation == observation
                && entry.request_sizes == request_sizes
                && entry.limits == limits
        });
        if !matches {
            let identity = history.composition_identity();
            let composition = history.history_composition().await?;
            // Bind a fresh generation nonce as well: two distinct histories can have identical
            // body-free metadata and sizes, and must never produce interchangeable cursors.
            let mut digest = HashWriter(Sha256::new());
            serde_json::to_writer(
                &mut digest,
                &(
                    runtime_id,
                    uuid::Uuid::new_v4(),
                    composition.context_index_revision,
                    composition.history_version,
                    &composition.sizes,
                    &observation,
                    &request_sizes,
                    &limits,
                ),
            )
            .map_err(|e| e.to_string())?;
            digest.write_all(b"\n").map_err(|e| e.to_string())?;
            for (position, fragment) in composition.fragments.iter().enumerate() {
                if position != 0 && position % 32 == 0 {
                    tokio::task::yield_now().await;
                }
                serde_json::to_writer(&mut digest, fragment).map_err(|e| e.to_string())?;
                digest.write_all(b"\n").map_err(|e| e.to_string())?;
            }
            *cached = Some(CompositionCache {
                runtime_id,
                identity,
                observation,
                request_sizes,
                limits,
                snapshot_id: format!("{:x}", digest.0.finalize()),
                composition,
            });
        }
        let entry = cached.as_ref().ok_or("composition cache unavailable")?;
        if requested_snapshot.is_some_and(|expected| expected != entry.snapshot_id) {
            return Err("stale context snapshot".into());
        }
        // The scan yields without holding the session lock. Reject a capture that changed
        // while it was being built, including on the first page.
        {
            let state = self.state.lock().await;
            if state.context_index_error.is_some()
                || !entry.identity.matches(&state.history)
                || entry.observation != cache_view(state.context_cache_metadata())?
                || entry.request_sizes != state.latest_context_request_sizes
                || entry.limits.requested_context_window_override
                    != state.session_configuration.context_window_override
            {
                return Err("stale context snapshot".into());
            }
        }
        let count = entry.composition.fragments.len();
        if cursor > count {
            return Err("cursor exceeds context fragment count".into());
        }
        let end = cursor.saturating_add(limit).min(count);
        Ok(ContextCompositionPage {
            snapshot_id: entry.snapshot_id.clone(),
            as_of,
            context_index_revision: entry.composition.context_index_revision,
            history_version: entry.composition.history_version,
            sizes: entry.composition.sizes.clone(),
            limits: entry.limits.clone(),
            fragments: entry.composition.fragments[cursor..end].to_vec(),
            cache: entry.observation.clone(),
            request_sizes: entry.request_sizes.clone(),
            next_cursor: (end < count).then_some(end),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tests::make_session_and_context;
    use codex_protocol::models::ResponseItem;
    use codex_utils_output_truncation::TruncationPolicy;

    fn item(id: &str, text: &str) -> ResponseItem {
        serde_json::from_value(serde_json::json!({
            "type":"message", "id":id, "role":"assistant",
            "content":[{"type":"output_text", "text":text}]
        }))
        .unwrap()
    }

    fn request_sizes_fixture(text: &str) -> crate::client_common::Prompt {
        let mut prompt = crate::client_common::Prompt::default();
        prompt.input = vec![item("synthetic-id", text)];
        prompt.base_instructions.text = "한🙂\"".into();
        prompt.tools = vec![codex_tools::ToolSpec::ToolSearch {
            execution: "server".into(),
            description: "synthetic-private-tool-description".into(),
            parameters: serde_json::from_value(serde_json::json!({"type":"object"})).unwrap(),
        }]
        .into();
        prompt.output_schema =
            Some(serde_json::json!({"type":"string","description":"synthetic-private-schema"}));
        prompt
    }

    #[tokio::test]
    async fn request_sizes_count_components_without_exposing_prompt_bodies() {
        let (session, _) = make_session_and_context().await;
        let cache = tokio::sync::Mutex::new(None);
        let runtime = uuid::Uuid::new_v4();
        let initial = session
            .context_composition_page(&cache, runtime, None, 0, 1)
            .await
            .unwrap();
        assert!(
            initial.request_sizes.is_none(),
            "no prepared prompt is unknown, not zero"
        );
        let prompt = request_sizes_fixture("synthetic-private-body-한🙂");
        session
            .record_context_request_sizes(&prompt, 9, 7)
            .await
            .unwrap();
        let page = session
            .context_composition_page(&cache, runtime, None, 0, 1)
            .await
            .unwrap();
        let sizes = page.request_sizes.as_ref().unwrap();
        assert_eq!(sizes.base_instructions_serialized_bytes, 11);
        assert_eq!(
            sizes.input_serialized_bytes,
            serde_json::to_vec(&prompt.input).unwrap().len() as u64
        );
        assert_eq!(
            sizes.tool_definitions_serialized_bytes,
            serde_json::to_vec(prompt.tools.as_ref()).unwrap().len() as u64
        );
        assert_eq!(
            sizes.output_schema_serialized_bytes,
            serde_json::to_vec(prompt.output_schema.as_ref().unwrap())
                .unwrap()
                .len() as u64
        );
        assert_eq!(
            sizes.component_payload_bytes,
            sizes.input_serialized_bytes
                + 11
                + sizes.tool_definitions_serialized_bytes
                + sizes.output_schema_serialized_bytes
        );
        assert_eq!(
            (sizes.history_version, sizes.context_index_revision),
            (9, 7)
        );
        assert_eq!(sizes.scope, "prepared_sampling_prompt");
        let encoded = serde_json::to_string(&page).unwrap();
        for sentinel in [
            "synthetic-private-body",
            "synthetic-private-tool-description",
            "synthetic-private-schema",
        ] {
            assert!(!encoded.contains(sentinel));
        }
        let empty = ContextRequestSizes::measure(&Default::default(), 0, 0).unwrap();
        assert_eq!(
            empty.output_schema_serialized_bytes, 0,
            "no output schema is known absent"
        );
        assert_eq!(
            empty.component_payload_bytes, 6,
            "empty input, instruction string and tools each serialize to two bytes"
        );
    }

    #[tokio::test]
    async fn request_sizes_new_preparation_invalidates_existing_cursor() {
        let (session, _) = make_session_and_context().await;
        let cache = tokio::sync::Mutex::new(None);
        let runtime = uuid::Uuid::new_v4();
        let prompt = request_sizes_fixture("synthetic-first");
        session
            .record_context_request_sizes(&prompt, 0, 0)
            .await
            .unwrap();
        let first = session
            .context_composition_page(&cache, runtime, None, 0, 1)
            .await
            .unwrap();
        session
            .record_context_request_sizes(&request_sizes_fixture("synthetic-second-longer"), 0, 0)
            .await
            .unwrap();
        assert_eq!(
            session
                .context_composition_page(&cache, runtime, Some(&first.snapshot_id), 0, 1)
                .await
                .unwrap_err(),
            "stale context snapshot"
        );
        let next = session
            .context_composition_page(&cache, runtime, None, 0, 1)
            .await
            .unwrap();
        assert_ne!(
            first.request_sizes.as_ref().unwrap().observation_id,
            next.request_sizes.as_ref().unwrap().observation_id
        );
        assert!(
            next.request_sizes.unwrap().input_serialized_bytes
                > first.request_sizes.unwrap().input_serialized_bytes
        );
    }

    #[tokio::test]
    async fn request_sizes_change_during_first_capture_rejects_mixed_snapshot() {
        let (session, _) = make_session_and_context().await;
        let cache = tokio::sync::Mutex::new(None);
        let guard = cache.lock().await;
        let read = session.context_composition_page(&cache, uuid::Uuid::new_v4(), None, 0, 1);
        tokio::pin!(read);
        tokio::select! {
            biased;
            result = &mut read => panic!("capture unexpectedly completed: {result:?}"),
            _ = tokio::task::yield_now() => {}
        }
        session
            .record_context_request_sizes(&request_sizes_fixture("synthetic-new-attempt"), 0, 0)
            .await
            .unwrap();
        drop(guard);
        assert_eq!(read.await.unwrap_err(), "stale context snapshot");
    }

    #[test]
    #[ignore = "explicit local measurement cost benchmark; no provider or whole-app speed claim"]
    fn request_sizes_measurement_cost() {
        for text_bytes in [64 * 1024, 1024 * 1024, 8 * 1024 * 1024] {
            let prompt = request_sizes_fixture(&"x".repeat(text_bytes));
            let expected = ContextRequestSizes::measure(&prompt, 0, 0)
                .unwrap()
                .component_payload_bytes;
            let serialize = || {
                std::hint::black_box(
                    serde_json::to_vec(&(
                        &prompt.input,
                        &prompt.base_instructions.text,
                        prompt.tools.as_ref(),
                        &prompt.output_schema,
                    ))
                    .unwrap(),
                );
            };
            let mut baseline = Vec::new();
            let mut measured = Vec::new();
            for iteration in 0..35 {
                let mut sample = |with_measurement: bool| {
                    let start = std::time::Instant::now();
                    serialize();
                    if with_measurement {
                        let sizes = ContextRequestSizes::measure(&prompt, 0, 0).unwrap();
                        assert_eq!(sizes.component_payload_bytes, expected);
                        std::hint::black_box(sizes);
                    }
                    let elapsed = start.elapsed().as_nanos();
                    if iteration >= 5 {
                        if with_measurement {
                            measured.push(elapsed);
                        } else {
                            baseline.push(elapsed);
                        }
                    }
                };
                sample(iteration % 2 == 0);
                sample(iteration % 2 != 0);
            }
            println!(
                "contextRequestSizeCost={}",
                serde_json::json!({"textBytes":text_bytes,"componentPayloadBytes":expected,"baselineSerializationNs":baseline,"serializationWithMeasurementNs":measured,"scope":"synthetic_local_json_serialization_only"})
            );
        }
    }

    #[test]
    fn cache_usage_preserves_values_and_unknowns() {
        let value = serde_json::json!({
            "status": "observed", "revision_status": "unknown",
            "counter_provenance": "inclusive_response_input_usage",
            "cached_input_tokens": 12, "write_input_tokens": 3,
            "uncached_input_tokens": 5, "total_input_tokens": 20,
            "observed_at": null, "observation_id": null,
            "request_context_index_revision": null,
            "requested_ttl_seconds": 1800, "effective_ttl_seconds": null,
            "ttl_support": "configured_provider_policy", "ttl_application": "request_cache_activity_observed",
            "request_started_at": "2026-11-14T00:00:00.000Z",
            "minimum_valid_until": null,
            "minimum_valid_until_provenance": "unknown",
            "minimum_valid_until_scope": "unknown",
            "estimated_expiry": null, "server_confirmed_expiry": null
        });
        let view = cache_view(value.clone()).unwrap();
        assert_eq!(view.cached_input_tokens, Some(12));
        assert_eq!(view.write_input_tokens, Some(3));
        assert_eq!(view.uncached_input_tokens, Some(5));
        assert_eq!(view.total_input_tokens, Some(20));
        assert_eq!(view.provenance, "inclusive_response_input_usage");
        assert_eq!(view.requested_ttl_seconds, Some(1800));
        assert_eq!(
            view.request_started_at.as_deref(),
            Some("2026-11-14T00:00:00.000Z")
        );
        assert_eq!(view.minimum_valid_until, None);
        assert_eq!(view.minimum_valid_until_provenance, "unknown");
        assert_eq!(view.ttl_application, "request_cache_activity_observed");
        assert_eq!(view.minimum_valid_until_scope, "unknown");
        assert_eq!(view.effective_ttl_seconds, None);
        assert_eq!(view.server_confirmed_expiry, None);
        let mut invalid = value;
        invalid["counter_provenance"] = "unexpected".into();
        assert!(cache_view(invalid).is_err());
    }

    #[tokio::test]
    async fn context_cache_typed_view_preserves_unconfirmed_ttl_and_usage() {
        use codex_model_provider_info::{PromptCachePolicy, PromptCacheTtl};
        let (session, _) = make_session_and_context().await;
        let initial = cache_view(session.context_cache_metadata().await).unwrap();
        assert_eq!(initial.minimum_valid_until, None);
        assert_eq!(initial.minimum_valid_until_provenance, "unknown");
        assert_eq!(initial.minimum_valid_until_scope, "unknown");
        let started = chrono::DateTime::parse_from_rfc3339("2026-11-14T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let usage = codex_protocol::ResponseUsageMetadata {
            amount: None,
            metadata: Some(
                serde_json::json!({"input_tokens":100,"input_tokens_details":{"cached_tokens":64,"cache_write_tokens":16}}),
            ),
        };
        for (ttl, seconds) in [
            (PromptCacheTtl::ThirtyMinutes, 1800),
            (PromptCacheTtl::TwelveHours, 43200),
        ] {
            session
                .record_context_cache_response(
                    Some(&usage),
                    Some(0),
                    Some(PromptCachePolicy::Implicit { ttl }),
                    started,
                )
                .await;
            let view = cache_view(session.context_cache_metadata().await).unwrap();
            assert_eq!(view.requested_ttl_seconds, Some(seconds));
            assert_eq!(
                view.request_started_at.as_deref(),
                Some("2026-11-14T00:00:00.000Z")
            );
            assert_eq!(view.cached_input_tokens, Some(64));
            assert_eq!(view.write_input_tokens, Some(16));
            assert_eq!(view.uncached_input_tokens, Some(20));
            assert_eq!(view.total_input_tokens, Some(100));
            assert_eq!(view.minimum_valid_until, None);
            assert_eq!(view.minimum_valid_until_provenance, "unknown");
            assert_eq!(view.minimum_valid_until_scope, "unknown");
            assert_eq!(view.effective_ttl_seconds, None);
            assert_eq!(view.server_confirmed_expiry, None);
        }
    }

    #[tokio::test]
    async fn pages_bind_generation_and_do_not_expose_bodies() {
        let (session, _) = make_session_and_context().await;
        let cache = tokio::sync::Mutex::new(None);
        let runtime = uuid::Uuid::new_v4();
        session.state.lock().await.history.record_items(
            [item("a", "secret synthetic body"), item("b", "other")].iter(),
            TruncationPolicy::Bytes(usize::MAX),
        );
        let first = session
            .context_composition_page(&cache, runtime, None, 0, 1)
            .await
            .unwrap();
        let second = session
            .context_composition_page(&cache, runtime, Some(&first.snapshot_id), 1, 1)
            .await
            .unwrap();
        assert_eq!(first.snapshot_id, second.snapshot_id);
        assert_eq!(first.next_cursor, Some(1));
        assert_eq!(second.next_cursor, None);
        let encoded = serde_json::to_string(&first).unwrap();
        assert!(!encoded.contains("secret synthetic body"));
        assert!(first.cache.server_confirmed_expiry.is_none());
        session.state.lock().await.history.record_items(
            [item("c", "new")].iter(),
            TruncationPolicy::Bytes(usize::MAX),
        );
        assert_eq!(
            session
                .context_composition_page(&cache, runtime, Some(&first.snapshot_id), 1, 1)
                .await
                .unwrap_err(),
            "stale context snapshot"
        );
    }

    #[tokio::test]
    async fn first_page_rejects_history_changed_during_capture() {
        let (session, _) = make_session_and_context().await;
        let cache = tokio::sync::Mutex::new(None);
        let guard = cache.lock().await;
        let read = session.context_composition_page(&cache, uuid::Uuid::new_v4(), None, 0, 1);
        tokio::pin!(read);
        // Poll the read through its state capture until it waits on the held cache mutex.
        tokio::select! {
            biased;
            result = &mut read => panic!("read unexpectedly completed: {result:?}"),
            _ = tokio::task::yield_now() => {}
        }
        session.state.lock().await.history.record_items(
            [item("changed", "synthetic")].iter(),
            TruncationPolicy::Bytes(usize::MAX),
        );
        drop(guard);
        assert_eq!(read.await.unwrap_err(), "stale context snapshot");
    }

    #[tokio::test]
    async fn cache_observation_and_runtime_change_invalidate_cursor() {
        let (session, _) = make_session_and_context().await;
        let cache = tokio::sync::Mutex::new(None);
        let runtime = uuid::Uuid::new_v4();
        let first = session
            .context_composition_page(&cache, runtime, None, 0, 1)
            .await
            .unwrap();
        session.record_context_cache_metadata(None, Some(0)).await;
        assert_eq!(
            session
                .context_composition_page(&cache, runtime, Some(&first.snapshot_id), 0, 1)
                .await
                .unwrap_err(),
            "stale context snapshot"
        );
        let second = session
            .context_composition_page(&cache, runtime, None, 0, 1)
            .await
            .unwrap();
        // A cache belongs to one CodexThread; using a different runtime must never reuse it.
        assert_eq!(
            session
                .context_composition_page(
                    &cache,
                    uuid::Uuid::new_v4(),
                    Some(&second.snapshot_id),
                    0,
                    1
                )
                .await
                .unwrap_err(),
            "stale context snapshot"
        );
    }

    #[tokio::test]
    async fn identical_observations_are_distinct_generations() {
        let (session, _) = make_session_and_context().await;
        let cache = tokio::sync::Mutex::new(None);
        let runtime = uuid::Uuid::new_v4();
        session.record_context_cache_metadata(None, Some(0)).await;
        let first = session
            .context_composition_page(&cache, runtime, None, 0, 1)
            .await
            .unwrap();
        assert!(first.cache.observation_id.is_some());
        session.record_context_cache_metadata(None, Some(0)).await;
        let second = session
            .context_composition_page(&cache, runtime, None, 0, 1)
            .await
            .unwrap();
        assert_ne!(first.cache.observation_id, second.cache.observation_id);
        assert_ne!(first.snapshot_id, second.snapshot_id);
    }
}
