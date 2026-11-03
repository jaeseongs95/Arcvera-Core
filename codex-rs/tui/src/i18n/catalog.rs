// Modified for OpenDIY Codex R35 (2026-11-03); see MODIFICATIONS.md.
//! Fluent catalogs for application-owned UI messages. Argument values are opaque.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::OnceLock;

use fluent_bundle::FluentArgs;
use fluent_bundle::FluentResource;
use fluent_bundle::concurrent::FluentBundle;
use fluent_syntax::ast;
use unic_langid::LanguageIdentifier;

use super::Locale;

type Sources<'a> = &'a [(&'a str, &'a str)];

const EN: Sources<'static> = &[
    ("core", include_str!("../../locales/en/core.ftl")),
    ("legacy", include_str!("../../locales/en/legacy.ftl")),
    ("cloud", include_str!("../../locales/en/cloud.ftl")),
    ("dialogs", include_str!("../../locales/en/dialogs.ftl")),
    ("status", include_str!("../../locales/en/status.ftl")),
    ("cli", include_str!("../../locales/en/cli.ftl")),
    ("picker", include_str!("../../locales/en/picker.ftl")),
    ("controls", include_str!("../../locales/en/controls.ftl")),
];
const KO: Sources<'static> = &[
    ("core", include_str!("../../locales/ko/core.ftl")),
    ("legacy", include_str!("../../locales/ko/legacy.ftl")),
    ("cloud", include_str!("../../locales/ko/cloud.ftl")),
    ("dialogs", include_str!("../../locales/ko/dialogs.ftl")),
    ("status", include_str!("../../locales/ko/status.ftl")),
    ("cli", include_str!("../../locales/ko/cli.ftl")),
    ("picker", include_str!("../../locales/ko/picker.ftl")),
    ("controls", include_str!("../../locales/ko/controls.ftl")),
];

struct Catalog {
    bundle: FluentBundle<FluentResource>,
    variables: BTreeMap<String, BTreeSet<String>>,
    static_messages: BTreeMap<String, String>,
    problems: Vec<String>,
}

impl Catalog {
    fn new(language: &str, sources: Sources<'_>) -> Self {
        let mut bundle = FluentBundle::new_concurrent(vec![
            language.parse::<LanguageIdentifier>().unwrap_or_default(),
        ]);
        // These terminal UI strings already preserve the caller's layout. Do not
        // add invisible direction-isolation characters around opaque arguments.
        bundle.set_use_isolating(false);
        let mut variables = BTreeMap::new();
        let mut problems = Vec::new();
        for (namespace, source) in sources {
            let resource = match FluentResource::try_new((*source).to_owned()) {
                Ok(resource) => resource,
                Err((resource, errors)) => {
                    problems.push(format!("{language}/{namespace}: {errors:?}"));
                    resource
                }
            };
            for entry in resource.entries() {
                let (id, value, attributes) = match entry {
                    ast::Entry::Message(message) => (
                        message.id.name.to_owned(),
                        message.value.as_ref(),
                        &message.attributes,
                    ),
                    ast::Entry::Term(term) => (
                        format!("-{}", term.id.name),
                        Some(&term.value),
                        &term.attributes,
                    ),
                    _ => continue,
                };
                let mut names = BTreeSet::new();
                if let Some(value) = value {
                    pattern_variables(value, &mut names);
                    if value.elements.is_empty() {
                        problems.push(format!("{language}/{namespace}: empty {id}"));
                    }
                } else if attributes.is_empty() {
                    problems.push(format!("{language}/{namespace}: empty {id}"));
                }
                if variables.insert(id.clone(), names).is_some() {
                    problems.push(format!("{language}/{namespace}: duplicate {id}"));
                }
                for attribute in attributes {
                    let mut names = BTreeSet::new();
                    pattern_variables(&attribute.value, &mut names);
                    let attribute_id = format!("{id}.{}", attribute.id.name);
                    if variables.insert(attribute_id.clone(), names).is_some() {
                        problems.push(format!("{language}/{namespace}: duplicate {attribute_id}"));
                    }
                }
            }
            if let Err(errors) = bundle.add_resource(resource) {
                problems.push(format!("{language}/{namespace}: {errors:?}"));
            }
        }
        let mut catalog = Self {
            bundle,
            variables,
            static_messages: BTreeMap::new(),
            problems,
        };
        for (id, names) in &catalog.variables {
            if names.is_empty()
                && let Some(rendered) = catalog.render(id, &FluentArgs::new())
            {
                catalog.static_messages.insert(id.clone(), rendered);
            }
        }
        catalog
    }

