# sparql-results

[![Crates.io](https://img.shields.io/crates/v/sparql-results.svg)](https://crates.io/crates/sparql-results)
[![Documentation](https://docs.rs/sparql-results/badge.svg)](https://docs.rs/sparql-results)
[![CI](https://github.com/legra-ai/sparql-results/actions/workflows/ci.yml/badge.svg)](https://github.com/legra-ai/sparql-results/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](https://github.com/legra-ai/sparql-results#license)
[![Downloads](https://img.shields.io/crates/d/sparql-results.svg)](https://crates.io/crates/sparql-results)

Async-first, bounded-memory parsers and incremental serializers for the W3C
SPARQL Query Results interchange formats:

- [SPARQL Query Results XML (SRX)](https://www.w3.org/TR/rdf-sparql-XMLres/)
- [SPARQL Query Results JSON (SRJ)](https://www.w3.org/TR/sparql12-results-json/)

The crate is deliberately independent of any RDF store, query engine, wire
protocol, or application data model. It represents one binding row at a time
with `ResultRow` and `ResultValue`; callers translate their own RDF terms at
the boundary.

Streaming parsers consume `tokio::io::AsyncRead` sources and await the async
row sink before reading the next row. Incremental writers and SRX
canonicalization await every write to a `tokio::io::AsyncWrite`. Memory is
independent of the number of solution rows: parser state, the current
token/event and row, and downstream buffers remain live. A single token,
value, or row must still fit in memory.

## Streaming serialization

Create a writer once, write each row as it becomes available, and finish the
document explicitly. No result-set-sized buffer is required.

```rust
use sparql_results::{ResultRow, ResultValue, SrjWriter};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> sparql_results::Result<()> {
    let mut writer = SrjWriter::start_select(
        tokio::io::sink(),
        &["name".to_owned()],
    ).await?;

    let mut bindings = HashMap::new();
    bindings.insert(
        "name".to_owned(),
        ResultValue::Literal {
            value: "Ada".to_owned(),
            lang: None,
            datatype: None,
        },
    );
    writer.write_row(&ResultRow { bindings: bindings.into_iter().collect() }).await?;
    let _sink = writer.finish().await?;
    Ok(())
}
```

`SrxWriter` provides the equivalent incremental XML serializer. Both writers
await every downstream write, preserving backpressure.

## Streaming parsing

Parsers emit the header, each row, or the ASK result to an async sink before
reading more input. A sink can forward rows to a database, another encoder,
or an application stream without collecting the document.

```rust
use sparql_results::{ResultRow, ResultValue, SrjStreamSink, parse_srj_streaming};

struct RowSink;

#[async_trait::async_trait]
impl SrjStreamSink for RowSink {
    async fn select_header(&mut self, _vars: Vec<String>) -> sparql_results::Result<()> {
        Ok(())
    }

    async fn select_row(&mut self, row: ResultRow) -> sparql_results::Result<()> {
        let _ = row.bindings.get("name").map(|value| match value {
            ResultValue::Literal { value, .. } => value,
            _ => "",
        });
        Ok(())
    }

    async fn ask(&mut self, _result: bool) -> sparql_results::Result<()> {
        Ok(())
    }
}
```

Use `parse_srj_streaming` or `parse_srx_streaming` with a sink such as
`RowSink`. The sink is awaited before the parser advances, so a slow consumer
creates backpressure instead of an unbounded in-memory queue.

Complete-document parsing and writing is isolated in the explicit `bounded`
module. Those adapters materialize every solution row in `SparqlResult` and
must only be used when the caller enforces a finite input/result limit. They
are not used by the streaming APIs.

## License

Licensed under either of:

- Apache License, Version 2.0 (`LICENSE-APACHE` or
  <https://www.apache.org/licenses/LICENSE-2.0>).
- MIT License (`LICENSE-MIT` or <https://opensource.org/licenses/MIT>).
