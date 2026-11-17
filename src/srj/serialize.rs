//! Incremental SRJ serialization to Tokio async writers.

use std::collections::BTreeMap;

use tokio::io::{
    AsyncWrite,
    AsyncWriteExt,
};

use crate::srj::wire::{
    SrjBinding,
    SrjHead,
    SrjTripleValue,
    SrjValue,
};
use crate::types::SparqlResult;
use crate::{
    Result,
    ResultHead,
    ResultRow,
    ResultValue,
    SparqlResultsError,
};

/// Incremental SRJ SELECT serializer.
pub struct SrjWriter<W: AsyncWrite + Unpin> {
    writer: W,
    head: ResultHead,
    first_row: bool,
    finished: bool,
}

impl<W: AsyncWrite + Unpin> SrjWriter<W> {
    /// Start a SELECT document and write its header.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying writer rejects any output.
    pub async fn start_select(mut writer: W, head: &ResultHead) -> Result<Self> {
        write_head(&mut writer, head, Some(head.vars())).await?;
        writer
            .write_all(br#","results":{"bindings":["#)
            .await
            .map_err(|error| json_io(&error))?;
        Ok(Self {
            writer,
            head: head.clone(),
            first_row: true,
            finished: false,
        })
    }

    /// Write one binding row and await completion before returning.
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
                "SRJ writer already finished".to_owned(),
            ));
        }
        row.check_against(&self.head)?;
        let mut bytes = if self.first_row {
            Vec::new()
        } else {
            vec![b',']
        }; // bounded: one row
        serde_json::to_writer(&mut bytes, &row_to_binding(row))
            .map_err(|err| SparqlResultsError::Json(err.to_string()))?;
        self.first_row = false;
        self.writer
            .write_all(&bytes)
            .await
            .map_err(|error| json_io(&error))
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
                "SRJ writer already finished".to_owned(),
            ));
        }
        self.finished = true;
        self.writer
            .write_all(br"]}}")
            .await
            .map_err(|error| json_io(&error))?;
        self.writer.flush().await.map_err(|error| json_io(&error))?;
        Ok(self.writer)
    }
}

/// Serialize a bounded/materialized result directly to an async writer.
///
/// The result itself contains a `Vec` and is therefore suitable only for
/// bounded callers. Large result sets should create [`SrjWriter`] and call
/// [`SrjWriter::write_row`] once per row.
///
/// The whole result is checked before the first byte is written, so a
/// refused result writes nothing.
///
/// # Errors
///
/// Returns an error if the result is invalid (an ASK header with
/// variables, or a row that cannot be written under its header) or the
/// underlying writer rejects output.
pub async fn write_srj<W>(writer: W, result: &SparqlResult) -> Result<W>
where
    W: AsyncWrite + Unpin,
{
    result.check()?;
    match result {
        SparqlResult::Select { head, rows } => {
            let mut writer = SrjWriter::start_select(writer, head).await?;
            for row in rows {
                writer.write_row(row).await?;
            }
            writer.finish().await
        }
        SparqlResult::Ask { head, result } => {
            let mut writer = writer;
            write_head(&mut writer, head, None).await?;
            writer
                .write_all(if *result {
                    br#","boolean":true}"#
                } else {
                    br#","boolean":false}"#
                })
                .await
                .map_err(|error| json_io(&error))?;
            writer.flush().await.map_err(|error| json_io(&error))?;
            Ok(writer)
        }
    }
}

/// Write `{"head":{...}` — the document opening and its header object.
async fn write_head<W>(writer: &mut W, head: &ResultHead, vars: Option<&[String]>) -> Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut bytes = br#"{"head":"#.to_vec(); // bounded: one header
    serde_json::to_writer(
        &mut bytes,
        &SrjHead {
            vars,
            link: head.links(),
        },
    )
    .map_err(|err| SparqlResultsError::Json(err.to_string()))?;
    writer
        .write_all(&bytes)
        .await
        .map_err(|error| json_io(&error))
}

fn row_to_binding(row: &ResultRow) -> SrjBinding {
    let mut binding = BTreeMap::new();
    for (var, value) in &row.bindings {
        binding.insert(var.clone(), result_value_to_wire(value));
    }
    binding
}

fn result_value_to_wire(value: &ResultValue) -> SrjValue {
    match value {
        ResultValue::Uri(iri) => SrjValue::Uri { value: iri.clone() },
        ResultValue::BNode(id) => SrjValue::BNode { value: id.clone() },
        ResultValue::Literal {
            value,
            lang,
            datatype,
            dir,
        } => SrjValue::Literal {
            value: value.clone(),
            lang: lang.clone(),
            datatype: datatype.clone(),
            dir: *dir,
        },
        ResultValue::Triple {
            subject,
            predicate,
            object,
        } => SrjValue::Triple {
            value: Box::new(SrjTripleValue {
                subject: result_value_to_wire(subject),
                predicate: result_value_to_wire(predicate),
                object: result_value_to_wire(object),
            }),
        },
    }
}

fn json_io(error: &std::io::Error) -> SparqlResultsError {
    SparqlResultsError::Json(error.to_string())
}
