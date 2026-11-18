# W3C SPARQL result-format test files

Every `.srj` (SPARQL Query Results JSON), `.srx` (SPARQL Query Results
XML), `.csv` and `.tsv` (SPARQL Query Results CSV and TSV) file from the
SPARQL 1.0, 1.1, and 1.2 test suites, vendored unchanged for
`tests/w3c_corpus.rs` and `tests/w3c_csv_tsv_corpus.rs`. The
`sparql11/csv-tsv-res` directory also keeps its `manifest.ttl`, which pairs
each `.csv` file with a `.tsv` file run on the same query and data, and its
`.gitattributes`, which keeps line endings byte-exact (`csvtsv04.csv` is
CRLF).

| Field | Value |
| ----- | ----- |
| Source | <https://github.com/w3c/rdf-tests>, directory `sparql/` |
| Commit | `7b5afb91857a9c282c03d2e45d3f76485797c901` (2026-11-12T07:04:28-08:00) |
| Vendored | 2026-11-17 |
| Files | 520 `.srj` / `.srx` (sparql10: 131, sparql11: 331, sparql12: 58); 4 `.csv` and 3 `.tsv` (sparql11/csv-tsv-res, vendored 2026-11-18); `manifest.ttl` and `.gitattributes` of csv-tsv-res |
| Checksums | SHA-256 of every file in [`SHA256SUMS`](SHA256SUMS) (`shasum -a 256 -c SHA256SUMS`) |
| License | [`LICENSE.md`](LICENSE.md): the W3C test suite dual license |

The paths under `sparql/` match the upstream repository. To refresh, clone
the repository at a new commit, copy every `sparql/**/*.srj`,
`sparql/**/*.srx`, `sparql/**/*.csv` and `sparql/**/*.tsv` here (with
`csv-tsv-res/manifest.ttl` and `csv-tsv-res/.gitattributes`), regenerate `SHA256SUMS` from this directory with
`find sparql -type f | sort | xargs shasum -a 256 > SHA256SUMS`, update this
table, and re-pin the counts in `tests/w3c_corpus.rs` and
`tests/w3c_csv_tsv_corpus.rs`.
