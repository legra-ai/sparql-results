#![allow(missing_docs)]

use std::fmt::Write as _;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use indexmap::IndexMap;
use sparql_results::bounded::{
    SparqlResult, parse_srj_bounded, parse_srx_bounded, write_srj, write_srx,
};
use sparql_results::{
    BaseDirection, Result, ResultRow, ResultValue, SparqlResultsError, SrjStreamSink,
    SrxStreamSink, canonicalize_srx, parse_srj_streaming,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

struct OneByteReader {
    input: Vec<u8>,
    offset: usize,
}

impl AsyncRead for OneByteReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.offset == self.input.len() {
            return Poll::Ready(Ok(()));
        }
        buf.put_slice(&[self.input[self.offset]]);
        self.offset += 1;
        Poll::Ready(Ok(()))
    }
}

#[derive(Default)]
struct CountingSink {
    rows: usize,
    vars: Vec<String>,
}

struct RejectingSink;

#[async_trait::async_trait]
impl SrjStreamSink for RejectingSink {
    async fn select_header(&mut self, _vars: Vec<String>) -> Result<()> {
        Ok(())
    }

    async fn select_row(&mut self, _row: ResultRow) -> Result<()> {
        Err(SparqlResultsError::InvalidDocument(
            "sink stopped".to_owned(),
        ))
    }

