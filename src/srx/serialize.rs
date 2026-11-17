//! Incremental SRX serialization to Tokio async writers.

use std::future::Future;
use std::pin::Pin;

use quick_xml::Writer;
use quick_xml::events::{
    BytesDecl,
    BytesEnd,
    BytesStart,
    BytesText,
    Event,
};
use tokio::io::{
    AsyncWrite,
    AsyncWriteExt,
};

use crate::types::SparqlResult;
use crate::{
    Result,
    ResultHead,
    ResultRow,
    ResultValue,
    SparqlResultsError,
};

const SPARQL_NS: &str = "http://www.w3.org/2005/sparql-results#";
/// W3C Internationalization Tag Set namespace, source of the `its:dir`
/// attribute used to carry an RDF 1.2 literal base direction in SRX.
const ITS_NS: &str = "http://www.w3.org/2005/11/its";

/// Incremental SRX SELECT serializer.
pub struct SrxWriter<W: AsyncWrite + Unpin> {
    writer: W,
    head: ResultHead,
    finished: bool,
}

impl<W: AsyncWrite + Unpin + Send> SrxWriter<W> {
    /// Start a SELECT document and write its header.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying writer rejects any output.
    pub async fn start_select(mut writer: W, head: &ResultHead) -> Result<Self> {
        write_open_and_head(&mut writer, head).await?;
        write_event(&mut writer, Event::Start(BytesStart::new("results"))).await?;
        Ok(Self {
            writer,
            head: head.clone(),
            finished: false,
        })
    }

    /// Write one result row and await all output writes.
    ///
    /// The row is checked against the header first; a refused row writes
    /// nothing.
    ///
    /// # Errors
    ///
    /// Returns an error if the row binds an undeclared variable or holds an
    /// invalid term, the writer rejects output, or this writer has already
    /// been finished.
    pub async fn write_row(&mut self, row: &ResultRow) -> Result<()> {
        if self.finished {
            return Err(SparqlResultsError::InvalidDocument(
                "SRX writer already finished".to_owned(),
            ));
        }
        row.check_against(&self.head)?;
        write_event(&mut self.writer, Event::Start(BytesStart::new("result"))).await?;
        for (var, value) in &row.bindings {
            let mut binding = BytesStart::new("binding");
            binding.push_attribute(("name", var.as_str()));
            write_event(&mut self.writer, Event::Start(binding)).await?;
            write_value(&mut self.writer, value).await?;
            write_event(&mut self.writer, Event::End(BytesEnd::new("binding"))).await?;
        }
        write_event(&mut self.writer, Event::End(BytesEnd::new("result"))).await
    }

    /// Finish the document and return the underlying writer.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer rejects the closing document or this
    /// writer has already been finished.
    pub async fn finish(mut self) -> Result<W> {
        if self.finished {
            return Err(SparqlResultsError::InvalidDocument(
                "SRX writer already finished".to_owned(),
            ));
        }
        self.finished = true;
        write_event(&mut self.writer, Event::End(BytesEnd::new("results"))).await?;
        write_event(&mut self.writer, Event::End(BytesEnd::new("sparql"))).await?;
        self.writer
            .write_all(b"\n")
            .await
            .map_err(|error| xml_io(&error))?;
        self.writer.flush().await.map_err(|error| xml_io(&error))?;
        Ok(self.writer)
    }
}

/// Serialize a bounded/materialized result directly to an async writer.
///
/// The result itself contains a `Vec` and is therefore suitable only for
/// bounded callers. Large result sets should create [`SrxWriter`] and call
/// [`SrxWriter::write_row`] once per row.
///
/// The whole result is checked before the first byte is written, so a
/// refused result writes nothing.
///
/// # Errors
///
/// Returns an error if the result is invalid (an ASK header with
/// variables, or a row that cannot be written under its header) or the
/// underlying writer rejects output.
pub async fn write_srx<W>(writer: W, result: &SparqlResult) -> Result<W>
where
    W: AsyncWrite + Unpin + Send,
{
    result.check()?;
    match result {
        SparqlResult::Select { head, rows } => {
            let mut writer = SrxWriter::start_select(writer, head).await?;
            for row in rows {
                writer.write_row(row).await?;
            }
            writer.finish().await
        }
        SparqlResult::Ask { head, result } => {
            let mut writer = writer;
            write_open_and_head(&mut writer, head).await?;
            write_event(&mut writer, Event::Start(BytesStart::new("boolean"))).await?;
            write_event(
                &mut writer,
                Event::Text(BytesText::new(if *result { "true" } else { "false" })),
            )
            .await?;
            write_event(&mut writer, Event::End(BytesEnd::new("boolean"))).await?;
            write_event(&mut writer, Event::End(BytesEnd::new("sparql"))).await?;
            writer
                .write_all(b"\n")
                .await
                .map_err(|error| xml_io(&error))?;
            writer.flush().await.map_err(|error| xml_io(&error))?;
            Ok(writer)
        }
    }
}

