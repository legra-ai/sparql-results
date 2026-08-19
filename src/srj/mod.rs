//! SPARQL Query Results JSON Format (SRJ) — parser and
//! serializer.
//!
//! Implements enough of the
//! [SPARQL 1.2 Query Results JSON Format](https://www.w3.org/TR/sparql12-results-json/)
//! to round-trip SELECT and ASK documents through
//! [`crate::bounded::SparqlResult`]. The wire schema is expressed as typed
//! Serde transfer structs in [`wire`], so no `serde_json::Value`
//! or ad-hoc `json!()` invocations are used.

mod document;
mod parse;
mod reader;
mod serialize;
mod wire;

pub use parse::{
    SrjStreamKind, SrjStreamSink, SrjStreamSummary, parse_srj_bounded, parse_srj_streaming,
};
pub use serialize::{SrjWriter, write_srj};
