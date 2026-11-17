//! A small, syntactic BCP 47 (RFC 5646) well-formedness checker.
//!
//! Validation is purely syntactic: it does not consult the IANA subtag
//! registry. Duplicate variants and duplicate extension singletons are
//! refused, as RFC 5646 §2.2.5/§2.2.6 require.

/// The RFC 5646 grandfathered tags (irregular and regular), lowercase.
const GRANDFATHERED: [&str; 26] = [
    "en-gb-oed",
    "i-ami",
    "i-bnn",
    "i-default",
    "i-enochian",
    "i-hak",
    "i-klingon",
    "i-lux",
    "i-mingo",
    "i-navajo",
    "i-pwn",
    "i-tao",
    "i-tay",
    "i-tsu",
    "sgn-be-fr",
    "sgn-be-nl",
    "sgn-ch-de",
    "art-lojban",
    "cel-gaulish",
    "no-bok",
    "no-nyn",
    "zh-guoyu",
    "zh-hakka",
    "zh-min",
    "zh-min-nan",
    "zh-xiang",
];

/// A borrowed language tag under validation.
pub(super) struct LanguageTagSyntax<'a>(&'a str);

impl<'a> LanguageTagSyntax<'a> {
    /// Wrap a candidate tag.
    pub(super) fn new(tag: &'a str) -> Self {
        Self(tag)
    }

    /// Whether the tag is a well-formed BCP 47 language tag.
    pub(super) fn is_well_formed(&self) -> bool {
        if self.0.is_empty() || !self.0.is_ascii() {
            return false;
        }
        let lower = self.0.to_ascii_lowercase();
        if GRANDFATHERED.contains(&lower.as_str()) {
            return true;
        }
        let subtags: Vec<&str> = lower.split('-').collect(); // bounded: one tag
        if subtags
            .iter()
            .any(|subtag| subtag.is_empty() || subtag.len() > 8)
        {
            return false;
        }
        if subtags[0] == "x" {
            return Self::is_private_use(&subtags[1..]);
        }
        Self::is_langtag(&subtags)
    }

    fn is_private_use(rest: &[&str]) -> bool {
        !rest.is_empty() && rest.iter().all(|subtag| is_alphanumeric(subtag))
    }

    fn is_langtag(subtags: &[&str]) -> bool {
        let mut index = 0;
        let language = subtags[0];
        if !is_alpha(language) || language.len() < 2 {
            return false;
        }
        index += 1;
        if language.len() <= 3 {
            let mut extlangs = 0;
            while extlangs < 3
                && subtags
                    .get(index)
                    .is_some_and(|s| s.len() == 3 && is_alpha(s))
            {
                extlangs += 1;
                index += 1;
            }
        }
        if subtags
            .get(index)
            .is_some_and(|s| s.len() == 4 && is_alpha(s))
        {
            index += 1;
        }
        if subtags.get(index).is_some_and(|s| is_region(s)) {
            index += 1;
        }
        let mut variants: Vec<&str> = Vec::new(); // bounded: one tag
        while let Some(&subtag) = subtags.get(index).filter(|s| is_variant(s)) {
            if variants.contains(&subtag) {
                return false;
            }
            variants.push(subtag);
            index += 1;
        }
        Self::is_extensions_and_private_use(&subtags[index..])
    }

    fn is_extensions_and_private_use(subtags: &[&str]) -> bool {
        let mut singletons: Vec<&str> = Vec::new(); // bounded: one tag
        let mut index = 0;
        while let Some(&singleton) = subtags.get(index) {
            if singleton == "x" {
                return Self::is_private_use(&subtags[index + 1..]);
            }
            if singleton.len() != 1
                || !is_alphanumeric(singleton)
                || singletons.contains(&singleton)
            {
                return false;
            }
            singletons.push(singleton);
            index += 1;
            let start = index;
            while subtags
                .get(index)
                .is_some_and(|s| s.len() >= 2 && is_alphanumeric(s))
            {
                index += 1;
            }
            if index == start {
                return false;
            }
        }
        true
    }
}

fn is_alpha(subtag: &str) -> bool {
    subtag.bytes().all(|byte| byte.is_ascii_alphabetic())
}

fn is_alphanumeric(subtag: &str) -> bool {
    subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

fn is_region(subtag: &str) -> bool {
    (subtag.len() == 2 && is_alpha(subtag))
        || (subtag.len() == 3 && subtag.bytes().all(|byte| byte.is_ascii_digit()))
}

fn is_variant(subtag: &str) -> bool {
    is_alphanumeric(subtag)
        && (subtag.len() >= 5 || (subtag.len() == 4 && subtag.as_bytes()[0].is_ascii_digit()))
}
