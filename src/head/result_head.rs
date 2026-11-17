//! The validated result header.

use std::collections::HashSet;

use super::HeaderError;
use super::variable_name::VariableNameSyntax;

/// The header of a SPARQL results document: the projected variables, in
/// declaration order, and any `link` IRIs.
///
/// A `ResultHead` is valid by construction: every variable name is a
/// SPARQL `VARNAME` and no variable is declared twice. An ASK result uses a
/// head with no variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResultHead {
    vars: Vec<String>,
    links: Vec<String>,
}

impl ResultHead {
    /// Build a header from variable names and `link` IRIs.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderError::InvalidVariableName`] for a name that is not a
    /// SPARQL `VARNAME`, or [`HeaderError::DuplicateVariable`] when a name
    /// repeats.
    pub fn try_new(vars: Vec<String>, links: Vec<String>) -> Result<Self, HeaderError> {
        let mut seen = HashSet::with_capacity(vars.len()); // bounded: header width
        for var in &vars {
            if !VariableNameSyntax::new(var).is_well_formed() {
                return Err(HeaderError::InvalidVariableName(var.clone()));
            }
            if !seen.insert(var.as_str()) {
                return Err(HeaderError::DuplicateVariable(var.clone()));
            }
        }
        Ok(Self { vars, links })
    }

    /// Build a SELECT header with no links.
    ///
    /// # Errors
    ///
    /// As for [`ResultHead::try_new`].
    pub fn select(vars: Vec<String>) -> Result<Self, HeaderError> {
        Self::try_new(vars, Vec::new())
    }

    /// Build an ASK header carrying only `link` IRIs.
    #[must_use]
    pub const fn ask(links: Vec<String>) -> Self {
        Self {
            vars: Vec::new(),
            links,
        }
    }

    /// The projected variables in declaration order.
    #[must_use]
    pub fn vars(&self) -> &[String] {
        &self.vars
    }

    /// The `link` IRIs in document order.
    #[must_use]
    pub fn links(&self) -> &[String] {
        &self.links
    }

    /// Whether `var` is a declared variable.
    #[must_use]
    pub fn declares(&self, var: &str) -> bool {
        self.vars.iter().any(|declared| declared == var)
    }

    /// Check that this header can head an ASK result.
    ///
    /// # Errors
    ///
    /// Returns [`HeaderError::VariablesInAsk`] when variables are declared.
    pub fn check_ask(&self) -> Result<(), HeaderError> {
        if self.vars.is_empty() {
            Ok(())
        } else {
            Err(HeaderError::VariablesInAsk)
        }
    }
}
