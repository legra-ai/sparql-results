//! SPARQL Query Results CSV — streaming writer only.
//!
//! Implements §3 of [SPARQL 1.2 Query Results CSV and TSV Formats][spec]
//! (which keeps the SPARQL 1.1 Recommendation's rules and adds triple
//! terms). Lines end with CRLF (§2); the header lists the variable names
//! without `?` (§3.1); every term is written as its plain lexical value,
//! quoted by the RFC 4180 rules when it contains `"`, `,`, LF or CR (§3.2).
//!
//! # Why there is no CSV reader
//!
//! The specification calls CSV "a lossy encoding of a table of results"
//! (§1). A CSV field drops the datatype, language tag and base direction of
//! a literal; it cannot tell an IRI from a literal that spells one, nor a
//! blank node from the literal `"_:b0"`, nor a triple term from a string
//! that looks like one; and "the standard CSV format does not distinguish
//! between missing values and empty strings" (§3.2). Reading CSV back would
//! have to invent the RDF terms it lost, so this crate does not offer it.
//! Use TSV (or SRJ / SRX) when results must be read back faithfully.
//!
//! [spec]: https://www.w3.org/TR/sparql12-results-csv-tsv/

mod dialect;
mod field;
mod write;

pub use dialect::{
    CsvDialect,
    CsvWriter,
};
pub use write::write_csv;
