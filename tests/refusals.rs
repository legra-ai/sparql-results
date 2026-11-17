//! A writer refuses an invalid result before it writes anything.

#![allow(missing_docs)]

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{
    AtomicUsize,
    Ordering,
};
use std::task::{
    Context,
    Poll,
};

use indexmap::IndexMap;
use sparql_results::bounded::{
    SparqlResult,
    parse_srj_bounded,
    parse_srx_bounded,
    write_srj,
    write_srx,
};
use sparql_results::{
    BaseDirection,
    HeaderError,
    LiteralError,
    ResultHead,
    ResultRow,
    ResultValue,
    RowError,
    SparqlResultsError,
    SrjWriter,
    SrxWriter,
};
use tokio::io::AsyncWrite;

/// Counts every byte handed to it; shared so the count survives a writer
/// that is consumed by a refused call.
#[derive(Clone, Default)]
struct ByteCounter(Arc<AtomicUsize>);

impl ByteCounter {
    fn bytes(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

impl AsyncWrite for ByteCounter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.0.fetch_add(buf.len(), Ordering::SeqCst);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn head(vars: &[&str]) -> ResultHead {
    ResultHead::select(vars.iter().map(|var| (*var).to_owned()).collect()).unwrap()
}

fn row(var: &str, value: ResultValue) -> ResultRow {
    ResultRow {
        bindings: IndexMap::from([(var.to_owned(), value)]),
    }
}

fn literal(lang: Option<&str>, dir: Option<BaseDirection>) -> ResultValue {
    ResultValue::Literal {
        value: "v".to_owned(),
        lang: lang.map(str::to_owned),
        datatype: None,
        dir,
    }
}

fn uri() -> ResultValue {
    ResultValue::Uri("urn:x".to_owned())
}

/// Invalid results, each with the error it must be refused with. Every
/// SELECT case starts with a valid row, so a writer that streams before
/// checking would emit bytes.
fn invalid_results() -> Vec<(&'static str, SparqlResult, SparqlResultsError)> {
    let triple = |subject, predicate| ResultValue::Triple {
        subject: Box::new(subject),
        predicate: Box::new(predicate),
        object: Box::new(uri()),
    };
    let select = |second: ResultRow| SparqlResult::Select {
        head: head(&["x"]),
        rows: vec![row("x", uri()), second],
    };
    vec![
        (
            "ASK header with variables",
            SparqlResult::Ask {
                head: head(&["x"]),
                result: true,
            },
            HeaderError::VariablesInAsk.into(),
        ),
        (
            "undeclared variable",
            select(row("y", uri())),
            RowError::UndeclaredVariable("y".to_owned()).into(),
        ),
        (
            "direction without language",
            select(row("x", literal(None, Some(BaseDirection::Rtl)))),
            LiteralError::DirectionWithoutLanguage.into(),
        ),
        (
            "malformed language tag",
            select(row("x", literal(Some("not a tag"), None))),
            LiteralError::InvalidLanguageTag("not a tag".to_owned()).into(),
        ),
        (
            "literal triple subject",
            select(row("x", triple(literal(None, None), uri()))),
            RowError::TripleSubject.into(),
        ),
        (
            "blank-node triple predicate",
            select(row("x", triple(uri(), ResultValue::BNode("b".to_owned())))),
            RowError::TriplePredicate.into(),
        ),
    ]
}

fn same_error(actual: &SparqlResultsError, expected: &SparqlResultsError) -> bool {
    actual.to_string() == expected.to_string()
}

#[tokio::test]
async fn bounded_writers_emit_zero_bytes_for_an_invalid_result() {
    for (case, result, expected) in invalid_results() {
        let json_out = ByteCounter::default();
        let error = write_srj(json_out.clone(), &result)
            .await
            .err()
            .expect(case);
        assert!(same_error(&error, &expected), "SRJ {case}: {error}");
        assert_eq!(json_out.bytes(), 0, "SRJ {case} wrote bytes");

        let xml_out = ByteCounter::default();
        let error = write_srx(xml_out.clone(), &result).await.err().expect(case);
        assert!(same_error(&error, &expected), "SRX {case}: {error}");
        assert_eq!(xml_out.bytes(), 0, "SRX {case} wrote bytes");
    }
}

#[tokio::test]
async fn streaming_writers_emit_zero_bytes_for_a_refused_row() {
    for (case, result, expected) in invalid_results() {
        let SparqlResult::Select { head, rows } = result else {
            continue;
        };
        let json_out = ByteCounter::default();
        let mut writer = SrjWriter::start_select(json_out.clone(), &head)
            .await
            .unwrap();
        writer.write_row(&rows[0]).await.unwrap();
        let before = json_out.bytes();
        let error = writer.write_row(&rows[1]).await.unwrap_err();
        assert!(same_error(&error, &expected), "SRJ {case}: {error}");
        assert_eq!(json_out.bytes(), before, "SRJ {case} wrote part of a row");

        let xml_out = ByteCounter::default();
        let mut writer = SrxWriter::start_select(xml_out.clone(), &head)
            .await
            .unwrap();
        writer.write_row(&rows[0]).await.unwrap();
        let before = xml_out.bytes();
        let error = writer.write_row(&rows[1]).await.unwrap_err();
        assert!(same_error(&error, &expected), "SRX {case}: {error}");
        assert_eq!(xml_out.bytes(), before, "SRX {case} wrote part of a row");
    }
}

#[test]
fn an_invalid_header_cannot_be_built() {
    for (vars, expected) in [
        (
            vec!["a b"],
            HeaderError::InvalidVariableName("a b".to_owned()),
        ),
        (vec![""], HeaderError::InvalidVariableName(String::new())),
        (
            vec!["?x"],
            HeaderError::InvalidVariableName("?x".to_owned()),
        ),
        (
            vec!["x", "x"],
            HeaderError::DuplicateVariable("x".to_owned()),
        ),
    ] {
        let vars = vars.into_iter().map(str::to_owned).collect();
        assert_eq!(ResultHead::select(vars), Err(expected));
    }
    assert!(ResultHead::select(vec!["_1".to_owned(), "été".to_owned()]).is_ok());
}

#[tokio::test]
async fn readers_refuse_invalid_headers() {
    let json_docs: [&[u8]; 3] = [
        br#"{"head":{"vars":["x","x"]},"results":{"bindings":[]}}"#,
        br#"{"head":{"vars":["x"]},"boolean":true}"#,
        br#"{"boolean":true,"head":{}}"#,
    ];
    for input in json_docs {
        let error = parse_srj_bounded(input).await.unwrap_err();
        assert!(
            matches!(error, SparqlResultsError::InvalidHeader(_)),
            "{error}"
        );
    }
    let xml_docs: [&[u8]; 2] = [
        br#"<sparql xmlns="http://www.w3.org/2005/sparql-results#"><head><variable name="a b"/></head><results/></sparql>"#,
        br#"<sparql xmlns="http://www.w3.org/2005/sparql-results#"><head><variable name="x"/></head><boolean>true</boolean></sparql>"#,
    ];
    for input in xml_docs {
        let error = parse_srx_bounded(input).await.unwrap_err();
        assert!(
            matches!(error, SparqlResultsError::InvalidHeader(_)),
            "{error}"
        );
    }
}
