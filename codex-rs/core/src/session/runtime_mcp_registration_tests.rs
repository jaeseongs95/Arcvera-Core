// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
use super::tests::make_session_and_context;
use crate::mcp::RuntimeMcpServers;
use crate::state::ActiveTurn;
use codex_config::McpServerConfig;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn servers(port: u16) -> HashMap<String, McpServerConfig> {
    HashMap::from([(
        "codex_tui".into(),
        serde_json::from_value(serde_json::json!({
            "url": format!("http://127.0.0.1:{port}/mcp"),
            "default_tools_approval_mode": "approve",
            "tools": {"create_thread": {"approval_mode": "prompt"}}
        }))
        .unwrap(),
    )])
}

// D31 reconnect contract: an explicit same-bridge offer is valid only after
// the previous connection ends; new registration identity and owner stay distinct.
#[tokio::test]
async fn same_bridge_explicitly_reregisters_after_owner_disconnect() {
    let (session, _) = make_session_and_context().await;
    let mut offered = servers(12340);
    offered.insert(
        "codex_tui".into(),
        serde_json::from_value(serde_json::json!({
            "url":"http://127.0.0.1:12340/mcp",
            "http_headers":{"Authorization":"Bearer synthetic-same-bridge"}
        }))
        .unwrap(),
    );
    let a = CancellationToken::new();
    let b = CancellationToken::new();
    let (applied_a, _, _, id_a) = session
        .register_runtime_tools(1, a.clone(), None, &[], Some(offered.clone()), None)
        .await
        .unwrap();
    assert!(applied_a);
    assert!(
        session
            .register_runtime_tools(2, b.clone(), None, &[], Some(offered.clone()), None)
            .await
            .is_err(),
        "live owner cannot be replaced"
    );
    let old = session
        .services
        .thread_extension_data
        .get::<RuntimeMcpServers>()
        .unwrap();
    a.cancel();
    let (applied_b, _, _, id_b) = session
        .register_runtime_tools(2, b, None, &[], Some(offered.clone()), None)
        .await
        .unwrap();
    assert!(applied_b);
    assert_ne!(id_a, id_b);
    assert!(old.observe_disconnect());
    assert!(!session.cancel_pending_runtime_tools(1, 1).await);
    let current = session
        .services
        .thread_extension_data
        .get::<RuntimeMcpServers>()
        .unwrap();
    assert!(current.matches(2, &offered));
    assert_eq!(Some(current.registration_id()), id_b.as_deref());
    assert!(
        !session
            .get_config()
            .await
            .mcp_servers
            .get()
            .contains_key("codex_tui")
    );
}

#[tokio::test]
async fn same_bridge_active_turn_queues_and_reuses_exact_registration_generation() {
    let (session, _) = make_session_and_context().await;
    let offered = servers(12340);
    let a = CancellationToken::new();
    let b = CancellationToken::new();
    let (applied, _, _, old_id) = session
        .register_runtime_tools(1, a.clone(), None, &[], Some(offered.clone()), None)
        .await
        .unwrap();
    assert!(applied);
    let old = session
        .services
        .thread_extension_data
        .get::<RuntimeMcpServers>()
        .unwrap();
    *session.active_turn.lock().await = Some(ActiveTurn::default());
    a.cancel();
    let (applied, generation, _, queued_id) = session
        .register_runtime_tools(2, b.clone(), None, &[], Some(offered.clone()), None)
        .await
        .unwrap();
    assert!(!applied);
    let generation = generation.unwrap();
    assert_ne!(old_id, queued_id);
    let (again_applied, again_generation, _, again_id) = session
        .register_runtime_tools(
            2,
            b.clone(),
            None,
            &[],
            Some(offered.clone()),
            Some(generation),
        )
        .await
        .unwrap();
    assert!(!again_applied);
    assert_eq!(again_generation, Some(generation));
    assert_eq!(again_id, queued_id);
    assert!(old.observe_disconnect());
    assert!(!session.cancel_pending_runtime_tools(1, generation).await);
    *session.active_turn.lock().await = None;
    let (applied, applied_generation, _, applied_id) = session
        .register_runtime_tools(2, b, None, &[], Some(offered.clone()), Some(generation))
        .await
        .unwrap();
    assert!(applied);
    assert_eq!(applied_generation, Some(generation));
    assert_eq!(applied_id, queued_id);
    let current = session
        .services
        .thread_extension_data
        .get::<RuntimeMcpServers>()
        .unwrap();
    assert!(current.matches(2, &offered));
    assert_eq!(Some(current.registration_id()), applied_id.as_deref());
}

