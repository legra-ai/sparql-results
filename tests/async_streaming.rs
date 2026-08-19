#![allow(missing_docs)]

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll};

use sparql_results::bounded::{SparqlResult, write_srj, write_srx};
use sparql_results::{
    ResultRow, ResultValue, SrjStreamSink, SrxStreamSink, canonicalize_srx, parse_srj_streaming,
    parse_srx_streaming,
};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::Notify;
use tokio::time::{Duration, timeout};

struct ByteAtATime {
    bytes: Vec<u8>,
    offset: usize,
}

impl AsyncRead for ByteAtATime {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if self.offset == self.bytes.len() {
            return Poll::Ready(Ok(()));
        }
        let byte = self.bytes[self.offset];
        self.offset += 1;
        buf.put_slice(&[byte]);
        Poll::Ready(Ok(()))
    }
}

struct FailingReader;

impl AsyncRead for FailingReader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        Poll::Ready(Err(std::io::Error::other("reader failed")))
    }
}

struct CollectSink<'a>(&'a mut Vec<ResultRow>);

#[async_trait::async_trait]
impl SrxStreamSink for CollectSink<'_> {
    async fn select_header(&mut self, _vars: Vec<String>) -> sparql_results::Result<()> {
        Ok(())
    }

    async fn select_row(&mut self, row: ResultRow) -> sparql_results::Result<()> {
        self.0.push(row);
        Ok(())
    }

    async fn ask(&mut self, _result: bool) -> sparql_results::Result<()> {
        Ok(())
    }
}

struct GatedSink {
    rows: Arc<AtomicUsize>,
    release: Arc<Notify>,
}

#[async_trait::async_trait]
impl SrjStreamSink for GatedSink {
    async fn select_header(&mut self, _vars: Vec<String>) -> sparql_results::Result<()> {
        Ok(())
    }

    async fn select_row(&mut self, _row: ResultRow) -> sparql_results::Result<()> {
        let row_number = self.rows.fetch_add(1, Ordering::SeqCst);
        if row_number == 0 {
            self.release.notified().await;
        }
        Ok(())
    }

