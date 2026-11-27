//! SPARQL `VARNAME` syntax.

/// A candidate variable name checked against the SPARQL grammar's
/// `VARNAME` production (without the leading `?` / `$`).
pub(crate) struct VariableNameSyntax<'a>(&'a str);

impl<'a> VariableNameSyntax<'a> {
    pub(super) const fn new(name: &'a str) -> Self {
        Self(name)
    }

    pub(super) fn is_well_formed(&self) -> bool {
        let mut chars = self.0.chars();
        chars
            .next()
            .is_some_and(|first| Self::is_pn_chars_u(first) || first.is_ascii_digit())
            && chars.all(Self::is_continuation)
    }

    /// A `VARNAME` character after the first; `PN_CHARS` minus `-`.
    pub(crate) fn is_continuation(c: char) -> bool {
        Self::is_pn_chars_u(c)
            || c.is_ascii_digit()
            || matches!(c, '\u{00B7}' | '\u{0300}'..='\u{036F}' | '\u{203F}'..='\u{2040}')
    }

    /// The `PN_CHARS_U` production.
    pub(crate) fn is_pn_chars_u(c: char) -> bool {
        c == '_' || Self::is_pn_chars_base(c)
    }

    fn is_pn_chars_base(c: char) -> bool {
        matches!(c,
            'A'..='Z'
            | 'a'..='z'
            | '\u{00C0}'..='\u{00D6}'
            | '\u{00D8}'..='\u{00F6}'
            | '\u{00F8}'..='\u{02FF}'
            | '\u{0370}'..='\u{037D}'
            | '\u{037F}'..='\u{1FFF}'
            | '\u{200C}'..='\u{200D}'
            | '\u{2070}'..='\u{218F}'
            | '\u{2C00}'..='\u{2FEF}'
            | '\u{3001}'..='\u{D7FF}'
            | '\u{F900}'..='\u{FDCF}'
            | '\u{FDF0}'..='\u{FFFD}'
            | '\u{10000}'..='\u{EFFFF}')
    }
}