#[tokio::test]
async fn queued_registration_is_latest_bound_and_stale_cancel_cannot_remove_new_owner() {
    let (session, _) = make_session_and_context().await;
    *session.active_turn.lock().await = Some(ActiveTurn::default());
    let a = CancellationToken::new();
    let b = CancellationToken::new();
    let (applied_a, id_a, _, _) = session
        .register_runtime_tools(1, a.clone(), None, &[], Some(servers(12340)), None)
        .await
        .unwrap();
    let (applied_b, id_b, _, registration_b) = session
        .register_runtime_tools(2, b.clone(), None, &[], Some(servers(12341)), None)
        .await
        .unwrap();
    assert!(!applied_a && !applied_b);
    let (id_a, id_b) = (id_a.unwrap(), id_b.unwrap());
    assert!(id_b > id_a);
    assert!(!session.cancel_pending_runtime_tools(1, id_a).await);
    *session.active_turn.lock().await = None;
    assert!(
        session
            .register_runtime_tools(1, a, None, &[], Some(servers(12340)), Some(id_a))
            .await
            .is_err()
    );
    assert!(
        session
            .register_runtime_tools(2, b.clone(), None, &[], Some(servers(12340)), Some(id_b))
            .await
            .is_err(),
        "generation cannot authorize changed endpoint or headers"
    );
    let (applied, id, lifetime, applied_registration) = session
        .register_runtime_tools(2, b, None, &[], Some(servers(12341)), Some(id_b))
        .await
        .unwrap();
    assert!(applied && id == Some(id_b) && lifetime.is_none());
    assert_eq!(registration_b, applied_registration);
    assert!(!session.cancel_pending_runtime_tools(1, id_a).await);
    assert!(
        session
            .services
            .thread_extension_data
            .get::<RuntimeMcpServers>()
            .unwrap()
            .matches(2, &servers(12341))
    );
}

#[tokio::test]
async fn disconnected_queued_owner_cannot_apply_or_replace_current_registration() {
    let (session, _) = make_session_and_context().await;
    *session.active_turn.lock().await = Some(ActiveTurn::default());
    let closed = CancellationToken::new();
    let (_, generation, _, _) = session
        .register_runtime_tools(1, closed.clone(), None, &[], Some(servers(12340)), None)
        .await
        .unwrap();
    closed.cancel();
    *session.active_turn.lock().await = None;
    assert!(
        session
            .register_runtime_tools(1, closed, None, &[], Some(servers(12340)), generation)
            .await
            .is_err()
    );
    assert!(
        session
            .services
            .thread_extension_data
            .get::<RuntimeMcpServers>()
            .is_none()
    );
}

#[tokio::test]
async fn queued_payload_is_released_on_disconnect_and_old_watcher_cannot_clear_newer() {
    let (session, _) = make_session_and_context().await;
    let session = Arc::new(session);
    *session.active_turn.lock().await = Some(ActiveTurn::default());
    let first = CancellationToken::new();
    let second = CancellationToken::new();
    let (_, a, lifetime_a, _) = session
        .register_runtime_tools(1, first, None, &[], Some(servers(12340)), None)
        .await
        .unwrap();
    let a = a.unwrap();
    session.watch_pending_runtime_tools(1, a, lifetime_a.unwrap());
    let (_, b, lifetime_b, _) = session
        .register_runtime_tools(2, second.clone(), None, &[], Some(servers(12341)), None)
        .await
        .unwrap();
    let b = b.unwrap();
    session.watch_pending_runtime_tools(2, b, lifetime_b.unwrap());
    tokio::task::yield_now().await;
    assert_eq!(
        session
            .state
            .lock()
            .await
            .pending_runtime_tools
            .as_ref()
            .unwrap()
            .generation,
        b
    );
    second.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            if session.state.lock().await.pending_runtime_tools.is_none() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("disconnected queued payload must be released");
}

