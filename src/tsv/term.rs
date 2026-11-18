//! Writing one RDF term in Turtle / N-Triples syntax (§4.2).

use std::fmt::Write as _;

use super::TsvError;
use super::lexical::{
    BlankNodeLabel,
    NumericLexical,
    is_iri_char,
};
use crate::{
    ResultValue,
    SparqlResultsError,
};

/// One term as a TSV field.
pub(super) struct TsvTerm<'a>(&'a ResultValue);

impl<'a> TsvTerm<'a> {
    pub(super) const fn new(value: &'a ResultValue) -> Self {
        Self(value)
    }

    /// Refuse a term with a blank node label that `_:label` cannot spell.
    pub(super) fn check(&self) -> Result<(), SparqlResultsError> {
        match self.0 {
            ResultValue::BNode(label) if !BlankNodeLabel::new(label).is_well_formed() => Err(
                SparqlResultsError::Tsv(TsvError::BlankNodeLabel(label.clone())),
            ),
            ResultValue::Triple {
                subject,
                predicate,
                object,
            } => [subject, predicate, object]
                .into_iter()
                .try_for_each(|part| TsvTerm::new(part).check()),
            _ => Ok(()),
        }
    }

    pub(super) fn push_to(&self, out: &mut String) {
        match self.0 {
            ResultValue::Uri(iri) => Self::push_iri(iri, out),
            ResultValue::BNode(label) => {
                out.push_str("_:");
                out.push_str(label);
            }
            ResultValue::Literal {
                value,
                lang,
                datatype,
                dir,
            } => {
                if let Some(lang) = lang {
                    Self::push_string(value, out);
                    out.push('@');
                    out.push_str(lang);
                    if let Some(dir) = dir {
                        out.push_str("--");
                        out.push_str(dir.as_str());
                    }
                } else if let Some(datatype) = datatype {
                    if NumericLexical::new(value).abbreviated_datatype() == Some(datatype.as_str())
                    {
                        out.push_str(value);
                    } else {
                        Self::push_string(value, out);
                        out.push_str("^^");
                        Self::push_iri(datatype, out);
                    }
                } else {
                    Self::push_string(value, out);
                }
            }
            ResultValue::Triple {
                subject,
                predicate,
                object,
            } => {
                out.push_str("<<( ");
                TsvTerm::new(subject).push_to(out);
                out.push(' ');
                TsvTerm::new(predicate).push_to(out);
                out.push(' ');
                TsvTerm::new(object).push_to(out);
                out.push_str(" )>>");
            }
        }
    }

    /// `IRIREF`; a character it excludes is written as a `UCHAR`.
    fn push_iri(iri: &str, out: &mut String) {
        out.push('<');
        for c in iri.chars() {
            if is_iri_char(c) {
                out.push(c);
            } else {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
        }
        out.push('>');
    }

    /// `STRING_LITERAL_QUOTE`; §4.2 requires tab, LF and CR to be escaped.
    fn push_string(value: &str, out: &mut String) {
        out.push('"');
        for c in value.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                other => out.push(other),
            }
        }
        out.push('"');
    }
}
