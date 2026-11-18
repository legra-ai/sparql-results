//! The golden result cases shared by the golden-output tests.

use indexmap::IndexMap;
use sparql_results::bounded::SparqlResult;
use sparql_results::{
    BaseDirection,
    ResultHead,
    ResultRow,
    ResultValue,
};

/// One named golden case; its files are `tests/fixtures/golden/<name>.srj`
/// and `<name>.srx`.
pub struct GoldenCase {
    pub name: &'static str,
    pub result: SparqlResult,
}

impl GoldenCase {
    pub fn all() -> Vec<Self> {
        vec![
            Self::new("directional", select(&["x"], directional_rows())),
            Self::new("nested-triple", select(&["t"], nested_triple())),
            Self::new("unbound", select(&["x", "y"], unbound_rows())),
            Self::new(
                "zero-variable",
                select(&[], vec![ResultRow::from_pairs([])]),
            ),
            Self::new("ask-true", ask(true, Vec::new())),
            Self::new("ask-false", ask(false, Vec::new())),
            Self::new("link-select", link_select()),
            Self::new("tabular-escapes", select(&["s", "o"], tabular_escapes())),
            Self::new(
                "link-ask",
                ask(true, vec!["http://example.org/about-ask".to_owned()]),
            ),
        ]
    }

    pub const fn new(name: &'static str, result: SparqlResult) -> Self {
        Self { name, result }
    }
}

trait RowFromPairs {
    fn from_pairs<const N: usize>(pairs: [(&str, ResultValue); N]) -> ResultRow;
}

impl RowFromPairs for ResultRow {
    fn from_pairs<const N: usize>(pairs: [(&str, ResultValue); N]) -> ResultRow {
        ResultRow {
            bindings: pairs
                .into_iter()
                .map(|(var, value)| (var.to_owned(), value))
                .collect::<IndexMap<_, _>>(),
        }
    }
}

fn vars(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn select(names: &[&str], rows: Vec<ResultRow>) -> SparqlResult {
    SparqlResult::Select {
        head: ResultHead::select(vars(names)).expect("valid golden header"),
        rows,
    }
}

fn ask(result: bool, links: Vec<String>) -> SparqlResult {
    SparqlResult::Ask {
        head: ResultHead::ask(links),
        result,
    }
}

fn uri(iri: &str) -> ResultValue {
    ResultValue::Uri(iri.to_owned())
}

fn lang_literal(value: &str, lang: &str, dir: Option<BaseDirection>) -> ResultValue {
    ResultValue::Literal {
        value: value.to_owned(),
        lang: Some(lang.to_owned()),
        datatype: None,
        dir,
    }
}

fn plain(value: &str) -> ResultValue {
    ResultValue::Literal {
        value: value.to_owned(),
        lang: None,
        datatype: None,
        dir: None,
    }
}

fn triple(subject: ResultValue, predicate: &str, object: ResultValue) -> ResultValue {
    ResultValue::Triple {
        subject: Box::new(subject),
        predicate: Box::new(uri(predicate)),
        object: Box::new(object),
    }
}

fn directional_rows() -> Vec<ResultRow> {
    vec![
        ResultRow::from_pairs([("x", lang_literal("مرحبا", "ar", Some(BaseDirection::Rtl)))]),
        ResultRow::from_pairs([("x", lang_literal("Hello", "en", Some(BaseDirection::Ltr)))]),
        ResultRow::from_pairs([("x", lang_literal("Hallo", "nl", None))]),
    ]
}

fn nested_triple() -> Vec<ResultRow> {
    let inner = triple(
        uri("http://example.org/s"),
        "http://example.org/p",
        lang_literal("שלום", "he", Some(BaseDirection::Rtl)),
    );
    let outer = triple(
        ResultValue::BNode("b0".to_owned()),
        "http://example.org/says",
        inner,
    );
    vec![ResultRow::from_pairs([("t", outer)])]
}

fn unbound_rows() -> Vec<ResultRow> {
    vec![
        ResultRow::from_pairs([("x", plain("only x"))]),
        ResultRow::from_pairs([("y", uri("http://example.org/only-y"))]),
        ResultRow::from_pairs([]),
    ]
}

const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

fn typed(value: &str, datatype: &str) -> ResultValue {
    ResultValue::Literal {
        value: value.to_owned(),
        lang: None,
        datatype: Some(if datatype.starts_with("http") {
            datatype.to_owned()
        } else {
            format!("{XSD}{datatype}")
        }),
        dir: None,
    }
}

/// The CSV quoting and TSV escaping corner cases: separators, quotes and
/// line breaks inside values, numeric abbreviation, explicit datatypes, an
/// IRI character Turtle must escape, and a triple term with a quoted
/// literal (the CSV and TSV §3.4 / §4.4 examples).
fn tabular_escapes() -> Vec<ResultRow> {
    let s = || uri("http://example.org/s");
    vec![
        ResultRow::from_pairs([("s", s()), ("o", plain("comma, here"))]),
        ResultRow::from_pairs([("s", s()), ("o", plain("say \"hi\""))]),
        ResultRow::from_pairs([("s", s()), ("o", plain("line\nbreak\r\ttab \\ slash"))]),
        ResultRow::from_pairs([("s", s()), ("o", typed("42", "integer"))]),
        ResultRow::from_pairs([("s", s()), ("o", typed("-0.5", "decimal"))]),
        ResultRow::from_pairs([("s", s()), ("o", typed("1.0E6", "double"))]),
        ResultRow::from_pairs([("s", s()), ("o", typed("forty-two", "integer"))]),
        ResultRow::from_pairs([("s", s()), ("o", typed("true", "boolean"))]),
        ResultRow::from_pairs([("s", s()), ("o", typed("x", "string"))]),
        ResultRow::from_pairs([("s", s()), ("o", typed("5,5", "http://example.org/dt"))]),
        ResultRow::from_pairs([("s", uri("http://example.org/a{b}")), ("o", plain(""))]),
        ResultRow::from_pairs([
            ("s", uri("http://example.org/carol")),
            (
                "o",
                triple(
                    uri("http://example.org/carol"),
                    "http://example.org/says",
                    plain("Hello world, my name is \"Alice\"."),
                ),
            ),
        ]),
    ]
}

fn link_select() -> SparqlResult {
    SparqlResult::Select {
        head: ResultHead::try_new(
            vars(&["x"]),
            vec![
                "http://example.org/about".to_owned(),
                "http://example.org/license".to_owned(),
            ],
        )
        .expect("valid golden header"),
        rows: vec![ResultRow::from_pairs([("x", plain("linked"))])],
    }
}

impl GoldenCase {
    pub fn named(name: &str) -> Self {
        Self::all()
            .into_iter()
            .find(|case| case.name == name)
            .expect("known golden case")
    }
}
