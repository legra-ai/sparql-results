//! Async, bounded-memory SRX parsing.

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use tokio::io::{AsyncBufRead, AsyncRead, BufReader};

use crate::{Result, ResultRow, SparqlResultsError};

mod row;

use row::{expect_end, parse_boolean, parse_result_row, read_text_content, send_header_once};

/// Streaming SRX result shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrxStreamKind {
    /// SELECT result set.
    Select,
    /// ASK boolean result.
    Ask,
}

/// Summary returned after a streaming SRX parse completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SrxStreamSummary {
    /// Parsed result shape.
    pub kind: SrxStreamKind,
    /// Number of rows emitted; ASK counts as one boolean event.
    pub row_count: u64,
}

/// Async sink for streaming SRX events.
#[async_trait::async_trait]
pub trait SrxStreamSink {
    /// Accept a SELECT header.
    async fn select_header(&mut self, vars: Vec<String>) -> Result<()>;
    /// Accept exactly one SELECT row.
    async fn select_row(&mut self, row: ResultRow) -> Result<()>;
    /// Accept an ASK result.
    async fn ask(&mut self, result: bool) -> Result<()>;
}

/// Parse SRX directly from an async reader, awaiting each sink operation
/// before reading the next XML event.
///
/// # Errors
///
/// Returns an error when the reader yields malformed XML, the document is not
/// a valid SPARQL Results XML document, or the sink rejects an event.
pub async fn parse_srx_streaming<R, S>(reader: R, sink: &mut S) -> Result<SrxStreamSummary>
where
    R: AsyncRead + Send,
    S: SrxStreamSink + Send,
{
    parse_srx_buf_reader(BufReader::new(Box::pin(reader)), sink).await
}

pub(crate) async fn parse_srx_buf_reader<R, S>(reader: R, sink: &mut S) -> Result<SrxStreamSummary>
where
    R: AsyncBufRead + Unpin + Send,
    S: SrxStreamSink + Send,
{
    let mut reader = Reader::from_reader(reader);
    let mut buffer = Vec::new();
    let mut vars = Vec::new();
    let mut row_count = 0_u64;
    let mut saw_results = false;
    let mut header_sent = false;
    let mut boolean_value = None;
    let mut root_closed = false;
    let mut results_closed = false;

    loop {
        let event = reader
            .read_event_into_async(&mut buffer)
            .await
            .map_err(|error| SparqlResultsError::Xml(error.to_string()))?;
        match event {
            Event::Start(e) => match local_name(e.name().as_ref()) {
                b"sparql" | b"head" => {}
                b"link" => expect_end(&mut reader, &mut buffer, b"link").await?,
                b"results" => {
                    saw_results = true;
                    send_header_once(sink, &vars, &mut header_sent).await?;
                }
                b"result" => {
                    let row = parse_result_row(&mut reader, &mut buffer).await?;
                    sink.select_row(row).await?;
                    row_count = row_count.checked_add(1).ok_or_else(|| {
                        SparqlResultsError::InvalidDocument("SRX row count overflow".to_owned())
                    })?;
                }
                b"boolean" => {
                    let text = read_text_content(&mut reader, &mut buffer, b"boolean").await?;
                    let result = parse_boolean(&text)?;
                    sink.ask(result).await?;
                    if boolean_value.replace(result).is_some() {
                        return Err(SparqlResultsError::InvalidDocument(
                            "SRX contains multiple boolean results".to_owned(),
                        ));
                    }
                }
                other => {
                    return Err(SparqlResultsError::UnexpectedElement(
                        String::from_utf8_lossy(other).into_owned(),
                    ));
                }
            },
            Event::Empty(e) => match local_name(e.name().as_ref()) {
                b"head" | b"link" => {}
                b"variable" => vars.push(required_attr(&e, b"name")?),
                b"results" => {
                    saw_results = true;
                    send_header_once(sink, &vars, &mut header_sent).await?;
                    results_closed = true;
                }
                other => {
                    return Err(SparqlResultsError::UnexpectedElement(
                        String::from_utf8_lossy(other).into_owned(),
                    ));
                }
            },
            Event::Eof => break,
            Event::End(e) => match local_name(e.name().as_ref()) {
                b"sparql" => root_closed = true,
                b"results" => results_closed = true,
                _ => {}
            },
            Event::Text(_)
            | Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_) => {}
        }
        buffer.clear();
    }

    finish_summary(
        root_closed,
        saw_results,
        results_closed,
        boolean_value,
        row_count,
    )
}

fn finish_summary(
    root_closed: bool,
    saw_results: bool,
    results_closed: bool,
    boolean_value: Option<bool>,
    row_count: u64,
) -> Result<SrxStreamSummary> {
    if !root_closed {
        return Err(SparqlResultsError::MissingElement(
            "closing SRX sparql".to_owned(),
        ));
    }
    if saw_results && !results_closed {
        return Err(SparqlResultsError::MissingElement(
            "closing SRX results".to_owned(),
        ));
    }

    match (boolean_value.is_some(), saw_results) {
        (true, _) => Ok(SrxStreamSummary {
            kind: SrxStreamKind::Ask,
            row_count: 1,
        }),
        (false, true) => Ok(SrxStreamSummary {
            kind: SrxStreamKind::Select,
            row_count,
        }),
        (false, false) => Err(SparqlResultsError::MissingElement(
            "SRX results or boolean".to_owned(),
        )),
    }
}

fn local_name(raw: &[u8]) -> &[u8] {
    match raw.iter().rposition(|&byte| byte == b':') {
        Some(position) => &raw[position + 1..],
        None => raw,
    }
}

fn required_attr(element: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Result<String> {
    let attribute = element
        .try_get_attribute(name)
        .map_err(|err| SparqlResultsError::Xml(err.to_string()))?
        .ok_or_else(|| {
            SparqlResultsError::MissingElement(format!(
                "SRX attribute {}",
                String::from_utf8_lossy(name)
            ))
        })?;
    Ok(attribute
        .unescape_value()
        .map_err(|err| SparqlResultsError::Xml(err.to_string()))?
        .into_owned())
}
