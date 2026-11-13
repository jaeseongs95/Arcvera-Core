// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolOutput;
use crate::tools::context::ToolPayload;
use crate::tools::context::boxed_tool_output;
use crate::tools::handlers::context_index_spec::CONTEXT_INDEX_TOOL_NAME;
use crate::tools::handlers::context_index_spec::create_context_index_tool;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolExecutor;
use codex_protocol::models::ResponseInputItem;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use serde_json::Value;

pub struct ContextIndexHandler;

struct ContextIndexOutput(Value);

impl ToolOutput for ContextIndexOutput {
    fn log_output(&self) -> String {
        self.0.to_string()
    }

    fn success_for_logging(&self) -> bool {
        true
    }

    fn to_response_item(&self, call_id: &str, payload: &ToolPayload) -> ResponseInputItem {
        FunctionToolOutput::from_text(self.0.to_string(), Some(true))
            .to_response_item(call_id, payload)
    }

    fn code_mode_result(&self, _payload: &ToolPayload) -> Value {
        self.0.clone()
    }
}

impl ToolExecutor<ToolInvocation> for ContextIndexHandler {
    fn tool_name(&self) -> ToolName {
        ToolName::plain(CONTEXT_INDEX_TOOL_NAME)
    }

    fn spec(&self) -> ToolSpec {
        create_context_index_tool()
    }

    fn handle<'a>(&'a self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        Box::pin(async move {
            let ToolPayload::Function { arguments } = invocation.payload else {
                return Err(FunctionCallError::RespondToModel(
                    "context_index handler received unsupported payload".to_string(),
                ));
            };
            let result = invocation
                .session
                .context_index_action(invocation.turn.as_ref(), &arguments)
                .await?;
            Ok(boxed_tool_output(ContextIndexOutput(result)))
        })
    }
}

impl CoreToolRuntime for ContextIndexHandler {
    fn is_builtin_control_tool(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::session::Session;
    use crate::session::step_context::StepContext;
    use crate::session::tests::make_session_and_context;
    use crate::session::turn_context::TurnContext;
    use crate::tools::context::ToolCallSource;
    use crate::turn_diff_tracker::TurnDiffTracker;
    use codex_protocol::ResponseItemId;
    use codex_protocol::models::ContentItem;
    use codex_protocol::models::ResponseItem;
    use serde_json::json;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    async fn invoke(payload: ToolPayload) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
        let (session, turn) = make_session_and_context().await;
        invoke_in_session(Arc::new(session), Arc::new(turn), payload).await
    }

    async fn invoke_in_session(
        session: Arc<Session>,
        turn: Arc<TurnContext>,
        payload: ToolPayload,
    ) -> Result<Box<dyn ToolOutput>, FunctionCallError> {
        ContextIndexHandler
            .handle(ToolInvocation {
                session,
                step_context: StepContext::for_test(Arc::clone(&turn)),
                turn,
                cancellation_token: tokio_util::sync::CancellationToken::new(),
                tracker: Arc::new(Mutex::new(TurnDiffTracker::default())),
                call_id: "context-index-test".to_string(),
                tool_name: ToolName::plain(CONTEXT_INDEX_TOOL_NAME),
                source: ToolCallSource::Direct,
                native_call: None,
                environments: Default::default(),
                payload,
            })
            .await
    }

    #[tokio::test]
    async fn context_index_handler_applies_and_audits_real_session_state() {
        let (session, turn) = make_session_and_context().await;
        let session = Arc::new(session);
        let turn = Arc::new(turn);
        let request = |value: Value| ToolPayload::Function {
            arguments: value.to_string(),
        };
        let listed = invoke_in_session(
            Arc::clone(&session),
            Arc::clone(&turn),
            request(json!({"action": "list"})),
        )
        .await
        .expect("list");
        let listed: Value = serde_json::from_str(&listed.log_output()).expect("list JSON");
        let revision = listed["revision"].as_u64().expect("index revision");
        let applied = invoke_in_session(
            Arc::clone(&session),
            Arc::clone(&turn),
            request(json!({
                "action": "apply", "expected_revision": revision,
                "operations": [
                    {"op": "create_task", "task_id": "native-task", "label": "Native tool test"},
                    {"op": "set_task_state", "task_id": "native-task", "state": "completed"},
                    {"op": "set_focus", "task_id": "native-task"}
                ]
            })),
        )
        .await
        .expect("apply");
        let applied: Value = serde_json::from_str(&applied.log_output()).expect("apply JSON");
        assert_eq!(applied["committed"], json!(true));
        assert_eq!(applied["revision"], json!(revision + 1));

        let listed = invoke_in_session(
            Arc::clone(&session),
            Arc::clone(&turn),
            request(json!({"action": "list"})),
        )
        .await
        .expect("updated list");
        let listed: Value = serde_json::from_str(&listed.log_output()).expect("updated list JSON");
        assert_eq!(listed["tasks"][0]["id"], json!("native-task"));
        assert_eq!(listed["tasks"][0]["state"], json!("completed"));
        assert_eq!(listed["focus_task_id"], json!("native-task"));
        let audit = invoke_in_session(
            Arc::clone(&session),
            Arc::clone(&turn),
            request(json!({"action": "audit"})),
        )
        .await
        .expect("audit");
        let audit: Value = serde_json::from_str(&audit.log_output()).expect("audit JSON");
        assert_eq!(audit["event_count"], json!(1));
        assert_eq!(audit["events"][0]["revision"], json!(revision + 1));
        let stale = invoke_in_session(
            session,
            turn,
            request(json!({
                "action": "apply", "expected_revision": revision,
                "operations": [{"op": "create_task", "task_id": "stale-task", "label": "Stale"}]
            })),
        )
        .await;
        assert!(matches!(stale, Err(FunctionCallError::RespondToModel(_))));
    }

