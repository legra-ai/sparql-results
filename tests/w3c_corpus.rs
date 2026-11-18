//! Round-trips every vendored W3C SPARQL result-format file that can be
//! read (see `tests/fixtures/w3c/README.md`): read the `.srj`, `.srx` or
//! `.tsv` file, then write it in each of SRJ, SRX and TSV, read it back and
//! compare models. TSV defines only SELECT results and has no `link`
//! header: an ASK file must be refused by the CSV and TSV writers, and a
//! SELECT model is compared without its links after a TSV round trip. The
//! W3C `.csv` files are checked in `tests/w3c_csv_corpus.rs`.

#![allow(missing_docs)]

use std::path::{
    Path,
    PathBuf,
};

use sparql_results::bounded::{
    SparqlResult,
    parse_srj_bounded,
    parse_srx_bounded,
    parse_tsv_bounded,
    write_csv,
    write_srj,
    write_srx,
    write_tsv,
};
use sparql_results::{
    ResultHead,
    SparqlResultsError,
};

const CORPUS: &str = "tests/fixtures/w3c";

/// Every `.srj` / `.srx` / `.tsv` file under the corpus.
const EXPECTED_FILES: usize = 523;

/// Files that do not round-trip, each with its reason. A listed file that
/// starts passing fails the test, so the list cannot go stale.
const SKIPS: &[(&str, &str)] = &[];

/// Files that must round-trip: `EXPECTED_FILES - SKIPS.len()`, pinned.
const EXPECTED_PASSES: usize = 523;

/// Passing files whose header carries `link` IRIs, so links are exercised.
const EXPECTED_WITH_LINKS: usize = 3;

/// Passing ASK files, each refused by the CSV and TSV writers.
const EXPECTED_ASK: usize = 31;

/// Passing SELECT files, each also round-tripped through TSV and written as
/// CSV.
const EXPECTED_SELECT: usize = EXPECTED_PASSES - EXPECTED_ASK;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Srj,
    Srx,
    Tsv,
}

impl Format {
    const ALL: [Self; 3] = [Self::Srj, Self::Srx, Self::Tsv];

    fn of(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "srj" => Some(Self::Srj),
            "srx" => Some(Self::Srx),
            "tsv" => Some(Self::Tsv),
            _ => None,
        }
    }

    async fn read(self, bytes: &[u8]) -> Result<SparqlResult, String> {
        match self {
            Self::Srj => parse_srj_bounded(bytes).await,
            Self::Srx => parse_srx_bounded(bytes).await,
            Self::Tsv => parse_tsv_bounded(bytes).await,
        }
        .map_err(|error| error.to_string())
    }

    async fn write(self, model: &SparqlResult) -> Result<Vec<u8>, String> {
        match self {
            Self::Srj => write_srj(Vec::new(), model).await,
            Self::Srx => write_srx(Vec::new(), model).await,
            Self::Tsv => write_tsv(Vec::new(), model).await,
        }
        .map_err(|error| error.to_string())
    }

    /// The model as this format can carry it: TSV has no `link` header.
    fn carried(self, model: &SparqlResult) -> SparqlResult {
        match (self, model) {
            (Self::Tsv, SparqlResult::Select { head, rows }) => SparqlResult::Select {
                head: ResultHead::select(head.vars().to_vec()).expect("valid header"),
                rows: rows.clone(),
            },
            _ => model.clone(),
        }
    }

    /// Write `model` in this format, read it back, and require equality.
    async fn round_trip(self, model: &SparqlResult) -> Result<(), String> {
        let written = self.write(model).await?;
        let reread = self.read(&written).await?;
        if reread == self.carried(model) {
            Ok(())
        } else {
            Err(format!("model changed on a {} round trip", self.name()))
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Srj => "SRJ",
            Self::Srx => "SRX",
            Self::Tsv => "TSV",
        }
    }
}

/// What a passing file exercised.
struct Checked {
    linked: bool,
    ask: bool,
}

struct CorpusFile {
    path: PathBuf,
    format: Format,
}

