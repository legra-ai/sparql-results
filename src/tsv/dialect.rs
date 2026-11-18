//! The TSV [`Dialect`].

use super::TsvError;
use super::term::TsvTerm;
use crate::tabular::{
    DelimitedWriter,
    Dialect,
    sealed,
};
use crate::{
    Result,
    ResultValue,
    SparqlResultsError,
    TabularFormat,
};

/// The TSV spelling rules (§4 of SPARQL 1.2 Query Results CSV and TSV).
#[derive(Debug, Clone, Copy)]
pub struct TsvDialect;

/// Incremental SPARQL Results TSV SELECT serializer.
pub type TsvWriter<W> = DelimitedWriter<W, TsvDialect>;

impl sealed::Sealed for TsvDialect {}

impl Dialect for TsvDialect {
    const FORMAT: TabularFormat = TabularFormat::Tsv;
    const SEPARATOR: u8 = b'\t';
    /// §2: the end-of-line in TSV is LF (U+000A).
    const EOL: &'static [u8] = b"\n";

    /// §4.1: variables "in SPARQL syntax, using question mark `?`".
    fn push_header_field(var: &str, out: &mut Vec<u8>) {
        out.push(b'?');
        out.extend_from_slice(var.as_bytes());
    }

    fn push_value(value: &ResultValue, out: &mut Vec<u8>) {
        let mut text = String::new(); // bounded: one value
        TsvTerm::new(value).push_to(&mut text);
        out.extend_from_slice(text.as_bytes());
    }

    fn check_value(value: &ResultValue) -> Result<()> {
        TsvTerm::new(value).check()
    }

    fn io_error(error: &std::io::Error) -> SparqlResultsError {
        SparqlResultsError::Tsv(TsvError::Io(error.to_string()))
    }
}