    #[tokio::test]
    async fn context_index_handler_returns_native_json_in_both_output_paths() {
        let payload = ToolPayload::Function {
            arguments: r#"{"action":"list"}"#.to_string(),
        };
        let output = invoke(payload.clone()).await.expect("list succeeds");
        let json: Value = serde_json::from_str(&output.log_output()).expect("JSON output");
        assert!(json.is_object());
        assert_eq!(json["cache"]["scope"], json!("response"));
        assert_eq!(json["cache"]["status"], json!("unknown"));
        assert_eq!(output.code_mode_result(&payload), json);
        assert!(output.success_for_logging());
        let ResponseInputItem::FunctionCallOutput { call_id, output } =
            output.to_response_item("original-call", &payload)
        else {
            panic!("expected native function output");
        };
        assert_eq!(call_id, "original-call");
        assert_eq!(output.success, Some(true));
    }

    #[tokio::test]
    async fn context_index_handler_large_audit_is_listed_as_summary_and_read_in_bounded_chunks() {
        use codex_history::context_index::ContextIndexOperation;
        let (session, turn) = make_session_and_context().await;
        let items = (0..1100)
            .map(|id| ResponseItem::Message {
                id: Some(ResponseItemId::with_suffix(
                    "msg",
                    format!("audit-{id:04}-{}", "a".repeat(80)),
                )),
                role: "assistant".to_string(),
                content: vec![ContentItem::OutputText {
                    text: "Synthetic audit source".to_string(),
                    annotations: None,
                    logprobs: None,
                }],
                status: None,
                encrypted_content: None,
                phase: None,
                internal_chat_message_metadata_passthrough: None,
            })
            .collect::<Vec<_>>();
        let source_ids = items
            .iter()
            .map(|item| item.id().expect("stable source ID").to_string())
            .collect::<Vec<_>>();
        session
            .record_conversation_items(&turn, turn.model_info(), &items)
            .await;
        let session = Arc::new(session);
        let turn = Arc::new(turn);
        let setup = vec![ContextIndexOperation::CreateTask {
            task_id: "large-audit".to_string(),
            label: "Large audit".to_string(),
        }];
        let created = session
            .context_index_action(
                &turn,
                &json!({"action":"apply","expected_revision":0,"operations":setup}).to_string(),
            )
            .await
            .expect("real index fixture setup");
        let mut revision = created["revision"].as_u64().expect("created revision");
        for batch in source_ids.chunks(256) {
            let arguments = json!({
                "action":"apply", "expected_revision":revision,
                "operations":[ContextIndexOperation::BindSources {
                    task_id: "large-audit".to_string(),
                    source_ids: batch.to_vec(),
                }]
            }).to_string();
            assert!(arguments.len() <= 65536);
            let applied = session
                .context_index_action(&turn, &arguments)
                .await
                .expect("bounded real index fixture transaction");
            revision = applied["revision"].as_u64().expect("applied revision");
        }
        let request = |value: Value| ToolPayload::Function {
            arguments: value.to_string(),
        };
        invoke_in_session(
            Arc::clone(&session),
            Arc::clone(&turn),
            request(json!({
                "action":"apply","expected_revision":revision,
                "operations":[{"op":"set_task_state","task_id":"large-audit","state":"completed"}]
            })),
        )
        .await
        .expect("native completion transaction");
        let event_revision = revision + 1;
        let summary = invoke_in_session(
            Arc::clone(&session),
            Arc::clone(&turn),
            request(json!({"action":"audit","limit":100})),
        )
        .await
        .expect("large event does not overflow summary page");
        let summary: Value = serde_json::from_str(&summary.log_output()).expect("summary JSON");
        let event = summary["events"].as_array().expect("audit summaries")
            .iter().find(|event| event["revision"] == json!(event_revision))
            .expect("completion event summary");
        assert_eq!(event["excluded_count"], json!(1100));
        assert_eq!(event["operation_count"], json!(1));
        let mut offset = 0;
        let mut reconstructed = String::new();
        loop {
            let page = invoke_in_session(
                Arc::clone(&session),
                Arc::clone(&turn),
                request(json!({
                    "action":"audit","revision":event_revision,"offset":offset,"max_bytes":16384
                })),
            )
            .await
            .expect("bounded original audit chunk");
            let page: Value = serde_json::from_str(&page.log_output()).expect("audit chunk JSON");
            assert_eq!(page["format"], json!("context_index_audit_json"));
            assert_eq!(page["event_revision"], json!(event_revision));
            assert!(page["total_bytes"].as_u64().expect("event bytes") > 65536);
            let chunk = page["content"].as_str().expect("UTF-8 chunk");
            assert!(chunk.len() <= 16384);
            reconstructed.push_str(chunk);
            match page["next_offset"].as_u64() {
                Some(next) => {
                    assert!(next > offset);
                    offset = next;
                }
                None => break,
            }
        }
        let original: Value =
            serde_json::from_str(&reconstructed).expect("whole original audit JSON");
        assert_eq!(
            original["excluded"]
                .as_array()
                .expect("excluded sources")
                .len(),
            1100
        );
        assert_eq!(
            original["operations"]
                .as_array()
                .expect("original operations")
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn context_index_handler_reads_only_registered_current_thread_source() {
        let (session, turn) = make_session_and_context().await;
        let source_id = ResponseItemId::with_suffix("msg", "native-index-source");
        let item = ResponseItem::Message {
            id: Some(source_id.clone()),
            role: "assistant".to_string(),
            content: vec![ContentItem::OutputText {
                text: "bounded original 한글".to_string(),
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
        let session = Arc::new(session);
        let turn = Arc::new(turn);
        let output = invoke_in_session(
            Arc::clone(&session),
            Arc::clone(&turn),
            ToolPayload::Function {
                arguments: json!({"action": "read", "source_id": source_id, "max_bytes": 16384})
                    .to_string(),
            },
        )
        .await
        .expect("read registered source");
        let output: Value = serde_json::from_str(&output.log_output()).expect("read JSON");
        let original: Value = serde_json::from_str(output["content"].as_str().expect("content"))
            .expect("original ResponseItem JSON");
        assert_eq!(
            original["content"][0]["text"],
            json!("bounded original 한글")
        );
        assert_eq!(output["source_id"], json!(source_id));
        assert_eq!(output["next_offset"], Value::Null);

        let bounded = invoke_in_session(
            Arc::clone(&session),
            Arc::clone(&turn),
            ToolPayload::Function {
                arguments: json!({"action": "read", "source_id": source_id, "max_bytes": 8})
                    .to_string(),
            },
        )
        .await
        .expect("bounded source read");
        let bounded: Value = serde_json::from_str(&bounded.log_output()).expect("bounded JSON");
        assert_eq!(bounded["content"].as_str().expect("chunk").len(), 8);
        assert_eq!(bounded["next_offset"], json!(8));

        for max_bytes in [0, 16385] {
            let invalid = invoke_in_session(
                Arc::clone(&session),
                Arc::clone(&turn),
                ToolPayload::Function {
                    arguments:
                        json!({"action": "read", "source_id": source_id, "max_bytes": max_bytes})
                            .to_string(),
                },
            )
            .await;
            assert!(matches!(
                invalid,
                Err(FunctionCallError::RespondToModel(message)) if message.contains("max_bytes")
            ));
        }
    }

    #[tokio::test]
    async fn context_index_handler_preserves_request_validation_errors() {
        for arguments in [
            "{",
            r#"{"action":"unknown"}"#,
            r#"{"action":"list","limit":101}"#,
            r#"{"action":"list","limit":0}"#,
            r#"{"action":"read","source_id":"unknown-source"}"#,
            r#"{"action":"read","source_id":"unknown-source","max_bytes":16385}"#,
            r#"{"action":"apply","operations":[]}"#,
            r#"{"action":"list","thread_id":"another-thread"}"#,
            r#"{"action":"audit","max_bytes":16}"#,
            r#"{"action":"audit","offset":1}"#,
        ] {
            let result = invoke(ToolPayload::Function {
                arguments: arguments.to_string(),
            })
            .await;
            assert!(
                matches!(result, Err(FunctionCallError::RespondToModel(_))),
                "request must fail explicitly: {arguments}"
            );
        }
    }

    #[tokio::test]
    async fn context_index_handler_rejects_non_function_payload() {
        let result = invoke(ToolPayload::Custom {
            input: r#"{"action":"list"}"#.to_string(),
        })
        .await;
        assert!(matches!(
            result,
            Err(FunctionCallError::RespondToModel(message))
                if message == "context_index handler received unsupported payload"
        ));
    }
}
