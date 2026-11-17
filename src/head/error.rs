//! Typed reasons a result header is refused.

use std::fmt;

/// Why a result header (`head` in SRJ, `<head>` in SRX) was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeaderError {
    /// A variable name is not a SPARQL `VARNAME`.
    InvalidVariableName(String),
    /// The same variable is declared twice.
    DuplicateVariable(String),
    /// An ASK result declares variables; ASK headers carry only links.
    VariablesInAsk,
    /// The result body appeared before the header.
    BodyBeforeHead,
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidVariableName(name) => write!(f, "invalid variable name {name:?}"),
            Self::DuplicateVariable(name) => write!(f, "variable {name:?} is declared twice"),
            Self::VariablesInAsk => f.write_str("an ASK result header cannot declare variables"),
            Self::BodyBeforeHead => f.write_str("the result body appeared before the header"),
        }
    }
}

impl std::error::Error for HeaderError {}
