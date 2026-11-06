// Modified for OpenDIY Codex R38 (2026-11-06); see MODIFICATIONS.md.
use super::*;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

#[test]
fn gov_cloud_guidance_requires_acknowledgement() {
    let mut state = BedrockState::guidance();
    insta::assert_snapshot!("gov_cloud_warning", render_visible(&state));
    assert!(!state.handle_guidance_key_event(&KeyEvent::new_with_kind(
        KeyCode::Enter,
        KeyModifiers::NONE,
        KeyEventKind::Repeat,
    )));
    assert!(state.handle_guidance_key_event(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)));

    let area = Rect::new(0, 0, 72, 24);
    let mut buffer = Buffer::empty(area);
    state.render(area, &mut buffer, /*error*/ None);
    assert!(
        buffer
            .content
            .iter()
            .any(|cell| cell.symbol().contains(GOV_CLOUD_GUIDANCE_URL))
    );
}

#[test]
fn gov_cloud_guidance_scrolls_in_short_panes() {
    let mut state = BedrockState::guidance();
    let area = Rect::new(0, 0, 24, 6);
    let initial = render_visible_at(&state, area);
    insta::assert_snapshot!("gov_cloud_warning_short", initial);

    // Even a one-row disclosure viewport must expose every line and the link.
    for height in [6, 2] {
        let area = Rect::new(0, 0, 24, height);
        let mut seen = String::new();
        for _ in 0..20 {
            let mut buffer = Buffer::empty(area);
            state.render(area, &mut buffer, /*error*/ None);
            seen.extend(buffer.content.iter().map(ratatui::buffer::Cell::symbol));
            assert!(render_visible_at(&state, area).contains("Acknowledge"));
            assert!(
                !state.handle_guidance_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))
            );
        }
        assert!(seen.contains("Using Codex with AWS"));
        assert!(seen.contains("proceeding."));
        assert!(seen.contains(GOV_CLOUD_GUIDANCE_URL));
        for _ in 0..20 {
            state.handle_guidance_key_event(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
            render_visible_at(&state, area);
        }
        assert!(render_visible_at(&state, area).starts_with("Using Codex with AWS"));
    }
    assert_eq!(render_visible_at(&state, area), initial);
    state.handle_guidance_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    render_visible(&state);
    assert_eq!(render_visible_at(&state, area), initial);
}

fn render_visible(state: &BedrockState) -> String {
    render_visible_at(state, Rect::new(0, 0, 72, 24))
}

fn render_visible_at(state: &BedrockState, area: Rect) -> String {
    let mut buffer = Buffer::empty(area);
    state.render(area, &mut buffer, /*error*/ None);
    let mut rows = (area.top()..area.bottom())
        .map(|row| {
            let mut visible = String::new();
            let mut column = area.left();
            while column < area.right() {
                let symbol = crate::terminal_hyperlinks::strip_osc8(buffer[(column, row)].symbol());
                // Wide glyphs own the following terminal cell; do not append that cell's filler.
                column +=
                    u16::try_from(unicode_width::UnicodeWidthStr::width(symbol.as_str()).max(1))
                        .unwrap_or(1);
                visible.push_str(&symbol);
            }
            visible.trim_end().to_string()
        })
        .collect::<Vec<_>>();
    while rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows.join("\n")
}

