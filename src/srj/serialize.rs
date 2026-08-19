//! Incremental SRJ serialization to Tokio async writers.

use std::collections::BTreeMap;

use tokio::io::{AsyncWrite, AsyncWriteExt};

use crate::srj::wire::{SrjBinding, SrjTripleValue, SrjValue};
use crate::types::SparqlResult;
use crate::{Result, ResultRow, ResultValue, SparqlResultsError};

/// Incremental SRJ SELECT serializer.
pub struct SrjWriter<W: AsyncWrite + Unpin> {
    writer: W,
    first_row: bool,
    finished: bool,
}

impl<W: AsyncWrite + Unpin> SrjWriter<W> {
    /// Start a SELECT document and write its header.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying writer rejects any output.
    pub async fn start_select(mut writer: W, vars: &[String]) -> Result<Self> {
        writer
            .write_all(br#"{"head":{"vars":"#)
            .await
            .map_err(|error| json_io(&error))?;
        write_json_value(&mut writer, vars).await?;
        writer
            .write_all(br#"},"results":{"bindings":["#)
            .await
            .map_err(|error| json_io(&error))?;
        Ok(Self {
            writer,
            first_row: true,
            finished: false,
        })
    }

    /// Write one binding row and await completion before returning.
    ///
    /// # Errors
    ///
    /// Returns an error if the writer rejects output or this writer has
    /// already been finished.
    pub async fn write_row(&mut self, row: &ResultRow) -> Result<()> {
        if self.finished {
            return Err(SparqlResultsError::InvalidDocument(
                "SRJ writer already finished".to_owned(),
            ));
        }
        if !self.first_row {
            self.writer
                .write_all(b",")
                .await
                .map_err(|error| json_io(&error))?;
        }
        self.first_row = false;
        write_json_value(&mut self.writer, &row_to_binding(row)).await
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
/// # Errors
///
/// Returns an error if the underlying writer rejects output.
pub async fn write_srj<W>(writer: W, result: &SparqlResult) -> Result<W>
where
    W: AsyncWrite + Unpin,
{
    match result {
        SparqlResult::Select { vars, rows } => {
            let mut writer = SrjWriter::start_select(writer, vars).await?;
            for row in rows {
                writer.write_row(row).await?;
            }
            writer.finish().await
        }
        SparqlResult::Ask { result } => {
            let mut writer = writer;
            writer
                .write_all(if *result {
                    br#"{"head":{},"boolean":true}"#
                } else {
                    br#"{"head":{},"boolean":false}"#
                })
                .await
                .map_err(|error| json_io(&error))?;
            writer.flush().await.map_err(|error| json_io(&error))?;
            Ok(writer)
        }
    }
}

async fn write_json_value<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: serde::Serialize + ?Sized,
{
    let bytes =
        serde_json::to_vec(value).map_err(|err| SparqlResultsError::Json(err.to_string()))?;
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
        } => SrjValue::Literal {
            value: value.clone(),
            lang: lang.clone(),
            datatype: datatype.clone(),
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
