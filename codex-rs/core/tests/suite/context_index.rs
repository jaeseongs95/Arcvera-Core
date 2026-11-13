// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R41 (2026-11-10); see MODIFICATIONS.md.
// The provider is synthetic; native tools, Session, projection and request serialization are real.
use anyhow::Context;
use anyhow::Result;
use codex_core::StartIfIdleSubmission;
use codex_core::TurnInputRequest;
use codex_extension_api::ExtensionRegistryBuilder;
use codex_protocol::protocol::EventMsg;
use codex_protocol::user_input::UserInput;
use core_test_support::ThreadIdle;
use core_test_support::responses::WebSocketConnectionConfig;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::responses::start_websocket_server_with_headers;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::TestCodex;
use core_test_support::test_codex::TestCodexBuilder;
use core_test_support::test_codex::test_codex;
use futures::FutureExt;
use pretty_assertions::assert_eq;
use serde_json::Value;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

const DETAIL_A: &str = "R41_FOCUSED_A_SOURCE_BODY_SENTINEL";
const DETAIL_B: &str = "R41_COMPLETED_REFERENCED_B_SOURCE_BODY_SENTINEL";
const DETAIL_C: &str = "R41_UNRELATED_ACTIVE_C_SOURCE_BODY_SENTINEL";
const LABEL_C: &str = "R41_UNRELATED_C_LABEL_SENTINEL";
const CURRENT_USER: &str = "R41_CURRENT_USER_CORRECTION_SENTINEL";
const CONTROL: &str =
    "R41_REQUIRED_CONTROL_SENTINEL: preserve permission checks and user corrections.";
const DISCOVER_CALL: &str = "r41-discover-sources";
const APPLY_CALL: &str = "r41-apply-focus";
const FOCUSED_LIST_CALL: &str = "r41-list-focused";
const REQUEST_COUNT: usize = 8;

fn builder() -> TestCodexBuilder {
    let mut extensions = ExtensionRegistryBuilder::new();
    extensions.thread_lifecycle_contributor(Arc::new(ThreadIdle));
    test_codex()
        .with_extensions(Arc::new(extensions.build()))
        .with_config(|config| {
            config.base_instructions = Some(CONTROL.to_string());
        })
}

fn seed_messages() -> Vec<Value> {
    vec![
        ev_assistant_message("msg-r41-focus-a", DETAIL_A),
        ev_assistant_message("msg-r41-dependency-b", DETAIL_B),
        ev_assistant_message("msg-r41-unrelated-c", DETAIL_C),
    ]
}

fn policy_arguments() -> Value {
    // These are IDs actually emitted by this provider, not guessed index identities. The
    // discovery response below must prove that the native index registered every exact ID.
    let source_ids = seed_messages()
        .into_iter()
        .map(|event| event["item"]["id"].clone())
        .collect::<Vec<_>>();
    json!({
        "action": "apply",
        "expected_revision": 0,
        "operations": [
            {"op": "create_task", "task_id": "task-a", "label": "Focused A"},
            {"op": "create_task", "task_id": "task-b", "label": "Referenced B"},
            {"op": "create_task", "task_id": "task-c", "label": LABEL_C},
            {"op": "bind_sources", "task_id": "task-a", "source_ids": [source_ids[0]]},
            {"op": "bind_sources", "task_id": "task-b", "source_ids": [source_ids[1]]},
            {"op": "bind_sources", "task_id": "task-c", "source_ids": [source_ids[2]]},
            {"op": "set_task_state", "task_id": "task-b", "state": "completed"},
            {"op": "add_dependency", "task_id": "task-a", "dependency_id": "task-b"},
            {"op": "set_focus", "task_id": "task-a"}
        ]
    })
}

fn response(id: &str, mut output: Vec<Value>) -> Vec<Value> {
    let mut events = vec![ev_response_created(id)];
    events.append(&mut output);
    events.push(ev_completed(id));
    events
}

fn scripted_responses() -> Vec<Vec<Value>> {
    vec![
        response("resp-r41-seed", seed_messages()),
        response(
            "resp-r41-discover",
            vec![ev_function_call(
                DISCOVER_CALL,
                "context_index",
                &json!({"action": "list", "include_unrelated": true, "limit": 100}).to_string(),
            )],
        ),
        response(
            "resp-r41-discovered",
            vec![ev_assistant_message(
                "msg-r41-discovered",
                "Sources inspected.",
            )],
        ),
        response(
            "resp-r41-apply",
            vec![ev_function_call(
                APPLY_CALL,
                "context_index",
                &policy_arguments().to_string(),
            )],
        ),
        response(
            "resp-r41-focused",
            vec![ev_assistant_message(
                "msg-r41-focused",
                "Continuing focused work.",
            )],
        ),
        response(
            "resp-r41-focused-list",
            vec![ev_function_call(
                FOCUSED_LIST_CALL,
                "context_index",
                &json!({"action": "list", "limit": 100}).to_string(),
            )],
        ),
        response(
            "resp-r41-listed",
            vec![ev_assistant_message(
                "msg-r41-listed",
                "Focused metadata inspected.",
            )],
        ),
        response(
            "resp-r41-stable",
            vec![ev_assistant_message(
                "msg-r41-stable",
                "Finished the focused check.",
            )],
        ),
    ]
}

