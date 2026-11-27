//! Public TSV parsing APIs and the bounded materialized sink.

use tokio::io::AsyncRead;

use super::reader::TsvReader;
use crate::types::SparqlResult;
use crate::{
    Result,
    ResultHead,
    ResultRow,
};

/// Summary returned after a streaming TSV parse completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TsvStreamSummary {
    /// Number of rows emitted.
    pub row_count: u64,
}

/// Async sink for streaming TSV events. TSV carries only SELECT results.
#[async_trait::async_trait]
pub trait TsvStreamSink {
    /// Accept the header before any rows are emitted.
    async fn select_header(&mut self, head: ResultHead) -> Result<()>;
    /// Accept exactly one row.
    async fn select_row(&mut self, row: ResultRow) -> Result<()>;
}

/// Parse a bounded TSV document into the materialized result model.
///
/// This API retains every row in a `Vec` and is intended only for callers
/// that already enforce a small response limit. Network result streams must
/// use [`parse_tsv_streaming`] instead.
///
/// # Errors
///
/// Returns an error when the document is not valid SPARQL Results TSV.
pub async fn parse_tsv_bounded<R>(reader: R) -> Result<SparqlResult>
where
    R: AsyncRead + Send,
{
    let mut sink = MaterializedSink::default();
    parse_tsv_streaming(reader, &mut sink).await?;
    Ok(SparqlResult::Select {
        head: sink.head,
        rows: sink.rows,
    })
}

/// Parse TSV directly from an async reader, one line at a time, awaiting
/// each sink operation before reading the next line.
///
/// Every term is checked as it is read, as the SRJ and SRX readers do:
/// literal annotations, and a triple term's subject and predicate.
///
/// # Errors
///
/// Returns [`crate::SparqlResultsError::Tsv`] for malformed input,
/// [`crate::SparqlResultsError::InvalidHeader`],
/// [`crate::SparqlResultsError::InvalidLiteral`] or
/// [`crate::SparqlResultsError::InvalidRow`] for an invalid header or term,
/// or the sink's own error.
pub async fn parse_tsv_streaming<R, S>(reader: R, sink: &mut S) -> Result<TsvStreamSummary>
where
    R: AsyncRead + Send,
    S: TsvStreamSink + Send,
{
    TsvReader::new(reader).parse_document(sink).await
}

#[derive(Default)]
struct MaterializedSink {
    head: ResultHead,
    rows: Vec<ResultRow>, // bounded: explicitly requested materialized API
}

#[async_trait::async_trait]
impl TsvStreamSink for MaterializedSink {
    async fn select_header(&mut self, head: ResultHead) -> Result<()> {
        self.head = head;
        Ok(())
    }

    async fn select_row(&mut self, row: ResultRow) -> Result<()> {
        self.rows.push(row);
        Ok(())
    }
}
