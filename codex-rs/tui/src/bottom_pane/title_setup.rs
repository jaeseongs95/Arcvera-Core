//! Terminal title configuration view for customizing the terminal window/tab title.
//!
//! This module provides an interactive picker for selecting which items appear
//! in the terminal title. Users can:
//!
//! - Select items
//! - Reorder items
//! - Preview the rendered title

use itertools::Itertools;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Line;
use strum::IntoEnumIterator;
use strum_macros::Display;
use strum_macros::EnumIter;
use strum_macros::EnumString;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::bottom_pane::ACTION_REQUIRED_PREVIEW_PREFIX;
use crate::bottom_pane::CancellationEvent;
use crate::bottom_pane::bottom_pane_view::BottomPaneView;
use crate::bottom_pane::build_action_required_title_text;
use crate::bottom_pane::multi_select_picker::MultiSelectItem;
use crate::bottom_pane::multi_select_picker::MultiSelectPicker;
use crate::bottom_pane::status_surface_preview::StatusSurfacePreviewData;
use crate::bottom_pane::status_surface_preview::StatusSurfacePreviewItem;
use crate::keymap::ListKeymap;
use crate::render::renderable::Renderable;

/// Available items that can be displayed in the terminal title.
///
/// Variants serialize to kebab-case identifiers (e.g. `AppName` -> `"app-name"`)
/// via strum. These identifiers are persisted in user config files, so renaming
/// or removing a variant is a breaking config change.
#[derive(EnumIter, EnumString, Display, Debug, Clone, Copy, Eq, PartialEq, Hash)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum TerminalTitleItem {
    /// Codex app name.
    AppName,
    /// Project root name, or a compact cwd fallback.
    #[strum(to_string = "project-name", serialize = "project")]
    Project,
    /// Current working directory path.
    CurrentDir,
    /// Terminal-title activity indicator while active.
    #[strum(to_string = "activity", serialize = "spinner")]
    Spinner,
    /// Compact runtime run-state text.
    #[strum(to_string = "run-state", serialize = "status")]
    Status,
    /// Current thread name, omitted when unnamed.
    ThreadName,
    /// Current thread title (if available).
    #[strum(to_string = "thread-title", serialize = "thread")]
    Thread,
    /// Current git branch (if available).
    GitBranch,
    /// Percentage of context window remaining.
    ContextRemaining,
    /// Percentage of context window used.
    #[strum(to_string = "context-used", serialize = "context-usage")]
    ContextUsed,
    /// Remaining usage on the primary rate limit.
    FiveHourLimit,
    /// Remaining usage on the secondary rate limit.
    WeeklyLimit,
    /// Codex application version.
    CodexVersion,
    /// Total tokens used in the current session.
    UsedTokens,
    /// Total input tokens consumed.
    TotalInputTokens,
    /// Total output tokens generated.
    TotalOutputTokens,
    /// Estimated credits attributed directly to the current enterprise thread.
    ThreadCredits,
    /// Estimated dollar cost attributed directly to the current enterprise thread.
    EstimatedThreadCost,
    /// Full thread UUID.
    #[strum(to_string = "thread-id", serialize = "session-id")]
    SessionId,
    /// Effective processing tier for the current model.
    FastMode,
    /// Whether Daybreak is enabled for this thread.
    Daybreak,
    /// Current model name.
    #[strum(to_string = "model", serialize = "model-name")]
    Model,
    /// Current model name with reasoning level.
    ModelWithReasoning,
    /// Current reasoning level.
    Reasoning,
    /// Latest checklist task progress from `update_plan` (if available).
    TaskProgress,
}

