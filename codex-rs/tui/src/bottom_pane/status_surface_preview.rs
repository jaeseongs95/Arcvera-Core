use std::collections::BTreeMap;

use super::footer_content::FooterContent;
use codex_protocol::ThreadId;

use super::status_line_content::StatusLineRenderer;
use super::status_line_setup::StatusLineItem;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum StatusSurfacePreviewItem {
    AppName,
    ProjectName,
    ProjectRoot,
    CurrentDir,
    Hostname,
    Status,
    ThreadName,
    ThreadTitle,
    GitBranch,
    PullRequestNumber,
    BranchChanges,
    Permissions,
    ApprovalMode,
    ContextRemaining,
    ContextUsed,
    FiveHourLimit,
    WeeklyLimit,
    CodexVersion,
    ContextWindowSize,
    UsedTokens,
    TotalInputTokens,
    TotalOutputTokens,
    ThreadCredits,
    EstimatedThreadCost,
    SessionId,
    FastMode,
    Daybreak,
    RawOutput,
    WorkspaceHeadline,
    Model,
    ModelWithReasoning,
    Reasoning,
    Profile,
    Provider,
    TaskProgress,
}

impl StatusSurfacePreviewItem {
    fn placeholder(self) -> &'static str {
        match self {
            StatusSurfacePreviewItem::AppName => "codex",
            StatusSurfacePreviewItem::ProjectName => "my-project",
            StatusSurfacePreviewItem::ProjectRoot => "my-project",
            StatusSurfacePreviewItem::CurrentDir => "~/my-project/subdir",
            StatusSurfacePreviewItem::Hostname => "my-host",
            StatusSurfacePreviewItem::Status => crate::i18n::text("Working"),
            StatusSurfacePreviewItem::ThreadName => crate::i18n::text("thread name"),
            StatusSurfacePreviewItem::ThreadTitle => crate::i18n::text("thread title"),
            StatusSurfacePreviewItem::GitBranch => "feat/awesome-feature",
            StatusSurfacePreviewItem::PullRequestNumber => "PR #123",
            StatusSurfacePreviewItem::BranchChanges => "+12 -3",
            StatusSurfacePreviewItem::Permissions => crate::i18n::text("Workspace"),
            StatusSurfacePreviewItem::ApprovalMode => "on-request",
            StatusSurfacePreviewItem::ContextRemaining => crate::i18n::text("Context 0% left"),
            StatusSurfacePreviewItem::ContextUsed => crate::i18n::text("Context 0% used"),
            StatusSurfacePreviewItem::FiveHourLimit => crate::i18n::text("primary 0%"),
            StatusSurfacePreviewItem::WeeklyLimit => crate::i18n::text("secondary 0%"),
            StatusSurfacePreviewItem::CodexVersion => "0.0.0",
            StatusSurfacePreviewItem::ContextWindowSize => crate::i18n::text("0 window"),
            StatusSurfacePreviewItem::UsedTokens => crate::i18n::text("0 used"),
            StatusSurfacePreviewItem::TotalInputTokens => crate::i18n::text("0 in"),
            StatusSurfacePreviewItem::TotalOutputTokens => crate::i18n::text("0 out"),
            StatusSurfacePreviewItem::ThreadCredits => crate::i18n::text("5.2 credits"),
            StatusSurfacePreviewItem::EstimatedThreadCost => "~$1.82",
            StatusSurfacePreviewItem::SessionId => "550e8400-e29b-41d4",
            StatusSurfacePreviewItem::FastMode => crate::i18n::text("Fast"),
            StatusSurfacePreviewItem::Daybreak => crate::i18n::text("Daybreak off"),
            StatusSurfacePreviewItem::RawOutput => crate::i18n::text("raw output"),
            StatusSurfacePreviewItem::WorkspaceHeadline => crate::i18n::text("Workspace headline"),
            StatusSurfacePreviewItem::Model => "gpt-5.2-codex",
            StatusSurfacePreviewItem::ModelWithReasoning => "gpt-5.2-codex medium",
            StatusSurfacePreviewItem::Reasoning => "medium",
            StatusSurfacePreviewItem::Profile => crate::i18n::text("profile: <name>"),
            StatusSurfacePreviewItem::Provider => crate::i18n::text("provider: <id>"),
            StatusSurfacePreviewItem::TaskProgress => crate::i18n::text("Tasks 0/0"),
        }
    }

    pub(crate) fn iter() -> impl Iterator<Item = Self> {
        [
            Self::AppName,
            Self::ProjectName,
            Self::ProjectRoot,
            Self::CurrentDir,
            Self::Hostname,
            Self::Status,
            Self::ThreadName,
            Self::ThreadTitle,
            Self::GitBranch,
            Self::PullRequestNumber,
            Self::BranchChanges,
            Self::Permissions,
            Self::ApprovalMode,
            Self::ContextRemaining,
            Self::ContextUsed,
            Self::FiveHourLimit,
            Self::WeeklyLimit,
            Self::CodexVersion,
            Self::ContextWindowSize,
            Self::UsedTokens,
            Self::TotalInputTokens,
            Self::TotalOutputTokens,
            Self::ThreadCredits,
            Self::EstimatedThreadCost,
            Self::SessionId,
            Self::FastMode,
            Self::Daybreak,
            Self::RawOutput,
            Self::WorkspaceHeadline,
            Self::Model,
            Self::ModelWithReasoning,
            Self::Reasoning,
            Self::Profile,
            Self::Provider,
            Self::TaskProgress,
        ]
        .into_iter()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PreviewValue {
    text: String,
    is_placeholder: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StatusSurfacePreviewData {
    pub(crate) thread_id: Option<ThreadId>,
    pub(crate) renderer: StatusLineRenderer,
    pub(crate) pull_request_url: Option<String>,
    values: BTreeMap<StatusSurfacePreviewItem, PreviewValue>,
}

impl Default for StatusSurfacePreviewData {
    fn default() -> Self {
        let mut data = Self {
            thread_id: None,
            renderer: StatusLineRenderer::Native,
            pull_request_url: None,
            values: BTreeMap::new(),
        };
        for item in StatusSurfacePreviewItem::iter() {
            data.set_placeholder(item, item.placeholder());
        }
        data
    }
}

impl StatusSurfacePreviewData {
    pub(crate) fn from_iter<I, V>(values: I) -> Self
    where
        I: IntoIterator<Item = (StatusSurfacePreviewItem, V)>,
        V: Into<String>,
    {
        let mut data = Self::default();
        for (item, value) in values {
            data.set_live(item, value);
        }
        data
    }

    pub(crate) fn set_live<V>(&mut self, item: StatusSurfacePreviewItem, value: V)
    where
        V: Into<String>,
    {
        self.values.insert(
            item,
            PreviewValue {
                text: value.into(),
                is_placeholder: false,
            },
        );
    }

    pub(crate) fn set_placeholder<V>(&mut self, item: StatusSurfacePreviewItem, value: V)
    where
        V: Into<String>,
    {
        if self
            .values
            .get(&item)
            .is_some_and(|value| !value.is_placeholder)
        {
            return;
        }
        self.values.insert(
            item,
            PreviewValue {
                text: value.into(),
                is_placeholder: true,
            },
        );
    }

    pub(crate) fn suppress_placeholder(&mut self, item: StatusSurfacePreviewItem) {
        if self
            .values
            .get(&item)
            .is_some_and(|value| value.is_placeholder)
        {
            self.values.remove(&item);
        }
    }

    /// Localize only known application-owned quota labels, leaving unknown data intact.
    pub(crate) fn display_rate_limit_label(label: &str) -> &str {
        match label {
            "usage" => crate::i18n::text("usage"),
            "secondary usage" => crate::i18n::text("secondary usage"),
            "5h" => crate::i18n::text("5h"),
            "daily" => crate::i18n::text("daily"),
            "weekly" => crate::i18n::text("weekly"),
            "monthly" => crate::i18n::text("monthly"),
            "annual" => crate::i18n::text("annual"),
            other => other,
        }
    }

    pub(crate) fn rate_limit_item_name(
        &self,
        item: StatusSurfacePreviewItem,
        fallback: &str,
    ) -> String {
        self.live_value_for(item)
            .and_then(rate_limit_preview_copy)
            .map(|copy| copy.name.to_string())
            .unwrap_or_else(|| fallback.to_string())
    }

    pub(crate) fn rate_limit_item_description(
        &self,
        item: StatusSurfacePreviewItem,
        fallback: &str,
    ) -> String {
        self.live_value_for(item)
            .and_then(rate_limit_preview_copy)
            .map(|copy| copy.description.to_string())
            .unwrap_or_else(|| fallback.to_string())
    }

    pub(crate) fn value_for(&self, item: StatusSurfacePreviewItem) -> Option<&str> {
        self.values.get(&item).map(|value| value.text.as_str())
    }

    fn live_value_for(&self, item: StatusSurfacePreviewItem) -> Option<&str> {
        self.values
            .get(&item)
            .filter(|value| !value.is_placeholder)
            .map(|value| value.text.as_str())
    }

    pub(crate) fn status_line_for_items<I>(
        &self,
        items: I,
        use_theme_colors: bool,
    ) -> Option<FooterContent>
    where
        I: IntoIterator<Item = StatusLineItem>,
    {
        let mut segments = items
            .into_iter()
            .filter_map(|item| {
                self.value_for(item.preview_item())
                    .map(|value| (item, value.to_string()))
            })
            .peekable();
        segments.peek()?;
        self.renderer.content(
            segments,
            use_theme_colors,
            self.thread_id,
            self.live_value_for(StatusSurfacePreviewItem::PullRequestNumber)
                .and(self.pull_request_url.as_deref()),
        )
    }
}

struct RateLimitPreviewCopy {
    name: &'static str,
    description: &'static str,
}

fn rate_limit_preview_copy(value: &str) -> Option<RateLimitPreviewCopy> {
    let value = value.trim_start();
    // Stable English prefixes remain accepted by command-alias discovery. A
    // locale display value uses the same aliases without translating the IDs.
    let matches_label = |raw_prefix: &str| {
        value.starts_with(raw_prefix)
            || value
                .strip_prefix(StatusSurfacePreviewData::display_rate_limit_label(
                    raw_prefix.trim_end(),
                ))
                .is_some_and(|suffix| suffix.starts_with(' '))
    };
    if matches_label("secondary usage ") {
        Some(RateLimitPreviewCopy {
            name: "secondary-usage-limit",
            description: crate::i18n::text(
                "Remaining usage on the secondary usage limit (omitted when unavailable)",
            ),
        })
    } else if matches_label("usage ") {
        Some(RateLimitPreviewCopy {
            name: "usage-limit",
            description: crate::i18n::text(
                "Remaining usage on the primary usage limit (omitted when unavailable)",
            ),
        })
    } else if matches_label("5h ") {
        Some(RateLimitPreviewCopy {
            name: "five-hour-limit",
            description: crate::i18n::text(
                "Remaining usage on the 5-hour usage limit (omitted when unavailable)",
            ),
        })
    } else if matches_label("daily ") {
        Some(RateLimitPreviewCopy {
            name: "daily-limit",
            description: crate::i18n::text(
                "Remaining usage on the daily usage limit (omitted when unavailable)",
            ),
        })
    } else if matches_label("weekly ") {
        Some(RateLimitPreviewCopy {
            name: "weekly-limit",
            description: crate::i18n::text(
                "Remaining usage on the weekly usage limit (omitted when unavailable)",
            ),
        })
    } else if matches_label("monthly ") {
        Some(RateLimitPreviewCopy {
            name: "monthly-limit",
            description: crate::i18n::text(
                "Remaining usage on the monthly usage limit (omitted when unavailable)",
            ),
        })
    } else if matches_label("annual ") {
        Some(RateLimitPreviewCopy {
            name: "annual-limit",
            description: crate::i18n::text(
                "Remaining usage on the annual usage limit (omitted when unavailable)",
            ),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_quota_labels_keep_the_existing_command_aliases() {
        for locale in [crate::i18n::Locale::English, crate::i18n::Locale::Korean] {
            crate::i18n::with_locale(locale, || {
                for (raw, alias) in [
                    ("secondary usage", "secondary-usage-limit"),
                    ("usage", "usage-limit"),
                    ("5h", "five-hour-limit"),
                    ("daily", "daily-limit"),
                    ("weekly", "weekly-limit"),
                    ("monthly", "monthly-limit"),
                    ("annual", "annual-limit"),
                ] {
                    let raw_value = format!("{raw} 25% left");
                    let localized_value = format!(
                        "{} 25%",
                        StatusSurfacePreviewData::display_rate_limit_label(raw)
                    );
                    assert_eq!(rate_limit_preview_copy(&raw_value).unwrap().name, alias);
                    assert_eq!(
                        rate_limit_preview_copy(&localized_value).unwrap().name,
                        alias
                    );
                }
                let unknown = "opaque-weekly-{name}";
                assert_eq!(
                    StatusSurfacePreviewData::display_rate_limit_label(unknown),
                    unknown
                );
                assert!(rate_limit_preview_copy("weeklyish 25%").is_none());
                assert!(rate_limit_preview_copy("unknown 25%").is_none());
            });
        }
    }
}