#[tokio::test]
async fn replacement_uses_complete_new_definition_without_old_headers() {
    let (session, _) = make_session_and_context().await;
    let old = CancellationToken::new();
    let make = |port: u16, secret: &str| {
        HashMap::from([(
            "codex_tui".to_owned(),
            serde_json::from_value::<McpServerConfig>(serde_json::json!({
                "url": format!("http://127.0.0.1:{port}/mcp"),
                "http_headers": {"Authorization": secret}
            }))
            .unwrap(),
        )])
    };
    let a = make(12340, "Bearer synthetic-A");
    let b = make(12341, "Bearer synthetic-B");
    assert!(
        session
            .supplement_runtime_tools(1, old.clone(), None, &[], Some(a))
            .await
            .unwrap()
    );
    old.cancel();
    assert!(
        session
            .supplement_runtime_tools(2, CancellationToken::new(), None, &[], Some(b.clone()))
            .await
            .unwrap()
    );
    let overlay = session
        .services
        .thread_extension_data
        .get::<RuntimeMcpServers>()
        .unwrap();
    assert!(overlay.matches(2, &b));
    let base = session.get_config().await;
    assert!(!base.mcp_servers.get().contains_key("codex_tui"));
    let projected = overlay.project(&base).unwrap();
    assert_eq!(
        projected.mcp_servers.get().get("codex_tui"),
        b.get("codex_tui")
    );
}

#[tokio::test]
async fn reconnect_never_inherits_an_omitted_authorization_header() {
    let (session, _) = make_session_and_context().await;
    let old = CancellationToken::new();
    let a: McpServerConfig = serde_json::from_value(serde_json::json!({
        "url":"http://127.0.0.1:12340/mcp", "http_headers":{"Authorization":"Bearer synthetic-A"}
    }))
    .unwrap();
    let b: McpServerConfig =
        serde_json::from_value(serde_json::json!({"url":"http://127.0.0.1:12341/mcp"})).unwrap();
    session
        .supplement_runtime_tools(
            1,
            old.clone(),
            None,
            &[],
            Some(HashMap::from([("codex_tui".into(), a)])),
        )
        .await
        .unwrap();
    old.cancel();
    session
        .supplement_runtime_tools(
            2,
            CancellationToken::new(),
            None,
            &[],
            Some(HashMap::from([("codex_tui".into(), b.clone())])),
        )
        .await
        .unwrap();
    let current = session
        .services
        .thread_extension_data
        .get::<RuntimeMcpServers>()
        .unwrap();
    let config = session.get_config().await;
    let projected = current.project(config.as_ref()).unwrap();
    assert_eq!(projected.mcp_servers.get().get("codex_tui"), Some(&b));
}

#[tokio::test]
async fn configured_same_origin_with_different_headers_cannot_be_claimed_by_overlay() {
    let (session, _) = make_session_and_context().await;
    let make = |header: &str| {
        serde_json::from_value::<McpServerConfig>(serde_json::json!({
            "url":"http://127.0.0.1:12345/mcp", "http_headers":{"Authorization":header}
        }))
        .unwrap()
    };
    let configured = make("Bearer configured");
    let offered = make("Bearer synthetic-runtime");
    let mut base = session.get_config().await.as_ref().clone();
    base.mcp_servers
        .set(HashMap::from([("codex_tui".into(), configured.clone())]))
        .unwrap();
    let candidate = RuntimeMcpServers::new(
        7,
        CancellationToken::new(),
        HashMap::from([("codex_tui".into(), offered)]),
    );
    assert!(candidate.project(&base).is_err());
    assert_eq!(base.mcp_servers.get().get("codex_tui"), Some(&configured));
}

#[tokio::test]
async fn runtime_registration_preserves_base_config_and_rejects_live_takeover() {
    let (session, _) = make_session_and_context().await;
    let base = session.get_config().await;
    let a = CancellationToken::new();
    assert!(
        session
            .supplement_runtime_tools(1, a.clone(), None, &[], Some(servers(12340)))
            .await
            .unwrap()
    );
    assert!(Arc::ptr_eq(&base, &session.get_config().await));
    assert!(!base.mcp_servers.get().contains_key("codex_tui"));
    assert!(
        session
            .supplement_runtime_tools(2, CancellationToken::new(), None, &[], Some(servers(12341)))
            .await
            .is_err()
    );
    let old = session
        .services
        .thread_extension_data
        .get::<RuntimeMcpServers>()
        .unwrap();
    assert!(old.matches(1, &servers(12340)));
    a.cancel();
    assert!(
        session
            .supplement_runtime_tools(2, CancellationToken::new(), None, &[], Some(servers(12341)))
            .await
            .unwrap()
    );
    assert!(old.observe_disconnect());
    assert!(!old.observe_disconnect());
    let new = session
        .services
        .thread_extension_data
        .get::<RuntimeMcpServers>()
        .unwrap();
    assert!(new.matches(2, &servers(12341)));
    assert_ne!(old.owner, new.owner);
    assert!(!new.observe_disconnect());
}