impl TerminalTitleItem {
    fn display_name(self) -> String {
        match self {
            Self::AppName => crate::i18n::message("status-title-name-app-name", &[]),
            Self::Project => crate::i18n::message("status-title-name-project-name", &[]),
            Self::CurrentDir => crate::i18n::message("status-title-name-current-dir", &[]),
            Self::Spinner => crate::i18n::message("status-title-name-activity", &[]),
            Self::Status => crate::i18n::message("status-title-name-run-state", &[]),
            Self::ThreadName => crate::i18n::message("status-title-name-thread-name", &[]),
            Self::Thread => crate::i18n::message("status-title-name-thread-title", &[]),
            Self::GitBranch => crate::i18n::message("status-title-name-git-branch", &[]),
            Self::ContextRemaining => {
                crate::i18n::message("status-title-name-context-remaining", &[])
            }
            Self::ContextUsed => crate::i18n::message("status-title-name-context-used", &[]),
            Self::FiveHourLimit => crate::i18n::message("status-title-name-five-hour-limit", &[]),
            Self::WeeklyLimit => crate::i18n::message("status-title-name-weekly-limit", &[]),
            Self::CodexVersion => crate::i18n::message("status-title-name-codex-version", &[]),
            Self::UsedTokens => crate::i18n::message("status-title-name-used-tokens", &[]),
            Self::TotalInputTokens => {
                crate::i18n::message("status-title-name-total-input-tokens", &[])
            }
            Self::TotalOutputTokens => {
                crate::i18n::message("status-title-name-total-output-tokens", &[])
            }
            Self::ThreadCredits => crate::i18n::message("status-title-name-thread-credits", &[]),
            Self::EstimatedThreadCost => {
                crate::i18n::message("status-title-name-estimated-thread-cost", &[])
            }
            Self::SessionId => crate::i18n::message("status-title-name-thread-id", &[]),
            Self::FastMode => crate::i18n::message("status-title-name-fast-mode", &[]),
            Self::Daybreak => crate::i18n::message("status-title-name-daybreak", &[]),
            Self::Model => crate::i18n::message("status-title-name-model", &[]),
            Self::ModelWithReasoning => {
                crate::i18n::message("status-title-name-model-with-reasoning", &[])
            }
            Self::Reasoning => crate::i18n::message("status-title-name-reasoning", &[]),
            Self::TaskProgress => crate::i18n::message("status-title-name-task-progress", &[]),
        }
    }

    pub(crate) fn description(self) -> String {
        match self {
            Self::AppName => crate::i18n::message("status-title-description-app-name", &[]),
            Self::Project => crate::i18n::message("status-title-description-project-name", &[]),
            Self::CurrentDir => crate::i18n::message("status-title-description-current-dir", &[]),
            Self::Spinner => crate::i18n::message("status-title-description-activity", &[]),
            Self::Status => crate::i18n::message("status-title-description-run-state", &[]),
            Self::ThreadName => crate::i18n::message("status-title-description-thread-name", &[]),
            Self::Thread => crate::i18n::message("status-title-description-thread-title", &[]),
            Self::GitBranch => crate::i18n::message("status-title-description-git-branch", &[]),
            Self::ContextRemaining => {
                crate::i18n::message("status-title-description-context-remaining", &[])
            }
            Self::ContextUsed => crate::i18n::message("status-title-description-context-used", &[]),
            Self::FiveHourLimit => {
                crate::i18n::message("status-title-description-five-hour-limit", &[])
            }
            Self::WeeklyLimit => crate::i18n::message("status-title-description-weekly-limit", &[]),
            Self::CodexVersion => {
                crate::i18n::message("status-title-description-codex-version", &[])
            }
            Self::UsedTokens => crate::i18n::message("status-title-description-used-tokens", &[]),
            Self::TotalInputTokens => {
                crate::i18n::message("status-title-description-total-input-tokens", &[])
            }
            Self::TotalOutputTokens => {
                crate::i18n::message("status-title-description-total-output-tokens", &[])
            }
            Self::ThreadCredits => {
                crate::i18n::message("status-title-description-thread-credits", &[])
            }
            Self::EstimatedThreadCost => {
                crate::i18n::message("status-title-description-estimated-thread-cost", &[])
            }
            Self::SessionId => crate::i18n::message("status-title-description-thread-id", &[]),
            Self::FastMode => crate::i18n::message("status-title-description-fast-mode", &[]),
            Self::Daybreak => crate::i18n::message("status-title-description-daybreak", &[]),
            Self::Model => crate::i18n::message("status-title-description-model", &[]),
            Self::ModelWithReasoning => {
                crate::i18n::message("status-title-description-model-with-reasoning", &[])
            }
            Self::Reasoning => crate::i18n::message("status-title-description-reasoning", &[]),
            Self::TaskProgress => {
                crate::i18n::message("status-title-description-task-progress", &[])
            }
        }
    }

