#![doc = include_str!("../README.md")]

//! Standalone parsers and serializers for the W3C SPARQL Query
//! Results interchange formats.
//!
//! This crate owns four formats:
//!
//! - SPARQL Query Results XML Format (quick-xml).
//! - SPARQL Query Results JSON Format (`serde_json`).
//! - SPARQL Query Results TSV Format (writer and reader).
//! - SPARQL Query Results CSV Format (writer only; the README explains why CSV
//!   cannot be read back).
//!
//! The public API is async-first: readers consume Tokio async readers,
//! sinks are awaited before the next row is read, and serializers write
//! directly to Tokio async writers. Complete-document materialization is
//! isolated in the explicit [`bounded`] module.

mod head;
mod literal;
mod row;
mod types;

pub mod bounded;

mod csv;
mod srj;
mod srx;
mod tabular;
mod tsv;

pub use csv::{
    CsvDialect,
    CsvWriter,
};
pub use head::{
    HeaderError,
    ResultHead,
};
pub use literal::LiteralError;
pub use row::RowError;
pub use srj::{
    SrjStreamKind,
    SrjStreamSink,
    SrjStreamSummary,
    SrjWriter,
    parse_srj_streaming,
};
pub use srx::{
    SrxStreamKind,
    SrxStreamSink,
    SrxStreamSummary,
    SrxWriter,
    canonicalize_srx,
    parse_srx_streaming,
};
pub use tabular::{
    DelimitedWriter,
    Dialect,
    TabularFormat,
};
pub use tsv::{
    TermSyntaxError,
    TsvDialect,
    TsvError,
    TsvStreamSink,
    TsvStreamSummary,
    TsvWriter,
    parse_tsv_streaming,
};
pub use types::{
    BaseDirection,
    ParseBaseDirectionError,
    ResultRow,
    ResultValue,
};

/// Errors raised while parsing or serializing SPARQL query result
/// documents in SRX, SRJ, CSV or TSV form.
#[derive(Debug, thiserror::Error)]
pub enum SparqlResultsError {
    /// The underlying XML reader or writer failed.
    #[error("XML error: {0}")]
    Xml(String),
    /// The underlying JSON reader or writer failed.
    #[error("JSON error: {0}")]
    Json(String),
    /// An element or field was not valid in the current document context.
    #[error("unexpected element: {0}")]
    UnexpectedElement(String),
    /// A required element or field was absent.
    #[error("missing element: {0}")]
    MissingElement(String),
    /// The document structure is internally inconsistent.
    #[error("invalid document: {0}")]
    InvalidDocument(String),
    /// A literal's language tag, datatype, or base direction is invalid
    /// or contradictory.
    #[error("invalid literal: {0}")]
    InvalidLiteral(#[from] LiteralError),
    /// The result header is invalid, or does not fit the result kind.
    #[error("invalid header: {0}")]
    InvalidHeader(#[from] HeaderError),
    /// A row cannot be written under its header.
    #[error("invalid row: {0}")]
    InvalidRow(#[from] RowError),
    /// The underlying CSV writer failed.
    #[error("CSV error: {0}")]
    Csv(String),
    /// A TSV document or value was refused, or the TSV reader or writer
    /// failed.
    #[error("TSV error: {0}")]
    Tsv(TsvError),
    /// An ASK result was given to a CSV or TSV writer. Those formats define
    /// only SELECT results, so nothing is written.
    #[error("the SPARQL results {0} format has no representation for an ASK result")]
    AskNotTabular(TabularFormat),
}

/// Result type used by all public parser and serializer operations.
pub type Result<T> = std::result::Result<T, SparqlResultsError>;