#[test]
fn discovery_prioritizes_profiles_and_groups_other_sign_in_methods() {
    let discovering = BedrockState::discovering(RequestId::Integer(1));
    insta::assert_snapshot!(render_visible(&discovering), @r###"
    > Set up Amazon Bedrock

      Checking for existing AWS credentials...
    "###);

    let mut configuring = discovering;
    configuring.view = BedrockView::Configuring(RequestId::Integer(2));
    insta::assert_snapshot!(render_visible(&configuring), @r###"
    > Set up Amazon Bedrock

      Setting up Amazon Bedrock...

      Press esc to go back
    "###);
    assert!(
        configuring
            .handle_key_event(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
            .is_none()
    );
    assert!(matches!(
        configuring.view,
        BedrockView::Methods(BedrockMethodList::Detected)
    ));

    let profile = BedrockAwsProfile {
        name: "engineering".to_string(),
        region: Some("us-east-2".to_string()),
    };
    let environment = BedrockEnvironmentCredential {
        credential_type: AwsCredentialType::AccessKeys,
        region: Some("us-west-2".to_string()),
    };
    for (profiles, environment_credentials, expected) in [
        (
            vec![profile.clone()],
            vec![environment.clone()],
            vec![
                BedrockMethod::Profile(0),
                BedrockMethod::Environment,
                BedrockMethod::OtherMethods,
            ],
        ),
        (
            vec![
                profile.clone(),
                BedrockAwsProfile {
                    name: "production".to_string(),
                    region: Some("us-west-2".to_string()),
                },
            ],
            vec![environment.clone()],
            vec![
                BedrockMethod::Profile(0),
                BedrockMethod::Profile(1),
                BedrockMethod::Environment,
                BedrockMethod::OtherMethods,
            ],
        ),
        (
            Vec::new(),
            vec![environment],
            vec![BedrockMethod::Environment, BedrockMethod::OtherMethods],
        ),
        (
            Vec::new(),
            Vec::new(),
            vec![
                BedrockMethod::ManualProfile,
                BedrockMethod::AccessKeys,
                BedrockMethod::EnvironmentInstructions,
                BedrockMethod::ApiKey,
            ],
        ),
    ] {
        let state = BedrockState::discovered(BedrockDiscoverResponse {
            profiles,
            environment_credentials,
        });
        assert_eq!(state.methods(), expected);
    }

    let mut profile_and_bearer = BedrockState::discovered(BedrockDiscoverResponse {
        profiles: vec![profile.clone()],
        environment_credentials: vec![BedrockEnvironmentCredential {
            credential_type: AwsCredentialType::BedrockApiKey,
            region: Some("us-east-2".to_string()),
        }],
    });
    assert_eq!(
        profile_and_bearer.methods(),
        vec![
            BedrockMethod::Profile(0),
            BedrockMethod::Environment,
            BedrockMethod::OtherMethods,
        ]
    );
    insta::assert_snapshot!(render_visible(&profile_and_bearer), @r###"
    > Set up Amazon Bedrock

      AWS profile detected: engineering
      Region: us-east-2

    > 1. Continue with engineering
         Use your existing AWS credentials

      2. Continue with detected credentials
         Use your existing Amazon Bedrock API key

      3. Other AWS sign-in methods
         Use another profile, access keys, environment variables, or a
    Bedrock API key


      Press enter to continue
      Press esc to go back
    "###);
    profile_and_bearer.select_method(BedrockMethod::OtherMethods);
    assert_eq!(
        profile_and_bearer.methods(),
        vec![
            BedrockMethod::ManualProfile,
            BedrockMethod::AccessKeys,
            BedrockMethod::EnvironmentInstructions,
            BedrockMethod::ApiKey,
        ]
    );
    insta::assert_snapshot!(render_visible(&profile_and_bearer), @r"
    > Set up Amazon Bedrock

      Choose how you authenticate with AWS.

    > 1. AWS profile
         Use AWS SSO or a named profile

      2. AWS access keys
         Enter an access key ID and secret access key

      3. Environment variables
         Configure AWS credentials in your environment, then return here

      4. Bedrock API key
         Enter a Bedrock API key


      Press enter to continue
      Press esc to go back
    ");

    let profile_state = BedrockState::discovered(BedrockDiscoverResponse {
        profiles: vec![profile],
        environment_credentials: Vec::new(),
    });
    insta::assert_snapshot!(render_visible(&profile_state), @r###"
    > Set up Amazon Bedrock

      AWS profile detected: engineering
      Region: us-east-2

    > 1. Continue with engineering
         Use your existing AWS credentials

      2. Other AWS sign-in methods
         Use another profile, access keys, environment variables, or a
    Bedrock API key


      Press enter to continue
      Press esc to go back
    "###);

    let multiple_profiles = BedrockState::discovered(BedrockDiscoverResponse {
        profiles: vec![
            BedrockAwsProfile {
                name: "engineering".to_string(),
                region: Some("us-east-2".to_string()),
            },
            BedrockAwsProfile {
                name: "production".to_string(),
                region: Some("us-west-2".to_string()),
            },
        ],
        environment_credentials: Vec::new(),
    });
    insta::assert_snapshot!(render_visible(&multiple_profiles), @r###"
    > Set up Amazon Bedrock

      Choose an AWS profile.

    > 1. engineering
         us-east-2

      2. production
         us-west-2

      3. Other AWS sign-in methods
         Use another profile, access keys, environment variables, or a
    Bedrock API key


      Press enter to continue
      Press esc to go back
    "###);

    let environment_state = BedrockState::discovered(BedrockDiscoverResponse {
        profiles: Vec::new(),
        environment_credentials: vec![BedrockEnvironmentCredential {
            credential_type: AwsCredentialType::AccessKeys,
            region: Some("us-west-2".to_string()),
        }],
    });
    insta::assert_snapshot!(render_visible(&environment_state), @r###"
    > Set up Amazon Bedrock

      AWS credentials detected in your environment.

    > 1. Continue with detected credentials

      2. Other AWS sign-in methods


      Press enter to continue
      Press esc to go back
    "###);

    let empty_state = BedrockState::discovered(BedrockDiscoverResponse {
        profiles: Vec::new(),
        environment_credentials: Vec::new(),
    });
    insta::assert_snapshot!(render_visible(&empty_state), @r"
    > Set up Amazon Bedrock

      No AWS credentials found.
      Choose how you authenticate with AWS.

    > 1. AWS profile
         Use AWS SSO or a named profile

      2. AWS access keys
         Enter an access key ID and secret access key

      3. Environment variables
         Configure AWS credentials in your environment, then return here

      4. Bedrock API key
         Enter a Bedrock API key


      Press enter to continue
      Press esc to go back
    ");

    let mut many_profiles = BedrockState::discovered(BedrockDiscoverResponse {
        profiles: (0..12)
            .map(|index| BedrockAwsProfile {
                name: format!("engineering-{index}"),
                region: Some("us-east-2".to_string()),
            })
            .collect(),
        environment_credentials: Vec::new(),
    });
    many_profiles.highlighted = many_profiles.methods().len() - 1;
    let area = Rect::new(0, 0, 72, 10);
    let mut buffer = Buffer::empty(area);
    many_profiles.render(area, &mut buffer, /*error*/ None);
    let visible = (area.top()..area.bottom())
        .flat_map(|row| (area.left()..area.right()).map(move |column| (column, row)))
        .map(|position| buffer[position].symbol())
        .collect::<String>();
    assert!(visible.contains("Other AWS sign-in methods"));
    assert!(visible.contains("Press enter to continue"));
    assert!(visible.contains("Press esc to go back"));

    let narrow_area = Rect::new(0, 0, 24, 12);
    let mut narrow_buffer = Buffer::empty(narrow_area);
    empty_state.render(
        narrow_area,
        &mut narrow_buffer,
        Some("AWS credentials unavailable in this terminal".to_string()),
    );
    let narrow_visible = (narrow_area.top()..narrow_area.bottom())
        .map(|row| {
            (narrow_area.left()..narrow_area.right())
                .map(|column| narrow_buffer[(column, row)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(narrow_visible.contains("Press enter to"));
    assert!(narrow_visible.contains("continue"));
    assert!(narrow_visible.contains("Press esc to go back"));
    assert!(narrow_visible.contains("AWS credentials"));
    assert!(narrow_visible.contains("unavailable"));
}

#[test]
fn credential_entries_share_region_and_keep_aws_secrets_hidden() {
    for (profiles, environment_credentials, method) in [
        (
            vec![BedrockAwsProfile {
                name: "engineering".to_string(),
                region: None,
            }],
            Vec::new(),
            BedrockMethod::Profile(0),
        ),
        (
            Vec::new(),
            vec![BedrockEnvironmentCredential {
                credential_type: AwsCredentialType::AccessKeys,
                region: None,
            }],
            BedrockMethod::Environment,
        ),
    ] {
        let mut state = BedrockState::discovered(BedrockDiscoverResponse {
            profiles,
            environment_credentials,
        });
        state.select_method(method);
        assert!(matches!(&state.view, BedrockView::RegionEntry { .. }));
        assert!(state.is_text_entry_active());
    }

    let mut access_keys = BedrockState::discovered(BedrockDiscoverResponse {
        profiles: Vec::new(),
        environment_credentials: Vec::new(),
    });
    access_keys.view = BedrockView::AccessKeyEntry {
        values: [
            "AKIAEXAMPLE".into(),
            "secret-value".into(),
            "session-value".into(),
        ],
        selected_field: 1,
    };
    let visible = render_visible(&access_keys);
    insta::assert_snapshot!(visible, @r###"
    > Set up Amazon Bedrock

      Enter your AWS access keys.

      AWS access key ID: AKIAEXAMPLE
    > AWS secret access key: •••••••••••e
      AWS session token (optional): •••••••••••••

      Press enter to continue
      Press esc to go back
    "###);
    assert!(visible.contains("AKIAEXAMPLE"));
    assert!(visible.contains("AWS secret access key: •••••••••••e"));
    assert!(!visible.contains("secret-value"));
    assert!(!visible.contains("session-value"));

    access_keys.handle_key_event(&KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
    assert!(render_visible(&access_keys).contains("AWS secret access key: ••••••••••••x"));

    access_keys.handle_key_event(&KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let visible = render_visible(&access_keys);
    insta::assert_snapshot!(visible, @r###"
    > Set up Amazon Bedrock

      Enter your AWS access keys.

      AWS access key ID: AKIAEXAMPLE
      AWS secret access key: •••••••••••••
    > AWS session token (optional): ••••••••••••e

      Press enter to continue
      Press esc to go back
    "###);
    assert!(!visible.contains("secret-value"));
    assert!(!visible.contains("session-value"));

    access_keys.view = BedrockView::ApiKeyEntry("old-key".into());
    access_keys.handle_key_event(&KeyEvent::new(KeyCode::Home, KeyModifiers::NONE));
    for _ in 0..3 {
        access_keys.handle_key_event(&KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
    }
    access_keys.handle_key_event(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    access_keys.active_input_mut().unwrap().paste("bedrock-api");
    assert_eq!(
        access_keys.active_input_mut().unwrap().value().as_deref(),
        Some("bedrock-api-key")
    );
    access_keys.handle_key_event(&KeyEvent::new(KeyCode::End, KeyModifiers::NONE));
    let visible = render_visible(&access_keys);
    insta::assert_snapshot!(visible, @r###"
    > Set up Amazon Bedrock

      Enter your Amazon Bedrock API key.

      Bedrock API key: ••••••••••••••y

      Press enter to continue
      Press esc to go back
    "###);
    assert!(!visible.contains("bedrock-api-key"));
    access_keys.handle_key_event(&KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
    assert!(render_visible(&access_keys).contains("Bedrock API key: •••••••••••••••z"));

    access_keys.view = BedrockView::ProfileEntry("engineering".into());
    insta::assert_snapshot!(render_visible(&access_keys), @r###"
    > Set up Amazon Bedrock

      Enter the name of your AWS profile.

      AWS profile: engineering

      Press enter to continue
      Press esc to go back
    "###);

    access_keys.enter_region(BedrockCredential::Environment, "us-west-1");
    insta::assert_snapshot!(render_visible(&access_keys), @r###"
    > Set up Amazon Bedrock

      Enter the AWS Region to use with Amazon Bedrock.

      AWS Region: us-west-1

      Press enter to continue
      Press esc to go back
    "###);

    access_keys.view = BedrockView::EnvironmentInstructions;
    insta::assert_snapshot!(render_visible(&access_keys), @r###"
    > Set up Amazon Bedrock

      Configure AWS credentials in your environment, then restart Codex.

      Setup guide: https://learn.chatgpt.com/docs/amazon-bedrock

    > 1. Choose another sign-in method


      Press enter to continue
      Press esc to go back
    "###);

    for view in [
        BedrockView::ProfileEntry(ProviderInput::default()),
        BedrockView::ApiKeyEntry(ProviderInput::default()),
        BedrockView::RegionEntry {
            credential: BedrockCredential::Environment,
            value: ProviderInput::default(),
        },
        BedrockView::AccessKeyEntry {
            values: Default::default(),
            selected_field: 0,
        },
    ] {
        access_keys.view = view;
        assert!(access_keys.is_text_entry_active());
    }

    for credential in [
        BedrockCredential::Profile("engineering".to_string()),
        BedrockCredential::Environment,
        BedrockCredential::AccessKeys {
            access_key_id: "AKIAEXAMPLE".to_string(),
            secret_access_key: "secret-value".to_string(),
            session_token: None,
        },
        BedrockCredential::ApiKey("bedrock-api-key".to_string()),
    ] {
        access_keys.enter_region(credential, "us-west-1");
        assert!(matches!(
            &access_keys.view,
            BedrockView::RegionEntry { value, .. } if value.value().as_deref() == Some("us-west-1")
        ));
    }
}

#[test]
fn localized_bedrock_views_preserve_credentials_choices_and_cell_widths() {
    use crate::i18n::Locale;
    let profile = "prod/한글-e\u{301}{ $profile }";
    let region = "us-test-1";
    for (
        locale,
        title,
        checking,
        configuring,
        choose,
        profile_label,
        region_label,
        access_label,
        api_label,
        environment,
        other_method,
    ) in [
        (
            Locale::English,
            "Set up Amazon Bedrock",
            "Checking for existing AWS credentials...",
            "Setting up Amazon Bedrock...",
            "Choose how you authenticate with AWS.",
            "AWS profile: ",
            "AWS Region: ",
            "AWS access key ID",
            "Bedrock API key: ",
            "Configure AWS credentials in your environment, then restart Codex.",
            "Choose another sign-in method",
        ),
        (
            Locale::Korean,
            "Amazon Bedrock 설정",
            "기존 AWS 자격 증명 확인 중...",
            "Amazon Bedrock 설정 중...",
            "AWS 인증 방법을 선택하세요.",
            "AWS 프로필: ",
            "AWS 리전: ",
            "AWS 액세스 키 ID",
            "Bedrock API 키: ",
            "환경에 AWS 자격 증명을 구성한 후 Codex를 다시 시작하세요.",
            "다른 로그인 방법 선택",
        ),
    ] {
        crate::i18n::with_locale(locale, || {
            let mut state = BedrockState::discovering(RequestId::Integer(91));
            let visible = render_visible(&state);
            assert!(visible.contains(title));
            assert!(visible.contains(checking));
            state.view = BedrockView::Configuring(RequestId::Integer(92));
            assert!(render_visible(&state).contains(configuring));
            state.view = BedrockView::Methods(BedrockMethodList::All);
            assert!(render_visible_at(&state, Rect::new(0, 0, 110, 40)).contains(choose));
            assert_eq!(
                state.methods(),
                vec![
                    BedrockMethod::ManualProfile,
                    BedrockMethod::AccessKeys,
                    BedrockMethod::EnvironmentInstructions,
                    BedrockMethod::ApiKey
                ]
            );

            state.view = BedrockView::ProfileEntry(profile.into());
            assert!(render_visible(&state).contains(&format!("{profile_label}{profile}")));
            state.view = BedrockView::RegionEntry {
                credential: BedrockCredential::Profile(profile.to_string()),
                value: region.into(),
            };
            assert!(render_visible(&state).contains(&format!("{region_label}{region}")));
            let Some(BedrockAction::Configure(
                BedrockCredential::Profile(actual_profile),
                actual_region,
            )) = state.confirm_input()
            else {
                panic!("configure profile")
            };
            assert_eq!(actual_profile, profile);
            assert_eq!(actual_region, region);

            let values = [
                "AKIA-OPAQUE".to_string(),
                "secret/opaque-z".to_string(),
                "session/opaque-y".to_string(),
            ];
            state.view = BedrockView::AccessKeyEntry {
                values: values.each_ref().map(|value| value.as_str().into()),
                selected_field: 1,
            };
            let visible = render_visible(&state);
            assert!(visible.contains(&format!("{access_label}: {}", values[0])));
            assert!(visible.contains(&format!("{}z", "•".repeat(values[1].chars().count() - 1))));
            assert!(!visible.contains(&values[1]));
            assert!(!visible.contains(&values[2]));
            let BedrockView::AccessKeyEntry {
                values: retained, ..
            } = &state.view
            else {
                panic!("access keys")
            };
            assert_eq!(
                retained.each_ref().map(ProviderInput::value),
                values.clone().map(Some)
            );
            state.view = BedrockView::ApiKeyEntry("opaque/api-key-z".into());
            assert!(render_visible(&state).contains(&format!("{api_label}{}z", "•".repeat(15))));
            state.view = BedrockView::EnvironmentInstructions;
            let visible = render_visible_at(&state, Rect::new(0, 0, 110, 40));
            assert!(visible.contains(environment));
            assert!(visible.contains(other_method));
            assert!(visible.contains("https://learn.chatgpt.com/docs/amazon-bedrock"));

            state.profiles = vec![BedrockAwsProfile {
                name: profile.to_string(),
                region: Some(region.to_string()),
            }];
            state.view = BedrockView::Methods(BedrockMethodList::Detected);
            assert!(render_visible_at(&state, Rect::new(0, 0, 110, 40)).contains(profile));
            assert_eq!(
                state.methods(),
                vec![BedrockMethod::Profile(0), BedrockMethod::OtherMethods]
            );
            for width in [18, 24, 72] {
                let visible = render_visible_at(&state, Rect::new(0, 0, width, 24));
                assert!(
                    visible
                        .lines()
                        .all(|line| unicode_width::UnicodeWidthStr::width(line)
                            <= usize::from(width))
                );
            }
            let Some(BedrockAction::Configure(
                BedrockCredential::Profile(actual_profile),
                actual_region,
            )) = state.select_method(BedrockMethod::Profile(0))
            else {
                panic!("selected profile")
            };
            assert_eq!(actual_profile, profile);
            assert_eq!(actual_region, region);
        });
    }
}

#[test]
fn localized_govcloud_guidance_keeps_link_scroll_and_acknowledgement() {
    use crate::i18n::Locale;
    for (locale, title, acknowledge, disclosure_end) in [
        (
            Locale::English,
            "Using Codex with AWS GovCloud",
            "Acknowledge",
            "proceeding.",
        ),
        (
            Locale::Korean,
            "AWS GovCloud에서 Codex 사용",
            "확인",
            "확인하세요.",
        ),
    ] {
        crate::i18n::with_locale(locale, || {
            let state = BedrockState::guidance();
            assert!(render_visible_at(&state, Rect::new(0, 0, 110, 24)).contains(title));
            for width in [18, 24, 72] {
                let mut state = BedrockState::guidance();
                let area = Rect::new(0, 0, width, 6);
                let mut raw_seen = String::new();
                let mut logical_seen = String::new();
                let mut link_seen = false;
                for _ in 0..100 {
                    let mut buffer = Buffer::empty(area);
                    state.render(area, &mut buffer, None);
                    link_seen |= buffer
                        .content
                        .iter()
                        .any(|cell| cell.symbol().contains(GOV_CLOUD_GUIDANCE_URL));
                    let visible = render_visible_at(&state, area);
                    assert!(
                        visible
                            .lines()
                            .all(|line| unicode_width::UnicodeWidthStr::width(line)
                                <= usize::from(width))
                    );
                    assert!(visible.contains(acknowledge));
                    raw_seen.push_str(&visible);
                    logical_seen.extend(visible.lines());
                    assert!(!state.handle_guidance_key_event(&KeyEvent::new(
                        KeyCode::Down,
                        KeyModifiers::NONE
                    )));
                }
                eprintln!(
                    "locale={locale:?} width={width} raw_contains={} logical_contains={} raw_seen={raw_seen:?} logical_seen={logical_seen:?}",
                    raw_seen.contains(disclosure_end),
                    logical_seen.contains(disclosure_end)
                );
                assert!(
                    logical_seen.contains(disclosure_end),
                    "locale={locale:?} width={width} raw_seen={raw_seen:?} logical_seen={logical_seen:?}"
                );
                assert!(link_seen);
                assert!(
                    state.handle_guidance_key_event(&KeyEvent::new(
                        KeyCode::Enter,
                        KeyModifiers::NONE
                    ))
                );
            }
            let error = "opaque://backend/{ $error }/한글-e\u{301}";
            for (id, prefix) in [
                (
                    "dialogs-bedrock-discovery-error",
                    if locale == Locale::English {
                        "Unable to check AWS credentials: "
                    } else {
                        "AWS 자격 증명을 확인할 수 없습니다: "
                    },
                ),
                (
                    "dialogs-bedrock-setup-error",
                    if locale == Locale::English {
                        "Unable to set up Amazon Bedrock: "
                    } else {
                        "Amazon Bedrock을 설정할 수 없습니다: "
                    },
                ),
            ] {
                assert_eq!(message(id, &[("error", error)]), format!("{prefix}{error}"));
            }
        });
    }
}
