//! Project transcript interactions and page-loading feedback into the existing footer.
//! Loading never inserts transcript rows; the owner schedules motion only while a page is pending.
//! Surface owners supply the latest shortcut because legacy pagers reserve Escape for editing.
//! Known navigation labels style keys separately from prose; transcript and query text stay untouched.

use super::*;
use crate::bottom_pane::TranscriptFooter;
use crate::footer_hint::first_fitting_line;
use crate::key_hint::key_label_spans;
use crate::motion::MotionMode;
use crate::motion::shimmer_text;
use crate::style::accent_color;
use crossterm::event::KeyCode;
use unicode_width::UnicodeWidthStr;

impl TranscriptView {
    pub(crate) fn footer_with_navigation(
        &self,
        width: u16,
        motion: MotionMode,
        latest_navigation: &str,
    ) -> Option<TranscriptFooter> {
        if let Some(mode) = &self.copy_mode {
            return Some(TranscriptFooter {
                text: vec![
                    Line::from(crate::i18n::message(
                        "r39-copy-format",
                        &[("format", &mode.label.to_lowercase())],
                    ))
                    .bold(),
                    navigation_line("↑/↓/j/k select · g/G ends · enter copy · esc close"),
                ]
                .into(),
                cursor_column: None,
                is_interactive: true,
            });
        }
        // A mouse-down anchor owns the gesture, but has no text to copy yet. Compare
        // source positions so movement across wrapping or padding is not a selection.
        let has_selected_text = self.selection.as_ref().is_some_and(|selection| {
            (selection.start.key, selection.start.offset)
                != (selection.end.key, selection.end.offset)
        });
        if let Some((query, cursor)) = self.search_footer(width) {
            return Some(TranscriptFooter {
                text: vec![
                    query,
                    if has_selected_text {
                        selection_hint(width)
                    } else {
                        self.search.status_line(width, self.history)
                    },
                ]
                .into(),
                cursor_column: Some(cursor),
                is_interactive: true,
            });
        }
        if self.select_mode.is_some() {
            let hints = if has_selected_text {
                let copy = crate::key_hint::ctrl(KeyCode::Char('c')).display_label();
                [
                    format!(
                        "arrows/hjkl/w/b/e/shift+arrows move · home/end/0/$/g/G/⌃home/⌃end ends · pgup/pgdn/⌃b/⌃f/⌃u/⌃d page · v/space/V mark · o swap · / find · y/{copy} copy · enter copy & latest · esc clear · q close"
                    ),
                    format!(
                        "arrows/hjkl move · o swap · y/{copy} copy · enter copy & latest · esc clear · q close"
                    ),
                    format!("y/{copy} copy · enter copy & latest · esc clear · q close"),
                    "enter copy · q close".to_owned(),
                ]
            } else {
                [
                    "arrows/hjkl/w/b/e move · home/end/0/$/g/G/⌃home/⌃end ends · pgup/pgdn/⌃b/⌃f/⌃u/⌃d page · v/space/V/shift+arrows mark · / find · n/N next · enter/q/esc close"
                        .to_owned(),
                    "arrows/hjkl/w/b/e move · 0/$/g/G ends · v/space/V mark · / find · enter/q/esc close"
                        .to_owned(),
                    "arrows/hjkl move · v/space/V mark · / find · enter/q/esc close".to_owned(),
                    "v mark · enter/q close".to_owned(),
                ]
            };
            return Some(TranscriptFooter {
                text: first_fitting_line(
                    hints.iter().map(String::as_str).map(navigation_line),
                    width,
                )
                .into(),
                cursor_column: None,
                is_interactive: true,
            });
        }
        let pending = self.is_loading_history()
            || matches!(
                self.history,
                TranscriptHistoryState::Deferred | TranscriptHistoryState::Failed
            );
        if has_selected_text {
            return Some(TranscriptFooter {
                text: if self.tail_visible && !pending {
                    selection_hint(width)
                } else {
                    first_fitting_line(
                        [
                            self.status_line_with_navigation(
                                &format!(
                                    "{} copy · enter copy & follow · esc clear",
                                    crate::key_hint::ctrl(KeyCode::Char('c')).display_label()
                                ),
                                motion,
                            ),
                            selection_hint(width),
                        ],
                        width,
                    )
                }
                .into(),
                cursor_column: None,
                is_interactive: true,
            });
        }
        if self.search.is_reading() && !pending && !self.is_activity_focused() {
            return Some(TranscriptFooter {
                text: self.search.status_line(width, self.history).into(),
                cursor_column: None,
                is_interactive: false,
            });
        }
        if self.is_activity_focused() && !pending {
            return self.disclosure_footer(width);
        }
        // Selection can pause following without hiding the current final row.
        let can_return =
            self.selection.is_none() && self.can_return_to_latest() && !self.tail_visible;
        (pending || self.unseen_activity || can_return).then(|| {
            let navigation = if self.can_return_to_latest() {
                latest_navigation
            } else {
                ""
            };
            let mut status = self.status_line_with_navigation(navigation, motion);
            // Loading must not resize the composer or move its caret. Shorten secondary text
            // before clipping either shortcut; failed loads prioritize their retry action.
            if status.width() > usize::from(width) {
                status = if self.is_loading_history() {
                    let loading = if navigation.is_empty() {
                        if "↑ ".width() + crate::i18n::text("Loading…").width()
                            <= usize::from(width)
                        {
                            crate::i18n::text("Loading…")
                        } else {
                            "…"
                        }
                    } else if "↑ ".width()
                        + crate::i18n::text("Loading… · ").width()
                        + navigation_line(navigation).width()
                        <= usize::from(width)
                    {
                        crate::i18n::text("Loading… · ")
                    } else {
                        "… "
                    };
                    let mut spans = vec!["↑ ".fg(accent_color())];
                    spans.extend(shimmer_text(loading, motion));
                    spans.extend(navigation_line(navigation).spans);
                    Line::from(spans)
                } else if self.history == TranscriptHistoryState::Failed {
                    let mut spans = vec![crate::i18n::text("Retry: ").dim()];
                    spans.extend(key_label_spans(&JumpTarget::Beginning.hint_label()));
                    Line::from(spans)
                } else if self.history == TranscriptHistoryState::Deferred {
                    Line::from(crate::i18n::text("Waiting for history to be saved")).dim()
                } else if navigation.is_empty() {
                    Line::from(crate::i18n::text("New activity")).dim()
                } else if self.unseen_activity {
                    let mut spans = vec![crate::i18n::text("New · ").dim()];
                    spans.extend(navigation_line(navigation).spans);
                    Line::from(spans)
                } else {
                    navigation_line(navigation)
                };
            }
            let retry = if self.history == TranscriptHistoryState::Failed {
                format!(
                    "{} retry",
                    crate::key_hint::ctrl(KeyCode::Home).display_label()
                )
            } else {
                navigation.to_owned()
            };
            TranscriptFooter {
                text: first_fitting_line(
                    [status, navigation_line(&retry), navigation_line(navigation)],
                    width,
                )
                .into(),
                cursor_column: None,
                is_interactive: false,
            }
        })
    }

