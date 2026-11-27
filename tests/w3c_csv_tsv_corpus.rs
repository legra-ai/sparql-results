//! The W3C SPARQL 1.1 `csv-tsv-res` result files, checked byte for byte
//! against this crate's writers (see `tests/fixtures/w3c/README.md`).
//!
//! CSV cannot be read back, so each `.csv` file is checked by writing the
//! model of the `.tsv` file that its manifest entry shares a query and data
//! with, and comparing bytes. The W3C files differ from a faithful writer in
//! a few pinned, explained places; each pinned difference must still occur,
//! so the list cannot go stale.

#![allow(missing_docs)]

use sparql_results::bounded::{
    parse_tsv_bounded,
    write_csv,
    write_tsv,
};

const DIR: &str = "tests/fixtures/w3c/sparql/sparql11/csv-tsv-res";

/// `.tsv` files: each must be written back byte for byte.
const TSV_FILES: [&str; 3] = ["csvtsv01.tsv", "csvtsv02.tsv", "csvtsv03.tsv"];

/// Each `.csv` file with the `.tsv` file that `manifest.ttl` runs on the same
/// query and data (`csv04` repeats `csv03` with CRLF line ends).
const CSV_PAIRS: [(&str, &str); 4] = [
    ("csvtsv01.csv", "csvtsv01.tsv"),
    ("csvtsv02.csv", "csvtsv02.tsv"),
    ("csvtsv03.csv", "csvtsv03.tsv"),
    ("csvtsv04.csv", "csvtsv03.tsv"),
];

/// `(file, W3C text, written text, reason)`: where an expected `.csv` file
/// differs from the CSV written for its `.tsv` model.
const CSV_DIFFERENCES: &[(&str, &str, &str, &str)] = &[
    (
        "csvtsv01.csv",
        "_:a",
        "_:b0",
        "blank node labels are scoped to one result (§3.2); the TSV file labels it b0",
    ),
    ("csvtsv02.csv", "_:a", "_:b0", "as csvtsv01.csv"),
    (
        "csvtsv03.csv",
        "1.0E6",
        "1.0e6",
        "the TSV file abbreviates the double as 1.0e6; CSV writes that lexical form",
    ),
    ("csvtsv04.csv", "1.0E6", "1.0e6", "as csvtsv03.csv"),
];

/// Files whose lines end with LF, which readers must accept (§2); the
/// writer emits CRLF. `csvtsv04.csv` ends its lines with CRLF.
const CSV_LF_FILES: [&str; 3] = ["csvtsv01.csv", "csvtsv02.csv", "csvtsv03.csv"];

fn read(name: &str) -> Vec<u8> {
    std::fs::read(format!("{DIR}/{name}")).expect("read corpus file")
}

/// The expected CSV bytes after the pinned differences.
fn expected_csv(name: &str) -> String {
    let mut text = String::from_utf8(read(name)).expect("UTF-8 corpus file");
    if CSV_LF_FILES.contains(&name) {
        assert!(!text.contains('\r'), "{name} is pinned as LF");
        text = text.replace('\n', "\r\n");
    } else {
        assert!(text.ends_with("\r\n"), "{name} is pinned as CRLF");
    }
    for (file, w3c, written, _reason) in CSV_DIFFERENCES {
        if *file == name {
            assert!(
                text.contains(w3c),
                "{name}: pinned difference {w3c:?} is stale"
            );
            text = text.replace(w3c, written);
        }
    }
    text
}

#[tokio::test]
async fn w3c_tsv_files_are_written_back_byte_for_byte() {
    for name in TSV_FILES {
        let bytes = read(name);
        let model = parse_tsv_bounded(bytes.as_slice()).await.expect(name);
        let written = write_tsv(Vec::new(), &model).await.expect(name);
        assert_eq!(
            String::from_utf8(written).unwrap(),
            String::from_utf8(bytes).unwrap(),
            "{name}"
        );
    }
}

#[tokio::test]
async fn w3c_csv_files_match_the_csv_writer() {
    for (csv, tsv) in CSV_PAIRS {
        let model = parse_tsv_bounded(read(tsv).as_slice()).await.expect(tsv);
        let written = write_csv(Vec::new(), &model).await.expect(csv);
        assert_eq!(
            String::from_utf8(written).unwrap(),
            expected_csv(csv),
            "{csv}"
        );
    }
}

#[test]
fn every_vendored_csv_and_tsv_file_is_checked() {
    let mut vendored: Vec<String> = std::fs::read_dir(DIR) // bounded: one corpus directory
        .expect("read corpus directory")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "csv" || ext == "tsv")
        })
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    vendored.sort_unstable();
    let mut checked: Vec<&str> = TSV_FILES // bounded: one corpus directory
        .into_iter()
        .chain(CSV_PAIRS.map(|(csv, _)| csv))
        .collect();
    checked.sort_unstable();
    assert_eq!(vendored, checked);
    assert_eq!(
        CSV_PAIRS.len() + TSV_FILES.len(),
        7,
        "pinned CSV/TSV file count"
    );
}
