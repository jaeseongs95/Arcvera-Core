// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
use super::*;
use codex_config::types::KeybindingSpec;
use codex_config::types::KeybindingsSpec;

#[tokio::test]
async fn image_paste_reservations_match_key_dispatch_and_preserve_copy_override() {
    let (mut chat, _sender, _events, mut ops) =
        crate::chatwidget::tests::make_chatwidget_manual_with_sender().await;
    chat.bottom_pane
        .set_composer_text("keep this input".into(), Vec::new(), Vec::new());
    let cases = [
        (
            "alt-v",
            KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT),
        ),
        (
            "ctrl-shift-v",
            KeyEvent::new(
                KeyCode::Char('V'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ),
        ),
    ];
    for (binding, event) in cases {
        let mut keymap = chat.config_ref().tui_keymap.clone();
        keymap.composer.submit = Some(KeybindingsSpec::One(KeybindingSpec(binding.into())));
        let error = crate::keymap::RuntimeKeymap::from_config(&keymap)
            .expect_err("a fixed image key must not be accepted as submit");
        assert!(error.contains("composer.submit"), "{error}");
        assert!(error.contains("fixed.paste_image"), "{error}");
        assert!(matches!(
            chat.handle_key_event(event),
            KeyEventAction::PasteImage { text_fallback }
                if text_fallback == (cfg!(windows) && event.code == KeyCode::Insert)
                    .then_some(KeyCode::Insert)
        ));
        chat.handle_key_event(KeyEvent {
            kind: KeyEventKind::Release,
            ..event
        });
        assert_eq!(chat.bottom_pane.composer_text(), "keep this input");
    }
    #[cfg(windows)]
    {
        // Legacy Shift+Insert arrives from the terminal even though Insert is
        // deliberately not accepted by the user keybinding parser.
        let event = KeyEvent::new(KeyCode::Insert, KeyModifiers::SHIFT);
        assert!(matches!(
            chat.handle_key_event(event),
            KeyEventAction::PasteImage {
                text_fallback: Some(KeyCode::Insert)
            }
        ));
        chat.handle_key_event(KeyEvent {
            kind: KeyEventKind::Release,
            ..event
        });
        assert_eq!(chat.bottom_pane.composer_text(), "keep this input");
    }
    assert!(ops.try_recv().is_err(), "paste must not submit a turn");

    let mut keymap = chat.config_ref().tui_keymap.clone();
    keymap.global.copy = Some(KeybindingsSpec::One(KeybindingSpec("alt-v".into())));
    keymap.composer.submit = Some(KeybindingsSpec::One(KeybindingSpec("f12".into())));
    let runtime = crate::keymap::RuntimeKeymap::from_config(&keymap)
        .expect("copy override and a distinct submit key remain valid");
    chat.apply_keymap_update(keymap, &runtime);
    chat.transcript.last_agent_markdown = Some("copy target".into());
    assert!(matches!(
        chat.handle_key_event(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::ALT)),
        KeyEventAction::CopyLastResponse(text) if &*text == "copy target"
    ));
    assert_eq!(chat.bottom_pane.composer_text(), "keep this input");
    assert!(ops.try_recv().is_err(), "copy must not submit a turn");

    chat.thread_id = Some(codex_protocol::ThreadId::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::F(12), KeyModifiers::NONE));
    let turn = std::iter::from_fn(|| ops.try_recv().ok())
        .find(|op| matches!(op, crate::app_command::AppCommand::UserTurn { .. }))
        .expect("the accepted submit key must dispatch a user turn");
    assert!(matches!(
        turn,
        crate::app_command::AppCommand::UserTurn { items, .. }
            if items == vec![codex_app_server_protocol::UserInput::Text {
                text: "keep this input".into(),
                text_elements: Vec::new(),
            }]
    ));
}
