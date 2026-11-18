//! The bounded TSV adapter.

use tokio::io::AsyncWrite;

use super::TsvDialect;
use crate::Result;
use crate::tabular::write_delimited;
use crate::types::SparqlResult;

/// Serialize a bounded/materialized SELECT result as SPARQL Results TSV.
///
/// Large result sets should create [`crate::TsvWriter`] and call
/// [`crate::TsvWriter::write_row`] once per row.
///
/// The TSV format defines only SELECT results (its scope, §1), so an ASK
/// result is refused with [`crate::SparqlResultsError::AskNotTabular`]. That
/// refusal, and every row check, happens before the first byte is written.
///
/// # Errors
///
/// Returns an error for an ASK result, a row that cannot be written under
/// its header, or an I/O failure.
pub async fn write_tsv<W>(writer: W, result: &SparqlResult) -> Result<W>
where
    W: AsyncWrite + Unpin,
{
    write_delimited::<W, TsvDialect>(writer, result).await
}
