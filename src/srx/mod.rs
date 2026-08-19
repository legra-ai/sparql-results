//! SPARQL Query Results XML Format (SRX) — parser and serializer.
//!
//! Implements enough of the
//! [SPARQL Query Results XML Format](https://www.w3.org/TR/rdf-sparql-XMLres/)
//! to drive the W3C conformance test suite and to serve gateway
//! SPARQL Protocol responses.

mod canonicalize;
mod parse;
mod serialize;
mod stream;

pub use canonicalize::canonicalize_srx;
pub use parse::parse_srx_bounded;
pub use serialize::{SrxWriter, write_srx};
pub use stream::{SrxStreamKind, SrxStreamSink, SrxStreamSummary, parse_srx_streaming};