    fn render(&self, id: &str, args: &FluentArgs<'_>) -> Option<String> {
        let message = self.bundle.get_message(id)?;
        let value = message.value()?;
        let mut errors = Vec::new();
        let rendered = self.bundle.format_pattern(value, Some(args), &mut errors);
        errors.is_empty().then(|| rendered.into_owned())
    }
}

struct Catalogs {
    english: Catalog,
    korean: Catalog,
    legacy: HashMap<String, String>,
}

impl Catalogs {
    fn new(english: Sources<'_>, korean: Sources<'_>) -> Self {
        let english = Catalog::new("en", english);
        let korean = Catalog::new("ko", korean);
        let mut legacy = HashMap::new();
        for (id, original) in english
            .static_messages
            .iter()
            .filter(|(id, _)| id.starts_with("legacy-"))
        {
            let translated = korean.static_messages.get(id).unwrap_or(original);
            legacy.insert(original.clone(), translated.clone());
        }
        Self {
            english,
            korean,
            legacy,
        }
    }

    fn render(&self, locale: Locale, id: &str, args: &FluentArgs<'_>) -> String {
        if locale == Locale::Korean
            && let Some(rendered) = self.korean.render(id, args).filter(|text| !text.is_empty())
        {
            return rendered;
        }
        self.english
            .render(id, args)
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| {
                // A catalog/call-site defect must not crash a running UI. The ID is
                // a stable last resort; never log user/server argument values.
                tracing::warn!(
                    message_id = id,
                    "UI message unavailable or invalid arguments"
                );
                id.to_owned()
            })
    }

    fn static_message(&self, locale: Locale, id: &'static str) -> &str {
        let localized = if locale == Locale::Korean {
            self.korean.static_messages.get(id)
        } else {
            None
        };
        localized
            .filter(|value| !value.is_empty())
            .or_else(|| {
                self.english
                    .static_messages
                    .get(id)
                    .filter(|value| !value.is_empty())
            })
            .map(String::as_str)
            .unwrap_or_else(|| {
                tracing::warn!(
                    message_id = id,
                    "Static UI message unavailable or requires arguments"
                );
                id
            })
    }

    fn problems(&self) -> Vec<String> {
        let mut problems = self.english.problems.clone();
        problems.extend(self.korean.problems.iter().cloned());
        for (id, variables) in &self.english.variables {
            match self.korean.variables.get(id) {
                None => problems.push(format!("ko: missing {id}")),
                Some(other) if variables != other => {
                    problems.push(format!(
                        "{id}: argument mismatch {variables:?} vs {other:?}"
                    ));
                }
                Some(_) => {}
            }
        }
        for id in self.korean.variables.keys() {
            if !self.english.variables.contains_key(id) {
                problems.push(format!("en: missing {id}"));
            }
        }
        for (language, catalog) in [("en", &self.english), ("ko", &self.korean)] {
            for (id, variables) in &catalog.variables {
                if variables.is_empty()
                    && catalog
                        .render(id, &FluentArgs::new())
                        .is_some_and(|text| text.is_empty())
                {
                    problems.push(format!("{language}: empty {id}"));
                }
            }
        }
        problems
    }
}

fn catalogs() -> &'static Catalogs {
    static CATALOGS: OnceLock<Catalogs> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        let catalogs = Catalogs::new(EN, KO);
        for problem in catalogs.problems() {
            tracing::warn!(%problem, "Invalid UI catalog; English fallback remains available");
        }
        catalogs
    })
}

pub(super) fn legacy_text(original: &str) -> Option<&'static str> {
    catalogs().legacy.get(original).map(String::as_str)
}

