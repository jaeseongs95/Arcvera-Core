// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
use std::path::PathBuf;

use ratatui::style::Style;
use ratatui::style::Styled;
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Selection {
    File(PathBuf),
    Tool {
        insert_text: String,
        path: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum MentionType {
    Plugin,
    Skill,
    Task,
    File,
    Directory,
}

impl MentionType {
    pub(super) fn is_filesystem(self) -> bool {
        matches!(self, Self::File | Self::Directory)
    }

    pub(super) fn span(self, base_style: Style) -> Span<'static> {
        let style = match self {
            Self::Plugin => base_style.fg(crate::style::accent_color()),
            Self::Skill => base_style.dim(),
            Self::Task => base_style.cyan(),
            Self::File => base_style.cyan(),
            Self::Directory => base_style,
        };
        let width = [
            Self::Plugin,
            Self::Skill,
            Self::Task,
            Self::File,
            Self::Directory,
        ]
        .into_iter()
        .map(|kind| kind.label().width())
        .max()
        .unwrap_or_default();
        let label = self.label();
        format!("{label}{}", " ".repeat(width.saturating_sub(label.width()))).set_style(style)
    }

    fn label(self) -> String {
        match self {
            Self::Plugin => crate::i18n::message("dialogs-mention-plugin", &[]),
            Self::Skill => crate::i18n::message("dialogs-mention-skill", &[]),
            Self::Task => crate::i18n::message("dialogs-mention-task", &[]),
            Self::File => crate::i18n::message("dialogs-mention-file", &[]),
            Self::Directory => crate::i18n::message("dialogs-mention-directory", &[]),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(super) display_name: String,
    pub(super) description: Option<String>,
    pub(super) search_terms: Vec<String>,
    pub(super) mention_type: MentionType,
    pub(super) selection: Selection,
}

#[derive(Clone, Debug)]
pub(super) struct SearchResult {
    pub(super) display_name: String,
    pub(super) description: Option<String>,
    pub(super) mention_type: MentionType,
    pub(super) selection: Selection,
    pub(super) match_indices: Option<Vec<usize>>,
    pub(super) score: i32,
}

impl Candidate {
    pub(super) fn to_result(&self, match_indices: Option<Vec<usize>>, score: i32) -> SearchResult {
        SearchResult {
            display_name: self.display_name.clone(),
            description: self.description.clone(),
            mention_type: self.mention_type,
            selection: self.selection.clone(),
            match_indices,
            score,
        }
    }
}
