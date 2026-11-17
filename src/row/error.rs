//! Typed reasons a result row is refused on write.

use std::fmt;

/// Why a row was refused before any of it was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowError {
    /// The row binds a variable the header does not declare.
    UndeclaredVariable(String),
    /// A triple term's subject is not an IRI or blank node.
    TripleSubject,
    /// A triple term's predicate is not an IRI.
    TriplePredicate,
}

impl fmt::Display for RowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UndeclaredVariable(var) => {
                write!(f, "row binds {var:?}, which the header does not declare")
            }
            Self::TripleSubject => {
                f.write_str("a triple term subject must be an IRI or blank node")
            }
            Self::TriplePredicate => f.write_str("a triple term predicate must be an IRI"),
        }
    }
}

impl std::error::Error for RowError {}
