//! The CSV [`Dialect`].

use super::field::CsvField;
use crate::tabular::{
    DelimitedWriter,
    Dialect,
    sealed,
};
use crate::{
    ResultValue,
    SparqlResultsError,
    TabularFormat,
};

/// The CSV spelling rules (§3 of SPARQL 1.2 Query Results CSV and TSV).
#[derive(Debug, Clone, Copy)]
pub struct CsvDialect;

/// Incremental SPARQL Results CSV SELECT serializer.
pub type CsvWriter<W> = DelimitedWriter<W, CsvDialect>;

impl sealed::Sealed for CsvDialect {}

impl Dialect for CsvDialect {
    const FORMAT: TabularFormat = TabularFormat::Csv;
    const SEPARATOR: u8 = b',';
    /// §2: CSV lines end with CRLF (U+000D U+000A).
    const EOL: &'static [u8] = b"\r\n";

    /// §3.1: variable names "without leading question marks".
    fn push_header_field(var: &str, out: &mut Vec<u8>) {
        CsvField::new(var).push_to(out);
    }

    fn push_value(value: &ResultValue, out: &mut Vec<u8>) {
        let mut text = String::new(); // bounded: one value
        CsvField::push_term(value, &mut text);
        CsvField::new(&text).push_to(out);
    }

    fn io_error(error: &std::io::Error) -> SparqlResultsError {
        SparqlResultsError::Csv(error.to_string())
    }
}
