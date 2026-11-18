//! Incremental CSV / TSV serialization to Tokio async writers.

use std::marker::PhantomData;

use tokio::io::{
    AsyncWrite,
    AsyncWriteExt,
};

use super::Dialect;
use crate::types::SparqlResult;
use crate::{
    Result,
    ResultHead,
    ResultRow,
    SparqlResultsError,
};

/// Incremental SELECT serializer for a delimited format; use it as
/// [`crate::CsvWriter`] or [`crate::TsvWriter`].
///
/// Each row is checked, rendered into a row-sized buffer, and written with a
/// single awaited write, so memory does not grow with the row count and a
/// refused row writes nothing.
pub struct DelimitedWriter<W: AsyncWrite + Unpin, D: Dialect> {
    writer: W,
    head: ResultHead,
    line: Vec<u8>, // bounded: one row, reused
    finished: bool,
    dialect: PhantomData<D>,
}

impl<W: AsyncWrite + Unpin, D: Dialect> DelimitedWriter<W, D> {
    /// Start a SELECT result and write its header line.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying writer rejects any output.
    pub async fn start_select(mut writer: W, head: &ResultHead) -> Result<Self> {
        let mut line = Vec::new();
        D::push_header(head, &mut line);
        writer
            .write_all(&line)
            .await
            .map_err(|error| D::io_error(&error))?;
        Ok(Self {
            writer,
            head: head.clone(),
            line,
            finished: false,
            dialect: PhantomData,
        })
    }

    /// Write one row line and await completion before returning.
    ///
    /// The row is checked against the header first; a refused row writes
    /// nothing.
    ///
    /// # Errors
    ///
    /// Returns an error if the row binds an undeclared variable, holds an
    /// invalid term or one the format cannot spell, the writer rejects
    /// output, or this writer has already been finished.
    pub async fn write_row(&mut self, row: &ResultRow) -> Result<()> {
        self.ensure_open()?;
        D::check_row(&self.head, row)?;
        self.line.clear();
        D::push_row(&self.head, row, &mut self.line);
        self.writer
            .write_all(&self.line)
            .await
            .map_err(|error| D::io_error(&error))
    }

    /// Flush and return the underlying writer. Both formats end with the
    /// last line's terminator, so nothing more is written.
    ///
    /// # Errors
    ///
    /// Returns an error if the flush fails or this writer has already been
    /// finished.
    pub async fn finish(mut self) -> Result<W> {
        self.ensure_open()?;
        self.finished = true;
        self.writer
            .flush()
            .await
            .map_err(|error| D::io_error(&error))?;
        Ok(self.writer)
    }

    fn ensure_open(&self) -> Result<()> {
        if self.finished {
            Err(SparqlResultsError::InvalidDocument(format!(
                "{} writer already finished",
                D::FORMAT
            )))
        } else {
            Ok(())
        }
    }
}

/// Serialize a bounded result in dialect `D`; see [`crate::bounded::write_csv`]
/// and [`crate::bounded::write_tsv`].
///
/// # Errors
///
/// Returns [`SparqlResultsError::AskNotTabular`] for an ASK result, the
/// first refused row, or an I/O failure. Refusals are found before the first
/// byte is written.
pub async fn write_delimited<W, D>(writer: W, result: &SparqlResult) -> Result<W>
where
    W: AsyncWrite + Unpin,
    D: Dialect,
{
    let SparqlResult::Select { head, rows } = result else {
        return Err(SparqlResultsError::AskNotTabular(D::FORMAT));
    };
    rows.iter().try_for_each(|row| D::check_row(head, row))?;
    let mut writer = DelimitedWriter::<W, D>::start_select(writer, head).await?;
    for row in rows {
        writer.write_row(row).await?;
    }
    writer.finish().await
}
