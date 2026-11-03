use super::TerminalTitleItem;

pub(crate) const ACTION_REQUIRED_PREVIEW_PREFIX: &str = "[ ! ] Action Required";

pub(crate) fn build_action_required_title_text<I, F>(
    prefix: &str,
    items: I,
    excluded_items: &[TerminalTitleItem],
    mut value_for: F,
) -> String
where
    I: IntoIterator<Item = TerminalTitleItem>,
    F: FnMut(TerminalTitleItem) -> Option<String>,
{
    // Only product-owned prefixes are localized. Callers' titles and item values are opaque.
    let prefix = match prefix {
        ACTION_REQUIRED_PREVIEW_PREFIX => crate::i18n::message("status-title-action-required", &[]),
        "[ . ] Action Required" => crate::i18n::message("status-title-action-required-hidden", &[]),
        _ => prefix.to_string(),
    };
    let mut parts = vec![prefix];
    for item in items {
        if item == TerminalTitleItem::Spinner || excluded_items.contains(&item) {
            continue;
        }
        if let Some(value) = value_for(item) {
            parts.push(value);
        }
    }
    parts.join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Locale, with_locale};

    #[test]
    fn localized_action_title_preserves_external_title_contract() {
        for (locale, active, hidden) in [
            (
                Locale::English,
                "[ ! ] Action Required",
                "[ . ] Action Required",
            ),
            (Locale::Korean, "[ ! ] 조치 필요", "[ . ] 조치 필요"),
        ] {
            with_locale(locale, || {
                for (prefix, expected) in [
                    (ACTION_REQUIRED_PREVIEW_PREFIX, active),
                    ("[ . ] Action Required", hidden),
                ] {
                    let title = build_action_required_title_text(
                        prefix,
                        [
                            TerminalTitleItem::Thread,
                            TerminalTitleItem::Spinner,
                            TerminalTitleItem::Status,
                            TerminalTitleItem::CurrentDir,
                        ],
                        &[TerminalTitleItem::Status],
                        |item| match item {
                            TerminalTitleItem::Thread => Some("사용자제목🧪{name}".to_string()),
                            TerminalTitleItem::CurrentDir => {
                                Some("C:/사용자/{path}/🧪".to_string())
                            }
                            _ => panic!("spinner/excluded values must not be requested"),
                        },
                    );
                    assert_eq!(
                        title,
                        format!("{expected} | 사용자제목🧪{{name}} | C:/사용자/{{path}}/🧪")
                    );
                }
                let custom = "[ ! ] Action Required 사용자🧪{title}";
                let title = build_action_required_title_text(
                    custom,
                    [TerminalTitleItem::Thread],
                    &[],
                    |_| Some("Action Required".to_string()),
                );
                assert_eq!(title, format!("{custom} | Action Required"));
                assert_eq!(
                    build_action_required_title_text(custom, [], &[], |_| None),
                    custom
                );
            });
        }
    }
}
