//! Row and value validation shared by the SRJ and SRX writers.

use super::RowError;
use crate::literal::LiteralCheck;
use crate::types::SparqlResult;
use crate::{
    ResultHead,
    ResultRow,
    ResultValue,
    SparqlResultsError,
};

impl ResultRow {
    /// Check that this row can be written under `head`: every bound variable
    /// is declared and every value is a valid RDF term.
    ///
    /// # Errors
    ///
    /// Returns [`SparqlResultsError::InvalidRow`] or
    /// [`SparqlResultsError::InvalidLiteral`] describing the first problem.
    pub fn check_against(&self, head: &ResultHead) -> Result<(), SparqlResultsError> {
        for (var, value) in &self.bindings {
            if !head.declares(var) {
                return Err(RowError::UndeclaredVariable(var.clone()).into());
            }
            value.check()?;
        }
        Ok(())
    }
}

impl ResultValue {
    /// Check that this value is a valid RDF term: literal annotations are
    /// consistent, and a triple term has an IRI or blank-node subject and an
    /// IRI predicate.
    ///
    /// # Errors
    ///
    /// Returns [`SparqlResultsError::InvalidRow`] or
    /// [`SparqlResultsError::InvalidLiteral`] describing the first problem.
    pub fn check(&self) -> Result<(), SparqlResultsError> {
        match self {
            Self::Uri(_) | Self::BNode(_) => Ok(()),
            Self::Literal {
                lang,
                datatype,
                dir,
                ..
            } => Ok(LiteralCheck::new(lang.as_deref(), datatype.as_deref(), *dir).validate()?),
            Self::Triple {
                subject,
                predicate,
                object,
            } => {
                if !matches!(**subject, Self::Uri(_) | Self::BNode(_)) {
                    return Err(RowError::TripleSubject.into());
                }
                if !matches!(**predicate, Self::Uri(_)) {
                    return Err(RowError::TriplePredicate.into());
                }
                object.check()
            }
        }
    }
}

impl SparqlResult {
    /// Check the whole result before anything is written: an ASK header
    /// declares no variables, and every SELECT row fits its header.
    ///
    /// # Errors
    ///
    /// Returns the first [`SparqlResultsError`] found.
    pub fn check(&self) -> Result<(), SparqlResultsError> {
        match self {
            Self::Ask { head, .. } => Ok(head.check_ask()?),
            Self::Select { head, rows } => rows.iter().try_for_each(|row| row.check_against(head)),
        }
    }
}
