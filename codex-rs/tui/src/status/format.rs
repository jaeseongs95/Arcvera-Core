use ratatui::prelude::*;
use ratatui::style::Stylize;
use std::collections::BTreeSet;

use crate::width::display_width;

#[derive(Debug, Clone)]
pub(crate) struct FieldFormatter {
    indent: &'static str,
    label_width: usize,
    value_offset: usize,
    value_indent: String,
}

impl FieldFormatter {
    pub(crate) const INDENT: &'static str = "  ";

    pub(crate) fn from_labels<S>(labels: impl IntoIterator<Item = S>) -> Self
    where
        S: AsRef<str>,
    {
        let label_width = labels
            .into_iter()
            .map(|label| display_width(&display_label(label.as_ref())))
            .max()
            .unwrap_or(0);
        let indent_width = display_width(Self::INDENT);
        let value_offset = indent_width + label_width + 1 + 2;

        Self {
            indent: Self::INDENT,
            label_width,
            value_offset,
            value_indent: " ".repeat(value_offset),
        }
    }

    pub(crate) fn line(
        &self,
        label: &'static str,
        value_spans: Vec<Span<'static>>,
    ) -> Line<'static> {
        Line::from(self.full_spans(label, value_spans))
    }

    pub(crate) fn continuation(&self, mut spans: Vec<Span<'static>>) -> Line<'static> {
        let mut all_spans = Vec::with_capacity(spans.len() + 1);
        all_spans.push(Span::from(self.value_indent.clone()).dim());
        all_spans.append(&mut spans);
        Line::from(all_spans)
    }

    pub(crate) fn value_width(&self, available_inner_width: usize) -> usize {
        available_inner_width.saturating_sub(self.value_offset)
    }

    pub(crate) fn full_spans(
        &self,
        label: &str,
        mut value_spans: Vec<Span<'static>>,
    ) -> Vec<Span<'static>> {
        let mut spans = Vec::with_capacity(value_spans.len() + 1);
        spans.push(self.label_span(label));
        spans.append(&mut value_spans);
        spans
    }

    fn label_span(&self, label: &str) -> Span<'static> {
        let mut buf = String::with_capacity(self.value_offset);
        buf.push_str(self.indent);

        let label = display_label(label);
        buf.push_str(&label);
        buf.push(':');

        let label_width = display_width(&label);
        let padding = 2 + self.label_width.saturating_sub(label_width);
        for _ in 0..padding {
            buf.push(' ');
        }

        Span::from(buf).dim()
    }
}

pub(crate) fn push_label(labels: &mut Vec<String>, seen: &mut BTreeSet<String>, label: &str) {
    if seen.contains(label) {
        return;
    }

    let owned = label.to_string();
    seen.insert(owned.clone());
    labels.push(owned);
}

// Labels are localized before both measuring and drawing the shared value column.
fn display_label(label: &str) -> String {
    match label {
        "Model" => crate::i18n::message("status-label-model", &[]),
        "Directory" => crate::i18n::message("status-label-directory", &[]),
        "Permissions" => crate::i18n::message("status-label-permissions", &[]),
        "Agents.md" => crate::i18n::message("status-label-agents-md", &[]),
        "Model provider" => crate::i18n::message("status-label-model-provider", &[]),
        "Account" => crate::i18n::message("status-label-account", &[]),
        "ChatGPT workspace" => crate::i18n::message("status-label-chatgpt-workspace", &[]),
        "Thread name" => crate::i18n::message("status-label-thread-name", &[]),
        "Session" => crate::i18n::message("status-label-session", &[]),
        "Forked from" => crate::i18n::message("status-label-forked-from", &[]),
        "Collaboration mode" => crate::i18n::message("status-label-collaboration-mode", &[]),
        "Token usage" => crate::i18n::message("status-label-token-usage", &[]),
        "Context window" => crate::i18n::message("status-label-context-window", &[]),
        "Limits" => crate::i18n::message("status-label-limits", &[]),
        "Warning" => crate::i18n::message("status-label-warning", &[]),
        "Server" => crate::i18n::message("status-label-server", &[]),
        "Thread usage" => crate::i18n::message("status-label-thread-usage", &[]),
        "  Models" => crate::i18n::message("status-label-detail-models", &[]),
        "  Reasoning" => crate::i18n::message("status-label-detail-reasoning", &[]),
        "  Speed" => crate::i18n::message("status-label-detail-speed", &[]),
        "  Billed tokens" => crate::i18n::message("status-label-detail-billed-tokens", &[]),
        "Credits" => crate::i18n::message("status-label-credits", &[]),
        "Monthly credit limit" => crate::i18n::message("status-label-monthly-credit-limit", &[]),
        _ => label.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Locale;
    use crate::i18n::with_locale;

    #[test]
    fn localized_labels_measure_and_draw_the_same_cell_width() {
        for locale in [Locale::English, Locale::Korean] {
            with_locale(locale, || {
                let formatter = FieldFormatter::from_labels(["Model", "ChatGPT workspace"]);
                let model = formatter.line("Model", vec![Span::raw("불투명🧪{name}")]);
                let workspace = formatter.line("ChatGPT workspace", vec![Span::raw("원문")]);
                assert_eq!(model.spans[0].width(), workspace.spans[0].width());
                assert_eq!(model.spans[0].width(), formatter.value_offset);
                assert_eq!(model.spans[1].content, "불투명🧪{name}");
                assert_eq!(formatter.value_width(40), 40 - formatter.value_offset);
                let expected = if locale == Locale::Korean {
                    "  모델:"
                } else {
                    "  Model:"
                };
                assert!(model.spans[0].content.starts_with(expected));
                assert_eq!(
                    formatter.continuation(vec![]).width(),
                    formatter.value_offset
                );
            });
        }
    }
}