    async fn ask(&mut self, _result: bool) -> Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl SrjStreamSink for CountingSink {
    async fn select_header(&mut self, vars: Vec<String>) -> Result<()> {
        self.vars = vars;
        Ok(())
    }
    async fn select_row(&mut self, _row: ResultRow) -> Result<()> {
        self.rows += 1;
        Ok(())
    }
    async fn ask(&mut self, _result: bool) -> Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl SrxStreamSink for CountingSink {
    async fn select_header(&mut self, vars: Vec<String>) -> Result<()> {
        self.vars = vars;
        Ok(())
    }
    async fn select_row(&mut self, _row: ResultRow) -> Result<()> {
        self.rows += 1;
        Ok(())
    }
    async fn ask(&mut self, _result: bool) -> Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn bounded_parsers_preserve_terms_and_datatypes() {
    let json = br#"{"head":{"vars":["x"]},"results":{"bindings":[{"x":{"type":"triple","value":{"subject":{"type":"uri","value":"urn:s"},"predicate":{"type":"uri","value":"urn:p"},"object":{"type":"literal","value":"v","datatype":"urn:mime:text"}}}}]}}"#;
    let result = parse_srj_bounded(OneByteReader {
        input: json.to_vec(),
        offset: 0,
    })
    .await
    .unwrap();
    assert!(
        matches!(result, SparqlResult::Select { rows, .. } if matches!(rows[0].bindings["x"], ResultValue::Triple { .. }))
    );

    let xml = br#"<sparql xmlns="http://www.w3.org/2005/sparql-results#"><head><variable name="x"/></head><results><result><binding name="x"><triple><subject><uri>urn:s</uri></subject><predicate><uri>urn:p</uri></predicate><object><literal datatype="urn:mime:text">v</literal></object></triple></binding></result></results></sparql>"#;
    let result = parse_srx_bounded(OneByteReader {
        input: xml.to_vec(),
        offset: 0,
    })
    .await
    .unwrap();
    assert!(
        matches!(result, SparqlResult::Select { rows, .. } if matches!(rows[0].bindings["x"], ResultValue::Triple { .. }))
    );
}

#[tokio::test]
async fn streaming_thousands_of_rows_does_not_require_expected_row_storage() {
    let mut json = String::from(r#"{"head":{"vars":["x"]},"results":{"bindings":["#);
    for row in 0..2_000 {
        if row != 0 {
            json.push(',');
        }
        let _ = write!(json, r#"{{"x":{{"type":"literal","value":"{row}"}}}}"#);
    }
    json.push_str("]}}");
    let mut sink = CountingSink::default();
    let summary = parse_srj_streaming(
        OneByteReader {
            input: json.into_bytes(),
            offset: 0,
        },
        &mut sink,
    )
    .await
    .unwrap();
    assert_eq!(summary.row_count, 2_000);
    assert_eq!(sink.rows, 2_000);
}

#[tokio::test]
async fn malformed_documents_and_sink_errors_propagate() {
    let malformed = parse_srj_bounded(OneByteReader {
        input: br#"{"head":{}"#.to_vec(),
        offset: 0,
    })
    .await
    .unwrap_err();
    assert!(matches!(malformed, SparqlResultsError::Json(_)));

    let malformed_srx = parse_srx_bounded(OneByteReader {
        input: br"<sparql><head></head><results>".to_vec(),
        offset: 0,
    })
    .await
    .unwrap_err();
    assert!(matches!(
        malformed_srx,
        SparqlResultsError::Xml(_) | SparqlResultsError::MissingElement(_)
    ));

    let mut sink = RejectingSink;
    let error = parse_srj_streaming(
        br#"{"head":{},"results":{"bindings":[{}]}}"#.as_slice(),
        &mut sink,
    )
    .await
    .unwrap_err();
    assert!(matches!(error, SparqlResultsError::InvalidDocument(_)));
}

struct PartialWriter {
    bytes: Vec<u8>,
    writes: Arc<AtomicUsize>,
}

impl AsyncWrite for PartialWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        if bytes.is_empty() {
            return Poll::Ready(Ok(0));
        }
        self.bytes.push(bytes[0]);
        Poll::Ready(Ok(1))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn async_writers_round_trip_through_partial_writes() {
    let result = SparqlResult::Select {
        vars: vec!["x".to_owned()],
        rows: vec![ResultRow {
            bindings: IndexMap::from([(
                "x".to_owned(),
                ResultValue::Literal {
                    value: "v".to_owned(),
                    lang: Some("en".to_owned()),
                    datatype: Some("urn:mime:text".to_owned()),
                    dir: None,
                },
            )]),
        }],
    };
    let writes = Arc::new(AtomicUsize::new(0));
    let json_writer = write_srj(
        PartialWriter {
            bytes: Vec::new(),
            writes: Arc::clone(&writes),
        },
        &result,
    )
    .await
    .unwrap();
    assert!(writes.load(Ordering::SeqCst) > 1);
    let parsed = parse_srj_bounded(OneByteReader {
        input: json_writer.bytes,
        offset: 0,
    })
    .await
    .unwrap();
    assert_eq!(parsed, result);

    let writes = Arc::new(AtomicUsize::new(0));
    let xml_writer = write_srx(
        PartialWriter {
            bytes: Vec::new(),
            writes: Arc::clone(&writes),
        },
        &result,
    )
    .await
    .unwrap();
    assert!(writes.load(Ordering::SeqCst) > 1);
    let parsed = parse_srx_bounded(OneByteReader {
        input: xml_writer.bytes,
        offset: 0,
    })
    .await
    .unwrap();
    assert_eq!(parsed, result);
}

/// A buffering async writer used to capture serializer output whole.
struct BufWriter(Vec<u8>);

impl AsyncWrite for BufWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.0.extend_from_slice(bytes);
        Poll::Ready(Ok(bytes.len()))
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

fn dir_lang_result(dir: BaseDirection) -> SparqlResult {
    SparqlResult::Select {
        vars: vec!["x".to_owned()],
        rows: vec![ResultRow {
            bindings: IndexMap::from([(
                "x".to_owned(),
                ResultValue::Literal {
                    value: "مرحبا".to_owned(),
                    lang: Some("ar".to_owned()),
                    datatype: None,
                    dir: Some(dir),
                },
            )]),
        }],
    }
}

/// (a) SRJ serialization of a directional language-tagged literal emits
/// the SPARQL 1.2 `"its:dir"` key alongside `"value"` and `"xml:lang"`.
#[tokio::test]
async fn srj_serializes_base_direction() {
    let result = dir_lang_result(BaseDirection::Ltr);
    let writer = write_srj(BufWriter(Vec::new()), &result).await.unwrap();
    let json = String::from_utf8(writer.0).unwrap();
    assert!(
        json.contains(r#""its:dir":"ltr""#),
        "SRJ must emit its:dir: {json}"
    );
    assert!(
        json.contains(r#""xml:lang":"ar""#),
        "SRJ must keep lang: {json}"
    );
}

/// (b) SRX serialization emits `its:dir` on the `<literal>` element with the
/// ITS namespace declared on that same element (not on the root `<sparql>`, so
/// non-directional output stays byte-identical to plain W3C SRX).
#[tokio::test]
async fn srx_serializes_base_direction_with_its_namespace() {
    let result = dir_lang_result(BaseDirection::Rtl);
    let writer = write_srx(BufWriter(Vec::new()), &result).await.unwrap();
    let xml = String::from_utf8(writer.0).unwrap();
    assert!(
        xml.contains(
            r#"<literal xml:lang="ar" its:dir="rtl" xmlns:its="http://www.w3.org/2005/11/its">"#
        ),
        "SRX must emit its:dir and its namespace on the literal: {xml}"
    );
    assert!(
        !xml.contains(r#"<sparql xmlns="http://www.w3.org/2005/sparql-results#" xmlns:its"#),
        "SRX must not declare the its namespace on the root: {xml}"
    );
}

/// (c) SRX parsing reads `its:dir` back into the typed base direction.
#[tokio::test]
async fn srx_parses_base_direction() {
    let xml = r#"<sparql xmlns="http://www.w3.org/2005/sparql-results#" xmlns:its="http://www.w3.org/2005/11/its"><head><variable name="x"/></head><results><result><binding name="x"><literal xml:lang="ar" its:dir="rtl">مرحبا</literal></binding></result></results></sparql>"#;
    let result = parse_srx_bounded(OneByteReader {
        input: xml.as_bytes().to_vec(),
        offset: 0,
    })
    .await
    .unwrap();
    let SparqlResult::Select { rows, .. } = result else {
        panic!("expected SELECT result");
    };
    assert_eq!(
        rows[0].bindings["x"],
        ResultValue::Literal {
            value: "مرحبا".to_owned(),
            lang: Some("ar".to_owned()),
            datatype: None,
            dir: Some(BaseDirection::Rtl),
        }
    );
}

/// (d) SRX serialize -> parse round-trips the base direction, and the
/// canonical form preserves the `its:dir` attribute.
#[tokio::test]
async fn srx_round_trip_and_canonicalize_preserve_base_direction() {
    let result = dir_lang_result(BaseDirection::Rtl);
    let writer = write_srx(BufWriter(Vec::new()), &result).await.unwrap();
    let xml = writer.0;

    let parsed = parse_srx_bounded(OneByteReader {
        input: xml.clone(),
        offset: 0,
    })
    .await
    .unwrap();
    assert_eq!(parsed, result, "SRX round-trip must preserve dir");

    let canonical = canonicalize_srx(xml.as_slice(), BufWriter(Vec::new()))
        .await
        .unwrap();
    let canonical = String::from_utf8(canonical.0).unwrap();
    assert!(
        canonical.contains(r#"its:dir="rtl""#),
        "canonicalize must preserve its:dir: {canonical}"
    );
}
