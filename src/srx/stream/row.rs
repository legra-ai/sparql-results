//! SRX row, value, and XML helper parsing.

use std::future::Future;
use std::pin::Pin;

use indexmap::IndexMap;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use tokio::io::AsyncBufRead;

use super::SrxStreamSink;
use crate::types::BaseDirection;
use crate::{Result, ResultRow, ResultValue, SparqlResultsError};

pub(super) async fn parse_result_row<R>(
    reader: &mut Reader<R>,
    buffer: &mut Vec<u8>,
) -> Result<ResultRow>
where
    R: AsyncBufRead + Unpin + Send,
{
    let mut bindings = IndexMap::new();
    loop {
        let event = reader
            .read_event_into_async(buffer)
            .await
            .map_err(|error| xml_err(&error))?;
        match event {
            Event::Start(e) if local_name(e.name().as_ref()) == b"binding" => {
                let var_name = required_attr(&e, b"name")?;
                let value = parse_binding_value(reader, buffer).await?;
                expect_end(reader, buffer, b"binding").await?;
                bindings.insert(var_name, value);
            }
            Event::Empty(e) if local_name(e.name().as_ref()) == b"binding" => {}
            Event::End(e) if local_name(e.name().as_ref()) == b"result" => {
                return Ok(ResultRow { bindings });
            }
            Event::Text(_) | Event::Comment(_) => {}
            Event::Eof => {
                return Err(SparqlResultsError::MissingElement(
                    "closing SRX result".to_owned(),
                ));
            }
            other => {
                return Err(SparqlResultsError::UnexpectedElement(format!("{other:?}")));
            }
        }
        buffer.clear();
    }
}

pub(super) fn parse_binding_value<'a, R>(
    reader: &'a mut Reader<R>,
    buffer: &'a mut Vec<u8>,
) -> Pin<Box<dyn Future<Output = Result<ResultValue>> + Send + 'a>>
where
    R: AsyncBufRead + Unpin + Send,
{
    Box::pin(async move {
        loop {
            let event = reader
                .read_event_into_async(buffer)
                .await
                .map_err(|error| xml_err(&error))?;
            match event {
                Event::Start(e) => {
                    let start = e.to_owned();
                    return parse_value_from_start(reader, buffer, start).await;
                }
                Event::Empty(e) if local_name(e.name().as_ref()) == b"literal" => {
                    return Ok(ResultValue::Literal {
                        value: String::new(),
                        lang: optional_attr(&e, b"xml:lang")?,
                        datatype: optional_attr(&e, b"datatype")?,
                        dir: optional_dir(&e)?,
                    });
                }
                Event::Text(_) | Event::Comment(_) => {}
                Event::Eof => {
                    return Err(SparqlResultsError::MissingElement(
                        "SRX binding value".to_owned(),
                    ));
                }
                other => {
                    return Err(SparqlResultsError::UnexpectedElement(format!("{other:?}")));
                }
            }
            buffer.clear();
        }
    })
}

async fn parse_value_from_start<R>(
    reader: &mut Reader<R>,
    buffer: &mut Vec<u8>,
    start: BytesStart<'static>,
) -> Result<ResultValue>
where
    R: AsyncBufRead + Unpin + Send,
{
    let tag = local_name(start.name().as_ref()).to_vec();
    match tag.as_slice() {
        b"uri" => Ok(ResultValue::Uri(
            read_text_content(reader, buffer, b"uri").await?,
        )),
        b"literal" => Ok(ResultValue::Literal {
            value: read_text_content(reader, buffer, b"literal").await?,
            lang: optional_attr(&start, b"xml:lang")?,
            datatype: optional_attr(&start, b"datatype")?,
            dir: optional_dir(&start)?,
        }),
        b"bnode" => Ok(ResultValue::BNode(
            read_text_content(reader, buffer, b"bnode").await?,
        )),
        b"triple" => parse_triple(reader, buffer).await,
        other => Err(SparqlResultsError::UnexpectedElement(
            String::from_utf8_lossy(other).into_owned(),
        )),
    }
}

async fn parse_triple<R>(reader: &mut Reader<R>, buffer: &mut Vec<u8>) -> Result<ResultValue>
where
    R: AsyncBufRead + Unpin + Send,
{
    let subject = parse_triple_component(reader, buffer, b"subject").await?;
    let predicate = parse_triple_component(reader, buffer, b"predicate").await?;
    let object = parse_triple_component(reader, buffer, b"object").await?;
    expect_end(reader, buffer, b"triple").await?;
    Ok(ResultValue::Triple {
        subject: Box::new(subject),
        predicate: Box::new(predicate),
        object: Box::new(object),
    })
}

async fn parse_triple_component<R>(
    reader: &mut Reader<R>,
    buffer: &mut Vec<u8>,
    component: &[u8],
) -> Result<ResultValue>
where
    R: AsyncBufRead + Unpin + Send,
{
    loop {
        let event = reader
            .read_event_into_async(buffer)
            .await
            .map_err(|error| xml_err(&error))?;
        match event {
            Event::Start(e) if local_name(e.name().as_ref()) == component => {
                let value = parse_binding_value(reader, buffer).await?;
                expect_end(reader, buffer, component).await?;
                return Ok(value);
            }
            Event::Text(_) | Event::Comment(_) => {}
            Event::Eof => {
                return Err(SparqlResultsError::MissingElement(format!(
                    "SRX triple component {}",
                    String::from_utf8_lossy(component)
                )));
            }
            other => {
                return Err(SparqlResultsError::UnexpectedElement(format!("{other:?}")));
            }
        }
        buffer.clear();
    }
}

