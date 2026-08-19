//! Shared result-set types used by both the SRX and SRJ codecs.
//!
//! These are a faithful representation of the W3C SPARQL Query
//! Results abstract model — not a zero-copy view — so they can be
//! produced by one codec and consumed by the other without
//! conversion.

use indexmap::IndexMap;

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
