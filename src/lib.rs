#![doc = include_str!("../README.md")]

//! Standalone parsers and serializers for the W3C SPARQL Query
//! Results interchange formats.
//!
//! This crate owns two formats:
//!
//! - SPARQL Query Results XML Format (quick-xml).
//! - SPARQL Query Results JSON Format (`serde_json`).
//!
//! The public API is async-first: readers consume Tokio async readers,
//! sinks are awaited before the next row is read, and serializers write
//! directly to Tokio async writers. Complete-document materialization is
//! isolated in the explicit [`bounded`] module.

mod types;

pub mod bounded;

mod srj;
mod srx;

pub use srj::{SrjStreamKind, SrjStreamSink, SrjStreamSummary, SrjWriter, parse_srj_streaming};
pub use srx::{
    SrxStreamKind, SrxStreamSink, SrxStreamSummary, SrxWriter, canonicalize_srx,
    parse_srx_streaming,
};
pub use types::{ResultRow, ResultValue};

/// Errors raised while parsing or serializing SPARQL query result
/// documents in SRX or SRJ form.
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
}

/// Result type used by all public parser and serializer operations.
pub type Result<T> = std::result::Result<T, SparqlResultsError>;