async fn submit_scenario(test: &TestCodex) -> Result<()> {
    // No shell tool or Unix-specific fixture is invoked: this module also runs on Windows.
    let correction = format!("{CURRENT_USER}: preserve this correction and apply the task focus.");
    for (step, prompt) in [
        "Produce the three fixture details.",
        "Inspect source identities with context_index.",
        correction.as_str(),
        "Inspect the default focused context index.",
        "Continue the same focused task.",
    ]
    .into_iter()
    .enumerate()
    {
        eprintln!("R41_SCENARIO step={step} phase=submit");
        let submission = test
            .codex
            .start_turn_if_idle(TurnInputRequest::user_input(vec![UserInput::Text {
                text: prompt.into(),
                text_elements: Vec::new(),
            }]))
            .await?;
        assert!(
            matches!(submission, StartIfIdleSubmission::Started { .. }),
            "sequential fixture turn must start: {submission:?}"
        );
        loop {
            let event = tokio::time::timeout(Duration::from_secs(10), test.codex.next_event())
                .await
                .expect("scenario event deadline")?;
            eprintln!(
                "R41_SCENARIO step={step} event={:?}",
                std::mem::discriminant(&event.msg)
            );
            if let EventMsg::Error(error) = &event.msg {
                panic!(
                    "owned fixture native error at step {step}: {}",
                    error.message
                );
            }
            if matches!(event.msg, EventMsg::TurnComplete(_)) {
                break;
            }
        }
        ThreadIdle::wait(&test.codex).await;
        eprintln!("R41_SCENARIO step={step} phase=idle");
    }
    Ok(())
}

fn tool_json(request: &Value, call_id: &str) -> Result<Value> {
    let output = request["input"]
        .as_array()
        .context("serialized Responses input array")?
        .iter()
        .find(|item| item["type"] == "function_call_output" && item["call_id"] == call_id)
        .with_context(|| format!("native function output for {call_id}"))?;
    serde_json::from_str(
        output["output"]
            .as_str()
            .context("native JSON output text")?,
    )
    .context("parse actual context_index tool result")
}

fn assert_registered_sources(request: &Value) -> Result<()> {
    let discovered = tool_json(request, DISCOVER_CALL)?;
    assert_eq!(discovered["revision"], 0);
    let sources = discovered["sources"]
        .as_array()
        .context("registered source records")?;
    for event in seed_messages() {
        let emitted_id = &event["item"]["id"];
        let registered = sources
            .iter()
            .find(|entry| entry["source"]["response_item_id"] == *emitted_id)
            .with_context(|| format!("native list must contain emitted source {emitted_id}"))?;
        assert_eq!(registered["source"]["id"], *emitted_id);
        assert_eq!(registered["active"], true);
    }
    Ok(())
}

fn assert_focused_request(request: &Value) {
    let serialized = request.to_string();
    for required in [DETAIL_A, DETAIL_B, CURRENT_USER, CONTROL] {
        assert!(
            serialized.contains(required),
            "focused request lost required context: {required}"
        );
    }
    for unrelated in [DETAIL_C, LABEL_C] {
        assert!(
            !serialized.contains(unrelated),
            "unrelated task data reached the model API: {unrelated}"
        );
    }
    // A successful apply is an index control transaction. Its historical arguments include
    // unrelated metadata, so its completed group must not reintroduce that metadata on replay.
    assert!(
        !serialized.contains(APPLY_CALL),
        "successful apply group leaked back into model input"
    );
}

