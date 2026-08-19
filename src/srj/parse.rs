//! Public SRJ parsing APIs and the bounded materialized sink.

use tokio::io::AsyncRead;

use super::reader::JsonReader;
use crate::types::SparqlResult;
use crate::{Result, ResultRow};

/// Shape observed by [`parse_srj_streaming`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrjStreamKind {
    /// A SELECT result set.
    Select,
    /// An ASK boolean result.
    Ask,
}

/// Summary returned after a streaming SRJ parse completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrjStreamSummary {
    /// Parsed result shape.
    pub kind: SrjStreamKind,
    /// Number of rows emitted. ASK emits one boolean event.
    pub row_count: u64,
}

/// Async sink for streaming SRJ events.
#[async_trait::async_trait]
pub trait SrjStreamSink {
    /// Accept a SELECT header before any rows are emitted.
    async fn select_header(&mut self, vars: Vec<String>) -> Result<()>;
    /// Accept exactly one SELECT row.
    async fn select_row(&mut self, row: ResultRow) -> Result<()>;
    /// Accept an ASK boolean result.
    async fn ask(&mut self, result: bool) -> Result<()>;
}

/// Parse a bounded SRJ document into the materialized result model.
///
/// This API retains every row in a `Vec` and is intended only for callers
/// that already enforce a small response limit. Network result streams must
/// use [`parse_srj_streaming`] instead.
///
/// # Errors
///
/// Returns an error when the reader yields invalid JSON or the document is
/// not a valid SPARQL Results JSON document.
pub async fn parse_srj_bounded<R>(reader: R) -> Result<SparqlResult>
where
    R: AsyncRead + Send,
{
    let mut sink = MaterializedSink::default();
    parse_srj_streaming(reader, &mut sink).await?;
    Ok(sink.result())
}

/// Parse SRJ directly from an async reader, awaiting each sink operation
/// before reading the next event or row.
///
/// # Errors
///
/// Returns an error when the reader yields invalid JSON, the document is not
/// a valid SPARQL Results JSON document, or the sink rejects an event.
pub async fn parse_srj_streaming<R, S>(reader: R, sink: &mut S) -> Result<SrjStreamSummary>
where
    R: AsyncRead + Send,
    S: SrjStreamSink + Send,
{
    let mut parser = JsonReader::new(Box::pin(reader));
    parser.parse_document(sink).await
}

#[derive(Default)]
struct MaterializedSink {
    vars: Vec<String>,
    rows: Vec<ResultRow>, // bounded: explicitly requested materialized API
    ask: Option<bool>,
}

impl MaterializedSink {
    fn result(self) -> SparqlResult {
        match self.ask {
            Some(result) => SparqlResult::Ask { result },
            None => SparqlResult::Select {
                vars: self.vars,
                rows: self.rows,
            },
        }
    }
}

#[async_trait::async_trait]
impl SrjStreamSink for MaterializedSink {
    async fn select_header(&mut self, vars: Vec<String>) -> Result<()> {
        self.vars = vars;
        Ok(())
    }

    async fn select_row(&mut self, row: ResultRow) -> Result<()> {
        self.rows.push(row);
        Ok(())
    }

    async fn ask(&mut self, result: bool) -> Result<()> {
        self.ask = Some(result);
        Ok(())
    }
}
