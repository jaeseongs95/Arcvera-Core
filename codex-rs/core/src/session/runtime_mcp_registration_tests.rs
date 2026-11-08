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
