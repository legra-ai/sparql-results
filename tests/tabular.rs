//! CSV and TSV: ASK refusal, TSV-only write refusals, and the TSV reader's
//! acceptance, refusals and streaming.

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
    parse_tsv_bounded,
    write_csv,
    write_tsv,
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
    TabularFormat,
    TermSyntaxError,
    TsvError,
    TsvStreamSink,
    TsvWriter,
    parse_tsv_streaming,
};
use tokio::io::{
    AsyncRead,
    AsyncWrite,
    ReadBuf,
};

#[derive(Clone, Debug, Default)]
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

struct ByteAtATime(&'static [u8]);

impl AsyncRead for ByteAtATime {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if let Some((first, rest)) = self.0.split_first() {
            buf.put_slice(&[*first]);
            self.0 = rest;
        }
        Poll::Ready(Ok(()))
    }
}

/// Counts rows and refuses the one at `fail_at`.
struct CountingSink {
    rows: u64,
    fail_at: Option<u64>,
}

#[async_trait::async_trait]
impl TsvStreamSink for CountingSink {
    async fn select_header(&mut self, _head: ResultHead) -> sparql_results::Result<()> {
        Ok(())
    }

    async fn select_row(&mut self, _row: ResultRow) -> sparql_results::Result<()> {
        self.rows += 1;
        if self.fail_at == Some(self.rows) {
            return Err(SparqlResultsError::InvalidDocument(
                "sink refused".to_owned(),
            ));
        }
        Ok(())
    }
}

fn uri(iri: &str) -> ResultValue {
    ResultValue::Uri(iri.to_owned())
}

fn literal(value: &str, lang: Option<&str>, datatype: Option<&str>) -> ResultValue {
    ResultValue::Literal {
        value: value.to_owned(),
        lang: lang.map(str::to_owned),
        datatype: datatype.map(str::to_owned),
        dir: None,
    }
}

#[tokio::test]
async fn ask_results_are_refused_before_any_byte() {
    for result in [true, false] {
        for links in [Vec::new(), vec!["http://example.org/about".to_owned()]] {
            let ask = SparqlResult::Ask {
                head: ResultHead::ask(links),
                result,
            };
            let out = ByteCounter::default();
            let error = write_csv(out.clone(), &ask).await.unwrap_err();
            assert!(
                matches!(error, SparqlResultsError::AskNotTabular(TabularFormat::Csv)),
                "{error}"
            );
            assert_eq!(out.bytes(), 0, "CSV wrote part of an ASK result");
            let error = write_tsv(out.clone(), &ask).await.unwrap_err();
            assert!(
                matches!(error, SparqlResultsError::AskNotTabular(TabularFormat::Tsv)),
                "{error}"
            );
            assert_eq!(out.bytes(), 0, "TSV wrote part of an ASK result");
        }
    }
}

#[tokio::test]
async fn tsv_refuses_a_blank_node_label_it_cannot_spell() {
    let head = ResultHead::select(vec!["x".to_owned()]).unwrap();
    let bad = ResultRow {
        bindings: IndexMap::from([("x".to_owned(), ResultValue::BNode("a\tb".to_owned()))]),
    };
    let result = SparqlResult::Select {
        head: head.clone(),
        rows: vec![bad.clone()],
    };
    let out = ByteCounter::default();
    let error = write_tsv(out.clone(), &result).await.unwrap_err();
    assert!(
        matches!(&error, SparqlResultsError::Tsv(TsvError::BlankNodeLabel(label)) if label == "a\tb"),
        "{error}"
    );
    assert_eq!(out.bytes(), 0);

    let mut writer = TsvWriter::start_select(out.clone(), &head).await.unwrap();
    let before = out.bytes();
    assert!(writer.write_row(&bad).await.is_err());
    assert_eq!(out.bytes(), before);
    // CSV spells any label as `_:label`, quoting it when needed.
    assert!(write_csv(Vec::new(), &result).await.is_ok());
}

#[tokio::test]
async fn tsv_reader_reads_every_turtle_term_form() {
    let input = "$s\t?o\n\
                 <http://e/\\u0041>\t'single \"quoted\"'\n\
                 _:b.1\t\"e\\u00E9\\U0001F600\\t\\b\\f\\'\"@en-GB\n\
                 \ttrue\n\
                 <http://e/s>\t\"x\"@ar--rtl\n\
                 <http://e/s>\t<<(<http://e/a><http://e/p>.5e1)>>";
    let SparqlResult::Select { head, rows } = parse_tsv_bounded(input.as_bytes()).await.unwrap()
    else {
        panic!("TSV is SELECT");
    };
    assert_eq!(head.vars(), ["s", "o"]);
    let o = |index: usize| rows[index].bindings.get("o").unwrap().clone();
    assert_eq!(rows[0].bindings.get("s"), Some(&uri("http://e/A")));
    assert_eq!(o(0), literal("single \"quoted\"", None, None));
    assert_eq!(
        rows[1].bindings.get("s"),
        Some(&ResultValue::BNode("b.1".to_owned()))
    );
    assert_eq!(
        o(1),
        literal("e\u{e9}\u{1F600}\t\u{8}\u{c}'", Some("en-GB"), None)
    );
    assert!(!rows[2].bindings.contains_key("s"));
    assert_eq!(
        o(2),
        literal(
            "true",
            None,
            Some("http://www.w3.org/2001/XMLSchema#boolean")
        )
    );
    assert_eq!(
        o(3),
        ResultValue::Literal {
            value: "x".to_owned(),
            lang: Some("ar".to_owned()),
            datatype: None,
            dir: Some(BaseDirection::Rtl),
        }
    );
    let ResultValue::Triple { object, .. } = o(4) else {
        panic!("triple term");
    };
    assert_eq!(
        *object,
        literal(
            ".5e1",
            None,
            Some("http://www.w3.org/2001/XMLSchema#double")
        )
    );
}