    async fn ask(&mut self, _result: bool) -> sparql_results::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl SrxStreamSink for GatedSink {
    async fn select_header(&mut self, _vars: Vec<String>) -> sparql_results::Result<()> {
        Ok(())
    }

    async fn select_row(&mut self, _row: ResultRow) -> sparql_results::Result<()> {
        let row_number = self.rows.fetch_add(1, Ordering::SeqCst);
        if row_number == 0 {
            self.release.notified().await;
        }
        Ok(())
    }

    async fn ask(&mut self, _result: bool) -> sparql_results::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn srj_waits_for_sink_before_reading_next_row() {
    let input = br#"{"head":{"vars":["x"]},"results":{"bindings":[{"x":{"type":"literal","value":"one"}},{"x":{"type":"literal","value":"two"}}]}}"#;
    let rows = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(Notify::new());
    let mut sink = GatedSink {
        rows: Arc::clone(&rows),
        release: Arc::clone(&release),
    };
    let parser = parse_srj_streaming(
        ByteAtATime {
            bytes: input.to_vec(),
            offset: 0,
        },
        &mut sink,
    );
    let releaser = async {
        for _ in 0..32 {
            if rows.load(Ordering::SeqCst) == 1 {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert_eq!(rows.load(Ordering::SeqCst), 1);
        release.notify_one();
    };
    let (summary, ()) = tokio::join!(parser, releaser);
    assert_eq!(summary.unwrap().row_count, 2);
}

#[tokio::test]
async fn reader_errors_propagate_from_both_formats() {
    let srj = parse_srj_streaming(
        FailingReader,
        &mut GatedSink {
            rows: Arc::new(AtomicUsize::new(0)),
            release: Arc::new(Notify::new()),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(srj, sparql_results::SparqlResultsError::Json(_)));

    let srx = parse_srx_streaming(
        FailingReader,
        &mut GatedSink {
            rows: Arc::new(AtomicUsize::new(0)),
            release: Arc::new(Notify::new()),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(srx, sparql_results::SparqlResultsError::Xml(_)));
}

#[tokio::test]
async fn cancelling_a_blocked_sink_cancels_the_parser() {
    let input = br#"{"head":{"vars":["x"]},"results":{"bindings":[{"x":{"type":"literal","value":"one"}}]}}"#;
    let mut sink = GatedSink {
        rows: Arc::new(AtomicUsize::new(0)),
        release: Arc::new(Notify::new()),
    };
    let result = timeout(
        Duration::from_millis(50),
        parse_srj_streaming(input.as_slice(), &mut sink),
    )
    .await;
    assert!(result.is_err(), "blocked sink should be cancellable");
}

#[tokio::test]
async fn srx_accepts_one_byte_reads_and_nested_triples() {
    let input = br#"<sparql xmlns="http://www.w3.org/2005/sparql-results#"><head><variable name="x"/></head><results><result><binding name="x"><triple><subject><uri>urn:s</uri></subject><predicate><uri>urn:p</uri></predicate><object><literal datatype="urn:mime:text">v</literal></object></triple></binding></result></results></sparql>"#;
    let mut rows = Vec::new();
    parse_srx_streaming(
        ByteAtATime {
            bytes: input.to_vec(),
            offset: 0,
        },
        &mut CollectSink(&mut rows),
    )
    .await
    .unwrap();
    assert!(matches!(rows[0].bindings["x"], ResultValue::Triple { .. }));
}

struct OneByteWriter(Vec<u8>);

impl AsyncWrite for OneByteWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if bytes.is_empty() {
            return Poll::Ready(Ok(0));
        }
        self.0.push(bytes[0]);
        Poll::Ready(Ok(1))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[derive(Debug)]
struct FailingWriter;

impl AsyncWrite for FailingWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _bytes: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Poll::Ready(Err(std::io::Error::other("writer failed")))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Err(std::io::Error::other("writer failed")))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn writer_errors_propagate_from_both_formats_and_canonicalization() {
    let vars = vec!["x".to_owned()];
    let srj = sparql_results::SrjWriter::start_select(FailingWriter, &vars)
        .await
        .err()
        .expect("SRJ writer fails");
    assert!(matches!(srj, sparql_results::SparqlResultsError::Json(_)));

    let srx = sparql_results::SrxWriter::start_select(FailingWriter, &vars)
        .await
        .err()
        .expect("SRX writer fails");
    assert!(matches!(srx, sparql_results::SparqlResultsError::Xml(_)));

    let canonical = canonicalize_srx(b"<sparql/>".as_slice(), FailingWriter)
        .await
        .unwrap_err();
    assert!(matches!(
        canonical,
        sparql_results::SparqlResultsError::Xml(_)
    ));
}

#[tokio::test]
async fn canonicalizer_streams_one_byte_reads_into_partial_writes() {
    let input = br#"<?xml version="1.0"?><sparql xmlns='http://www.w3.org/2005/sparql-results#'><head></head><boolean>true</boolean></sparql>"#;
    let writer = canonicalize_srx(
        ByteAtATime {
            bytes: input.to_vec(),
            offset: 0,
        },
        OneByteWriter(Vec::new()),
    )
    .await
    .unwrap();
    assert_eq!(
        String::from_utf8(writer.0).unwrap(),
        "<sparql xmlns=\"http://www.w3.org/2005/sparql-results#\"><head/><boolean>true</boolean></sparql>"
    );
}

#[tokio::test]
async fn writers_handle_partial_writes_without_materializing_a_document() {
    let result = SparqlResult::Select {
        vars: vec!["x".to_owned()],
        rows: vec![ResultRow {
            bindings: [(
                "x".to_owned(),
                ResultValue::Literal {
                    value: "value".to_owned(),
                    lang: None,
                    datatype: Some("urn:mime:text".to_owned()),
                },
            )]
            .into_iter()
            .collect(),
        }],
    };
    let srj_writer = write_srj(OneByteWriter(Vec::new()), &result).await.unwrap();
    assert!(
        String::from_utf8(srj_writer.0)
            .unwrap()
            .contains("urn:mime:text")
    );
    let srx_writer = write_srx(OneByteWriter(Vec::new()), &result).await.unwrap();
    assert!(
        String::from_utf8(srx_writer.0)
            .unwrap()
            .contains("urn:mime:text")
    );
}
