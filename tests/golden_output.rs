//! Golden files and exact-bytes checks for the SRJ, SRX, CSV and TSV
//! writers.
//!
//! Comparing decoded models cannot catch a writer and reader that are wrong
//! in the same way, so these tests compare the written bytes. Regenerate the
//! golden files with `UPDATE_GOLDEN=1 cargo test --test golden_output`, then
//! review the diff.

#![allow(missing_docs)]

#[path = "support/cases.rs"]
mod cases;

use cases::GoldenCase;
use indexmap::IndexMap;
use sparql_results::ResultHead;
use sparql_results::bounded::{
    SparqlResult,
    parse_srj_bounded,
    parse_srx_bounded,
    parse_tsv_bounded,
    write_csv,
    write_srj,
    write_srx,
    write_tsv,
};

const GOLDEN: &str = "tests/fixtures/golden";

#[derive(Clone, Copy)]
enum Format {
    Srj,
    Srx,
    Csv,
    Tsv,
}

impl Format {
    const ALL: [Self; 4] = [Self::Srj, Self::Srx, Self::Csv, Self::Tsv];

    const fn extension(self) -> &'static str {
        match self {
            Self::Srj => "srj",
            Self::Srx => "srx",
            Self::Csv => "csv",
            Self::Tsv => "tsv",
        }
    }

    /// CSV and TSV define only SELECT results.
    const fn writes(self, result: &SparqlResult) -> bool {
        matches!(self, Self::Srj | Self::Srx) || matches!(result, SparqlResult::Select { .. })
    }

    async fn write(self, result: &SparqlResult) -> String {
        let bytes = match self {
            Self::Srj => write_srj(Vec::new(), result).await,
            Self::Srx => write_srx(Vec::new(), result).await,
            Self::Csv => write_csv(Vec::new(), result).await,
            Self::Tsv => write_tsv(Vec::new(), result).await,
        }
        .expect("golden case writes");
        String::from_utf8(bytes).expect("UTF-8 output")
    }

    /// What reading the written text gives back, or `None` for CSV, which
    /// has no reader. TSV has no `link` header, so its expected result
    /// drops the links.
    async fn read_back(
        self,
        text: &str,
        written: &SparqlResult,
    ) -> Option<(SparqlResult, SparqlResult)> {
        let read = match self {
            Self::Srj => parse_srj_bounded(text.as_bytes()).await,
            Self::Srx => parse_srx_bounded(text.as_bytes()).await,
            Self::Tsv => parse_tsv_bounded(text.as_bytes()).await,
            Self::Csv => return None,
        }
        .expect("golden file reads");
        let expected = match (self, written) {
            (Self::Tsv, SparqlResult::Select { head, rows }) => SparqlResult::Select {
                head: ResultHead::select(head.vars().to_vec()).expect("valid header"),
                rows: rows.clone(),
            },
            _ => written.clone(),
        };
        Some((read, expected))
    }
}

struct Golden {
    case: GoldenCase,
    format: Format,
}

