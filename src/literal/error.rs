//! Typed reasons a result literal is refused on read or write.

use std::fmt;

/// Why a literal's annotations were refused while reading or writing a results
/// document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiteralError {
    /// A bare `dir` key (SRJ) or unprefixed `dir` attribute (SRX). SPARQL
    /// 1.2 results carry the base direction only as `its:dir`.
    BareDirKey,
    /// A base direction was given without a language tag.
    DirectionWithoutLanguage,
    /// The literal has a language tag but its datatype is not the one the
    /// language tag (and optional direction) implies.
    DatatypeContradictsLanguage {
        /// The datatype IRI found in the document.
        datatype: String,
        /// The datatype IRI implied by the language tag and direction.
        expected: &'static str,
    },
    /// The datatype is `rdf:langString` or `rdf:dirLangString`, but the
    /// literal has no language tag.
    DatatypeRequiresLanguage {
        /// The datatype IRI found in the document.
        datatype: String,
    },
    /// The language tag is not a well-formed BCP 47 tag.
    InvalidLanguageTag(String),
}

impl fmt::Display for LiteralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BareDirKey => f.write_str(
                "bare \"dir\" is not a SPARQL 1.2 results key; the base direction is \"its:dir\"",
            ),
            Self::DirectionWithoutLanguage => {
                f.write_str("a base direction (its:dir) requires a language tag")
            }
            Self::DatatypeContradictsLanguage { datatype, expected } => write!(
                f,
                "datatype <{datatype}> contradicts the language tag; expected <{expected}>"
            ),
            Self::DatatypeRequiresLanguage { datatype } => {
                write!(f, "datatype <{datatype}> requires a language tag")
            }
            Self::InvalidLanguageTag(tag) => write!(f, "invalid BCP 47 language tag {tag:?}"),
        }
    }
}

impl std::error::Error for LiteralError {}