#[tokio::test]
async fn tsv_reader_refuses_malformed_input() {
    let term = |line, field, reason| {
        SparqlResultsError::Tsv(TsvError::Term {
            line,
            field,
            reason,
        })
    };
    let cases: Vec<(&[u8], SparqlResultsError)> = vec![
        (b"", TsvError::MissingHeader.into()),
        (b"x\n", TsvError::HeaderField("x".to_owned()).into()),
        (
            b"?x\t?x\n",
            HeaderError::DuplicateVariable("x".to_owned()).into(),
        ),
        (
            b"?x\n<a>\t<b>\n",
            TsvError::FieldCount {
                line: 2,
                expected: 1,
                found: 2,
            }
            .into(),
        ),
        (b"?x\n\xff\n", TsvError::InvalidUtf8 { line: 2 }.into()),
        (
            b"?x\n<a b>\n",
            term(2, 1, TermSyntaxError::UnexpectedChar(' ')),
        ),
        (b"?x\n\"open\n", term(2, 1, TermSyntaxError::UnexpectedEnd)),
        (
            b"?x\n\"a\\q\"\n",
            term(2, 1, TermSyntaxError::InvalidEscape),
        ),
        (
            b"?x\n\"\"\"long\"\"\"\n",
            term(2, 1, TermSyntaxError::TrailingInput),
        ),
        (b"?x\n<a>\r\n", term(2, 1, TermSyntaxError::TrailingInput)),
        (
            b"?x\n\"a\"@en--up\n",
            term(2, 1, TermSyntaxError::LanguageTag),
        ),
        (b"?x\n_:a.\n", term(2, 1, TermSyntaxError::BlankNodeLabel)),
        (
            b"?x\nex:a\n",
            term(2, 1, TermSyntaxError::UnexpectedStart('e')),
        ),
        (
            b"?x\n\"a\"@abcdefghi\n",
            LiteralError::InvalidLanguageTag("abcdefghi".to_owned()).into(),
        ),
        (
            b"?x\n<<( <a> \"p\" <b> )>>\n",
            RowError::TriplePredicate.into(),
        ),
        (
            b"?x\n<<( <a> <p> <<( <a> _:p <b> )>> )>>\n",
            RowError::TriplePredicate.into(),
        ),
        (
            b"?x\n<<( \"s\" <p> <b> )>>\n",
            RowError::TripleSubject.into(),
        ),
    ];
    for (input, expected) in cases {
        let error = parse_tsv_bounded(input).await.unwrap_err();
        assert_eq!(
            error.to_string(),
            expected.to_string(),
            "{}",
            String::from_utf8_lossy(input)
        );
    }
}

#[tokio::test]
async fn tsv_reader_streams_one_byte_reads_and_stops_at_a_sink_error() {
    const DOC: &[u8] = b"?x\n<http://e/1>\n\n<http://e/3>\n";
    let mut sink = CountingSink {
        rows: 0,
        fail_at: None,
    };
    let summary = parse_tsv_streaming(ByteAtATime(DOC), &mut sink)
        .await
        .unwrap();
    assert_eq!(summary.row_count, 3);
    assert_eq!(sink.rows, 3);

    let mut sink = CountingSink {
        rows: 0,
        fail_at: Some(2),
    };
    let error = parse_tsv_streaming(ByteAtATime(DOC), &mut sink)
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "invalid document: sink refused");
    assert_eq!(sink.rows, 2, "the parser stopped at the refused row");
}

#[tokio::test]
async fn zero_variable_and_empty_results_round_trip_through_tsv() {
    for (text, rows) in [("\n", 0_usize), ("\n\n\n", 2), ("?x\n", 0), ("?x\n\n", 1)] {
        let read = parse_tsv_bounded(text.as_bytes()).await.unwrap();
        let SparqlResult::Select {
            rows: read_rows, ..
        } = &read
        else {
            panic!("SELECT");
        };
        assert_eq!(read_rows.len(), rows, "{text:?}");
        let written = write_tsv(Vec::new(), &read).await.unwrap();
        assert_eq!(written, text.as_bytes(), "{text:?}");
    }
}
