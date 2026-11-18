//! Typed reasons a TSV document or term is refused.

use std::fmt;

use crate::SparqlResultsError;

/// Why a TSV document was refused on read, or a value on write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TsvError {
    /// The underlying reader or writer failed.
    Io(String),
    /// The input is empty: there is no header line.
    MissingHeader,
    /// A header field is not a `?`-prefixed variable (§4.1).
    HeaderField(String),
    /// A line is not valid UTF-8.
    InvalidUtf8 {
        /// The 1-based line number.
        line: u64,
    },
    /// A row does not have one field per variable (§4.1).
    FieldCount {
        /// The 1-based line number.
        line: u64,
        /// The number of variables in the header.
        expected: usize,
        /// The number of fields found.
        found: usize,
    },
    /// A field is not an RDF term in Turtle syntax (§4.2).
    Term {
        /// The 1-based line number.
        line: u64,
        /// The 1-based field number.
        field: usize,
        /// What is wrong with it.
        reason: TermSyntaxError,
    },
    /// A blank node label is not a Turtle `BLANK_NODE_LABEL`, so it cannot
    /// be written as `_:label`.
    BlankNodeLabel(String),
}

/// What is wrong with one TSV field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TermSyntaxError {
    /// The field does not start an RDF term.
    UnexpectedStart(char),
    /// The input ended inside a term.
    UnexpectedEnd,
    /// A character is not allowed where it occurs.
    UnexpectedChar(char),
    /// An escape sequence is not a Turtle `ECHAR` or `UCHAR`.
    InvalidEscape,
    /// The field has characters after its term.
    TrailingInput,
    /// A blank node label is not a Turtle `BLANK_NODE_LABEL`.
    BlankNodeLabel,
    /// A language tag is malformed, or a base direction is not `ltr` or
    /// `rtl`.
    LanguageTag,
}

impl fmt::Display for TsvError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::MissingHeader => f.write_str("the TSV document has no header line"),
            Self::HeaderField(field) => {
                write!(f, "header field {field:?} is not a ?-prefixed variable")
            }
            Self::InvalidUtf8 { line } => write!(f, "line {line} is not valid UTF-8"),
            Self::FieldCount {
                line,
                expected,
                found,
            } => write!(f, "line {line} has {found} fields, expected {expected}"),
            Self::Term {
                line,
                field,
                reason,
            } => write!(f, "line {line}, field {field}: {reason}"),
            Self::BlankNodeLabel(label) => {
                write!(f, "blank node label {label:?} cannot be written in TSV")
            }
        }
    }
}

impl fmt::Display for TermSyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedStart(c) => write!(f, "{c:?} does not start an RDF term"),
            Self::UnexpectedEnd => f.write_str("the field ends inside a term"),
            Self::UnexpectedChar(c) => write!(f, "unexpected character {c:?}"),
            Self::InvalidEscape => f.write_str("invalid escape sequence"),
            Self::TrailingInput => f.write_str("characters follow the term"),
            Self::BlankNodeLabel => f.write_str("invalid blank node label"),
            Self::LanguageTag => f.write_str("invalid language tag or base direction"),
        }
    }
}

impl std::error::Error for TsvError {}

impl std::error::Error for TermSyntaxError {}

impl From<TsvError> for SparqlResultsError {
    fn from(error: TsvError) -> Self {
        Self::Tsv(error)
    }
}
