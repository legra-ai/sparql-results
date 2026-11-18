//! One CSV field: the plain spelling of a term (§3.2) and RFC 4180 quoting.

use crate::ResultValue;

/// The text of one CSV field, before quoting.
pub(super) struct CsvField<'a>(&'a str);

impl<'a> CsvField<'a> {
    pub(super) const fn new(text: &'a str) -> Self {
        Self(text)
    }

    /// §3.2: a field containing `"`, `,`, LF or CR "MUST be quoted using
    /// the quoting mechanism of RFC4180", which doubles every `"`.
    fn needs_quotes(&self) -> bool {
        self.0
            .bytes()
            .any(|byte| matches!(byte, b'"' | b',' | b'\n' | b'\r'))
    }

    pub(super) fn push_to(&self, out: &mut Vec<u8>) {
        if self.needs_quotes() {
            out.push(b'"');
            Self::push_doubling_quotes(self.0, out);
            out.push(b'"');
        } else {
            out.extend_from_slice(self.0.as_bytes());
        }
    }

    fn push_doubling_quotes(text: &str, out: &mut Vec<u8>) {
        for byte in text.bytes() {
            if byte == b'"' {
                out.push(b'"');
            }
            out.push(byte);
        }
    }

    /// The unquoted field text of a term (§3.2): an IRI's characters, a
    /// literal's lexical form "without any language tag, base direction, or
    /// datatype information", `_:label` for a blank node, and
    /// `<<( s p o )>>` for a triple term.
    pub(super) fn push_term(value: &ResultValue, text: &mut String) {
        match value {
            ResultValue::Literal { value, .. } => text.push_str(value),
            other => Self::push_component(other, text),
        }
    }

    /// A term at the top level or inside a triple term. §3.2: inside a
    /// triple term the parts are "recursively serialized", and a literal
    /// "MUST be encapsulated within a pair of quotation marks", its own
    /// quotes doubled (the §3.4 example).
    fn push_component(value: &ResultValue, text: &mut String) {
        match value {
            ResultValue::Uri(iri) => text.push_str(iri),
            ResultValue::BNode(label) => {
                text.push_str("_:");
                text.push_str(label);
            }
            ResultValue::Literal { value, .. } => {
                text.push('"');
                text.push_str(&value.replace('"', "\"\""));
                text.push('"');
            }
            ResultValue::Triple {
                subject,
                predicate,
                object,
            } => {
                text.push_str("<<( ");
                Self::push_component(subject, text);
                text.push(' ');
                Self::push_component(predicate, text);
                text.push(' ');
                Self::push_component(object, text);
                text.push_str(" )>>");
            }
        }
    }
}