    pub(crate) fn status_line_with_navigation(
        &self,
        navigation: &str,
        motion: MotionMode,
    ) -> Line<'static> {
        if self.is_loading_history() {
            let mut spans = vec!["↑ ".fg(accent_color())];
            spans.extend(shimmer_text(
                crate::i18n::text("Loading earlier messages…"),
                motion,
            ));
            if self.unseen_activity {
                spans.push(" · New activity".dim());
            }
            if !navigation.is_empty() {
                spans.push(" · ".dim());
                spans.extend(navigation_line(navigation).spans);
            }
            return Line::from(spans);
        }
        if self.selection.is_some() && self.history == TranscriptHistoryState::Failed {
            let mut spans = navigation_line(navigation).spans;
            spans.push(" · Retry: ".dim());
            spans.extend(key_label_spans(&JumpTarget::Beginning.hint_label()));
            return Line::from(spans);
        }
        let history = match self.history {
            TranscriptHistoryState::Failed => {
                let mut spans = vec![crate::i18n::text("Retry history: ").dim()];
                spans.extend(key_label_spans(&JumpTarget::Beginning.hint_label()));
                spans.push(".  ".dim());
                spans
            }
            TranscriptHistoryState::Partial => {
                vec![crate::i18n::text("Earlier messages available.  ").dim()]
            }
            TranscriptHistoryState::Deferred => {
                vec![crate::i18n::text("Waiting for history to be saved.  ").dim()]
            }
            TranscriptHistoryState::LoadingOlder
            | TranscriptHistoryState::LoadingBeginning
            | TranscriptHistoryState::Complete
            | TranscriptHistoryState::Idle => Vec::new(),
        };
        let activity = if self.unseen_activity {
            if history.is_empty() && navigation.is_empty() {
                crate::i18n::text("New activity")
            } else {
                crate::i18n::text("New activity · ")
            }
        } else {
            ""
        };
        let mut spans = vec![activity.dim()];
        spans.extend(history);
        spans.extend(navigation_line(navigation).spans);
        Line::from(spans)
    }

    pub(crate) fn is_loading_history(&self) -> bool {
        matches!(
            self.history,
            TranscriptHistoryState::LoadingOlder | TranscriptHistoryState::LoadingBeginning
        )
    }
}

