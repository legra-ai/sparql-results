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
- [SPARQL Query Results CSV and TSV](https://www.w3.org/TR/sparql12-results-csv-tsv/)
  (writers for both, a reader for TSV only; see below)

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
use sparql_results::{ResultHead, ResultRow, ResultValue, SrjWriter};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> sparql_results::Result<()> {
    let head = ResultHead::select(vec!["name".to_owned()])?;
    let mut writer = SrjWriter::start_select(tokio::io::sink(), &head).await?;

    let mut bindings = HashMap::new();
    bindings.insert(
        "name".to_owned(),
        ResultValue::Literal {
            value: "Ada".to_owned(),
            lang: None,
            datatype: None,
            dir: None,
        },
    );
    writer.write_row(&ResultRow { bindings: bindings.into_iter().collect() }).await?;
    let _sink = writer.finish().await?;
    Ok(())
}
```

`SrxWriter`, `CsvWriter` and `TsvWriter` provide the same incremental
interface for XML, CSV and TSV. Every writer awaits each downstream write,
preserving backpressure.

A `ResultHead` holds the projected variables and any `link` IRIs. It is valid
by construction: every variable is a SPARQL `VARNAME`, declared once
(`HeaderError`). Writers check before they write, so a refusal emits zero
bytes: `write_row` refuses a row that binds an undeclared variable, holds an
invalid literal, or has a triple term whose subject is not an IRI or blank
node or whose predicate is not an IRI (`RowError`); `write_srj` / `write_srx`
check the whole bounded result first, including that an ASK header declares
no variables. Output is deterministic: SRJ bindings are written in variable
name order, SRX bindings in row order.

## CSV and TSV

`CsvWriter` and `TsvWriter` have the same `start_select` / `write_row` /
`finish` interface as `SrjWriter`. Each row is checked, rendered into one
row-sized buffer and written with one awaited write. They follow the
[SPARQL 1.2 Query Results CSV and TSV Formats](https://www.w3.org/TR/sparql12-results-csv-tsv/)
draft, which keeps the SPARQL 1.1 rules and adds triple terms and
directional literals.

| | CSV (§3) | TSV (§4) |
| - | -------- | -------- |
| Line end (§2) | CRLF | LF |
| Header | `x,y` | `?x<TAB>?y` |
| IRI | `http://example.org/a` | `<http://example.org/a>` |
| Literal | lexical form only | `"v"`, `"v"@en`, `"v"@en--ltr`, `"v"^^<dt>`, or `42` / `-0.5` / `1.0E6` for XSD integers, decimals and doubles |
| Blank node | `_:b0` | `_:b0` |
| Triple term | `<<( http://s http://p "o" )>>` | `<<( <http://s> <http://p> "o" )>>` |
| Unbound | empty field | empty field |
| Escaping | RFC 4180: a field with `"`, `,`, LF or CR is quoted and its `"` doubled | `\t`, `\n`, `\r`, `\"`, `\\` in literals; IRI characters that Turtle excludes become `\uXXXX` |

Both formats define only SELECT results. `write_csv` and `write_tsv` refuse
an ASK result with `SparqlResultsError::AskNotTabular` before writing any
bytes. Neither format has a `link` header, so links are not written. A TSV
row is refused with `TsvError::BlankNodeLabel` when it holds a blank node
label that `_:label` cannot spell. Both writers refuse a triple term whose
predicate is not an IRI, at any nesting depth (`RowError::TriplePredicate`).

`parse_tsv_streaming` reads TSV one line at a time and awaits a
`TsvStreamSink` for each row. It accepts every Turtle term form in §4.2:
single- or double-quoted strings with `ECHAR` / `UCHAR` escapes, `@lang` and
`@lang--dir`, `^^<datatype>`, numbers, `true` / `false`, and `<<( s p o )>>`.
It checks each term as the SRJ and SRX readers do. Malformed input is
refused with a typed `TsvError` that gives the line and field.

There is no CSV reader. The specification calls CSV "a lossy encoding of a
table of results". A CSV field drops a literal's datatype, language tag and
base direction. It cannot tell an IRI from a literal that spells one, a
blank node from the literal `"_:b0"`, or a triple term from a string that
looks like one. CSV also writes an unbound variable and an empty string the
same way. A CSV reader would have to guess the RDF terms, so the crate does
not offer one. Use TSV, SRJ or SRX when results must be read back.

## Streaming parsing

Parsers emit the header (variables and `link` IRIs), each row, or the ASK
result to an async sink before reading more input. A sink can forward rows
to a database, another encoder, or an application stream without collecting
the document.

```rust
use sparql_results::{ResultHead, ResultRow, ResultValue, SrjStreamSink, parse_srj_streaming};

struct RowSink;

#[async_trait::async_trait]
impl SrjStreamSink for RowSink {
    async fn select_header(&mut self, _head: ResultHead) -> sparql_results::Result<()> {
        Ok(())
    }

    async fn select_row(&mut self, row: ResultRow) -> sparql_results::Result<()> {
        let _ = row.bindings.get("name").map(|value| match value {
            ResultValue::Literal { value, .. } => value,
            _ => "",
        });
        Ok(())
    }

    async fn ask(&mut self, _head: ResultHead, _result: bool) -> sparql_results::Result<()> {
        Ok(())
    }
}
```

Use `parse_srj_streaming` or `parse_srx_streaming` with a sink such as
`RowSink` (`parse_tsv_streaming` takes a `TsvStreamSink`, which has no
`ask` event). The sink is awaited before the parser advances, so a slow consumer
creates backpressure instead of an unbounded in-memory queue.

Complete-document parsing and writing is isolated in the explicit `bounded`
module. Those adapters materialize every solution row in `SparqlResult` and
must only be used when the caller enforces a finite input/result limit. They
are not used by the streaming APIs.

## Literal validation

Both readers check every literal's annotations and stop with
`SparqlResultsError::InvalidLiteral(LiteralError)` when they are invalid:

| Input | `LiteralError` |
| ----- | -------------- |
| A bare `"dir"` key in SRJ, or an unprefixed `dir` attribute in SRX. The base direction is written only as `its:dir`. | `BareDirKey` |
| `its:dir` without a language tag | `DirectionWithoutLanguage` |
| A language tag whose datatype is not `rdf:langString` (or `rdf:dirLangString` when `its:dir` is present) | `DatatypeContradictsLanguage` |
| A datatype of `rdf:langString` / `rdf:dirLangString` with no language tag | `DatatypeRequiresLanguage` |
| A language tag that is not well-formed BCP 47 (RFC 5646, syntax only; the IANA registry is not consulted) | `InvalidLanguageTag` |

The checks run on each value as it is read, so streaming and bounded memory
are unchanged.

## License

Licensed under either of:

- Apache License, Version 2.0 (`LICENSE-APACHE` or
  <https://www.apache.org/licenses/LICENSE-2.0>).
- MIT License (`LICENSE-MIT` or <https://opensource.org/licenses/MIT>).