impl Golden {
    fn all() -> Vec<Self> {
        GoldenCase::all()
            .into_iter()
            .flat_map(|case| {
                Format::ALL
                    .into_iter()
                    .filter(|format| format.writes(&case.result))
                    .map(|format| Self {
                        case: GoldenCase::new(case.name, case.result.clone()),
                        format,
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    fn path(&self) -> String {
        format!("{GOLDEN}/{}.{}", self.case.name, self.format.extension())
    }

    fn expected(&self) -> String {
        std::fs::read_to_string(self.path()).expect("read golden file")
    }
}

#[tokio::test]
async fn writers_match_golden_files() {
    let update = std::env::var_os("UPDATE_GOLDEN").is_some();
    for golden in Golden::all() {
        let written = golden.format.write(&golden.case.result).await;
        if update {
            std::fs::write(golden.path(), &written).expect("write golden file");
        }
        assert_eq!(written, golden.expected(), "{}", golden.path());
        if let Some((read, expected)) = golden.format.read_back(&written, &golden.case.result).await
        {
            assert_eq!(read, expected, "{} reads back", golden.path());
        }
    }
}

#[tokio::test]
async fn writers_are_deterministic() {
    for golden in Golden::all() {
        let first = golden.format.write(&golden.case.result).await;
        let second = golden.format.write(&golden.case.result).await;
        assert_eq!(first, second, "{}", golden.path());
    }
}

#[tokio::test]
async fn srj_binding_order_does_not_depend_on_insertion_order() {
    let SparqlResult::Select { head, rows } = GoldenCase::named("unbound").result else {
        panic!("unbound is a SELECT");
    };
    let mut row = rows[0].clone();
    row.bindings.insert(
        "y".to_owned(),
        sparql_results::ResultValue::BNode("b".to_owned()),
    );
    let mut reversed = row.clone();
    reversed.bindings = row
        .bindings
        .iter()
        .rev()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<IndexMap<_, _>>();
    let write = |row| {
        let result = SparqlResult::Select {
            head: head.clone(),
            rows: vec![row],
        };
        async move { Format::Srj.write(&result).await }
    };
    assert_eq!(write(row).await, write(reversed).await);
}

/// Spellings fixed by the specifications, checked in the golden bytes.
#[test]
fn golden_files_spell_keywords_exactly() {
    let read = |name: &str| std::fs::read_to_string(format!("{GOLDEN}/{name}")).unwrap();

    let directional = read("directional.srj");
    for needle in [
        r#"{"type":"literal","value":"مرحبا","xml:lang":"ar","its:dir":"rtl"}"#,
        r#""its:dir":"ltr""#,
    ] {
        assert!(
            directional.contains(needle),
            "SRJ lacks {needle}: {directional}"
        );
    }
    for banned in [r#""dir":"#, r#""lang":"#, "dirLangString"] {
        assert!(
            !directional.contains(banned),
            "SRJ has {banned}: {directional}"
        );
    }
    assert!(read("nested-triple.srj").contains(r#"{"type":"triple","value":{"subject":"#));
    assert!(read("ask-true.srj").contains(r#""boolean":true}"#));
    assert!(read("ask-false.srj").contains(r#""boolean":false}"#));
    assert!(read("link-select.srj").contains(r#""link":["http://example.org/about","#));

    let directional = read("directional.srx");
    assert!(directional.contains(
        r#"<literal xml:lang="ar" its:dir="rtl" xmlns:its="http://www.w3.org/2005/11/its">"#
    ));
    assert!(
        !directional.contains(" dir="),
        "SRX has bare dir: {directional}"
    );
    let nested = read("nested-triple.srx");
    assert!(nested.contains("<triple><subject><bnode>b0</bnode></subject>"));
    assert!(nested.contains("<object><triple>"));
    assert!(read("ask-true.srx").contains("<boolean>true</boolean>"));
    assert!(read("ask-false.srx").contains("<boolean>false</boolean>"));
    assert!(read("link-select.srx").contains(r#"<link href="http://example.org/about"/>"#));
}

/// Exact CSV bytes: CRLF line ends (§2), bare header names (§3.1), plain
/// lexical values and RFC 4180 quoting (§3.2), and the recursively quoted
/// triple term of the §3.4 example.
#[test]
fn golden_csv_files_have_exact_bytes() {
    let read = |name: &str| std::fs::read_to_string(format!("{GOLDEN}/{name}")).unwrap();
    assert_eq!(
        read("unbound.csv"),
        "x,y\r\nonly x,\r\n,http://example.org/only-y\r\n,\r\n"
    );
    assert_eq!(read("zero-variable.csv"), "\r\n\r\n");
    assert_eq!(read("directional.csv"), "x\r\nمرحبا\r\nHello\r\nHallo\r\n");
    assert_eq!(
        read("nested-triple.csv"),
        "t\r\n\"<<( _:b0 http://example.org/says <<( http://example.org/s \
         http://example.org/p \"\"שלום\"\" )>> )>>\"\r\n"
    );
    let escapes = read("tabular-escapes.csv");
    let lines: Vec<&str> = escapes.split("\r\n").collect();
    assert_eq!(lines[0], "s,o");
    assert_eq!(lines[1], "http://example.org/s,\"comma, here\"");
    assert_eq!(lines[2], "http://example.org/s,\"say \"\"hi\"\"\"");
    assert_eq!(
        lines[3],
        "http://example.org/s,\"line\nbreak\r\ttab \\ slash\""
    );
    assert_eq!(lines[4], "http://example.org/s,42");
    assert_eq!(lines[10], "http://example.org/s,\"5,5\"");
    assert_eq!(lines[11], "http://example.org/a{b},");
    assert_eq!(
        lines[12],
        "http://example.org/carol,\"<<( http://example.org/carol http://example.org/says \
         \"\"Hello world, my name is \"\"\"\"Alice\"\"\"\".\"\" )>>\""
    );
}

/// Exact TSV bytes: LF line ends (§2), `?`-prefixed header (§4.1), Turtle
/// terms with `\t` / `\n` / `\r` escapes, abbreviated numbers, `@lang--dir`
/// and `<<( s p o )>>` (§4.2).
#[test]
fn golden_tsv_files_have_exact_bytes() {
    let read = |name: &str| std::fs::read_to_string(format!("{GOLDEN}/{name}")).unwrap();
    assert_eq!(
        read("unbound.tsv"),
        "?x\t?y\n\"only x\"\t\n\t<http://example.org/only-y>\n\t\n"
    );
    assert_eq!(read("zero-variable.tsv"), "\n\n");
    assert_eq!(
        read("directional.tsv"),
        "?x\n\"مرحبا\"@ar--rtl\n\"Hello\"@en--ltr\n\"Hallo\"@nl\n"
    );
    assert_eq!(
        read("nested-triple.tsv"),
        "?t\n<<( _:b0 <http://example.org/says> <<( <http://example.org/s> \
         <http://example.org/p> \"שלום\"@he--rtl )>> )>>\n"
    );
    let escapes = read("tabular-escapes.tsv");
    let lines: Vec<&str> = escapes.split('\n').collect();
    let s = "<http://example.org/s>\t";
    assert_eq!(lines[0], "?s\t?o");
    assert_eq!(lines[1], format!("{s}\"comma, here\""));
    assert_eq!(lines[2], format!("{s}\"say \\\"hi\\\"\""));
    assert_eq!(lines[3], format!("{s}\"line\\nbreak\\r\\ttab \\\\ slash\""));
    assert_eq!(lines[4], format!("{s}42"));
    assert_eq!(lines[5], format!("{s}-0.5"));
    assert_eq!(lines[6], format!("{s}1.0E6"));
    assert_eq!(
        lines[7],
        format!("{s}\"forty-two\"^^<http://www.w3.org/2001/XMLSchema#integer>")
    );
    assert_eq!(
        lines[8],
        format!("{s}\"true\"^^<http://www.w3.org/2001/XMLSchema#boolean>")
    );
    assert_eq!(
        lines[9],
        format!("{s}\"x\"^^<http://www.w3.org/2001/XMLSchema#string>")
    );
    assert_eq!(lines[11], "<http://example.org/a\\u007Bb\\u007D>\t\"\"");
    assert_eq!(
        lines[12],
        "<http://example.org/carol>\t<<( <http://example.org/carol> <http://example.org/says> \
         \"Hello world, my name is \\\"Alice\\\".\" )>>"
    );
    assert!(
        !read("link-select.tsv").contains("about"),
        "TSV has no links"
    );
}
