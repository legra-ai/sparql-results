//! The shared machinery of the delimited result formats, CSV and TSV
//! ([SPARQL 1.2 Query Results CSV and TSV Formats][spec]).
//!
//! Both formats serialize a SELECT results table one line per solution, in
//! the header's variable order (§3.1, §4.1). They differ only in how a
//! header field and an RDF term are spelled, the separator, and the line
//! terminator, which a [`Dialect`] supplies.
//!
//! [spec]: https://www.w3.org/TR/sparql12-results-csv-tsv/

mod dialect;
mod format;
mod writer;

pub use dialect::Dialect;
pub(crate) use dialect::sealed;
pub use format::TabularFormat;
pub use writer::DelimitedWriter;
pub(crate) use writer::write_delimited;
