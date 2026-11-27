//! The two delimited result formats.

use std::fmt;

/// One of the two delimited SPARQL result formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TabularFormat {
    /// SPARQL Query Results CSV (`text/csv`).
    Csv,
    /// SPARQL Query Results TSV (`text/tab-separated-values`).
    Tsv,
}

impl TabularFormat {
    /// The short name of the format (`"CSV"` or `"TSV"`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Tsv => "TSV",
        }
    }

    /// The registered media type, with the UTF-8 charset the
    /// specification recommends (§5, §6).
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Csv => "text/csv; charset=utf-8",
            Self::Tsv => "text/tab-separated-values; charset=utf-8",
        }
    }
}

impl fmt::Display for TabularFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
