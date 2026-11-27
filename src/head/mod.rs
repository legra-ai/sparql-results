//! The result-document header: projected variables and `link` IRIs.

mod error;
mod result_head;
mod variable_name;

pub use error::HeaderError;
pub use result_head::ResultHead;
pub(crate) use variable_name::VariableNameSyntax;
