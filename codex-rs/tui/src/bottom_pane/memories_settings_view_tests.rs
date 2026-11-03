// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
//! Verify that memory settings reserve enough space for wrapped choices.

use super::*;

#[test]
fn reset_confirmation_keeps_both_choices_at_wrap_boundary() {
    let (app_tx, _app_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = MemoriesSettingsView::new(
        /*use_memories*/ true,
        /*generate_memories*/ true,
        AppEventSender::new(app_tx),
        crate::keymap::RuntimeKeymap::defaults().list,
    );
    view.open_reset_confirmation();

    let mut snapshots = Vec::new();
    for width in [68, 69, 70, 71, 72] {
        let area = Rect::new(
            /*x*/ 0,
            /*y*/ 0,
            width,
            view.desired_height(width),
        );
        let mut buffer = Buffer::empty(area);
        view.render(area, &mut buffer);
        let text = buffer
            .content
            .chunks(usize::from(width))
            .map(|row| {
                row.iter()
                    .map(ratatui::buffer::Cell::symbol)
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            text.contains("Go back"),
            "missing cancel choice at width {width}"
        );
        snapshots.push(format!("width={width}\n{text}"));
    }
    insta::assert_snapshot!(snapshots.join("\n\n"));
}

#[test]
fn project_reset_never_dispatches_global_reset() {
    let (app_tx, mut app_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut view = MemoriesSettingsView::new(
        true,
        true,
        AppEventSender::new(app_tx),
        crate::keymap::RuntimeKeymap::defaults().list,
    )
    .with_scope("project");
    view.state.selected_idx = view.items.len().checked_sub(1);
    view.handle_key_event(KeyEvent::from(KeyCode::Enter));
    assert!(
        matches!(app_rx.try_recv(), Ok(AppEvent::MemoryCommand { args, .. }) if args == "reset-project-unavailable")
    );
    assert!(app_rx.try_recv().is_err());
    assert!(view.reset_confirmation.is_none());
}

#[test]
fn localized_memory_reset_keeps_cancel_and_reset_meaning_at_narrow_widths() {
    for (locale, cancel) in [
        (crate::i18n::Locale::English, "Go back"),
        (crate::i18n::Locale::Korean, "돌아가기"),
    ] {
        crate::i18n::with_locale(locale, || {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            let mut view = MemoriesSettingsView::new(
                true,
                true,
                AppEventSender::new(tx),
                crate::keymap::RuntimeKeymap::defaults().list,
            );
            view.open_reset_confirmation();
            for width in [30, 40, 60, 80] {
                let rows = view.build_rows();
                assert!(rows[1].name.contains(cancel));
                let area = Rect::new(0, 0, width, view.desired_height(width));
                let mut buffer = Buffer::empty(area);
                view.render(area, &mut buffer);
                let text = buffer
                    .content
                    .chunks(usize::from(width))
                    .map(|row| {
                        row.iter()
                            .map(ratatui::buffer::Cell::symbol)
                            .collect::<String>()
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(
                    text.replace(' ', "").contains(&cancel.replace(' ', "")),
                    "{width}: {text}"
                );
            }
            view.handle_key_event(KeyEvent::from(KeyCode::Esc));
            assert!(view.reset_confirmation.is_none());
            assert!(rx.try_recv().is_err());
        });
    }
}
