//! Root-scoped, event-driven agent footer. Expanded details never exceed three rows.
//! Only cached UI metadata is rendered; this surface never reads backend state.

use crate::i18n::message;
use crate::i18n::text;
use crate::line_truncation::truncate_line_with_ellipsis_if_overflow;
use crate::render::renderable::Renderable;
use codex_protocol::ThreadId;
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::widgets::Widget;
use std::cell::Cell;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FINISHED_REVEAL: Duration = Duration::from_secs(5);
const MAX_AGENT_ROWS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubagentStatusEntry {
    pub(crate) name: String,
    pub(crate) state: SubagentState,
    pub(crate) task: Option<String>,
    pub(crate) started_at: Option<i64>,
    pub(crate) duration_ms: Option<i64>,
    pub(crate) finished_at: Option<Instant>,
}

impl SubagentStatusEntry {
    fn visible(&self) -> bool {
        self.state.priority() != 4
            || self
                .finished_at
                .is_some_and(|at| at.elapsed() < FINISHED_REVEAL)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SubagentState {
    Running,
    NeedsApproval,
    NeedsInput,
    Completed,
    Failed,
    Stopped,
    Idle,
    Closed,
}

impl SubagentState {
    fn label(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::NeedsApproval => "approval needed",
            Self::NeedsInput => "input needed",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Idle => "idle",
            Self::Closed => "closed",
        }
    }

    fn priority(self) -> u8 {
        match self {
            Self::NeedsApproval | Self::NeedsInput => 0,
            Self::Failed => 1,
            Self::Running => 2,
            Self::Idle => 3,
            Self::Completed | Self::Stopped | Self::Closed => 4,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SubagentStatusSummary {
    counts: [usize; 8],
    pub(crate) root: Option<ThreadId>,
    entries: Vec<SubagentStatusEntry>,
}

impl SubagentStatusSummary {
    pub(crate) fn add(&mut self, state: SubagentState) {
        self.counts[state as usize] += 1;
    }

    pub(crate) fn add_entry(&mut self, entry: SubagentStatusEntry) {
        self.add(entry.state);
        self.entries.push(entry);
    }

    #[cfg(test)]
    pub(crate) fn counts(&self) -> [usize; 8] {
        self.counts
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.counts.iter().all(|count| *count == 0)
    }

    fn line(&self, width: u16) -> Line<'static> {
        let states = [
            SubagentState::NeedsApproval,
            SubagentState::NeedsInput,
            SubagentState::Failed,
            SubagentState::Running,
            SubagentState::Idle,
            SubagentState::Completed,
            SubagentState::Stopped,
            SubagentState::Closed,
        ];
        let mut spans = vec![format!("{}: ", text("Subagents")).dim()];
        for state in states {
            let count = self.counts[state as usize];
            if count == 0 {
                continue;
            }
            if spans.len() > 1 {
                spans.push(" · ".dim());
            }
            spans.push(format!("{} {count}", text(state.label())).dim());
        }
        const HINT: &str = " · /subagents";
        if usize::from(width) >= HINT.len() + 2 {
            let mut line = truncate_line_with_ellipsis_if_overflow(
                Line::from(spans),
                usize::from(width) - HINT.len(),
            );
            line.push_span(HINT.dim());
            line
        } else {
            truncate_line_with_ellipsis_if_overflow(Line::from("/subagents".dim()), width as usize)
        }
    }
}

impl Renderable for SubagentStatusSummary {
    fn desired_height(&self, width: u16) -> u16 {
        u16::from(!self.is_empty() && width > 0)
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if !self.is_empty() && !area.is_empty() {
            Paragraph::new(self.line(area.width)).render(Rect { height: 1, ..area }, buf);
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct SubagentStatusPanel {
    summary: SubagentStatusSummary,
    expanded: bool,
    offset: usize,
    // Last painted geometry is used only for hit-testing. Hidden surfaces reject mouse events.
    area: Cell<Rect>,
}

impl SubagentStatusPanel {
    pub(crate) fn is_empty(&self) -> bool {
        self.summary.is_empty()
            || (!self.summary.entries.is_empty()
                && !self
                    .summary
                    .entries
                    .iter()
                    .any(SubagentStatusEntry::visible))
    }

    fn visible_summary(&self) -> SubagentStatusSummary {
        if self.summary.entries.is_empty() {
            return self.summary.clone();
        }
        let mut summary = SubagentStatusSummary::default();
        for entry in self.summary.entries.iter().filter(|entry| entry.visible()) {
            summary.add(entry.state);
        }
        summary
    }

    pub(crate) fn update(&mut self, summary: SubagentStatusSummary) -> bool {
        if self.summary == summary {
            return false;
        }
        if self.summary.root != summary.root {
            self.expanded = false;
            self.offset = 0;
            self.area.set(Rect::default());
        }
        if [
            SubagentState::NeedsApproval,
            SubagentState::NeedsInput,
            SubagentState::Failed,
        ]
        .iter()
        .any(|state| summary.counts[*state as usize] > self.summary.counts[*state as usize])
        {
            self.offset = 0;
        }
        self.summary = summary;
        self.offset = self
            .offset
            .min(self.entries().len().saturating_sub(MAX_AGENT_ROWS));
        true
    }

    fn entries(&self) -> Vec<&SubagentStatusEntry> {
        let mut entries: Vec<_> = self
            .summary
            .entries
            .iter()
            .filter(|entry| entry.visible())
            .collect();
        // Stable sorting preserves the existing first-seen spawn order within each urgency group.
        entries.sort_by_key(|entry| entry.state.priority());
        entries
    }

    pub(crate) fn needs_tick(&self) -> bool {
        self.summary.entries.iter().any(|entry| {
            (self.expanded && entry.state.priority() < 4 && entry.started_at.is_some())
                || entry
                    .finished_at
                    .is_some_and(|at| at.elapsed() < FINISHED_REVEAL)
        })
    }

    fn scroll(&mut self, down: bool) {
        let max = self.entries().len().saturating_sub(MAX_AGENT_ROWS);
        self.offset = if down {
            self.offset.saturating_add(1).min(max)
        } else {
            self.offset.saturating_sub(1)
        };
    }

    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> bool {
        if self.is_empty() || !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return false;
        }
        if key.modifiers == KeyModifiers::ALT
            && matches!(key.code, KeyCode::Char('s') | KeyCode::Char('S'))
            || key.modifiers == (KeyModifiers::ALT | KeyModifiers::SHIFT)
                && key.code == KeyCode::Char('S')
        {
            if key.kind == KeyEventKind::Press {
                self.expanded = !self.expanded;
            }
            return true;
        }
        if self.expanded
            && key.modifiers == KeyModifiers::ALT
            && matches!(key.code, KeyCode::PageUp | KeyCode::PageDown)
        {
            self.scroll(key.code == KeyCode::PageDown);
            return true;
        }
        false
    }

    pub(crate) fn contains_mouse(&self, event: MouseEvent) -> bool {
        !self.is_empty() && self.area.get().contains((event.column, event.row).into())
    }

    pub(crate) fn handle_mouse(&mut self, event: MouseEvent) -> bool {
        if !self.contains_mouse(event) {
            return false;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if event.row == self.area.get().y => {
                self.expanded = !self.expanded
            }
            MouseEventKind::ScrollDown if self.expanded => self.scroll(true),
            MouseEventKind::ScrollUp if self.expanded => self.scroll(false),
            _ => return false,
        }
        true
    }

    pub(crate) fn view(&self, allow_expanded: bool) -> SubagentStatusView<'_> {
        SubagentStatusView {
            panel: self,
            allow_expanded,
        }
    }
}

pub(crate) struct SubagentStatusView<'a> {
    panel: &'a SubagentStatusPanel,
    allow_expanded: bool,
}

impl Renderable for SubagentStatusView<'_> {
    fn desired_height(&self, width: u16) -> u16 {
        if width == 0 || self.panel.is_empty() {
            return 0;
        }
        1 + if self.allow_expanded && self.panel.expanded && width >= 48 {
            self.panel.entries().len().min(MAX_AGENT_ROWS) as u16
        } else {
            0
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        self.panel.area.set(area);
        if area.is_empty() || self.panel.is_empty() {
            return;
        }
        let entries = self.panel.entries();
        let offset = self
            .panel
            .offset
            .min(entries.len().saturating_sub(MAX_AGENT_ROWS));
        let summary = self.panel.visible_summary();
        let mut line = summary.line(area.width);
        if area.width >= 40 {
            let mut hint = format!(
                " · {}",
                message(
                    if self.panel.expanded {
                        "r36-agents-collapse"
                    } else {
                        "r36-agents-expand"
                    },
                    &[]
                )
            );
            if self.panel.expanded && entries.len() > MAX_AGENT_ROWS {
                hint.push_str(&format!(" [{}/{}]", offset + 1, entries.len()));
                let scroll = format!(" · {}", message("r36-agents-scroll", &[]));
                if Line::from(format!("{hint}{scroll}")).width() + 28 < usize::from(area.width) {
                    hint.push_str(&scroll);
                }
            }
            let hint_width = Line::from(hint.as_str()).width();
            if hint_width + 14 < usize::from(area.width) {
                line = summary.line(area.width.saturating_sub(hint_width as u16));
                line.push_span(hint.dim());
            }
        }
        let mut lines = vec![line];
        if self.allow_expanded && self.panel.expanded && area.width >= 48 {
            for entry in entries.iter().skip(offset).take(MAX_AGENT_ROWS) {
                let name = crate::text_formatting::truncate_text(&entry.name, 24);
                let mut detail = format!("  {name} · {}", text(entry.state.label()));
                detail.push_str(&format!(
                    " · {}",
                    entry
                        .task
                        .clone()
                        .unwrap_or_else(|| message("r36-agents-no-activity", &[]))
                ));
                let elapsed = entry
                    .duration_ms
                    .filter(|ms| *ms >= 0)
                    .map(|ms| ms as u64 / 1000)
                    .or_else(|| {
                        if entry.state.priority() == 4 || entry.state == SubagentState::Failed {
                            return None;
                        }
                        let started = u64::try_from(entry.started_at?).ok()?;
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .ok()?
                            .as_secs()
                            .checked_sub(started)
                    });
                let suffix = elapsed
                    .map(|seconds| {
                        format!(
                            " · {}",
                            crate::status_indicator_widget::fmt_elapsed_compact(seconds)
                        )
                    })
                    .unwrap_or_default();
                let mut line = truncate_line_with_ellipsis_if_overflow(
                    Line::from(detail.dim()),
                    usize::from(area.width).saturating_sub(suffix.len()),
                );
                line.push_span(suffix.dim());
                lines.push(line);
            }
        }
        Paragraph::new(lines).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary_with_agents(count: usize, root: ThreadId) -> SubagentStatusSummary {
        let mut summary = SubagentStatusSummary {
            root: Some(root),
            ..Default::default()
        };
        for index in 0..count {
            summary.add_entry(SubagentStatusEntry {
                name: format!("worker-{index:03}"),
                state: SubagentState::Running,
                task: Some("Checking focused tests".to_string()),
                started_at: None,
                duration_ms: None,
                finished_at: None,
            });
        }
        summary
    }

    fn render_panel(
        panel: &SubagentStatusPanel,
        width: u16,
        allow_expanded: bool,
    ) -> (u16, String) {
        let view = panel.view(allow_expanded);
        let height = view.desired_height(width);
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        view.render(area, &mut buffer);
        (
            height,
            buffer.content().iter().map(|cell| cell.symbol()).collect(),
        )
    }

    #[test]
    fn bounded_agent_footer_expands_scrolls_and_preserves_narrow_budget() {
        for count in [15, 100] {
            let mut panel = SubagentStatusPanel::default();
            panel.update(summary_with_agents(count, ThreadId::new()));
            assert_eq!(render_panel(&panel, 100, true).0, 1);
            assert!(panel.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT)));
            let (height, text) = render_panel(&panel, 100, true);
            assert_eq!(height, 4);
            assert!(text.contains("worker-000"));
            assert!(text.contains("worker-002"));
            assert!(!text.contains("worker-003"));
            assert!(text.contains("/subagents"));
            assert!(text.contains("alt+s"));
            for _ in 0..count + 1 {
                panel.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::ALT));
            }
            let (_, scrolled) = render_panel(&panel, 100, true);
            assert!(scrolled.contains(&format!("worker-{:03}", count - 1)));
            assert!(!scrolled.contains("worker-000"));
            panel.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::ALT));
            assert!(
                render_panel(&panel, 100, true)
                    .1
                    .contains(&format!("worker-{:03}", count - 4))
            );
            for width in [0, 1, 24, 47] {
                assert_eq!(render_panel(&panel, width, true).0, u16::from(width > 0));
            }
            assert_eq!(render_panel(&panel, 100, false).0, 1);
            assert!(!panel.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
            assert!(
                !panel.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::ALT)),
                "existing queue-edit binding remains available"
            );
            assert!(panel.handle_key(KeyEvent::new(
                KeyCode::Char('S'),
                KeyModifiers::ALT | KeyModifiers::SHIFT
            )));
            assert_eq!(render_panel(&panel, 100, true).0, 1);
        }
    }

    #[test]
    fn agent_footer_prioritizes_attention_and_expires_finished_rows_without_invented_time() {
        let mut summary = summary_with_agents(15, ThreadId::new());
        for (state, name, finished_at) in [
            (SubagentState::Failed, "failed-agent", None),
            (SubagentState::NeedsApproval, "approval-agent", None),
            (SubagentState::NeedsInput, "input-agent", None),
            (
                SubagentState::Completed,
                "old-completion",
                Some(Instant::now() - Duration::from_secs(6)),
            ),
        ] {
            summary.add_entry(SubagentStatusEntry {
                name: name.to_string(),
                state,
                task: None,
                started_at: None,
                duration_ms: None,
                finished_at,
            });
        }
        let mut panel = SubagentStatusPanel::default();
        panel.update(summary);
        panel.expanded = true;
        let (_, text) = render_panel(&panel, 120, true);
        let approval = text.find("approval-agent").unwrap();
        let input = text.find("input-agent").unwrap();
        let failed = text.find("failed-agent").unwrap();
        assert!(approval < input && input < failed);
        assert!(!text.contains("worker-000"));
        assert!(!text.contains("old-completion"));
        assert!(!text.contains("0s"));
        assert!(
            !panel.needs_tick(),
            "missing backend timestamps must not create an elapsed timer"
        );
    }

    #[test]
    fn agent_footer_root_switch_resets_expansion_scroll_and_mouse_geometry() {
        let mut panel = SubagentStatusPanel::default();
        let root = ThreadId::new();
        panel.update(summary_with_agents(15, root));
        panel.expanded = true;
        panel.scroll(true);
        render_panel(&panel, 100, true);
        assert_eq!(panel.offset, 1);
        panel.update(summary_with_agents(15, root));
        assert!(panel.expanded);
        assert_eq!(panel.offset, 1);
        panel.update(summary_with_agents(15, ThreadId::new()));
        assert!(!panel.expanded);
        assert_eq!(panel.offset, 0);
        assert!(panel.area.get().is_empty());
        panel.update(SubagentStatusSummary::default());
        assert_eq!(render_panel(&panel, 100, true).0, 0);
    }

    #[test]
    fn agent_footer_mouse_only_toggles_summary_and_scrolls_inside_painted_area() {
        let mut panel = SubagentStatusPanel::default();
        panel.update(summary_with_agents(15, ThreadId::new()));
        render_panel(&panel, 100, true);
        let mouse = |kind, column, row| MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        };
        assert!(!panel.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 0, 5)));
        assert!(panel.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 0, 0)));
        assert!(panel.expanded);
        render_panel(&panel, 100, true);
        assert!(panel.handle_mouse(mouse(MouseEventKind::ScrollDown, 5, 2)));
        assert_eq!(panel.offset, 1);
        assert!(!panel.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 2)));
        assert!(panel.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 0)));
        assert!(!panel.expanded);
    }

    #[test]
    fn finished_agents_leave_footer_after_reveal_and_backend_duration_stays_visible() {
        let mut panel = SubagentStatusPanel::default();
        let mut summary = SubagentStatusSummary {
            root: Some(ThreadId::new()),
            ..Default::default()
        };
        summary.add_entry(SubagentStatusEntry {
            name: "finished-agent".to_string(),
            state: SubagentState::Completed,
            task: Some("Done".to_string()),
            started_at: None,
            duration_ms: Some(61_000),
            finished_at: Some(Instant::now()),
        });
        panel.update(summary.clone());
        assert!(
            panel.needs_tick(),
            "collapsed completions must expire without backend events"
        );
        panel.expanded = true;
        assert!(render_panel(&panel, 80, true).1.contains("1m 01s"));
        summary.entries[0].finished_at = Some(Instant::now() - Duration::from_secs(6));
        panel.update(summary);
        assert_eq!(render_panel(&panel, 80, true).0, 0);
        assert!(!panel.needs_tick());
    }

    #[test]
    fn running_agent_footer_elapsed_advances_with_real_time() {
        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut summary = summary_with_agents(1, ThreadId::new());
        summary.entries[0].started_at = Some(i64::try_from(started_at).unwrap());
        let mut panel = SubagentStatusPanel::default();
        panel.update(summary.clone());
        assert!(!panel.needs_tick());
        assert!(panel.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT)));
        assert!(panel.needs_tick());

        let mut elapsed = Vec::new();
        for sample in 0..2 {
            if sample > 0 {
                std::thread::sleep(Duration::from_millis(1_100));
            }
            let before = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - started_at;
            let (height, text) = render_panel(&panel, 100, true);
            let after = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - started_at;
            assert_eq!(height, 2);
            assert!(text.contains("worker-000"));
            assert!(text.contains("Checking focused tests"));
            let seconds: u64 = text
                .rsplit(" · ")
                .next()
                .unwrap()
                .split_whitespace()
                .map(|part| {
                    let (value, unit) = part.split_at(part.len() - 1);
                    value.parse::<u64>().unwrap()
                        * match unit {
                            "h" => 3_600,
                            "m" => 60,
                            "s" => 1,
                            _ => panic!("unexpected elapsed unit: {unit}"),
                        }
                })
                .sum();
            assert!((before..=after).contains(&seconds));
            elapsed.push(seconds);
            assert!(panel.needs_tick());
        }
        assert!(elapsed[1] > elapsed[0]);
        assert_eq!(panel.summary, summary);
    }

    #[test]
    fn subagent_status_row_bounds_width_and_keeps_command_hint() {
        let mut summary = SubagentStatusSummary::default();
        for state in [
            SubagentState::Running,
            SubagentState::NeedsApproval,
            SubagentState::NeedsInput,
            SubagentState::Completed,
            SubagentState::Failed,
            SubagentState::Stopped,
            SubagentState::Idle,
            SubagentState::Closed,
        ] {
            summary.add(state);
        }
        crate::i18n::with_locale(crate::i18n::Locale::English, || {
            assert_eq!(
                summary.line(240).to_string(),
                "Subagents: approval needed 1 · input needed 1 · failed 1 · running 1 · idle 1 · completed 1 · stopped 1 · closed 1 · /subagents"
            );
        });
        for language in [crate::i18n::Locale::English, crate::i18n::Locale::Korean] {
            crate::i18n::with_locale(language, || {
                for width in [0, 1, 8, 14, 24, 80, 200] {
                    let line = summary.line(width);
                    assert!(line.width() <= usize::from(width));
                    if width >= 14 {
                        assert!(line.to_string().ends_with("/subagents"));
                    }
                    assert_eq!(summary.desired_height(width), u16::from(width > 0));
                }
            });
        }
    }
}