#[tokio::test]
async fn busy_registration_does_not_mutate_either_catalog() {
    let (session, _) = make_session_and_context().await;
    let a = CancellationToken::new();
    session
        .supplement_runtime_tools(1, a.clone(), None, &[], Some(servers(12340)))
        .await
        .unwrap();
    *session.active_turn.lock().await = Some(ActiveTurn::default());
    assert!(
        session
            .supplement_runtime_tools(1, a.clone(), None, &[], Some(servers(12340)))
            .await
            .unwrap()
    );
    assert!(
        !session
            .supplement_runtime_tools(1, a.clone(), Some(Vec::new()), &[], Some(servers(12341)))
            .await
            .unwrap()
    );
    assert!(
        session
            .supplement_runtime_tools(2, CancellationToken::new(), None, &[], Some(servers(12341)))
            .await
            .is_err(),
        "a live owner must not be reported as merely queued"
    );
    a.cancel();
    assert!(
        !session
            .supplement_runtime_tools(
                2,
                CancellationToken::new(),
                Some(Vec::new()),
                &[],
                Some(servers(12341))
            )
            .await
            .unwrap()
    );
    assert!(session.state.lock().await.runtime_dynamic_tools.is_none());
    assert_eq!(
        session
            .services
            .thread_extension_data
            .get::<RuntimeMcpServers>()
            .unwrap()
            .owner,
        1
    );
    *session.active_turn.lock().await = None;
    assert!(
        session
            .supplement_runtime_tools(
                2,
                CancellationToken::new(),
                Some(Vec::new()),
                &[],
                Some(servers(12341))
            )
            .await
            .unwrap()
    );
    assert_eq!(
        session
            .services
            .thread_extension_data
            .get::<RuntimeMcpServers>()
            .unwrap()
            .owner,
        2
    );
}

#[tokio::test]
async fn policy_failure_cannot_partially_install_native_tools() {
    let (session, _) = make_session_and_context().await;
    let mut config = session.get_config().await.as_ref().clone();
    config.mcp_servers.set(servers(12340)).unwrap();
    session
        .state
        .lock()
        .await
        .session_configuration
        .original_config_do_not_use = Arc::new(config);
    assert!(
        session
            .supplement_runtime_tools(
                1,
                CancellationToken::new(),
                Some(Vec::new()),
                &[],
                Some(servers(12341))
            )
            .await
            .is_err()
    );
    assert!(session.state.lock().await.runtime_dynamic_tools.is_none());
    assert!(
        session
            .services
            .thread_extension_data
            .get::<RuntimeMcpServers>()
            .is_none()
    );
    let config = session.get_config().await;
    let mut constrained = config.as_ref().clone();
    constrained.mcp_servers = codex_config::Constrained::normalized(
        HashMap::<String, McpServerConfig>::new(),
        |mut servers| {
            for server in servers.values_mut() {
                server.enabled = false;
            }
            servers
        },
    )
    .unwrap();
    let registration = RuntimeMcpServers::new(1, CancellationToken::new(), servers(12341));
    assert!(registration.validate(&constrained).is_err());
}

#[tokio::test]
async fn cancelled_and_closing_connections_cannot_ack_registration() {
    let (session, _) = make_session_and_context().await;
    let token = CancellationToken::new();
    token.cancel();
    assert!(
        session
            .supplement_runtime_tools(1, token, None, &[], Some(servers(12340)))
            .await
            .is_err()
    );
    session.state.lock().await.shutting_down = true;
    assert!(
        session
            .supplement_runtime_tools(1, CancellationToken::new(), None, &[], Some(servers(12340)))
            .await
            .is_err()
    );
    assert!(
        session
            .services
            .thread_extension_data
            .get::<RuntimeMcpServers>()
            .is_none()
    );
}
