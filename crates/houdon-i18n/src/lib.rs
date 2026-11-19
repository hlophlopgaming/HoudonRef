//! User interface translations.
//!
//! Source strings are English. Every language lives in its own Fluent file
//! under `locales/<code>/main.ftl` at the repository root; nothing
//! user-facing is hard-coded in the app.

use fluent_bundle::{FluentArgs, FluentBundle, FluentResource, FluentValue};
use unic_langid::LanguageIdentifier;

/// Languages the interface is translated into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Language {
    #[default]
    English,
    Russian,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::English, Self::Russian];

    /// BCP 47 language code.
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Russian => "ru",
        }
    }

    /// The language's name in that language, for the language picker.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        let primary = code.split(['-', '_']).next()?.to_ascii_lowercase();
        Self::ALL.into_iter().find(|l| l.code() == primary)
    }

    fn source(self) -> &'static str {
        match self {
            Self::English => include_str!("../../../locales/en/main.ftl"),
            Self::Russian => include_str!("../../../locales/ru/main.ftl"),
        }
    }
}

/// Looks up translated strings, falling back to English for missing ones.
pub struct Localizer {
    language: Language,
    bundle: FluentBundle<FluentResource>,
    fallback: Option<FluentBundle<FluentResource>>,
}

impl Localizer {
    pub fn new(language: Language) -> Self {
        let fallback = (language != Language::English).then(|| make_bundle(Language::English));
        Self {
            language,
            bundle: make_bundle(language),
            fallback,
        }
    }

    pub fn language(&self) -> Language {
        self.language
    }

    /// Translates a message without arguments.
    pub fn tr(&self, id: &str) -> String {
        self.format(id, None)
    }

    /// Translates a message with named arguments.
    pub fn tr_args(&self, id: &str, args: &[(&str, FluentValue<'_>)]) -> String {
        let mut fluent_args = FluentArgs::new();
        for (name, value) in args {
            fluent_args.set(*name, value.clone());
        }
        self.format(id, Some(&fluent_args))
    }

    fn format(&self, id: &str, args: Option<&FluentArgs<'_>>) -> String {
        for bundle in std::iter::once(&self.bundle).chain(&self.fallback) {
            if let Some(pattern) = bundle.get_message(id).and_then(|m| m.value()) {
                let mut errors = Vec::new();
                let text = bundle.format_pattern(pattern, args, &mut errors);
                if !errors.is_empty() {
                    log::warn!("translation errors in {id:?}: {errors:?}");
                }
                return text.into_owned();
            }
        }
        log::warn!("missing translation {id:?}");
        id.to_owned()
    }
}

pub use fluent_bundle::FluentValue as Value;

fn make_bundle(language: Language) -> FluentBundle<FluentResource> {
    let lang_id: LanguageIdentifier = language.code().parse().expect("valid language code");
    let mut bundle = FluentBundle::new(vec![lang_id]);
    // Unicode isolation marks show up as boxes in egui.
    bundle.set_use_isolating(false);
    let resource =
        FluentResource::try_new(language.source().to_owned()).unwrap_or_else(|(res, errors)| {
            log::error!(
                "syntax errors in {} translations: {errors:?}",
                language.code()
            );
            res
        });
    if let Err(errors) = bundle.add_resource(resource) {
        log::error!(
            "duplicate messages in {} translations: {errors:?}",
            language.code()
        );
    }
    bundle
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn message_ids(language: Language) -> BTreeSet<String> {
        if let Err((_, errors)) = FluentResource::try_new(language.source().to_owned()) {
            panic!("syntax errors in {}: {errors:?}", language.code());
        }
        // Messages start at the beginning of a line with `id =`.
        language
            .source()
            .lines()
            .filter_map(|line| line.split_once(" = ").map(|(id, _)| id))
            .filter(|id| id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .filter(|id| id.starts_with(|c: char| c.is_ascii_lowercase()))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn every_language_has_every_message() {
        let english = message_ids(Language::English);
        assert!(!english.is_empty());
        for language in Language::ALL {
            let ids = message_ids(language);
            let missing: Vec<_> = english.difference(&ids).collect();
            let extra: Vec<_> = ids.difference(&english).collect();
            assert!(
                missing.is_empty(),
                "{} is missing {missing:?}",
                language.code()
            );
            assert!(
                extra.is_empty(),
                "{} has unknown {extra:?}",
                language.code()
            );
        }
    }

    #[test]
    fn translates_with_arguments() {
        let ru = Localizer::new(Language::Russian);
        assert_eq!(ru.tr("section-studio"), "Студия");
        let text = ru.tr_args("model-stats", &[("triangles", 3592.into())]);
        assert!(text.contains("3592"), "{text}");
    }

    #[test]
    fn parses_language_codes() {
        assert_eq!(Language::from_code("ru-RU"), Some(Language::Russian));
        assert_eq!(Language::from_code("en_US"), Some(Language::English));
        assert_eq!(Language::from_code("de"), None);
    }
}
