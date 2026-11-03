// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
//! Display-only cron snapshots; the scheduler retains the unmodified execution prompt.

use super::*;

const PROMPT_PREVIEW_GRAPHEMES: usize = 50;

#[derive(Debug)]
pub(crate) struct CronJobHistoryCell {
    activity_id: String,
    summary: String,
    prompt: String,
}

impl CronJobHistoryCell {
    pub(crate) fn new(summary: String, prompt: String) -> Self {
        Self {
            // Each list invocation is an independent snapshot, including repeated job IDs.
            activity_id: format!("cron-preview:{}", uuid::Uuid::new_v4()),
            summary,
            prompt,
        }
    }

    fn lines(&self, prompt: &str, truncated: bool, width: u16) -> Vec<Line<'static>> {
        let mut escaped = String::new();
        for ch in serde_json::json!(prompt).to_string().chars() {
            if ch.is_control() {
                escaped.push_str(&format!("\\u{:04x}", u32::from(ch)));
            } else {
                escaped.push(ch);
            }
        }
        adaptive_wrap_lines(
            vec![
                Line::from(self.summary.clone()),
                Line::from(format!(
                    "  prompt={escaped}{}",
                    if truncated { "…" } else { "" }
                )),
            ],
            RtOptions::new(usize::from(width.max(1))),
        )
    }
}

impl HistoryCell for CronJobHistoryCell {
    fn activity_ids(&self) -> Vec<String> {
        vec![self.activity_id.clone()]
    }

    fn activity_disclosure(&self, _width: u16) -> Option<ActivityDisclosure> {
        self.prompt
            .graphemes(true)
            .nth(PROMPT_PREVIEW_GRAPHEMES)
            .map(|_| ActivityDisclosure::Generic)
    }

    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let preview: String = self
            .prompt
            .graphemes(true)
            .take(PROMPT_PREVIEW_GRAPHEMES)
            .collect();
        self.lines(&preview, preview.len() < self.prompt.len(), width)
    }

    fn transcript_lines(&self, width: u16) -> Vec<Line<'static>> {
        self.lines(&self.prompt, false, width)
    }

    fn raw_lines(&self) -> Vec<Line<'static>> {
        self.transcript_lines(u16::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(lines: Vec<Line<'static>>) -> String {
        lines
            .into_iter()
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn cron_prompt_preview_bounds_unicode_without_losing_source() {
        for grapheme in ["a", "한", "e\u{301}", "👩‍💻"] {
            let first_fifty = grapheme.repeat(50);
            let prompt = format!("{first_fifty}SECRET_TAIL\n\u{1b}\u{7f}");
            let cell = CronJobHistoryCell::new("#1 Ready".into(), prompt.clone());
            assert_eq!(
                text(cell.display_lines(1000)),
                format!("#1 Ready\n  prompt=\"{first_fifty}\"…")
            );
            assert!(cell.activity_disclosure(1000).is_some());
            let full = text(cell.transcript_lines(1000));
            assert!(full.contains("SECRET_TAIL\\n\\u001b\\u007f"));
            assert!(!full.contains('\u{1b}'));
            assert_eq!(cell.prompt, prompt);
        }
        for length in [0, 49, 50] {
            let cell = CronJobHistoryCell::new("#1 Ready".into(), "한".repeat(length));
            assert_eq!(cell.activity_disclosure(1000), None);
            assert_eq!(cell.display_lines(1000), cell.transcript_lines(1000));
        }
    }
}
