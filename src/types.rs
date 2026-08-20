//! Shared result-set types used by both the SRX and SRJ codecs.
//!
//! These are a faithful representation of the W3C SPARQL Query
//! Results abstract model — not a zero-copy view — so they can be
//! produced by one codec and consumed by the other without
//! conversion.

use std::fmt;
use std::str::FromStr;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// The base direction of an RDF 1.2 directional language-tagged literal
/// (`rdf:dirLangString`).
///
/// A directional literal carries both a language tag and this base
/// direction, per the RDF 1.2 / SPARQL 1.2 abstract model. The two
/// values map to the `"ltr"` / `"rtl"` tokens used on the wire (the
/// SRJ `"its:dir"` key and the SRX `its:dir` attribute).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseDirection {
    /// Left-to-right base direction (`"ltr"`).
    Ltr,
    /// Right-to-left base direction (`"rtl"`).
    Rtl,
}

impl BaseDirection {
    /// The lowercase wire token for this direction (`"ltr"` or `"rtl"`).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ltr => "ltr",
            Self::Rtl => "rtl",
        }
    }
}

impl fmt::Display for BaseDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for BaseDirection {
    type Err = ParseBaseDirectionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ltr" => Ok(Self::Ltr),
            "rtl" => Ok(Self::Rtl),
            other => Err(ParseBaseDirectionError(other.to_owned())),
        }
    }
}

/// Error returned when a string is not a valid [`BaseDirection`] token.
///
/// The only accepted tokens are `"ltr"` and `"rtl"`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseBaseDirectionError(String);

impl fmt::Display for ParseBaseDirectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid base direction {:?}, expected \"ltr\" or \"rtl\"",
            self.0
        )
    }
}

impl std::error::Error for ParseBaseDirectionError {}

/// A parsed SPARQL result set.
///
/// Mirrors the two top-level result shapes the W3C formats
/// distinguish: a tabular `SELECT` result and a scalar `ASK`
/// boolean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SparqlResult {
    /// SELECT query result — a header listing the projected
    /// variables plus one row per solution.
    Select {
        /// Variable names from the `<head>` / `head.vars`, in the
        /// order the producer declared them.
        vars: Vec<String>,
        /// Result rows.
        rows: Vec<ResultRow>,
    },
    /// ASK query result — a single boolean.
    Ask {
        /// The boolean answer the endpoint returned.
        result: bool,
    },
}

/// A single row in a SELECT result.
///
/// Bindings are stored by variable name. A variable that was not
/// bound in this row is simply absent from the map (matching the
/// SPARQL `OPTIONAL` semantics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRow {
    /// Variable bindings for this row.
    pub bindings: IndexMap<String, ResultValue>,
}

/// A value bound to a variable inside a row binding.
///
/// The three variants correspond to the `<uri>`, `<literal>`, and
/// `<bnode>` / `<triple>` wrappers in SRX and to the `{ "type": ... }` tag in
/// SRJ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultValue {
    /// An IRI.
    Uri(String),
    /// A literal (possibly with language tag or datatype).
    Literal {
        /// The lexical form.
        value: String,
        /// Optional language tag (`xml:lang` in SRX; `xml:lang`
        /// or `lang` in SRJ, depending on the version).
        lang: Option<String>,
        /// Optional datatype IRI.
        datatype: Option<String>,
        /// Optional base direction, present only for an RDF 1.2
        /// directional language-tagged literal (`rdf:dirLangString`).
        ///
        /// When this is `Some`, `lang` is also `Some` — a base
        /// direction only exists alongside a language tag. It maps to
        /// the SRJ `"its:dir"` key and the SRX `its:dir` attribute.
        dir: Option<BaseDirection>,
    },
    /// A blank node identifier.
    BNode(String),
    /// RDF-star/SPARQL 1.2 embedded triple term.
    Triple {
        /// Triple subject.
        subject: Box<ResultValue>,
        /// Triple predicate.
        predicate: Box<ResultValue>,
        /// Triple object.
        object: Box<ResultValue>,
    },
}
