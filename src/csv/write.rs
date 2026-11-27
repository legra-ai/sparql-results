//! The bounded CSV adapter.

use tokio::io::AsyncWrite;

use super::CsvDialect;
use crate::Result;
use crate::tabular::write_delimited;
use crate::types::SparqlResult;

/// Serialize a bounded/materialized SELECT result as SPARQL Results CSV.
///
/// Large result sets should create [`crate::CsvWriter`] and call
/// [`crate::CsvWriter::write_row`] once per row.
///
/// The CSV format defines only SELECT results (its scope, §1), so an ASK
/// result is refused with [`crate::SparqlResultsError::AskNotTabular`]. That
/// refusal, and every row check, happens before the first byte is written.
///
/// # Errors
///
/// Returns an error for an ASK result, a row that cannot be written under
/// its header, or an I/O failure.
pub async fn write_csv<W>(writer: W, result: &SparqlResult) -> Result<W>
where
    W: AsyncWrite + Unpin,
{
    write_delimited::<W, CsvDialect>(writer, result).await
}
