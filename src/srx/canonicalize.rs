//! SRX canonical form.
//!
//! W3C `sparql11` SRX fixtures use a mix of surface forms — single
//! vs. double attribute quotes, optional XML declaration, varying
//! inter-element whitespace, and inline vs. indented `<binding>`
//! children — none of which are retained by the bounded parser
//! or emitted by the incremental writer. Direct byte comparison
//! between fixture text and serializer output is therefore not a
//! useful gate.
//!
//! [`canonicalize_srx`] re-emits an SRX document through a fixed
//! [`quick_xml::Writer`] profile so two semantically equivalent
//! documents collapse to identical bytes:
//!
//! - the XML declaration is dropped (SRX does not depend on it);
//! - attribute values are written with `quick_xml`'s default double-quote
//!   style;
//! - pure-whitespace text between element boundaries is dropped (SRX leaf
//!   elements never carry whitespace-only content — `<uri>`, `<literal>`,
//!   `<bnode>`, and `<boolean>` always carry concrete text);
//! - XML comments are dropped (they are presentation metadata and are never
//!   produced by [`crate::write_srx`]);
//! - an element opened with `<x>` and immediately closed with `</x>` (no
//!   children, no text) collapses to the empty form `<x/>`, so `<head></head>`
//!   and `<head/>` canonicalize equal;
//! - everything else is re-emitted verbatim, so meaningful text and attribute
//!   order are preserved.
//!
//! This is **not** XML c14n — it does not normalize namespace
//! declarations, attribute order, or character references. It is a
//! minimal surface-form normalizer scoped to the SRX shape
//! produces and the W3C fixture suite consumes.

use quick_xml::Writer;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};

use crate::SparqlResultsError;

type Result<T> = std::result::Result<T, SparqlResultsError>;

/// Re-emit an SRX document in canonical form so semantically
/// equivalent documents have identical bytes.
///
/// See the module-level docs for the exact normalizations applied.
///
/// # Errors
///
/// Returns [`SparqlResultsError::Xml`] if `input` is not well-formed
/// XML or the underlying writer fails.
pub async fn canonicalize_srx<R, W>(reader: R, mut writer: W) -> Result<W>
where
    R: AsyncRead + Send,
    W: AsyncWrite + Unpin + Send,
{
    let mut reader = Reader::from_reader(BufReader::new(Box::pin(reader)));
    let mut buffer = Vec::new();
    // Holds a Start event we have not yet emitted, so we can fold
    // `<x></x>` into `<x/>` when the next event closes it without
    // any intervening content.
    let mut pending_start: Option<BytesStart<'static>> = None;

    loop {
        match reader
            .read_event_into_async(&mut buffer)
            .await
            .map_err(|error| xml_err(&error))?
        {
            Event::Eof => break,
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Text(text) => {
                let raw = text.as_ref();
                if raw.iter().all(u8::is_ascii_whitespace) {
                    // Pure pretty-printing whitespace between SRX
                    // elements has no semantic content.
                    continue;
                }
                flush_pending(&mut pending_start, &mut writer).await?;
                write_event(&mut writer, Event::Text(text)).await?;
            }
            Event::Start(e) => {
                flush_pending(&mut pending_start, &mut writer).await?;
                pending_start = Some(rebuild_start(&e)?);
            }
            Event::End(e) => {
                if let Some(start) = pending_start.take() {
                    if start.name().as_ref() == e.name().as_ref() {
                        // `<x></x>` with no children — emit as
                        // `<x/>` so it matches the self-closing
                        // surface form.
                        write_event(&mut writer, Event::Empty(start)).await?;
                        continue;
                    }
                    write_event(&mut writer, Event::Start(start)).await?;
                }
                write_event(&mut writer, Event::End(e)).await?;
            }
            Event::Empty(e) => {
                flush_pending(&mut pending_start, &mut writer).await?;
                let rebuilt = rebuild_start(&e)?;
                write_event(&mut writer, Event::Empty(rebuilt)).await?;
            }
            Event::CData(c) => {
                flush_pending(&mut pending_start, &mut writer).await?;
                write_event(&mut writer, Event::CData(c)).await?;
            }
            Event::PI(p) => {
                flush_pending(&mut pending_start, &mut writer).await?;
                write_event(&mut writer, Event::PI(p)).await?;
            }
            Event::DocType(d) => {
                flush_pending(&mut pending_start, &mut writer).await?;
                write_event(&mut writer, Event::DocType(d)).await?;
            }
            Event::GeneralRef(r) => {
                flush_pending(&mut pending_start, &mut writer).await?;
                write_event(&mut writer, Event::GeneralRef(r)).await?;
            }
        }
        buffer.clear();
    }
    flush_pending(&mut pending_start, &mut writer).await?;
    writer.flush().await.map_err(|error| io_err(&error))?;
    Ok(writer)
}

