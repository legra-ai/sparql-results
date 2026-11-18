//! Turtle lexical productions shared by the TSV writer and reader.

use crate::head::VariableNameSyntax;

pub(super) const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
pub(super) const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
pub(super) const XSD_DOUBLE: &str = "http://www.w3.org/2001/XMLSchema#double";
pub(super) const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// A blank node label checked against Turtle `BLANK_NODE_LABEL` (without
/// the `_:` prefix).
pub(super) struct BlankNodeLabel<'a>(&'a str);

impl<'a> BlankNodeLabel<'a> {
    pub(super) const fn new(label: &'a str) -> Self {
        Self(label)
    }

    pub(super) fn is_well_formed(&self) -> bool {
        let mut chars = self.0.chars();
        chars
            .next()
            .is_some_and(|c| VariableNameSyntax::is_pn_chars_u(c) || c.is_ascii_digit())
            && !self.0.ends_with('.')
            && chars.all(|c| Self::is_label_char(c) || c == '.')
    }

    /// `PN_CHARS`.
    pub(super) fn is_label_char(c: char) -> bool {
        VariableNameSyntax::is_continuation(c) || c == '-'
    }
}

/// A literal's lexical form checked against the Turtle numeric
/// productions, which §4.2 says "SHOULD be used" for XSD integers, decimals
/// and doubles.
pub(super) struct NumericLexical<'a>(&'a str);

impl<'a> NumericLexical<'a> {
    pub(super) const fn new(lexical: &'a str) -> Self {
        Self(lexical)
    }

    /// The datatype whose abbreviated form spells this lexical form, if any.
    pub(super) fn abbreviated_datatype(&self) -> Option<&'static str> {
        let unsigned = self.0.strip_prefix(['+', '-']).unwrap_or(self.0);
        let (mantissa, exponent) = match unsigned.find(['e', 'E']) {
            Some(at) => (&unsigned[..at], Some(&unsigned[at + 1..])),
            None => (unsigned, None),
        };
        let (whole, fraction) = match mantissa.split_once('.') {
            Some((whole, fraction)) => (whole, Some(fraction)),
            None => (mantissa, None),
        };
        if !Self::digits(whole, true) || !fraction.is_none_or(|f| Self::digits(f, true)) {
            return None;
        }
        match (fraction, exponent) {
            (None, None) => Self::digits(whole, false).then_some(XSD_INTEGER),
            (Some(fraction), None) => Self::digits(fraction, false).then_some(XSD_DECIMAL),
            (fraction, Some(exponent)) => {
                let exponent = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
                let mantissa_has_digit =
                    !whole.is_empty() || fraction.is_some_and(|f| !f.is_empty());
                (mantissa_has_digit && Self::digits(exponent, false)).then_some(XSD_DOUBLE)
            }
        }
    }

    fn digits(text: &str, allow_empty: bool) -> bool {
        (allow_empty || !text.is_empty()) && text.bytes().all(|byte| byte.is_ascii_digit())
    }
}

/// Whether `c` may appear unescaped in a Turtle `IRIREF`.
pub(super) const fn is_iri_char(c: char) -> bool {
    !matches!(
        c,
        '\u{0}'..='\u{20}' | '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\'
    )
}