pub(super) async fn read_text_content<R>(
    reader: &mut Reader<R>,
    buffer: &mut Vec<u8>,
    end_tag: &[u8],
) -> Result<String>
where
    R: AsyncBufRead + Unpin + Send,
{
    let mut text = String::new();
    loop {
        let event = reader
            .read_event_into_async(buffer)
            .await
            .map_err(|error| xml_err(&error))?;
        match event {
            Event::Text(value) => text.push_str(
                &value
                    .decode()
                    .map_err(|err| SparqlResultsError::Xml(err.to_string()))?,
            ),
            Event::CData(value) => text.push_str(
                &value
                    .decode()
                    .map_err(|err| SparqlResultsError::Xml(err.to_string()))?,
            ),
            Event::GeneralRef(value) => {
                let name = value
                    .decode()
                    .map_err(|err| SparqlResultsError::Xml(err.to_string()))?;
                text.push_str(&resolve_entity(&name));
            }
            Event::End(value) if local_name(value.name().as_ref()) == end_tag => return Ok(text),
            Event::End(_) => {
                return Err(SparqlResultsError::UnexpectedElement(format!(
                    "unexpected SRX closing tag while reading {}",
                    String::from_utf8_lossy(end_tag)
                )));
            }
            Event::Eof => {
                return Err(SparqlResultsError::MissingElement(format!(
                    "closing SRX {}",
                    String::from_utf8_lossy(end_tag)
                )));
            }
            _ => {}
        }
        buffer.clear();
    }
}

pub(super) async fn expect_end<R>(
    reader: &mut Reader<R>,
    buffer: &mut Vec<u8>,
    tag: &[u8],
) -> Result<()>
where
    R: AsyncBufRead + Unpin + Send,
{
    loop {
        let event = reader
            .read_event_into_async(buffer)
            .await
            .map_err(|error| xml_err(&error))?;
        match event {
            Event::End(e) if local_name(e.name().as_ref()) == tag => return Ok(()),
            Event::Eof => {
                return Err(SparqlResultsError::MissingElement(format!(
                    "closing SRX {}",
                    String::from_utf8_lossy(tag)
                )));
            }
            _ => {}
        }
        buffer.clear();
    }
}

fn local_name(raw: &[u8]) -> &[u8] {
    match raw.iter().rposition(|&byte| byte == b':') {
        Some(position) => &raw[position + 1..],
        None => raw,
    }
}

fn required_attr(element: &BytesStart<'_>, name: &[u8]) -> Result<String> {
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

fn optional_attr(element: &BytesStart<'_>, name: &[u8]) -> Result<Option<String>> {
    element
        .try_get_attribute(name)
        .map_err(|err| SparqlResultsError::Xml(err.to_string()))?
        .map(|attribute| {
            attribute
                .unescape_value()
                .map(std::borrow::Cow::into_owned)
                .map_err(|err| SparqlResultsError::Xml(err.to_string()))
        })
        .transpose()
}

/// Read the RDF 1.2 base direction from a `<literal>`'s `its:dir`
/// attribute, regardless of the namespace prefix bound to the ITS
/// namespace. An absent attribute yields `None`; an unrecognized token
/// is reported as an invalid document.
fn optional_dir(element: &BytesStart<'_>) -> Result<Option<BaseDirection>> {
    for attribute in element.attributes() {
        let attribute = attribute.map_err(|err| SparqlResultsError::Xml(err.to_string()))?;
        if local_name(attribute.key.as_ref()) != b"dir" {
            continue;
        }
        let token = attribute
            .unescape_value()
            .map_err(|err| SparqlResultsError::Xml(err.to_string()))?;
        return token
            .parse()
            .map(Some)
            .map_err(|err| SparqlResultsError::InvalidDocument(format!("{err}")));
    }
    Ok(None)
}

pub(super) fn parse_boolean(text: &str) -> Result<bool> {
    match text.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(SparqlResultsError::InvalidDocument(format!(
            "invalid SRX boolean {other:?}"
        ))),
    }
}

fn resolve_entity(name: &str) -> String {
    match name {
        "amp" => "&".to_owned(),
        "lt" => "<".to_owned(),
        "gt" => ">".to_owned(),
        "quot" => "\"".to_owned(),
        "apos" => "'".to_owned(),
        _ if name.starts_with("#x") || name.starts_with("#X") => {
            u32::from_str_radix(&name[2..], 16)
                .ok()
                .and_then(char::from_u32)
                .map_or_else(|| format!("&{name};"), |value| value.to_string())
        }
        _ if name.starts_with('#') => name[1..]
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map_or_else(|| format!("&{name};"), |value| value.to_string()),
        _ => format!("&{name};"),
    }
}

fn xml_err(error: &quick_xml::Error) -> SparqlResultsError {
    SparqlResultsError::Xml(error.to_string())
}

pub(super) async fn send_header_once<S>(
    sink: &mut S,
    vars: &[String],
    sent: &mut bool,
) -> Result<()>
where
    S: SrxStreamSink + Send,
{
    if !*sent {
        sink.select_header(vars.to_vec()).await?;
        *sent = true;
    }
    Ok(())
}
