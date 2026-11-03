//! A compact footer advertises primary actions; help lists the configured task shortcuts.

use super::*;
use crate::key_hint;
use crate::shortcut_help::Group;
use crate::shortcut_help::Shortcut;
use crate::style::footer_hint_label_style;

pub(super) fn hint_line(items: &[(String, String)]) -> Line<'static> {
    let mut line = Line::default();
    for (key, label) in items.iter().filter(|(key, _)| !key.is_empty()) {
        if !line.spans.is_empty() {
            line.spans.push("  ".dim());
        }
        line.spans.extend(key_hint::key_label_spans(key));
        line.spans.push(Span::styled(
            format!(" {label}"),
            footer_hint_label_style().not_bold(),
        ));
    }
    line
}

impl AgentsOverviewView {
    fn center_list_hint(&self, action: ListAction) -> Option<ShortcutHint> {
        self.keymap.primary_hint(action).filter(|hint| {
            !matches!(hint, ShortcutHint::Single(binding) if [
                &self.agents_keymap.resume,
                &self.agents_keymap.search,
                &self.agents_keymap.new_task,
                &self.agents_keymap.new_worktree,
                &self.agents_keymap.fork,
                &self.agents_keymap.rename,
                &self.agents_keymap.stop,
                &self.agents_keymap.archive,
                &self.agents_keymap.delete,
                &self.agents_keymap.hide,
                &self.agents_keymap.toggle_pin,
                &self.agents_keymap.toggle_grouping,
            ].into_iter().any(|bindings| bindings.contains(binding)))
        })
    }

    pub(super) fn center_filter_hint(&self) -> String {
        [key_hint::plain(KeyCode::Tab), key_hint::shift(KeyCode::Tab)]
            .into_iter()
            .filter(|hint| {
                let (code, modifiers) = hint.parts();
                !self
                    .center_shortcut_keys
                    .is_pressed(KeyEvent::new(code, modifiers))
            })
            .map(|hint| hint.display_label())
            .collect::<Vec<_>>()
            .join("/")
    }

