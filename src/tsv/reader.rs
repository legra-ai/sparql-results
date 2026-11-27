//! The line-at-a-time TSV reader.

use std::pin::Pin;

use indexmap::IndexMap;
use tokio::io::{
    AsyncBufReadExt,
    AsyncRead,
    BufReader,
};

use super::term_parser::TermParser;
use super::{
    TsvError,
    TsvStreamSink,
    TsvStreamSummary,
};
use crate::{
    Result,
    ResultHead,
    ResultRow,
};

/// Reads one LF-terminated line at a time into a reused buffer, so memory
/// is bounded by the longest line, not the document.
pub(super) struct TsvReader<R: AsyncRead> {
    reader: Pin<Box<BufReader<R>>>,
    line: Vec<u8>, // bounded: one line, reused
    line_number: u64,
}

impl<R: AsyncRead> TsvReader<R> {
    pub(super) fn new(reader: R) -> Self {
        Self {
            reader: Box::pin(BufReader::new(reader)),
            line: Vec::new(),
            line_number: 0,
        }
    }

    /// Read the header, then every row, awaiting the sink for each.
    pub(super) async fn parse_document<S>(&mut self, sink: &mut S) -> Result<TsvStreamSummary>
    where
        S: TsvStreamSink + Send,
    {
        let Some(header) = self.next_line().await? else {
            return Err(TsvError::MissingHeader.into());
        };
        let head = Self::head(header.1)?;
        sink.select_header(head.clone()).await?;
        let mut row_count = 0_u64;
        while let Some((line_number, line)) = self.next_line().await? {
            let row = Self::row(&head, line, line_number)?;
            sink.select_row(row).await?;
            row_count += 1;
        }
        Ok(TsvStreamSummary { row_count })
    }

    /// The next line's 1-based number and text without its LF, or `None` at
    /// the end of input. A last line without an LF is still a line.
    async fn next_line(&mut self) -> Result<Option<(u64, &str)>> {
        self.line.clear();
        let read = self
            .reader
            .as_mut()
            .read_until(b'\n', &mut self.line)
            .await
            .map_err(|error| TsvError::Io(error.to_string()))?;
        if read == 0 {
            return Ok(None);
        }
        self.line_number += 1;
        let line = self.line.strip_suffix(b"\n").unwrap_or(&self.line);
        let number = self.line_number;
        std::str::from_utf8(line)
            .map(|text| Some((number, text)))
            .map_err(|_| {
                TsvError::InvalidUtf8 {
                    line: self.line_number,
                }
                .into()
            })
    }

    /// §4.1: `?`-prefixed variables separated by tabs; an empty line
    /// declares no variables.
    fn head(line: &str) -> Result<ResultHead> {
        if line.is_empty() {
            return Ok(ResultHead::select(Vec::new())?);
        }
        let vars = line // bounded: header width
            .split('\t')
            .map(|field| {
                field
                    .strip_prefix(['?', '$'])
                    .map(str::to_owned)
                    .ok_or_else(|| TsvError::HeaderField(field.to_owned()))
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(ResultHead::select(vars)?)
    }

    /// One row: a field per variable, an empty field leaving it unbound.
    fn row(head: &ResultHead, line: &str, line_number: u64) -> Result<ResultRow> {
        let vars = head.vars();
        let found = if line.is_empty() && vars.is_empty() {
            0
        } else {
            line.split('\t').count()
        };
        if found != vars.len() {
            return Err(TsvError::FieldCount {
                line: line_number,
                expected: vars.len(),
                found,
            }
            .into());
        }
        let mut bindings = IndexMap::new(); // bounded: one row
        for (index, (var, field)) in vars.iter().zip(line.split('\t')).enumerate() {
            if field.is_empty() {
                continue;
            }
            let value = TermParser::parse_field(field).map_err(|reason| TsvError::Term {
                line: line_number,
                field: index + 1,
                reason,
            })?;
            bindings.insert(var.clone(), value);
        }
        let row = ResultRow { bindings };
        row.check_against(head)?;
        Ok(row)
    }
}