/// Write the XML declaration, the `<sparql>` root, and the `<head>`.
async fn write_open_and_head<W>(writer: &mut W, head: &ResultHead) -> Result<()>
where
    W: AsyncWrite + Unpin + Send,
{
    write_event(writer, Event::Decl(BytesDecl::new("1.0", None, None))).await?;
    let mut sparql = BytesStart::new("sparql");
    sparql.push_attribute(("xmlns", SPARQL_NS));
    write_event(writer, Event::Start(sparql)).await?;
    if head.vars().is_empty() && head.links().is_empty() {
        return write_event(writer, Event::Empty(BytesStart::new("head"))).await;
    }
    write_event(writer, Event::Start(BytesStart::new("head"))).await?;
    for var in head.vars() {
        let mut variable = BytesStart::new("variable");
        variable.push_attribute(("name", var.as_str()));
        write_event(writer, Event::Empty(variable)).await?;
    }
    for href in head.links() {
        let mut link = BytesStart::new("link");
        link.push_attribute(("href", href.as_str()));
        write_event(writer, Event::Empty(link)).await?;
    }
    write_event(writer, Event::End(BytesEnd::new("head"))).await
}

fn write_value<'a, W>(
    writer: &'a mut W,
    value: &'a ResultValue,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>
where
    W: AsyncWrite + Unpin + Send,
{
    Box::pin(async move {
        match value {
            ResultValue::Uri(value) => {
                write_event(writer, Event::Start(BytesStart::new("uri"))).await?;
                write_event(writer, Event::Text(BytesText::new(value))).await?;
                write_event(writer, Event::End(BytesEnd::new("uri"))).await?;
            }
            ResultValue::Literal {
                value,
                lang,
                datatype,
                dir,
            } => {
                let mut literal = BytesStart::new("literal");
                if let Some(lang) = lang {
                    literal.push_attribute(("xml:lang", lang.as_str()));
                }
                if let Some(dir) = dir {
                    literal.push_attribute(("its:dir", dir.as_str()));
                    literal.push_attribute(("xmlns:its", ITS_NS));
                }
                if let Some(datatype) = datatype {
                    literal.push_attribute(("datatype", datatype.as_str()));
                }
                write_event(writer, Event::Start(literal)).await?;
                write_event(writer, Event::Text(BytesText::new(value))).await?;
                write_event(writer, Event::End(BytesEnd::new("literal"))).await?;
            }
            ResultValue::BNode(value) => {
                write_event(writer, Event::Start(BytesStart::new("bnode"))).await?;
                write_event(writer, Event::Text(BytesText::new(value))).await?;
                write_event(writer, Event::End(BytesEnd::new("bnode"))).await?;
            }
            ResultValue::Triple {
                subject,
                predicate,
                object,
            } => {
                write_event(writer, Event::Start(BytesStart::new("triple"))).await?;
                write_triple_component(writer, "subject", subject).await?;
                write_triple_component(writer, "predicate", predicate).await?;
                write_triple_component(writer, "object", object).await?;
                write_event(writer, Event::End(BytesEnd::new("triple"))).await?;
            }
        }
        Ok(())
    })
}

async fn write_triple_component<W>(writer: &mut W, name: &str, value: &ResultValue) -> Result<()>
where
    W: AsyncWrite + Unpin + Send,
{
    write_event(writer, Event::Start(BytesStart::new(name))).await?;
    write_value(writer, value).await?;
    write_event(writer, Event::End(BytesEnd::new(name))).await
}

async fn write_event<W>(writer: &mut W, event: Event<'_>) -> Result<()>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut output = Vec::new(); // bounded: one XML event, never a result set.
    Writer::new(&mut output)
        .write_event(event)
        .map_err(|error| xml_io(&error))?;
    writer
        .write_all(&output)
        .await
        .map_err(|error| xml_io(&error))
}

fn xml_io(error: &std::io::Error) -> SparqlResultsError {
    SparqlResultsError::Xml(error.to_string())
}
