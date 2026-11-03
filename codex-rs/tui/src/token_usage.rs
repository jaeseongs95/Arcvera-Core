//! TUI token usage models and display formatting.

use std::fmt;

use codex_protocol::num_format::format_with_separators;
use serde::Deserialize;
use serde::Serialize;

const BASELINE_TOKENS: i64 = 12000;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub reasoning_output_tokens: i64,
    pub total_tokens: i64,
}

impl TokenUsage {
    /// Localized terminal UI; the public Display format remains stable for other consumers.
    pub(crate) fn ui_summary(&self, so_far: bool) -> String {
        let cached = if self.cached_input() > 0 {
            crate::i18n::message(
                "controls-tokens-cached",
                &[(
                    "value",
                    format_with_separators(self.cached_input()).as_str(),
                )],
            )
        } else {
            String::new()
        };
        let reasoning = if self.reasoning_output_tokens > 0 {
            crate::i18n::message(
                "controls-tokens-reasoning",
                &[(
                    "value",
                    format_with_separators(self.reasoning_output_tokens).as_str(),
                )],
            )
        } else {
            String::new()
        };
        crate::i18n::message(
            if so_far {
                "controls-tokens-so-far"
            } else {
                "controls-tokens"
            },
            &[
                (
                    "total",
                    format_with_separators(self.blended_total()).as_str(),
                ),
                (
                    "input",
                    format_with_separators(self.non_cached_input()).as_str(),
                ),
                ("cached", cached.as_str()),
                (
                    "output",
                    format_with_separators(self.output_tokens).as_str(),
                ),
                ("reasoning", reasoning.as_str()),
            ],
        )
    }

    pub fn is_zero(&self) -> bool {
        self.total_tokens == 0
    }

    pub(crate) fn cached_input(&self) -> i64 {
        self.cached_input_tokens.max(0)
    }

    pub(crate) fn non_cached_input(&self) -> i64 {
        (self.input_tokens - self.cached_input()).max(0)
    }

    pub(crate) fn blended_total(&self) -> i64 {
        (self.non_cached_input() + self.output_tokens.max(0)).max(0)
    }

    /// Returns the raw `total_tokens` value. For `last_token_usage`, this is the latest active
    /// context size; for `total_token_usage`, this is the accumulated session total.
    pub(crate) fn tokens_in_context_window(&self) -> i64 {
        self.total_tokens
    }

    pub(crate) fn percent_of_context_window_remaining(&self, context_window: i64) -> i64 {
        if context_window <= BASELINE_TOKENS {
            return 0;
        }
        let effective_window = context_window - BASELINE_TOKENS;
        let used = (self.tokens_in_context_window() - BASELINE_TOKENS).max(0);
        let remaining = (effective_window - used).max(0);
        ((remaining as f64 / effective_window as f64) * 100.0)
            .clamp(0.0, 100.0)
            .round() as i64
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TokenUsageInfo {
    pub(crate) total_token_usage: TokenUsage,
    pub(crate) last_token_usage: TokenUsage,
    pub(crate) model_context_window: Option<i64>,
}

impl fmt::Display for TokenUsage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Token usage: total={} input={}{} output={}{}",
            format_with_separators(self.blended_total()),
            format_with_separators(self.non_cached_input()),
            if self.cached_input() > 0 {
                format!(
                    " (+ {} cached)",
                    format_with_separators(self.cached_input())
                )
            } else {
                String::new()
            },
            format_with_separators(self.output_tokens),
            if self.reasoning_output_tokens > 0 {
                format!(
                    " (reasoning {})",
                    format_with_separators(self.reasoning_output_tokens)
                )
            } else {
                String::new()
            }
        )
    }
}

#[cfg(test)]
mod localization_tests {
    use super::*;

    #[test]
    fn token_usage_ui_translates_labels_without_changing_numbers_or_public_display() {
        let usage = TokenUsage {
            input_tokens: 200,
            cached_input_tokens: 50,
            output_tokens: 300,
            reasoning_output_tokens: 100,
            total_tokens: 500,
        };
        let english = "Token usage: total=450 input=150 (+ 50 cached) output=300 (reasoning 100)";
        for (locale, expected, so_far) in [
            (
                crate::i18n::Locale::English,
                english,
                "Token usage so far: total=450 input=150 (+ 50 cached) output=300 (reasoning 100)",
            ),
            (
                crate::i18n::Locale::Korean,
                "토큰 사용량: 합계=450 입력=150 (+ 캐시 50) 출력=300 (추론 100)",
                "현재까지 토큰 사용량: 합계=450 입력=150 (+ 캐시 50) 출력=300 (추론 100)",
            ),
        ] {
            crate::i18n::with_locale(locale, || {
                assert_eq!(usage.ui_summary(false), expected);
                assert_eq!(usage.ui_summary(true), so_far);
                assert_eq!(usage.to_string(), english);
                assert!(!TokenUsage::default().ui_summary(false).contains('('));
            });
        }
    }
}
