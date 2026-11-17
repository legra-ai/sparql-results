//! The annotations a reader collects for one literal, validated on
//! conversion into a [`ResultValue`].

use super::LiteralError;
use super::language_tag::LanguageTagSyntax;
use crate::types::BaseDirection;
use crate::{
    ResultValue,
    SparqlResultsError,
};

const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";
const RDF_DIR_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString";

/// Language tag, datatype, and base direction read for one literal.
#[derive(Debug, Default)]
pub(crate) struct LiteralAnnotations {
    lang: Option<String>,
    datatype: Option<String>,
    dir: Option<BaseDirection>,
}

impl LiteralAnnotations {
    /// Create annotations from the raw values a reader found.
    pub(crate) fn new(
        lang: Option<String>,
        datatype: Option<String>,
        dir: Option<BaseDirection>,
    ) -> Self {
        Self {
            lang,
            datatype,
            dir,
        }
    }

    /// Validate the annotations and build the literal value.
    ///
    /// # Errors
    /// Returns [`SparqlResultsError::InvalidLiteral`] when the direction
    /// lacks a language, the datatype contradicts the language, or the
    /// language tag is not well-formed BCP 47.
    pub(crate) fn into_literal(self, value: String) -> Result<ResultValue, SparqlResultsError> {
        self.validate()?;
        Ok(ResultValue::Literal {
            value,
            lang: self.lang,
            datatype: self.datatype,
            dir: self.dir,
        })
    }

    fn validate(&self) -> Result<(), LiteralError> {
        LiteralCheck::new(self.lang.as_deref(), self.datatype.as_deref(), self.dir).validate()
    }
}

/// Borrowed literal annotations, checked without taking ownership; used by
/// the readers (through [`LiteralAnnotations`]) and by the writers.
pub(crate) struct LiteralCheck<'a> {
    lang: Option<&'a str>,
    datatype: Option<&'a str>,
    dir: Option<BaseDirection>,
}

impl<'a> LiteralCheck<'a> {
    pub(crate) const fn new(
        lang: Option<&'a str>,
        datatype: Option<&'a str>,
        dir: Option<BaseDirection>,
    ) -> Self {
        Self {
            lang,
            datatype,
            dir,
        }
    }

    /// Validate the annotations.
    ///
    /// # Errors
    /// Returns the [`LiteralError`] describing the first inconsistency.
    pub(crate) fn validate(&self) -> Result<(), LiteralError> {
        match self.lang {
            Some(lang) => {
                if !LanguageTagSyntax::new(lang).is_well_formed() {
                    return Err(LiteralError::InvalidLanguageTag(lang.to_owned()));
                }
                self.validate_language_datatype()
            }
            None if self.dir.is_some() => Err(LiteralError::DirectionWithoutLanguage),
            None => self.validate_plain_datatype(),
        }
    }

    fn implied_datatype(&self) -> &'static str {
        if self.dir.is_some() {
            RDF_DIR_LANG_STRING
        } else {
            RDF_LANG_STRING
        }
    }

    fn validate_language_datatype(&self) -> Result<(), LiteralError> {
        let expected = self.implied_datatype();
        match self.datatype {
            Some(datatype) if datatype != expected => {
                Err(LiteralError::DatatypeContradictsLanguage {
                    datatype: datatype.to_owned(),
                    expected,
                })
            }
            _ => Ok(()),
        }
    }

    fn validate_plain_datatype(&self) -> Result<(), LiteralError> {
        match self.datatype {
            Some(datatype) if datatype == RDF_LANG_STRING || datatype == RDF_DIR_LANG_STRING => {
                Err(LiteralError::DatatypeRequiresLanguage {
                    datatype: datatype.to_owned(),
                })
            }
            _ => Ok(()),
        }
    }
}