impl CorpusFile {
    fn collect() -> Vec<Self> {
        let mut files = Vec::new(); // bounded: the vendored corpus
        let mut dirs = vec![PathBuf::from(CORPUS)];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(&dir).expect("read corpus directory") {
                let path = entry.expect("corpus entry").path();
                if path.is_dir() {
                    dirs.push(path);
                } else if let Some(format) = Format::of(&path) {
                    files.push(Self { path, format });
                }
            }
        }
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }

    fn name(&self) -> String {
        self.path
            .strip_prefix(CORPUS)
            .expect("corpus path")
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// Round-trip the file through every readable format; an ASK file must
    /// instead be refused by the CSV and TSV writers.
    async fn check(&self) -> Result<Checked, String> {
        let bytes = std::fs::read(&self.path).map_err(|error| error.to_string())?;
        let model = self.format.read(&bytes).await?;
        let ask = matches!(model, SparqlResult::Ask { .. });
        for format in Format::ALL {
            if ask && format == Format::Tsv {
                continue;
            }
            format.round_trip(&model).await?;
        }
        Self::check_csv(&model).await?;
        let (SparqlResult::Select { head, .. } | SparqlResult::Ask { head, .. }) = &model;
        Ok(Checked {
            linked: !head.links().is_empty(),
            ask,
        })
    }

    /// A SELECT model writes as CSV; an ASK model is refused by CSV and TSV.
    async fn check_csv(model: &SparqlResult) -> Result<(), String> {
        let csv = write_csv(Vec::new(), model).await;
        match model {
            SparqlResult::Select { .. } => csv.map(drop).map_err(|error| error.to_string()),
            SparqlResult::Ask { .. } => {
                let tsv = write_tsv(Vec::new(), model).await;
                match (csv, tsv) {
                    (
                        Err(SparqlResultsError::AskNotTabular(_)),
                        Err(SparqlResultsError::AskNotTabular(_)),
                    ) => Ok(()),
                    _ => Err("CSV or TSV accepted an ASK result".to_owned()),
                }
            }
        }
    }
}

#[tokio::test]
async fn every_w3c_result_file_round_trips() {
    let files = CorpusFile::collect();
    assert_eq!(files.len(), EXPECTED_FILES, "corpus file count");
    assert_eq!(EXPECTED_PASSES + SKIPS.len(), EXPECTED_FILES);

    let mut passes = 0_usize;
    let mut with_links = 0_usize;
    let mut asks = 0_usize;
    let mut failures = Vec::new(); // bounded: the vendored corpus
    for file in &files {
        let name = file.name();
        let skip = SKIPS.iter().find(|(path, _)| *path == name);
        match (file.check().await, skip) {
            (Ok(checked), None) => {
                passes += 1;
                with_links += usize::from(checked.linked);
                asks += usize::from(checked.ask);
            }
            (Ok(_), Some((_, reason))) => {
                failures.push(format!("{name}: stale skip ({reason}), now passes"));
            }
            (Err(_), Some(_)) => {}
            (Err(error), None) => failures.push(format!("{name}: {error}")),
        }
    }
    for (path, _) in SKIPS {
        assert!(
            files.iter().any(|file| file.name() == *path),
            "skip lists {path}, which is not in the corpus"
        );
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(passes, EXPECTED_PASSES, "pinned pass count");
    assert_eq!(with_links, EXPECTED_WITH_LINKS, "pinned files with links");
    assert_eq!(asks, EXPECTED_ASK, "pinned ASK files");
    assert_eq!(passes - asks, EXPECTED_SELECT, "pinned SELECT files");
}

#[test]
fn corpus_matches_its_checksum_manifest() {
    let manifest =
        std::fs::read_to_string(format!("{CORPUS}/SHA256SUMS")).expect("read SHA256SUMS");
    let mut listed: Vec<&str> = manifest // bounded: the vendored corpus
        .lines()
        .map(|line| line.split_once("  ").expect("`<sha256>  <path>` line").1)
        .collect();
    listed.sort_unstable();
    assert_eq!(
        listed,
        vendored_files(),
        "SHA256SUMS lists exactly the vendored files"
    );
}

/// Every file under `sparql/`, by its path relative to the corpus.
fn vendored_files() -> Vec<String> {
    let mut files = Vec::new(); // bounded: the vendored corpus
    let mut dirs = vec![PathBuf::from(CORPUS).join("sparql")];
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).expect("read corpus directory") {
            let path = entry.expect("corpus entry").path();
            if path.is_dir() {
                dirs.push(path);
            } else {
                let name = path.strip_prefix(CORPUS).expect("corpus path");
                files.push(name.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    files.sort_unstable();
    files
}