    pub(super) fn center_help_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut navigate = Group {
            title: crate::i18n::static_message("picker-center-navigate"),
            entries: Vec::new(),
        };
        for (action, label) in [
            (
                ListAction::MoveUp,
                crate::i18n::static_message("picker-center-up"),
            ),
            (
                ListAction::MoveDown,
                crate::i18n::static_message("picker-center-down"),
            ),
            (
                ListAction::Accept,
                crate::i18n::static_message("picker-center-open-title"),
            ),
            (
                ListAction::PageUp,
                crate::i18n::static_message("picker-center-page-up"),
            ),
            (
                ListAction::PageDown,
                crate::i18n::static_message("picker-center-page-down"),
            ),
        ] {
            navigate.push(self.center_list_hint(action), label);
        }
        navigate.entries.push(Shortcut::new(
            key_hint::ctrl(KeyCode::Char('c')),
            crate::i18n::static_message("picker-center-quit-title"),
        ));
        let mut tasks = Group {
            title: crate::i18n::static_message("picker-center-tasks"),
            entries: Vec::new(),
        };
        for (action, bindings, label) in [
            (
                "new_task",
                &self.agents_keymap.new_task,
                crate::i18n::static_message("picker-center-new-title"),
            ),
            (
                "new_worktree",
                &self.agents_keymap.new_worktree,
                crate::i18n::static_message("picker-center-new-worktree"),
            ),
            (
                "resume",
                &self.agents_keymap.resume,
                crate::i18n::static_message("picker-center-resume-title"),
            ),
            (
                "fork",
                &self.agents_keymap.fork,
                crate::i18n::static_message("picker-center-fork-title"),
            ),
            (
                "rename",
                &self.agents_keymap.rename,
                crate::i18n::static_message("picker-center-rename-title"),
            ),
            (
                "stop",
                &self.agents_keymap.stop,
                crate::i18n::static_message("picker-center-stop-title"),
            ),
            (
                "archive",
                &self.agents_keymap.archive,
                crate::i18n::static_message("picker-center-archive-title"),
            ),
            (
                "hide",
                &self.agents_keymap.hide,
                crate::i18n::static_message("picker-center-hide-title"),
            ),
            (
                "delete",
                &self.agents_keymap.delete,
                crate::i18n::static_message("picker-center-delete-title"),
            ),
            (
                "toggle_pin",
                &self.agents_keymap.toggle_pin,
                crate::i18n::static_message("picker-center-toggle-pin"),
            ),
        ] {
            if (action != "new_worktree" || self.worktrees_enabled)
                && (action != "toggle_pin" || self.can_toggle_selected_pin())
            {
                tasks.push(self.agents_keymap.primary_hint(action, bindings), label);
            }
        }
        let mut view = Group {
            title: crate::i18n::static_message("picker-center-view"),
            entries: Vec::new(),
        };
        let filter = self.center_filter_hint();
        if !filter.is_empty() {
            view.entries.push(Shortcut {
                key: filter,
                action: crate::i18n::static_message("picker-center-filter-title"),
            });
        }
        for (action, bindings, label) in [
            (
                "search",
                &self.agents_keymap.search,
                crate::i18n::static_message("picker-center-search-title"),
            ),
            (
                "toggle_grouping",
                &self.agents_keymap.toggle_grouping,
                crate::i18n::static_message("picker-center-group-title"),
            ),
        ] {
            view.push(self.agents_keymap.primary_hint(action, bindings), label);
        }
        let mut lines = vec![
            crate::i18n::static_message("picker-center-task-shortcuts")
                .bold()
                .into(),
            Line::default(),
        ];
        lines.extend(crate::shortcut_help::group_lines(
            [navigate, tasks, view],
            width,
        ));
        crate::wrapping::word_wrap_lines(lines, usize::from(width.max(/*other*/ 1)))
    }

    pub(super) fn center_footer_hints(&self) -> Vec<(String, String)> {
        let state = self.state();
        if let Some(items) = &state.key_chord_hint {
            return items.clone();
        }
        let mut hints = Vec::new();
        if !state.help
            && !state.editing_metadata()
            && ![KeyModifiers::NONE, KeyModifiers::SHIFT]
                .into_iter()
                .any(|modifiers| {
                    self.center_shortcut_keys
                        .is_pressed(KeyEvent::new(KeyCode::Char('?'), modifiers))
                })
        {
            hints.push((
                "?".into(),
                crate::i18n::static_message("picker-center-help").into(),
            ));
        }
        let cancel = if state.help {
            self.keymap.primary_hint(ListAction::Cancel)
        } else {
            self.center_list_hint(ListAction::Cancel)
        };
        if let Some(hint) = cancel {
            hints.push((
                hint.display_label(),
                crate::i18n::static_message("picker-center-back").into(),
            ));
        }
        if state.help {
            return hints;
        }
        if !state.editing_metadata() {
            let navigation = [ListAction::MoveUp, ListAction::MoveDown]
                .into_iter()
                .filter_map(|action| self.center_list_hint(action))
                .map(ShortcutHint::display_label)
                .collect::<Vec<_>>()
                .join("/");
            if !navigation.is_empty() {
                hints.push((
                    navigation,
                    crate::i18n::static_message("picker-center-move").into(),
                ));
            }
        }
        if let Some(hint) = self.center_list_hint(ListAction::Accept) {
            hints.push((
                hint.display_label(),
                if state.rename_target.is_some() {
                    crate::i18n::static_message("picker-center-rename")
                } else {
                    crate::i18n::static_message("picker-center-open")
                }
                .into(),
            ));
        }
        if !state.editing_metadata()
            && let Some(hint) = self
                .agents_keymap
                .primary_hint("new_task", &self.agents_keymap.new_task)
        {
            hints.push((
                hint.display_label(),
                crate::i18n::static_message("picker-new").into(),
            ));
        }
        hints
    }
}