    pub(crate) fn preview_item(self) -> Option<StatusSurfacePreviewItem> {
        match self {
            TerminalTitleItem::AppName => Some(StatusSurfacePreviewItem::AppName),
            TerminalTitleItem::Project => Some(StatusSurfacePreviewItem::ProjectName),
            TerminalTitleItem::CurrentDir => Some(StatusSurfacePreviewItem::CurrentDir),
            TerminalTitleItem::Spinner => None,
            TerminalTitleItem::Status => Some(StatusSurfacePreviewItem::Status),
            TerminalTitleItem::ThreadName => Some(StatusSurfacePreviewItem::ThreadName),
            TerminalTitleItem::Thread => Some(StatusSurfacePreviewItem::ThreadTitle),
            TerminalTitleItem::GitBranch => Some(StatusSurfacePreviewItem::GitBranch),
            TerminalTitleItem::ContextRemaining => Some(StatusSurfacePreviewItem::ContextRemaining),
            TerminalTitleItem::ContextUsed => Some(StatusSurfacePreviewItem::ContextUsed),
            TerminalTitleItem::FiveHourLimit => Some(StatusSurfacePreviewItem::FiveHourLimit),
            TerminalTitleItem::WeeklyLimit => Some(StatusSurfacePreviewItem::WeeklyLimit),
            TerminalTitleItem::CodexVersion => Some(StatusSurfacePreviewItem::CodexVersion),
            TerminalTitleItem::UsedTokens => Some(StatusSurfacePreviewItem::UsedTokens),
            TerminalTitleItem::TotalInputTokens => Some(StatusSurfacePreviewItem::TotalInputTokens),
            TerminalTitleItem::TotalOutputTokens => {
                Some(StatusSurfacePreviewItem::TotalOutputTokens)
            }
            TerminalTitleItem::ThreadCredits => Some(StatusSurfacePreviewItem::ThreadCredits),
            TerminalTitleItem::EstimatedThreadCost => {
                Some(StatusSurfacePreviewItem::EstimatedThreadCost)
            }
            TerminalTitleItem::SessionId => Some(StatusSurfacePreviewItem::SessionId),
            TerminalTitleItem::FastMode => Some(StatusSurfacePreviewItem::FastMode),
            TerminalTitleItem::Daybreak => Some(StatusSurfacePreviewItem::Daybreak),
            TerminalTitleItem::Model => Some(StatusSurfacePreviewItem::Model),
            TerminalTitleItem::ModelWithReasoning => {
                Some(StatusSurfacePreviewItem::ModelWithReasoning)
            }
            TerminalTitleItem::Reasoning => Some(StatusSurfacePreviewItem::Reasoning),
            TerminalTitleItem::TaskProgress => Some(StatusSurfacePreviewItem::TaskProgress),
        }
    }

    /// Returns the separator to place before this item in a rendered title.
    ///
    /// The activity indicator gets a plain space on either side so it reads as
    /// `my-project <activity> Working` rather than `my-project | <activity> | Working`.
    /// All other adjacent items are joined with ` | `.
    pub(crate) fn separator_from_previous(self, previous: Option<Self>) -> &'static str {
        match previous {
            None => "",
            Some(previous)
                if previous == TerminalTitleItem::Spinner || self == TerminalTitleItem::Spinner =>
            {
                " "
            }
            Some(_) => " | ",
        }
    }
}

pub(crate) fn preview_line_for_title_items(
    items: &[TerminalTitleItem],
    preview_data: &StatusSurfacePreviewData,
) -> Option<Line<'static>> {
    if items.contains(&TerminalTitleItem::Spinner) {
        let preview = build_action_required_title_text(
            ACTION_REQUIRED_PREVIEW_PREFIX,
            items.iter().copied(),
            &[],
            |item| {
                item.preview_item()
                    .and_then(|preview_item| preview_data.value_for(preview_item))
                    .map(str::to_owned)
            },
        );
        return Some(Line::from(preview));
    }

    let mut previous = None;
    let preview = items
        .iter()
        .copied()
        .fold(String::new(), |mut preview, item| {
            let Some(value) = item
                .preview_item()
                .and_then(|preview_item| preview_data.value_for(preview_item))
            else {
                return preview;
            };
            preview.push_str(item.separator_from_previous(previous));
            preview.push_str(value);
            previous = Some(item);
            preview
        });
    if preview.is_empty() {
        None
    } else {
        Some(Line::from(preview))
    }
}

