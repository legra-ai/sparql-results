//! Validation of literal annotations (language tag, datatype, base
//! direction) shared by the SRJ and SRX readers.

mod annotations;
mod error;
mod language_tag;

pub(crate) use annotations::LiteralAnnotations;
pub use error::LiteralError;

#[cfg(test)]
mod language_tag_tests;
