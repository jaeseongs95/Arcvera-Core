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
use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const FINISHED_REVEAL: Duration = Duration::from_secs(5);
const MAX_AGENT_ROWS: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubagentStatusEntry {
    pub(crate) thread_id: ThreadId,
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
    // Navigation metadata is separate from descendant state/counts.
    pub(crate) show_main: bool,
    pub(crate) current_thread_id: Option<ThreadId>,
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

    pub(crate) fn contains_visible_thread(&self, thread_id: ThreadId) -> bool {
        self.root == Some(thread_id)
            || self
                .entries
                .iter()
                .any(|entry| entry.thread_id == thread_id && entry.visible())
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubagentHover {
    Summary,
    Agent(ThreadId),
}

#[derive(Debug, Default)]
pub(crate) struct SubagentStatusPanel {
    summary: SubagentStatusSummary,
    expanded: bool,
    offset: usize,
    focused: Cell<bool>,
    selected: Option<ThreadId>,
    selection: Option<(ThreadId, ThreadId)>,
    painted_rows: RefCell<Vec<ThreadId>>,
    hovered: Cell<Option<SubagentHover>>,
    // Last painted geometry is used only for hit-testing. Hidden surfaces reject mouse events.
    area: Cell<Rect>,
}

impl SubagentStatusPanel {
    pub(crate) fn is_painted(&self) -> bool {
        !self.is_empty() && !self.area.get().is_empty()
    }

    pub(crate) fn owns_selection_key(&self, key: KeyEvent) -> bool {
        self.focused.get()
            && self.expanded
            && !self.is_empty()
            && key.modifiers == KeyModifiers::NONE
            && matches!(
                key.code,
                KeyCode::Up | KeyCode::Down | KeyCode::Enter | KeyCode::Esc
            )
    }

    pub(crate) fn clear_painted(&self) {
        self.area.set(Rect::default());
        self.painted_rows.borrow_mut().clear();
        self.clear_hover();
    }

    pub(crate) fn clear_hover(&self) -> bool {
        self.hovered.replace(None).is_some()
    }

    /// Only a change of actionable row needs a repaint; moving within it is a no-op.
    pub(crate) fn handle_hover(&self, event: MouseEvent) -> bool {
        if event.kind != MouseEventKind::Moved {
            return false;
        }
        let area = self.area.get();
        let next = if !self.contains_mouse(event) {
            None
        } else if event.row == area.y {
            Some(SubagentHover::Summary)
        } else {
            self.painted_rows
                .borrow()
                .get(usize::from(event.row - area.y - 1))
                .copied()
                .filter(|id| self.summary.contains_visible_thread(*id))
                .map(SubagentHover::Agent)
        };
        self.hovered.replace(next) != next
    }

    pub(crate) fn take_selection(&mut self) -> Option<(ThreadId, ThreadId)> {
        self.selection.take()
    }

    fn select(&mut self, thread_id: ThreadId) {
        if self.summary.contains_visible_thread(thread_id)
            && self.painted_rows.borrow().contains(&thread_id)
            && let Some(root) = self.summary.root
        {
            self.selection = Some((root, thread_id));
            // Keep Enter owned until the thread switch replaces the pane (or the user exits).
            // A held key cannot submit the draft while selection is queued asynchronously.
            self.focused.set(true);
            self.selected = None;
        }
    }
    pub(crate) fn is_empty(&self) -> bool {
        !self.has_main()
            && (self.summary.is_empty()
                || (!self.summary.entries.is_empty()
                    && !self
                        .summary
                        .entries
                        .iter()
                        .any(SubagentStatusEntry::visible)))
    }

    fn has_main(&self) -> bool {
        self.summary.show_main && self.summary.root.is_some()
    }

    fn child_rows(&self) -> usize {
        MAX_AGENT_ROWS - usize::from(self.has_main())
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
        self.clear_painted();
        self.selection = None;
        if self.summary.root != summary.root {
            self.expanded = false;
            self.offset = 0;
            self.area.set(Rect::default());
            self.focused.set(false);
            self.selected = None;
        }
        if self
            .selected
            .is_some_and(|id| !summary.contains_visible_thread(id))
        {
            self.selected = None;
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
            .min(self.entries().len().saturating_sub(self.child_rows()));
        if self.focused.get()
            && let Some(index) = self.selected.and_then(|id| {
                self.entries()
                    .iter()
                    .position(|entry| entry.thread_id == id)
            })
        {
            if index < self.offset {
                self.offset = index;
            } else if index >= self.offset + self.child_rows() {
                self.offset = index + 1 - self.child_rows();
            }
        }
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
        let max = self.entries().len().saturating_sub(self.child_rows());
        self.offset = if down {
            self.offset.saturating_add(1).min(max)
        } else {
            self.offset.saturating_sub(1)
        };
        if self.focused.get() {
            self.selected = self.entries().get(self.offset).map(|entry| entry.thread_id);
        }
        self.painted_rows.borrow_mut().clear();
        self.clear_hover();
    }

    fn toggle(&mut self) {
        self.expanded = !self.expanded;
        self.focused.set(self.expanded);
        self.selected = self
            .expanded
            .then(|| {
                self.entries()
                    .get(self.offset)
                    .map(|entry| entry.thread_id)
                    .or_else(|| self.summary.root.filter(|_| self.has_main()))
            })
            .flatten();
        self.painted_rows.borrow_mut().clear();
        self.clear_hover();
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
                self.toggle();
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
        if self.expanded && self.focused.get() && key.modifiers == KeyModifiers::NONE {
            match key.code {
                KeyCode::Esc => {
                    self.focused.set(false);
                    return true;
                }
                KeyCode::Enter => {
                    if key.kind == KeyEventKind::Press
                        && let Some(id) = self.selected
                    {
                        self.select(id);
                    }
                    return true;
                }
                KeyCode::Up | KeyCode::Down => {
                    let entries = self.entries();
                    let index = self
                        .selected
                        .and_then(|id| entries.iter().position(|entry| entry.thread_id == id));
                    if self.has_main()
                        && key.code == KeyCode::Up
                        && (index == Some(0) || self.selected == self.summary.root)
                    {
                        self.selected = self.summary.root;
                        self.painted_rows.borrow_mut().clear();
                        self.clear_hover();
                        return true;
                    }
                    let next = match (key.code, index) {
                        (KeyCode::Up, Some(index)) => index.saturating_sub(1),
                        (KeyCode::Down, Some(index)) => {
                            index.saturating_add(1).min(entries.len().saturating_sub(1))
                        }
                        _ => 0,
                    };
                    self.selected = entries
                        .get(next)
                        .map(|entry| entry.thread_id)
                        .or_else(|| self.summary.root.filter(|_| self.has_main()));
                    if next < self.offset {
                        self.offset = next;
                    } else if next >= self.offset + self.child_rows() {
                        self.offset = next + 1 - self.child_rows();
                    }
                    self.painted_rows.borrow_mut().clear();
                    self.clear_hover();
                    return true;
                }
                _ => self.focused.set(false),
            }
        }
        // Modified editing keys and global shortcuts keep their normal owner. Returning to
        // the composer must also remove the footer's Enter ownership.
        self.focused.set(false);
        false
    }

    pub(crate) fn contains_mouse(&self, event: MouseEvent) -> bool {
        let area = self.area.get();
        !self.is_empty()
            && area.contains((event.column, event.row).into())
            && (!matches!(
                event.kind,
                MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Moved
            ) || event.row == area.y
                || self
                    .painted_rows
                    .borrow()
                    .get(usize::from(event.row - area.y - 1))
                    .is_some())
    }

    pub(crate) fn handle_mouse(&mut self, event: MouseEvent) -> bool {
        if !self.contains_mouse(event) {
            return false;
        }
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) if event.row == self.area.get().y => {
                self.toggle();
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let index = usize::from(event.row - self.area.get().y - 1);
                let target = self.painted_rows.borrow().get(index).copied();
                let Some(target) = target else {
                    return false;
                };
                self.select(target);
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
        1 + if self.allow_expanded && self.panel.expanded && (width >= 48 || self.panel.has_main())
        {
            u16::from(self.panel.has_main())
                + self.panel.entries().len().min(self.panel.child_rows()) as u16
        } else {
            0
        }
    }

    fn render(&self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() || self.panel.is_empty() {
            self.panel.clear_painted();
            self.panel.focused.set(false);
            return;
        }
        let entries = self.panel.entries();
        let show_details = self.allow_expanded
            && self.panel.expanded
            && (area.width >= 48 || self.panel.has_main())
            && usize::from(area.height)
                > entries.len().min(self.panel.child_rows()) + usize::from(self.panel.has_main());
        if !show_details {
            self.panel.focused.set(false);
        }
        let offset = self
            .panel
            .offset
            .min(entries.len().saturating_sub(self.panel.child_rows()));
        let mut painted_rows: Vec<_> = Vec::new();
        if show_details && self.panel.has_main() {
            painted_rows.extend(self.panel.summary.root);
        }
        if show_details {
            painted_rows.extend(
                entries
                    .iter()
                    .skip(offset)
                    .take(self.panel.child_rows())
                    .map(|entry| entry.thread_id),
            );
        }
        if self.panel.area.get() != area || *self.panel.painted_rows.borrow() != painted_rows {
            self.panel.clear_hover();
        }
        self.panel.area.set(area);
        *self.panel.painted_rows.borrow_mut() = painted_rows;
        let hovered = self.panel.hovered.get();
        let summary = self.panel.visible_summary();
        let mut line = summary.line(area.width);
        let click_hint = format!(" · {}", message("r37-agents-click", &[]));
        let mut hint = click_hint.clone();
        if area.width >= 40 {
            let keyboard_hint = format!(
                " · {}",
                message(
                    if self.panel.expanded {
                        "r36-agents-collapse"
                    } else {
                        "r36-agents-expand"
                    },
                    &[],
                )
            );
            hint = keyboard_hint;
            if show_details && self.panel.focused.get() {
                let selection = format!(" · {}", message("r37-agents-select", &[]));
                if Line::from(format!("{hint}{selection}")).width() + 14 < usize::from(area.width) {
                    hint.push_str(&selection);
                }
            }
            if Line::from(format!("{hint}{click_hint}")).width() + 14 < usize::from(area.width) {
                hint.push_str(&click_hint);
            }
            if self.panel.expanded && entries.len() > self.panel.child_rows() {
                let position = format!(" [{}/{}]", offset + 1, entries.len());
                if Line::from(format!("{hint}{position}")).width() + 14 < usize::from(area.width) {
                    hint.push_str(&position);
                }
                let scroll = format!(" · {}", message("r36-agents-scroll", &[]));
                if Line::from(format!("{hint}{scroll}")).width() + 28 < usize::from(area.width) {
                    hint.push_str(&scroll);
                }
            }
        }
        let hint_width = Line::from(hint.as_str()).width();
        if hint_width + 14 < usize::from(area.width) {
            line = summary.line(area.width.saturating_sub(hint_width as u16));
            line.push_span(hint.dim());
        }
        if hovered == Some(SubagentHover::Summary)
            && let Some(span) = line.spans.first_mut()
            && let Some(first) = span.content.chars().next()
        {
            span.content = span.content[first.len_utf8()..].to_owned().into();
            line.spans.insert(0, first.to_string().dim().underlined());
        }
        let mut lines = vec![line];
        if show_details && self.panel.has_main() {
            let root = self.panel.summary.root.unwrap();
            let current = self.panel.summary.current_thread_id == Some(root);
            let selected = self.panel.focused.get() && self.panel.selected == Some(root);
            let marker = if selected {
                "›"
            } else if current {
                "●"
            } else {
                " "
            };
            let marker = format!("{marker} ").dim();
            let marker = if hovered == Some(SubagentHover::Agent(root)) {
                marker.underlined()
            } else {
                marker
            };
            lines.push(truncate_line_with_ellipsis_if_overflow(
                Line::from(vec![marker, message("r38-agents-main", &[]).dim()]),
                usize::from(area.width),
            ));
        }
        if show_details {
            for entry in entries.iter().skip(offset).take(self.panel.child_rows()) {
                let name = crate::text_formatting::truncate_text(&entry.name, 24);
                let selected =
                    self.panel.focused.get() && self.panel.selected == Some(entry.thread_id);
                let hovered = hovered == Some(SubagentHover::Agent(entry.thread_id));
                let marker = if selected {
                    "›"
                } else if self.panel.summary.current_thread_id == Some(entry.thread_id) {
                    "●"
                } else {
                    " "
                };
                let mut detail = format!("{name} · {}", text(entry.state.label()));
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
                let mut suffix = String::new();
                suffix.push_str(
                    &elapsed
                        .map(|seconds| {
                            format!(
                                " · {}",
                                crate::status_indicator_widget::fmt_elapsed_compact(seconds)
                            )
                        })
                        .unwrap_or_default(),
                );
                let suffix_width = Line::from(suffix.as_str()).width();
                let marker = if hovered {
                    format!("{marker} ").dim().underlined()
                } else {
                    format!("{marker} ").dim()
                };
                let mut line = truncate_line_with_ellipsis_if_overflow(
                    Line::from(vec![marker, detail.dim()]),
                    usize::from(area.width).saturating_sub(suffix_width),
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
                thread_id: ThreadId::new(),
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

    fn render_panel_at(panel: &SubagentStatusPanel, area: Rect) -> String {
        let mut buffer = Buffer::empty(area);
        panel.view(true).render(area, &mut buffer);
        buffer.content().iter().map(|cell| cell.symbol()).collect()
    }

    fn click(column: u16, row: u16) -> MouseEvent {
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }
    }

    // Skip the padding cells occupied by wide glyphs when reading a painted row.
    fn hover_row_text(buffer: &Buffer, row: u16) -> String {
        let area = buffer.area();
        let mut column = area.x;
        let mut text = String::new();
        while column < area.right() {
            let symbol = buffer[(column, row)].symbol();
            text.push_str(symbol);
            column += Line::from(symbol).width().max(1) as u16;
        }
        text
    }

    #[test]
    fn footer_hover_is_subtle_and_keeps_text_geometry_and_click_behavior() {
        use ratatui::style::Modifier;
        for locale in [crate::i18n::Locale::English, crate::i18n::Locale::Korean] {
            crate::i18n::with_locale(locale, || {
                let root = ThreadId::new();
                let summary = summary_with_agents(15, root);
                let child = summary.entries[0].thread_id;
                let mut panel = SubagentStatusPanel::default();
                panel.update(summary);
                for expanded in [false, true] {
                    if expanded {
                        assert!(panel.handle_mouse(click(9, 11)));
                        assert!(panel.handle_key(KeyCode::Esc.into()));
                    }
                    let area = Rect::new(7, 11, 120, if expanded { 4 } else { 1 });
                    let mut before = Buffer::empty(area);
                    panel.view(true).render(area, &mut before);
                    for row in 11..area.bottom() {
                        let moved = MouseEvent {
                            kind: MouseEventKind::Moved,
                            ..click(9, row)
                        };
                        assert!(panel.handle_hover(moved));
                        assert!(!panel.handle_hover(MouseEvent {
                            column: 10,
                            ..moved
                        }));
                        let mut hovered = Buffer::empty(area);
                        panel.view(true).render(area, &mut hovered);
                        assert_eq!(hover_row_text(&hovered, row), hover_row_text(&before, row));
                        assert!(
                            !hovered
                                .content()
                                .iter()
                                .any(|cell| cell.modifier.contains(Modifier::REVERSED))
                        );
                        let underlined = hovered
                            .content()
                            .iter()
                            .filter(|cell| cell.modifier.contains(Modifier::UNDERLINED))
                            .count();
                        assert!(
                            (1..=2).contains(&underlined),
                            "only the small action marker reacts"
                        );
                        assert_eq!(panel.take_selection(), None);
                        assert!(!panel.owns_selection_key(KeyCode::Enter.into()));
                    }
                }
                assert!(panel.handle_mouse(click(9, 12)));
                assert_eq!(panel.take_selection(), Some((root, child)));
            });
        }
    }

    #[test]
    fn main_agent_is_only_in_the_expanded_selection_list() {
        for count in [0, 15, 100] {
            let root = ThreadId::new();
            let mut summary = summary_with_agents(count, root);
            summary.show_main = true;
            summary.current_thread_id = Some(ThreadId::new());
            assert_eq!(summary.counts()[SubagentState::Running as usize], count);
            let mut panel = SubagentStatusPanel::default();
            panel.update(summary.clone());
            for expanded in [false, true] {
                panel.expanded = expanded;
                for _ in 0..count + 1 {
                    panel.scroll(true);
                }
                for (width, allowed) in
                    [(1, true), (24, true), (47, true), (120, true), (120, false)]
                {
                    let (height, text) = render_panel(&panel, width, allowed);
                    if expanded && allowed {
                        assert!((2..=4).contains(&height));
                        if width >= 24 {
                            assert!(text.contains(&message("r38-agents-main", &[])));
                        }
                        assert_eq!(panel.painted_rows.borrow().first(), Some(&root));
                        assert!(panel.painted_rows.borrow().len() <= MAX_AGENT_ROWS);
                        assert!(panel.handle_mouse(click(0, 1)));
                        assert_eq!(panel.take_selection(), Some((root, root)));
                    } else {
                        assert_eq!(height, 1);
                        assert!(!text.contains(&message("r38-agents-main", &[])));
                        assert!(panel.painted_rows.borrow().is_empty());
                        assert!(!panel.handle_mouse(click(0, 1)));
                        assert_eq!(panel.take_selection(), None);
                    }
                }
            }
            // A clipped terminal keeps its summary as the list toggle.
            render_panel_at(&panel, Rect::new(7, 11, 24, 1));
            assert!(panel.painted_rows.borrow().is_empty());
            assert!(panel.handle_mouse(click(7, 11)));
            assert_eq!(panel.take_selection(), None);
            summary.current_thread_id = Some(root);
            panel.update(summary);
            panel.expanded = true;
            let (_, text) = render_panel(&panel, 120, true);
            assert!(text.contains("●"), "current main is marked in the list");
            panel.update(SubagentStatusSummary::default());
            assert!(!panel.handle_mouse(click(0, 1)));
            assert_eq!(panel.take_selection(), None);
        }
    }

    #[test]
    fn main_only_expanded_list_keeps_keyboard_selection_and_enter_ownership() {
        let root = ThreadId::new();
        let mut summary = summary_with_agents(0, root);
        summary.show_main = true;
        let mut panel = SubagentStatusPanel::default();
        panel.update(summary);
        render_panel(&panel, 24, true);
        assert!(panel.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT)));
        render_panel(&panel, 24, true);
        for key in [KeyCode::Down, KeyCode::Up, KeyCode::Enter] {
            assert!(panel.handle_key(key.into()));
            render_panel(&panel, 24, true);
        }
        assert_eq!(panel.take_selection(), Some((root, root)));
        assert!(panel.owns_selection_key(KeyCode::Enter.into()));
        assert!(panel.handle_key(KeyCode::Enter.into()));
        assert_eq!(panel.take_selection(), None);
        assert!(panel.handle_key(KeyCode::Esc.into()));
        assert!(!panel.owns_selection_key(KeyCode::Enter.into()));
    }

    #[test]
    fn footer_hover_invalidates_scroll_reorder_resize_removed_and_root_rows() {
        let mut summary = summary_with_agents(15, ThreadId::new());
        let mut panel = SubagentStatusPanel::default();
        panel.update(summary.clone());
        panel.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        let area = Rect::new(7, 11, 120, 4);
        let moved = MouseEvent {
            kind: MouseEventKind::Moved,
            ..click(9, 12)
        };
        render_panel_at(&panel, area);
        assert!(panel.handle_hover(moved));
        assert!(panel.hovered.get().is_some());
        panel.scroll(true);
        assert!(panel.hovered.get().is_none());
        assert!(
            !panel.handle_hover(moved),
            "unpainted scrolled rows cannot be hovered"
        );
        render_panel_at(&panel, area);
        assert!(panel.handle_hover(moved));
        summary.entries[14].state = SubagentState::NeedsInput;
        panel.update(summary.clone());
        assert!(panel.hovered.get().is_none());
        assert!(!panel.handle_hover(moved));
        render_panel_at(&panel, area);
        assert!(panel.handle_hover(moved));
        render_panel_at(
            &panel,
            Rect {
                width: 47,
                height: 1,
                ..area
            },
        );
        assert!(panel.hovered.get().is_none());
        assert!(!panel.handle_hover(moved));
        render_panel_at(&panel, area);
        assert!(panel.handle_hover(moved));
        let hovered = panel.painted_rows.borrow()[0];
        summary.entries.retain(|entry| entry.thread_id != hovered);
        panel.update(summary);
        assert!(panel.hovered.get().is_none());
        assert!(!panel.handle_hover(moved));
        render_panel_at(&panel, area);
        assert!(panel.handle_hover(moved));
        panel.update(summary_with_agents(15, ThreadId::new()));
        assert!(panel.hovered.get().is_none());
        assert!(!panel.handle_hover(moved));
        panel.clear_painted();
        assert!(!panel.handle_hover(MouseEvent { row: 11, ..moved }));
    }

    #[test]
    fn footer_hover_discoverability_keeps_narrow_width_and_three_detail_rows() {
        for locale in [crate::i18n::Locale::English, crate::i18n::Locale::Korean] {
            crate::i18n::with_locale(locale, || {
                for width in [0, 1, 14, 24, 47, 80, 120] {
                    let mut panel = SubagentStatusPanel::default();
                    panel.update(summary_with_agents(15, ThreadId::new()));
                    let height = panel.view(true).desired_height(width);
                    let area = Rect::new(0, 0, width, height);
                    let mut buffer = Buffer::empty(area);
                    panel.view(true).render(area, &mut buffer);
                    let text = if height > 0 {
                        hover_row_text(&buffer, 0)
                    } else {
                        String::new()
                    };
                    assert_eq!(height, u16::from(width > 0));
                    if width >= 24 {
                        let click = if locale == crate::i18n::Locale::English {
                            "click"
                        } else {
                            "클릭"
                        };
                        assert!(text.contains(click), "{width}: {text}");
                        assert!(text.contains("/subagents"), "{width}: {text}");
                    }
                    panel.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
                    let height = panel.view(true).desired_height(width);
                    assert_eq!(height, if width >= 48 { 4 } else { u16::from(width > 0) });
                    let area = Rect::new(7, 11, width, height);
                    let mut buffer = Buffer::empty(Rect::new(0, 0, 140, 20));
                    buffer[(area.right(), 11)].set_symbol("#");
                    panel.view(true).render(area, &mut buffer);
                    panel.handle_hover(MouseEvent {
                        kind: MouseEventKind::Moved,
                        ..click(7, 11)
                    });
                    panel.view(true).render(area, &mut buffer);
                    assert_eq!(buffer[(area.right(), 11)].symbol(), "#");
                    assert!(panel.painted_rows.borrow().len() <= MAX_AGENT_ROWS);
                }
            });
        }
    }

    #[test]
    fn footer_click_selects_painted_ids_after_priority_sort_and_scroll_at_terminal_offset() {
        let root = ThreadId::new();
        let mut summary = summary_with_agents(15, root);
        let attention = summary.entries[14].thread_id;
        let failed = summary.entries[13].thread_id;
        let first_running = summary.entries[0].thread_id;
        summary.entries[14].state = SubagentState::NeedsInput;
        summary.entries[13].state = SubagentState::Failed;
        let mut panel = SubagentStatusPanel::default();
        panel.update(summary);
        panel.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        let area = Rect::new(7, 11, 120, 4);
        let text = render_panel_at(&panel, area);
        assert!(text.find("worker-014").unwrap() < text.find("worker-013").unwrap());
        assert!(panel.handle_mouse(click(9, 12)));
        assert_eq!(panel.take_selection(), Some((root, attention)));
        assert!(panel.handle_mouse(click(9, 13)));
        assert_eq!(panel.take_selection(), Some((root, failed)));
        assert!(!panel.handle_mouse(click(6, 13)));
        assert!(panel.handle_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            ..click(9, 13)
        }));
        assert!(
            !panel.handle_mouse(click(9, 13)),
            "scroll invalidates old row mapping before repaint"
        );
        render_panel_at(&panel, area);
        assert!(panel.handle_mouse(click(9, 13)));
        assert_eq!(panel.take_selection(), Some((root, first_running)));
    }

    #[test]
    fn footer_keyboard_focus_preserves_identity_across_reorder_and_rejects_removed_hidden_rows() {
        let root = ThreadId::new();
        let mut summary = summary_with_agents(15, root);
        let selected = summary.entries[1].thread_id;
        let mut panel = SubagentStatusPanel::default();
        panel.update(summary.clone());
        panel.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
        render_panel(&panel, 120, true);
        assert!(panel.handle_key(KeyCode::Down.into()));
        summary.entries[1].state = SubagentState::NeedsApproval;
        panel.update(summary.clone());
        assert!(
            !panel.handle_mouse(click(0, 1)),
            "metadata updates invalidate the old painted rows"
        );
        render_panel(&panel, 120, true);
        assert!(panel.handle_key(KeyCode::Enter.into()));
        assert_eq!(panel.take_selection(), Some((root, selected)));
        panel.focused.set(true);
        summary.entries.retain(|entry| entry.thread_id != selected);
        panel.update(summary);
        render_panel(&panel, 120, true);
        assert!(panel.handle_key(KeyCode::Enter.into()));
        assert_eq!(
            panel.take_selection(),
            None,
            "removal must not select the replacement row"
        );
        render_panel(&panel, 47, true);
        assert!(!panel.handle_mouse(click(0, 1)));
        assert!(
            !panel.handle_key(KeyCode::Enter.into()),
            "hidden details release keyboard focus"
        );
        panel.focused.set(true);
        render_panel_at(&panel, Rect::new(0, 0, 120, 2));
        assert!(!panel.owns_selection_key(KeyCode::Enter.into()));
        assert!(
            !panel.handle_mouse(click(0, 1)),
            "a clipped footer never leaves selectable hidden rows"
        );
        panel.update(summary_with_agents(15, ThreadId::new()));
        assert!(!panel.handle_mouse(click(0, 1)));
        assert_eq!(panel.take_selection(), None);
    }

    #[test]
    fn footer_focused_hints_keep_collapse_and_selection_before_optional_scroll_in_both_languages() {
        for locale in [crate::i18n::Locale::English, crate::i18n::Locale::Korean] {
            crate::i18n::with_locale(locale, || {
                let mut panel = SubagentStatusPanel::default();
                panel.update(summary_with_agents(15, ThreadId::new()));
                panel.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::ALT));
                for width in [80, 100, 120] {
                    let (height, text) = render_panel(&panel, width, true);
                    assert_eq!(height, 4);
                    assert!(text.contains("alt+s"), "{width}: {text}");
                    assert!(
                        text.contains("enter") && text.contains("esc"),
                        "{width}: {text}"
                    );
                }
            });
        }
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
                thread_id: ThreadId::new(),
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
        render_panel(&panel, 100, true);
        assert!(panel.handle_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 5, 2)));
        assert!(panel.take_selection().is_some());
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
            thread_id: ThreadId::new(),
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