fn assert_focused_metadata(request: &Value) -> Result<()> {
    let listed = tool_json(request, FOCUSED_LIST_CALL)?;
    assert_eq!(listed["revision"], 1, "native apply must actually commit");
    assert_eq!(listed["focus_task_id"], "task-a");
    assert_eq!(listed["include_unrelated"], false);
    let tasks = listed["tasks"].as_array().context("focused task records")?;
    assert!(
        tasks
            .iter()
            .any(|task| task["id"] == "task-a" && task["state"] == "active")
    );
    assert!(
        tasks
            .iter()
            .any(|task| task["id"] == "task-b" && task["state"] == "completed")
    );
    assert!(!tasks.iter().any(|task| task["id"] == "task-c"));
    let sources = listed["sources"]
        .as_array()
        .context("focused source records")?;
    for expected in ["msg-r41-focus-a", "msg-r41-dependency-b"] {
        assert!(
            sources
                .iter()
                .any(|entry| entry["source"]["id"] == expected && entry["active"] == true)
        );
    }
    assert!(
        !sources
            .iter()
            .any(|entry| entry["source"]["id"] == "msg-r41-unrelated-c")
    );
    assert!(!listed.to_string().contains(LABEL_C));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_context_index_filters_serialized_http_request() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let server = start_mock_server().await;
    let mock =
        mount_sse_sequence(&server, scripted_responses().into_iter().map(sse).collect()).await;
    let test = builder().build(&server).await?;
    submit_scenario(&test).await?;

    let requests = mock
        .requests()
        .iter()
        .map(|request| request.body_json())
        .collect::<Vec<_>>();
    assert_eq!(
        requests.len(),
        REQUEST_COUNT,
        "no whole-history summary request is needed"
    );
    let before_focus = requests[1].to_string();
    for detail in [DETAIL_A, DETAIL_B, DETAIL_C] {
        assert!(
            before_focus.contains(detail),
            "seed detail must exist before filtering: {detail}"
        );
    }
    assert_registered_sources(&requests[2])?;
    for index in [4, 6, 7] {
        assert_focused_request(&requests[index]);
    }
    assert_focused_metadata(&requests[6])?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_context_index_policy_change_resets_websocket_continuation() -> Result<()> {
    skip_if_no_network!(Ok(()));
    let script = scripted_responses();
    // Startup prewarm is a non-generating request, not the first scripted user turn.
    let mut first_connection = vec![response("resp-r41-prewarm", Vec::new())];
    first_connection.extend(script.clone());
    let server = start_websocket_server_with_headers(vec![
        WebSocketConnectionConfig {
            // Keep all remaining responses available if the client incorrectly retains its
            // old connection. No server close is allowed to manufacture a successful reset.
            requests: first_connection,
            response_headers: Vec::new(),
            accept_delay: None,
            close_after_requests: false,
        },
        WebSocketConnectionConfig {
            requests: script[4..].to_vec(),
            response_headers: Vec::new(),
            accept_delay: None,
            close_after_requests: false,
        },
    ])
    .await;
    let test = builder().build_with_websocket_server(&server).await?;
    let scenario = std::panic::AssertUnwindSafe(submit_scenario(&test))
        .catch_unwind()
        .await;
    let connections = server.connections();
    let connection_count = connections.len();
    let captured = connections
        .into_iter()
        .flatten()
        .map(|request| request.body_json())
        .collect::<Vec<_>>();
    eprintln!(
        "R41_WS_CAPTURE {}",
        json!({
            "connections": connection_count,
            "requests": captured.iter().map(|request| json!({
                "generate": request.get("generate"),
                "previous_response_id": request.get("previous_response_id"),
            })).collect::<Vec<_>>()
        })
    );
    // Preserve failure while also shutting down the explicitly owned provider on panic.
    server.shutdown().await;
    match scenario {
        Ok(result) => result?,
        Err(panic) => std::panic::resume_unwind(panic),
    }
    assert_eq!(captured.len(), REQUEST_COUNT + 1);
    assert_eq!(captured[0]["generate"], false);
    assert_eq!(
        captured
            .iter()
            .filter(|request| request["generate"] == false)
            .count(),
        1
    );
    assert_eq!(
        connection_count, 2,
        "policy mutation must invalidate the live socket"
    );
    let requests = captured
        .into_iter()
        .filter(|request| request["generate"] != false)
        .collect::<Vec<_>>();

    assert_eq!(requests.len(), REQUEST_COUNT);
    assert_registered_sources(&requests[2])?;
    assert_eq!(
        requests[2]["previous_response_id"], "resp-r41-discover",
        "unchanged policy must preserve ordinary native-tool continuation"
    );
    assert!(
        requests[4].get("previous_response_id").is_none(),
        "changed policy must not retain server-side context from the old response"
    );
    assert_focused_request(&requests[4]);
    assert_focused_metadata(&requests[6])?;
    for request in &requests[4..] {
        let serialized = request.to_string();
        assert!(!serialized.contains(DETAIL_C));
        assert!(!serialized.contains(LABEL_C));
    }
    Ok(())
}
