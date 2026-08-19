//! Private SRJ transfer structs.
//!
//! These mirror the SPARQL Results JSON document layout exactly so
//! the parse and serialize paths can share the same Serde derives
//! without resorting to `serde_json::Value` or `json!()` macros
//! (which are forbidden by ADR 0008).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One row: `var-name -> value`. `BTreeMap` gives deterministic
/// serialization order.
pub(super) type SrjBinding = BTreeMap<String, SrjValue>;

/// A single value. Matches the W3C spec's `{ "type": ..., ... }`
/// shape via Serde's `internally tagged` enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(super) enum SrjValue {
    /// An IRI.
    Uri {
        /// The IRI string.
        value: String,
    },
    /// A literal — optionally with a language tag and / or datatype.
    Literal {
        /// Lexical form.
        value: String,
        /// Optional language tag. The W3C spec uses
        /// `"xml:lang"` as the field name (kept verbatim here so
        /// Serde deserializes standard SRJ documents unchanged).
        #[serde(rename = "xml:lang", default, skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
        /// Optional datatype IRI.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        datatype: Option<String>,
    },
    /// A blank node identifier.
    #[serde(rename = "bnode")]
    BNode {
        /// The blank node identifier.
        value: String,
    },
    /// RDF-star/SPARQL 1.2 embedded triple term.
    Triple {
        /// Nested triple value object.
        value: Box<SrjTripleValue>,
    },
}

/// Nested JSON object payload for `"type": "triple"` values.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SrjTripleValue {
    /// Triple subject term.
    pub(super) subject: SrjValue,
    /// Triple predicate term.
    pub(super) predicate: SrjValue,
    /// Triple object term.
    pub(super) object: SrjValue,
}