/// Format an application-owned message with named, opaque string arguments.
pub fn message(id: &str, args: &[(&str, &str)]) -> String {
    message_for(super::locale(), id, args)
}

/// Missing/broken localized messages fall back to English, then to the stable ID.
pub fn message_for(locale: Locale, id: &str, args: &[(&str, &str)]) -> String {
    let args: FluentArgs<'_> = args.iter().map(|(name, value)| (*name, *value)).collect();
    catalogs().render(locale, id, &args)
}

/// Borrow a cached, argument-free message for APIs that require a static label.
/// Messages requiring arguments and unknown IDs fall back to the static ID.
pub fn static_message(id: &'static str) -> &'static str {
    catalogs().static_message(super::locale(), id)
}

/// Like `message`, but `$count` is numeric so Fluent plural selectors work.
pub fn message_with_count(id: &str, args: &[(&str, &str)], count: i64) -> String {
    message_with_count_for(super::locale(), id, args, count)
}

pub fn message_with_count_for(
    locale: Locale,
    id: &str,
    args: &[(&str, &str)],
    count: i64,
) -> String {
    let mut args: FluentArgs<'_> = args.iter().map(|(name, value)| (*name, *value)).collect();
    args.set("count", count);
    catalogs().render(locale, id, &args)
}

fn pattern_variables(pattern: &ast::Pattern<&str>, names: &mut BTreeSet<String>) {
    for element in &pattern.elements {
        if let ast::PatternElement::Placeable { expression } = element {
            expression_variables(expression, names);
        }
    }
}

fn expression_variables(expression: &ast::Expression<&str>, names: &mut BTreeSet<String>) {
    match expression {
        ast::Expression::Inline(inline) => inline_variables(inline, names),
        ast::Expression::Select { selector, variants } => {
            inline_variables(selector, names);
            for variant in variants {
                pattern_variables(&variant.value, names);
            }
        }
    }
}

fn inline_variables(inline: &ast::InlineExpression<&str>, names: &mut BTreeSet<String>) {
    match inline {
        ast::InlineExpression::VariableReference { id } => {
            names.insert(id.name.to_owned());
        }
        ast::InlineExpression::Placeable { expression } => expression_variables(expression, names),
        ast::InlineExpression::FunctionReference { arguments, .. }
        | ast::InlineExpression::TermReference {
            arguments: Some(arguments),
            ..
        } => {
            for argument in &arguments.positional {
                inline_variables(argument, names);
            }
            for argument in &arguments.named {
                inline_variables(&argument.value, names);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use sha2::Digest;
    use sha2::Sha256;

    use super::*;

    #[test]
    fn catalogs_have_matching_ids_and_named_arguments() {
        assert_eq!(catalogs().problems(), Vec::<String>::new());
        let mut args = FluentArgs::new();
        for names in catalogs().english.variables.values() {
            for name in names {
                if name == "count" {
                    args.set(name.as_str(), 2_i64);
                } else {
                    args.set(name.as_str(), "원문 {id} { $reason } 🦀 \"quote\"");
                }
            }
        }
        for (language, catalog) in [("en", &catalogs().english), ("ko", &catalogs().korean)] {
            for id in catalog
                .variables
                .keys()
                .filter(|id| !id.contains('.') && !id.starts_with('-'))
            {
                assert!(catalog.render(id, &args).is_some(), "{language}: {id}");
            }
        }
    }

    #[test]
    fn all_475_legacy_translations_match_the_pre_migration_dictionary() {
        let legacy = &catalogs().legacy;
        assert_eq!(legacy.len(), 475);
        let sorted: BTreeMap<_, _> = legacy.iter().collect();
        let mut hash = Sha256::new();
        for (original, translated) in sorted {
            hash.update(original.as_bytes());
            hash.update([0]);
            hash.update(translated.as_bytes());
            hash.update([0]);
        }
        assert_eq!(
            format!("{:x}", hash.finalize()),
            "ebda53264c3740cd25bbebba7a99d802f361a38ec051656bb166d18651e78a6e"
        );
    }

    #[test]
    fn catalog_validation_rejects_actual_missing_duplicate_and_misspelled_arguments() {
        for (english, korean) in [
            ("title = Title\n", "other = 제목\n"),
            ("title = Title\ntitle = Again\n", "title = 제목\n"),
            ("title = Title { $id }\n", "title = 제목 { $reason }\n"),
            ("title = Title\n", "title =\n"),
            ("title = Title\n", "title = { \"\" }\n"),
            ("title = Title\n", "title = { $id\n"),
        ] {
            let catalogs = Catalogs::new(&[("test", english)], &[("test", korean)]);
            assert!(!catalogs.problems().is_empty(), "{english:?} / {korean:?}");
        }
    }

    #[test]
    fn named_arguments_are_opaque_and_can_be_reordered() {
        let catalogs = Catalogs::new(
            &[("test", "notice = { $id }: { $reason }\n")],
            &[("test", "notice = { $reason } / { $id }\n")],
        );
        let mut args = FluentArgs::new();
        args.set("id", "{reason} { $reason } { \"Fluent\" } 🦀");
        args.set("reason", "한글 \"quote\"\n{id}");
        assert_eq!(
            catalogs.render(Locale::Korean, "notice", &args),
            "한글 \"quote\"\n{id} / {reason} { $reason } { \"Fluent\" } 🦀"
        );
    }

    #[test]
    fn public_message_api_preserves_argument_values_in_both_languages() {
        let error = "{id} { $error } 한글 🦀 \"quote\"";
        assert_eq!(
            message_for(
                Locale::English,
                "language-save-write-failed",
                &[("error", error)]
            ),
            format!("Failed to save UI language: {error}")
        );
        assert_eq!(
            message_for(
                Locale::Korean,
                "language-save-write-failed",
                &[("error", error)]
            ),
            format!("표시 언어 저장 실패: {error}")
        );
        assert_eq!(
            message_for(Locale::Korean, "unknown-message-id", &[]),
            "unknown-message-id"
        );
    }

    #[test]
    fn static_messages_preserve_the_static_label_contract_without_leaking() {
        for (locale, expected) in [
            (
                Locale::English,
                "schedule per-chat prompts while this CLI is running",
            ),
            (Locale::Korean, "CLI 실행 중에만 대화별 프롬프트 예약 실행"),
        ] {
            super::super::with_locale(locale, || {
                let label: &'static str = static_message("cron-description");
                assert_eq!(label, expected);
                assert_eq!(
                    static_message("language-save-write-failed"),
                    "language-save-write-failed"
                );
                assert_eq!(static_message("unknown-message-id"), "unknown-message-id");
            });
        }
    }

    #[test]
    fn missing_or_broken_korean_falls_back_per_message_without_panicking() {
        let catalogs = Catalogs::new(
            &[("test", "title = English { $id }\n")],
            &[("test", "title = 한국어 { $missing }\n")],
        );
        let mut args = FluentArgs::new();
        args.set("id", "opaque");
        assert_eq!(
            catalogs.render(Locale::Korean, "title", &args),
            "English opaque"
        );
        let missing = Catalogs::new(&[("test", "title = English\n")], &[]);
        assert_eq!(missing.render(Locale::Korean, "title", &args), "English");
        assert_eq!(missing.render(Locale::Korean, "typo", &args), "typo");
        assert_eq!(
            catalogs.render(Locale::Korean, "title", &FluentArgs::new()),
            "title"
        );
    }

    #[test]
    fn numeric_counts_use_fluent_plural_rules() {
        let catalogs = Catalogs::new(
            &[(
                "test",
                "tasks = { $count ->\n    [one] One task\n   *[other] { $count } tasks\n}\n",
            )],
            &[("test", "tasks = 작업 { $count }개\n")],
        );
        assert_eq!(catalogs.problems(), Vec::<String>::new());
        let mut args = FluentArgs::new();
        args.set("count", 1_i64);
        assert_eq!(catalogs.render(Locale::English, "tasks", &args), "One task");
        args.set("count", 2_i64);
        assert_eq!(catalogs.render(Locale::English, "tasks", &args), "2 tasks");
        assert_eq!(catalogs.render(Locale::Korean, "tasks", &args), "작업 2개");
    }
}