fn selection_hint(width: u16) -> Line<'static> {
    first_fitting_line(
        [
            &format!(
                "{} copy · enter copy & follow · esc clear",
                crate::key_hint::ctrl(KeyCode::Char('c')).display_label()
            ),
            "enter copy & follow · esc clear",
            "enter copy+↓ · esc",
            "esc clear",
        ]
        .map(navigation_line),
        width,
    )
}

/// Style only the explicit key-label prefixes of trusted navigation hints.
///
/// Callers supply UI-authored hints, including runtime key chords followed by a known action.
/// Unknown segments remain dim prose; this must never be applied to transcript or query content.
pub(super) fn navigation_line(navigation: &str) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, hint) in navigation.split(" · ").enumerate() {
        if index > 0 {
            spans.push(" · ".dim());
        }
        let action = [
            " copy & follow",
            " copy & latest",
            " clear selection",
            " copy+↓",
            " move",
            " ends",
            " page",
            " mark",
            " find",
            " swap",
            " previous",
            " accept",
            " cancel",
            " older",
            " newer",
            " latest",
            " retry",
            " select",
            " copy",
            " clear",
            " next",
            " close",
        ]
        .into_iter()
        .find(|action| hint.ends_with(*action));
        if let Some(action) = action {
            spans.extend(key_label_spans(&hint[..hint.len() - action.len()]));
            spans.push(crate::i18n::text(action).to_owned().dim());
        } else if hint == "esc" {
            spans.extend(key_label_spans(hint));
        } else {
            spans.push(hint.to_owned().dim());
        }
    }
    Line::from(spans)
}

#[cfg(test)]
#[path = "footer_tests.rs"]
mod tests;

#[cfg(test)]
mod r39_localization_tests {
    use super::*;
    use crate::i18n::Locale;
    use crate::i18n::with_locale;

    #[test]
    fn translated_navigation_preserves_key_spans_and_unknown_input() {
        let english = with_locale(Locale::English, || navigation_line("enter/esc latest"));
        let korean = with_locale(Locale::Korean, || navigation_line("enter/esc latest"));
        assert_eq!(english.spans.len(), korean.spans.len());
        for (en, ko) in english.spans.iter().zip(&korean.spans) {
            if en.content == " latest" {
                assert_eq!(ko.content, " 최신 위치");
            } else {
                assert_eq!(en.content, ko.content);
            }
            assert_eq!(en.style, ko.style);
        }
        for locale in [Locale::English, Locale::Korean] {
            with_locale(locale, || {
                assert_eq!(
                    navigation_line("opaque-{name} button").to_string(),
                    "opaque-{name} button"
                )
            });
        }
    }
}
