// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
use super::*;

#[test]
fn core_version_declaration_is_explicit_and_exact() {
    let expected = "1.0.0-rc.1+build.7";
    let missing = InitializeParams::default();
    assert!(!missing.supports_arcvera_core_version(expected));
    for declaration in [
        serde_json::Value::Null,
        serde_json::json!(expected),
        serde_json::json!({}),
        serde_json::json!({"version": 1}),
        serde_json::json!({"version": "0.168.0"}),
        serde_json::json!({"version": "1.0.0"}),
        serde_json::json!({"version": "1.0.0-rc.1+build.8"}),
    ] {
        let mut params = missing.clone().with_arcvera_core_version(expected);
        params
            .capabilities
            .as_mut()
            .unwrap()
            .extensions
            .as_mut()
            .unwrap()
            .insert("arcvera/core".to_string(), declaration);
        assert!(!params.supports_arcvera_core_version(expected));
    }
    let mut params = InitializeParams {
        client_info: ClientInfo {
            version: "2.0.0".to_string(),
            ..Default::default()
        },
        capabilities: Some(InitializeCapabilities {
            extensions: Some(HashMap::from([(
                "openai/form".to_string(),
                serde_json::json!({"userVerification": {}}),
            )])),
            ..Default::default()
        }),
    }
    .with_arcvera_core_version(expected);
    assert!(params.supports_arcvera_core_version(expected));
    assert_eq!(params.client_info.version, "2.0.0");
    assert!(
        params
            .capabilities
            .as_ref()
            .unwrap()
            .extensions
            .as_ref()
            .unwrap()
            .contains_key("openai/form")
    );
    params = params.with_arcvera_core_version("1.0.0");
    assert!(!params.supports_arcvera_core_version(expected));
}