async fn flush_pending<W>(pending: &mut Option<BytesStart<'static>>, writer: &mut W) -> Result<()>
where
    W: AsyncWrite + Unpin + Send,
{
    if let Some(start) = pending.take() {
        write_event(writer, Event::Start(start)).await?;
    }
    Ok(())
}

async fn write_event<W>(writer: &mut W, event: Event<'_>) -> Result<()>
where
    W: AsyncWrite + Unpin + Send,
{
    let mut encoded = Vec::new(); // bounded: one XML event, never a result set.
    Writer::new(&mut encoded)
        .write_event(event)
        .map_err(|error| io_err(&error))?;
    writer
        .write_all(&encoded)
        .await
        .map_err(|error| io_err(&error))
}

/// Rebuild a [`BytesStart`] with attributes pushed via
/// [`BytesStart::push_attribute`] so the writer always emits them
/// with double-quoted values regardless of the source form.
fn rebuild_start(src: &BytesStart<'_>) -> Result<BytesStart<'static>> {
    let name = std::str::from_utf8(src.name().as_ref())
        .map_err(|err| SparqlResultsError::Xml(format!("invalid UTF-8 in element name: {err}")))?
        .to_owned();
    let mut out = BytesStart::new(name);
    for attr in src.attributes() {
        let attr =
            attr.map_err(|err| SparqlResultsError::Xml(format!("invalid attribute: {err}")))?;
        let key = std::str::from_utf8(attr.key.as_ref()).map_err(|err| {
            SparqlResultsError::Xml(format!("invalid UTF-8 in attribute name: {err}"))
        })?;
        let value = attr
            .unescape_value()
            .map_err(|err| SparqlResultsError::Xml(format!("invalid attribute value: {err}")))?;
        out.push_attribute((key, value.as_ref()));
    }
    Ok(out)
}

fn xml_err(err: &quick_xml::Error) -> SparqlResultsError {
    SparqlResultsError::Xml(err.to_string())
}

fn io_err(err: &std::io::Error) -> SparqlResultsError {
    SparqlResultsError::Xml(err.to_string())
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use super::*;

    struct TestWriter(Vec<u8>);

    impl AsyncWrite for TestWriter {
        fn poll_write(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            self.0.extend_from_slice(bytes);
            Poll::Ready(Ok(bytes.len()))
        }

        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Poll::Ready(Ok(()))
        }
    }

    async fn canonical(input: &str) -> String {
        let writer = canonicalize_srx(input.as_bytes(), TestWriter(Vec::new()))
            .await
            .expect("canonicalizes");
        String::from_utf8(writer.0).expect("canonical XML is UTF-8")
    }

    /// Two surface forms of the same SRX document — single-quoted
    /// attributes with inline `<binding>` children versus our
    /// serializer's double-quoted attributes with indented children
    /// — must canonicalize to identical bytes.
    #[tokio::test]
    async fn fixture_form_and_writer_form_canonicalize_equal() {
        let fixture = "<sparql xmlns='http://www.w3.org/2005/sparql-results#'>\n\
                       <head>\n<variable name='x'/>\n</head>\n\
                       <results>\n<result>\n\
                       <binding name='x'><uri>http://ex/a</uri></binding>\n\
                       </result>\n</results>\n</sparql>\n";
        let writer_form = "<?xml version=\"1.0\"?>\n\
                           <sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\n  \
                           <head>\n    <variable name=\"x\"/>\n  </head>\n  \
                           <results>\n    <result>\n      \
                           <binding name=\"x\">\n        \
                           <uri>http://ex/a</uri>\n      </binding>\n    \
                           </result>\n  </results>\n</sparql>\n";

        let a = canonical(fixture).await;
        let b = canonical(writer_form).await;
        assert_eq!(a, b, "canonical forms must match");
    }

    /// Meaningful text content (`<literal>2</literal>`) survives
    /// canonicalization — only pure-whitespace inter-element text
    /// is dropped.
    #[tokio::test]
    async fn meaningful_text_is_preserved() {
        let input = "<sparql xmlns='http://www.w3.org/2005/sparql-results#'>\n\
                     <head><variable name='x'/></head>\n\
                     <results>\n<result><binding name='x'>\
                     <literal datatype='http://www.w3.org/2001/XMLSchema#integer'>2</literal>\
                     </binding></result>\n</results>\n</sparql>\n";
        let canonical = canonical(input).await;
        assert!(
            canonical.contains(">2<"),
            "literal content must survive: {canonical}"
        );
    }

    /// `<x></x>` with no children collapses to `<x/>`, so fixture
    /// `<head></head>` matches our serializer's `<head/>` output.
    #[tokio::test]
    async fn empty_open_close_collapses_to_self_closing() {
        let open_close = "<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\
                          <head></head><boolean>true</boolean></sparql>";
        let self_closing = "<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\
                            <head/><boolean>true</boolean></sparql>";
        assert_eq!(canonical(open_close).await, canonical(self_closing).await,);
    }

    /// The XML declaration is dropped so fixtures with and without
    /// it produce identical canonical bytes.
    #[tokio::test]
    async fn xml_declaration_is_dropped() {
        let with_decl = "<?xml version=\"1.0\"?>\n\
                         <sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\
                         <head/><boolean>true</boolean></sparql>";
        let without_decl = "<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\">\
                            <head/><boolean>true</boolean></sparql>";
        assert_eq!(canonical(with_decl).await, canonical(without_decl).await,);
    }
}
