//! What distinguishes CSV from TSV: field spelling, separator, line end.

use crate::{
    Result,
    ResultHead,
    ResultRow,
    ResultValue,
    SparqlResultsError,
    TabularFormat,
};

/// Seals [`Dialect`]: only this crate's CSV and TSV dialects implement it.
pub mod sealed {
    /// The private supertrait of [`super::Dialect`].
    pub trait Sealed {}
}

/// The spelling rules of one delimited format.
///
/// Implementors supply how one header field and one RDF term are written;
/// the default methods lay out a whole header line or row line. Only
/// [`crate::CsvDialect`] and [`crate::TsvDialect`] implement it.
pub trait Dialect: sealed::Sealed {
    /// Which format this dialect writes.
    const FORMAT: TabularFormat;
    /// The field separator.
    const SEPARATOR: u8;
    /// The line terminator.
    const EOL: &'static [u8];

    /// Append the header field for the variable `var`.
    fn push_header_field(var: &str, out: &mut Vec<u8>);

    /// Append the field for one bound value. The value has passed
    /// [`ResultValue::check`] and [`Dialect::check_value`].
    fn push_value(value: &ResultValue, out: &mut Vec<u8>);

    /// Refuse a value this dialect cannot spell, before any byte of its row
    /// is written. The default accepts every value.
    ///
    /// # Errors
    ///
    /// Returns the dialect's refusal for `value`.
    fn check_value(value: &ResultValue) -> Result<()> {
        let _ = value;
        Ok(())
    }

    /// Wrap an I/O failure of the underlying writer.
    fn io_error(error: &std::io::Error) -> SparqlResultsError;

    /// Check that `row` can be written under `head` in this dialect.
    ///
    /// # Errors
    ///
    /// Returns the first refusal: an undeclared variable, an invalid term,
    /// or a term this dialect cannot spell.
    fn check_row(head: &ResultHead, row: &ResultRow) -> Result<()> {
        row.check_against(head)?;
        row.bindings.values().try_for_each(Self::check_value)
    }

    /// Append the header line: every variable in declaration order.
    fn push_header(head: &ResultHead, out: &mut Vec<u8>) {
        for (index, var) in head.vars().iter().enumerate() {
            if index > 0 {
                out.push(Self::SEPARATOR);
            }
            Self::push_header_field(var, out);
        }
        out.extend_from_slice(Self::EOL);
    }

    /// Append one row line; an unbound variable is an empty field.
    fn push_row(head: &ResultHead, row: &ResultRow, out: &mut Vec<u8>) {
        for (index, var) in head.vars().iter().enumerate() {
            if index > 0 {
                out.push(Self::SEPARATOR);
            }
            if let Some(value) = row.bindings.get(var) {
                Self::push_value(value, out);
            }
        }
        out.extend_from_slice(Self::EOL);
    }
}
