use crate::clock_format::ClockFormat;
use chrono::DateTime;
use chrono::Local;
use chrono::Utc;
use codex_app_server_protocol::RateLimitResetCreditStatus;
use codex_app_server_protocol::RateLimitResetCreditsSummary;

#[derive(Debug, Eq, PartialEq)]
pub(super) struct ResetCreditOption {
    pub(super) credit_id: Option<String>,
    pub(super) name: String,
    pub(super) detail: Option<String>,
    pub(super) description: String,
}

pub(super) fn reset_credit_options(
    summary: &RateLimitResetCreditsSummary,
    clock_format: ClockFormat,
) -> Vec<ResetCreditOption> {
    let available_count = summary.available_count.max(0);
    let detail_limit = usize::try_from(available_count).unwrap_or(usize::MAX);
    let mut available_credits = summary
        .credits
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|credit| credit.status == RateLimitResetCreditStatus::Available)
        .collect::<Vec<_>>();
    available_credits.sort_by_key(|credit| credit.expires_at.unwrap_or(i64::MAX));

    let mut options = available_credits
        .into_iter()
        .take(detail_limit)
        .map(|credit| {
            let expiration = match credit.expires_at {
                Some(expires_at) => DateTime::<Utc>::from_timestamp(expires_at, 0)
                    .map(|expires_at| {
                        let expires_at = expires_at.with_timezone(&Local);
                        crate::i18n::message(
                            "r39-surfaces-reset-expiration",
                            &[
                                (
                                    "time",
                                    &expires_at.format(clock_format.time_format()).to_string(),
                                ),
                                (
                                    "date",
                                    &expires_at
                                        .format(crate::i18n::text("%-d %b %Y"))
                                        .to_string(),
                                ),
                            ],
                        )
                    })
                    .unwrap_or_else(|| crate::i18n::text("Expiration unavailable").to_string()),
                None => crate::i18n::text("Does not expire").to_string(),
            };
            let reset_title = credit
                .title
                .as_deref()
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .unwrap_or(crate::i18n::text("Full reset"));
            let reset_description = credit
                .description
                .as_deref()
                .map(str::trim)
                .filter(|description| !description.is_empty())
                .unwrap_or(crate::i18n::text("Reset your current usage limits"));
            ResetCreditOption {
                credit_id: Some(credit.id.clone()),
                name: reset_title.to_string(),
                detail: Some(expiration),
                description: reset_description.to_string(),
            }
        })
        .collect::<Vec<_>>();

    if options.is_empty() {
        options.push(ResetCreditOption {
            credit_id: None,
            name: crate::i18n::text("Full reset").to_string(),
            detail: None,
            description: crate::i18n::text("Reset your current usage limits").to_string(),
        });
    }

    options
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_app_server_protocol::RateLimitResetCredit;
    use codex_app_server_protocol::RateLimitResetType;

    #[test]
    fn localizes_only_missing_backend_copy_even_when_backend_copy_matches_fallback() {
        for (locale, fallback_name, fallback_description) in [
            (
                crate::i18n::Locale::English,
                "Full reset",
                "Reset your current usage limits",
            ),
            (
                crate::i18n::Locale::Korean,
                "전체 초기화",
                "현재 사용 한도를 초기화합니다",
            ),
        ] {
            crate::i18n::with_locale(locale, || {
                let credit = RateLimitResetCredit {
                    id: "opaque-credit-{name}".to_string(),
                    reset_type: RateLimitResetType::CodexRateLimits,
                    status: RateLimitResetCreditStatus::Available,
                    granted_at: 0,
                    expires_at: None,
                    title: Some("Full reset".to_string()),
                    description: Some("Reset your current usage limits".to_string()),
                };
                let summary = RateLimitResetCreditsSummary {
                    available_count: 2,
                    credits: Some(vec![
                        credit.clone(),
                        RateLimitResetCredit {
                            id: "missing-copy".to_string(),
                            title: None,
                            description: None,
                            ..credit
                        },
                    ]),
                };
                let options = reset_credit_options(&summary, ClockFormat::TwentyFourHour);
                assert_eq!(options.len(), 2);
                assert_eq!(
                    options[0].credit_id.as_deref(),
                    Some("opaque-credit-{name}")
                );
                assert_eq!(options[0].name, "Full reset");
                assert_eq!(options[0].description, "Reset your current usage limits");
                assert_eq!(options[1].name, fallback_name);
                assert_eq!(options[1].description, fallback_description);
            });
        }
    }
}