fn parse_terminal_title_items<T>(ids: impl Iterator<Item = T>) -> Option<Vec<TerminalTitleItem>>
where
    T: AsRef<str>,
{
    // Treat parsing as all-or-nothing so preview/confirm callbacks never emit
    // a partially interpreted ordering. Invalid ids are ignored when building
    // the picker, but once the user is interacting with the picker we only want
    // to persist or preview a fully valid selection.
    ids.map(|id| id.as_ref().parse::<TerminalTitleItem>())
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

/// Interactive view for configuring terminal-title items.
pub(crate) struct TerminalTitleSetupView {
    picker: MultiSelectPicker,
}

impl TerminalTitleSetupView {
    /// Creates the terminal-title picker, preserving the configured item order first.
    ///
    /// Unknown configured ids are skipped here instead of surfaced inline. The
    /// main TUI still warns about them when rendering the actual title, but the
    /// picker itself only exposes the selectable items it can meaningfully
    /// preview and persist.
    pub(crate) fn new(
        title_items: Option<&[String]>,
        preview_data: StatusSurfacePreviewData,
        app_event_tx: AppEventSender,
        list_keymap: ListKeymap,
    ) -> Self {
        let selected_items = title_items
            .into_iter()
            .flatten()
            .filter_map(|id| id.parse::<TerminalTitleItem>().ok())
            .unique()
            .collect_vec();
        let selected_set = selected_items
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let items = selected_items
            .into_iter()
            .map(|item| Self::title_select_item(item, /*enabled*/ true, &preview_data))
            .chain(
                TerminalTitleItem::iter()
                    .filter(|item| !selected_set.contains(item))
                    .map(|item| {
                        Self::title_select_item(item, /*enabled*/ false, &preview_data)
                    }),
            )
            .collect();

        Self {
            picker: MultiSelectPicker::builder(
                crate::i18n::message("status-title-configure", &[]),
                Some(crate::i18n::message(
                    "status-title-configure-description",
                    &[],
                )),
                app_event_tx,
            )
            .list_keymap(list_keymap)
            .items(items)
            .enable_ordering()
            .on_preview(move |items| {
                let items = parse_terminal_title_items(
                    items
                        .iter()
                        .filter(|item| item.enabled)
                        .map(|item| item.id.as_str()),
                )?;
                preview_line_for_title_items(&items, &preview_data)
            })
            .on_change(|items, app_event| {
                let Some(items) = parse_terminal_title_items(
                    items
                        .iter()
                        .filter(|item| item.enabled)
                        .map(|item| item.id.as_str()),
                ) else {
                    return;
                };
                app_event.send(AppEvent::TerminalTitleSetupPreview { items });
            })
            .on_confirm(|ids, app_event| {
                let Some(items) = parse_terminal_title_items(ids.iter().map(String::as_str)) else {
                    return;
                };
                app_event.send(AppEvent::TerminalTitleSetup { items });
            })
            .on_cancel(|app_event| {
                app_event.send(AppEvent::TerminalTitleSetupCancelled);
            })
            .build(),
        }
    }

    fn title_select_item(
        item: TerminalTitleItem,
        enabled: bool,
        preview_data: &StatusSurfacePreviewData,
    ) -> MultiSelectItem {
        let default_name = item.display_name();
        let default_description = item.description();
        let (name, description) = match item.preview_item() {
            Some(
                preview_item @ (StatusSurfacePreviewItem::FiveHourLimit
                | StatusSurfacePreviewItem::WeeklyLimit),
            ) => (
                preview_data.rate_limit_item_name(preview_item, &default_name),
                preview_data.rate_limit_item_description(preview_item, &default_description),
            ),
            _ => (default_name, default_description),
        };

        MultiSelectItem {
            id: item.to_string(),
            name,
            description: Some(description),
            enabled,
            orderable: true,
            section_break_after: false,
        }
    }
}

impl BottomPaneView for TerminalTitleSetupView {
    fn keymap_contexts(&self) -> crate::keymap::KeymapContextSet {
        crate::keymap::KeymapContextSet::new(crate::keymap::KeymapContext::List)
    }

    fn accepts_input_when_disconnected(&self) -> bool {
        true
    }

    fn handle_key_event(&mut self, key_event: crossterm::event::KeyEvent) {
        self.picker.handle_key_event(key_event);
    }

    fn is_complete(&self) -> bool {
        self.picker.complete
    }

    fn on_ctrl_c(&mut self) -> CancellationEvent {
        self.picker.close();
        CancellationEvent::Handled
    }
}

impl Renderable for TerminalTitleSetupView {
    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.picker.render(area, buf);
    }

    fn desired_height(&self, width: u16) -> u16 {
        self.picker.desired_height(width)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use insta::assert_snapshot;
    use pretty_assertions::assert_eq;
    use tokio::sync::mpsc::unbounded_channel;

    fn render_lines(view: &TerminalTitleSetupView, width: u16) -> String {
        let height = view.desired_height(width);
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        view.render(area, &mut buf);

        let lines: Vec<String> = (0..area.height)
            .map(|row| {
                let mut line = String::new();
                for col in 0..area.width {
                    let symbol = buf[(area.x + col, area.y + row)].symbol();
                    if symbol.is_empty() {
                        line.push(' ');
                    } else {
                        line.push_str(symbol);
                    }
                }
                line
            })
            .collect();
        lines.join("\n")
    }

    #[test]
    fn renders_title_setup_popup() {
        let (tx_raw, _rx) = unbounded_channel::<AppEvent>();
        let tx = AppEventSender::new(tx_raw);
        let selected = [
            "project-name".to_string(),
            "activity".to_string(),
            "run-state".to_string(),
            "thread-title".to_string(),
        ];
        let view = TerminalTitleSetupView::new(
            Some(&selected),
            StatusSurfacePreviewData::default(),
            tx,
            crate::keymap::RuntimeKeymap::defaults().list,
        );
        assert_snapshot!(
            "terminal_title_setup_basic",
            render_lines(&view, /*width*/ 84)
        );
    }

    #[test]
    fn parse_terminal_title_items_preserves_order() {
        let items = parse_terminal_title_items(
            ["project-name", "activity", "run-state", "thread-title"].into_iter(),
        );
        assert_eq!(
            items,
            Some(vec![
                TerminalTitleItem::Project,
                TerminalTitleItem::Spinner,
                TerminalTitleItem::Status,
                TerminalTitleItem::Thread,
            ])
        );
    }

    #[test]
    fn parse_terminal_title_items_rejects_invalid_ids() {
        let items = parse_terminal_title_items(["project", "not-a-title-item"].into_iter());
        assert_eq!(items, None);
    }

    #[test]
    fn activity_is_canonical_and_accepts_spinner_legacy_id() {
        assert_eq!(TerminalTitleItem::Spinner.to_string(), "activity");
        assert_eq!(
            "activity".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Spinner)
        );
        assert_eq!(
            "spinner".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Spinner)
        );
    }

    #[test]
    fn project_name_is_canonical_and_accepts_project_legacy_id() {
        assert_eq!(TerminalTitleItem::Project.to_string(), "project-name");
        assert_eq!(
            "project-name".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Project)
        );
        assert_eq!(
            "project".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Project)
        );
    }

    #[test]
    fn thread_title_is_canonical_and_accepts_thread_legacy_id() {
        assert_eq!(TerminalTitleItem::Thread.to_string(), "thread-title");
        assert_eq!(
            "thread-title".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Thread)
        );
        assert_eq!(
            "thread".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Thread)
        );
    }

    #[test]
    fn model_is_canonical_and_accepts_model_name_legacy_id() {
        assert_eq!(TerminalTitleItem::Model.to_string(), "model");
        assert_eq!(
            "model".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Model)
        );
        assert_eq!(
            "model-name".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Model)
        );
    }

    #[test]
    fn run_state_is_canonical_and_accepts_status_legacy_id() {
        assert_eq!(TerminalTitleItem::Status.to_string(), "run-state");
        assert_eq!(
            "run-state".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Status)
        );
        assert_eq!(
            "status".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Status)
        );
    }

    #[test]
    fn model_with_reasoning_has_distinct_id() {
        assert_eq!(
            TerminalTitleItem::ModelWithReasoning.to_string(),
            "model-with-reasoning"
        );
        assert_eq!(
            "model-with-reasoning".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::ModelWithReasoning)
        );
    }

    #[test]
    fn reasoning_is_selectable_id() {
        assert_eq!(TerminalTitleItem::Reasoning.to_string(), "reasoning");
        assert_eq!(
            "reasoning".parse::<TerminalTitleItem>(),
            Ok(TerminalTitleItem::Reasoning)
        );
    }

    #[test]
    fn thread_usage_items_are_independently_selectable() {
        assert_eq!(
            parse_terminal_title_items(["thread-credits", "estimated-thread-cost"].into_iter()),
            Some(vec![
                TerminalTitleItem::ThreadCredits,
                TerminalTitleItem::EstimatedThreadCost,
            ])
        );
    }

    #[test]
    fn parse_terminal_title_items_accepts_kebab_case_variants() {
        let items = parse_terminal_title_items(
            [
                "app-name",
                "context-remaining",
                "context-used",
                "five-hour-limit",
                "git-branch",
                "activity",
                "current-dir",
                "project-name",
                "model",
                "model-with-reasoning",
                "reasoning",
                "weekly-limit",
                "codex-version",
                "used-tokens",
                "total-input-tokens",
                "total-output-tokens",
                "thread-credits",
                "estimated-thread-cost",
                "session-id",
                "fast-mode",
            ]
            .into_iter(),
        );
        assert_eq!(
            items,
            Some(vec![
                TerminalTitleItem::AppName,
                TerminalTitleItem::ContextRemaining,
                TerminalTitleItem::ContextUsed,
                TerminalTitleItem::FiveHourLimit,
                TerminalTitleItem::GitBranch,
                TerminalTitleItem::Spinner,
                TerminalTitleItem::CurrentDir,
                TerminalTitleItem::Project,
                TerminalTitleItem::Model,
                TerminalTitleItem::ModelWithReasoning,
                TerminalTitleItem::Reasoning,
                TerminalTitleItem::WeeklyLimit,
                TerminalTitleItem::CodexVersion,
                TerminalTitleItem::UsedTokens,
                TerminalTitleItem::TotalInputTokens,
                TerminalTitleItem::TotalOutputTokens,
                TerminalTitleItem::ThreadCredits,
                TerminalTitleItem::EstimatedThreadCost,
                TerminalTitleItem::SessionId,
                TerminalTitleItem::FastMode,
            ])
        );
    }

    #[test]
    fn localized_title_setup_preserves_config_ids_and_preview_values() {
        use crate::i18n::Locale;
        use crate::i18n::with_locale;
        for (locale, heading, item_name, prefix) in [
            (
                Locale::English,
                "Configure Terminal Title",
                "thread-title",
                "[ ! ] Action Required",
            ),
            (
                Locale::Korean,
                "터미널 제목 설정",
                "대화 제목",
                "[ ! ] 조치 필요",
            ),
        ] {
            with_locale(locale, || {
                let title = "사용자🧪{name}제목";
                let preview_data = StatusSurfacePreviewData::from_iter([(
                    StatusSurfacePreviewItem::ThreadTitle,
                    title,
                )]);
                let item = TerminalTitleSetupView::title_select_item(
                    TerminalTitleItem::Thread,
                    true,
                    &preview_data,
                );
                assert_eq!(item.id, "thread-title");
                assert_eq!(item.name, item_name);
                for title_item in TerminalTitleItem::iter() {
                    let selectable =
                        TerminalTitleSetupView::title_select_item(title_item, false, &preview_data);
                    assert_eq!(selectable.id.parse::<TerminalTitleItem>(), Ok(title_item));
                    assert!(!selectable.name.is_empty());
                    assert!(!selectable.description.unwrap().is_empty());
                }
                let selected = ["thread-title".to_string(), "activity".to_string()];
                let preview = preview_line_for_title_items(
                    &[TerminalTitleItem::Thread, TerminalTitleItem::Spinner],
                    &preview_data,
                )
                .unwrap();
                assert_eq!(preview.to_string(), format!("{prefix} | {title}"));
                let normal =
                    preview_line_for_title_items(&[TerminalTitleItem::Thread], &preview_data)
                        .unwrap();
                assert_eq!(normal.to_string(), title);
                for width in [40, 60, 80, 120] {
                    let (tx_raw, mut rx) = unbounded_channel::<AppEvent>();
                    let mut view = TerminalTitleSetupView::new(
                        Some(&selected),
                        preview_data.clone(),
                        AppEventSender::new(tx_raw),
                        crate::keymap::RuntimeKeymap::defaults().list,
                    );
                    let rendered = render_lines(&view, width);
                    let compact: String =
                        rendered.chars().filter(|ch| !ch.is_whitespace()).collect();
                    let expected: String =
                        heading.chars().filter(|ch| !ch.is_whitespace()).collect();
                    assert!(
                        compact.contains(&expected),
                        "{locale:?}/{width}: {rendered}"
                    );
                    view.handle_key_event(crossterm::event::KeyCode::Enter.into());
                    let confirmed =
                        std::iter::from_fn(|| rx.try_recv().ok()).find_map(|event| match event {
                            AppEvent::TerminalTitleSetup { items } => Some(items),
                            _ => None,
                        });
                    assert_eq!(
                        confirmed,
                        Some(vec![TerminalTitleItem::Thread, TerminalTitleItem::Spinner])
                    );
                }
            });
        }
    }
}
