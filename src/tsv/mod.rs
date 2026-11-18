//! SPARQL Query Results TSV — streaming writer and streaming reader.
//!
//! Implements §4 of [SPARQL 1.2 Query Results CSV and TSV Formats][spec].
//! Lines end with LF (§2); the header lists the variables "in SPARQL
//! syntax, using question mark `?`" (§4.1); every term is written in
//! Turtle / N-Triples syntax (§4.2): `<iri>`, `_:label`, `"lexical"` with
//! `@lang`, `@lang--dir` or `^^<datatype>`, the abbreviated forms for XSD
//! integers, decimals and doubles, and `<<( s p o )>>` for an RDF 1.2
//! triple term. Tab, LF and CR inside a literal are escaped as `\t`, `\n`,
//! `\r`, so a raw tab always separates fields and a raw LF always ends a
//! line.
//!
//! [spec]: https://www.w3.org/TR/sparql12-results-csv-tsv/

mod dialect;
mod error;
mod lexical;
mod parse;
mod reader;
mod term;
mod term_parser;
mod write;

pub use dialect::{
    TsvDialect,
    TsvWriter,
};
pub use error::{
    TermSyntaxError,
    TsvError,
};
pub use parse::{
    TsvStreamSink,
    TsvStreamSummary,
    parse_tsv_bounded,
    parse_tsv_streaming,
};
pub use write::write_tsv;
