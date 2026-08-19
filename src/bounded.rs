//! Explicitly bounded, complete-document adapters.
//!
//! These APIs materialize every solution row in [`SparqlResult`]. They are
//! intended only for callers that enforce a small input or already own a
//! bounded result. Unbounded result streams must use the crate-root streaming
//! parsers and incremental writers instead.

pub use crate::srj::{parse_srj_bounded, write_srj};
pub use crate::srx::{parse_srx_bounded, write_srx};
pub use crate::types::SparqlResult;
