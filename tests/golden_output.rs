//! Golden files and exact-bytes checks for the SRJ and SRX writers.
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
use sparql_results::bounded::{
    SparqlResult,
    parse_srj_bounded,
    parse_srx_bounded,
    write_srj,
    write_srx,
};

const GOLDEN: &str = "tests/fixtures/golden";

#[derive(Clone, Copy)]
enum Format {
    Srj,
    Srx,
}

impl Format {
    const ALL: [Self; 2] = [Self::Srj, Self::Srx];

    const fn extension(self) -> &'static str {
        match self {
            Self::Srj => "srj",
            Self::Srx => "srx",
        }
    }

    async fn write(self, result: &SparqlResult) -> String {
        let bytes = match self {
            Self::Srj => write_srj(Vec::new(), result).await,
            Self::Srx => write_srx(Vec::new(), result).await,
        }
        .expect("golden case writes");
        String::from_utf8(bytes).expect("UTF-8 output")
    }

    async fn read(self, text: &str) -> SparqlResult {
        match self {
            Self::Srj => parse_srj_bounded(text.as_bytes()).await,
            Self::Srx => parse_srx_bounded(text.as_bytes()).await,
        }
        .expect("golden file reads")
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
                Format::ALL.map(|format| Self {
                    case: GoldenCase::new(case.name, case.result.clone()),
                    format,
                })
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
        assert_eq!(
            golden.format.read(&written).await,
            golden.case.result,
            "{} reads back",
            golden.path()
        );
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
