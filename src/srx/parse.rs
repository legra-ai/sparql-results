//! Bounded/materialized SRX parsing.

use super::stream::{
    SrxStreamSink,
    parse_srx_streaming,
};
use crate::types::SparqlResult;
use crate::{
    Result,
    ResultHead,
    ResultRow,
};

/// Parse a bounded SRX document into the materialized result model.
///
/// This API retains every row in a `Vec` and is intended only for callers
/// that already enforce a small response limit. Network result streams must
/// use [`super::stream::parse_srx_streaming`] instead.
///
/// # Errors
///
/// Returns an error when the reader yields malformed XML or the document is
/// not a valid SPARQL Results XML document.
pub async fn parse_srx_bounded<R>(reader: R) -> Result<SparqlResult>
where
    R: tokio::io::AsyncRead + Send,
{
    let mut sink = MaterializedSink::default();
    parse_srx_streaming(reader, &mut sink).await?;
    Ok(sink.result())
}

#[derive(Default)]
struct MaterializedSink {
    head: ResultHead,
    rows: Vec<ResultRow>, // bounded: explicitly requested materialized API
    ask: Option<bool>,
}

impl MaterializedSink {
    fn result(self) -> SparqlResult {
        match self.ask {
            Some(result) => SparqlResult::Ask {
                head: self.head,
                result,
            },
            None => SparqlResult::Select {
                head: self.head,
                rows: self.rows,
            },
        }
    }
}

#[async_trait::async_trait]
impl SrxStreamSink for MaterializedSink {
    async fn select_header(&mut self, head: ResultHead) -> Result<()> {
        self.head = head;
        Ok(())
    }

    async fn select_row(&mut self, row: ResultRow) -> Result<()> {
        self.rows.push(row);
        Ok(())
    }

    async fn ask(&mut self, head: ResultHead, result: bool) -> Result<()> {
        self.head = head;
        self.ask = Some(result);
        Ok(())
    }
}
