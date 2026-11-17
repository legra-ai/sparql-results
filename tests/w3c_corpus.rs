//! Round-trips every vendored W3C SPARQL result-format file (see
//! `tests/fixtures/w3c/README.md`): read, write in the same format, read
//! again and compare models; then the same through the other format.

#![allow(missing_docs)]

use std::path::{
    Path,
    PathBuf,
};

use sparql_results::bounded::{
    SparqlResult,
    parse_srj_bounded,
    parse_srx_bounded,
    write_srj,
    write_srx,
};

const CORPUS: &str = "tests/fixtures/w3c";

/// Every `.srj` / `.srx` file under the corpus.
const EXPECTED_FILES: usize = 520;

/// Files that do not round-trip, each with its reason. A listed file that
/// starts passing fails the test, so the list cannot go stale.
const SKIPS: &[(&str, &str)] = &[];

/// Files that must round-trip: `EXPECTED_FILES - SKIPS.len()`, pinned.
const EXPECTED_PASSES: usize = 520;

/// Passing files whose header carries `link` IRIs, so links are exercised.
const EXPECTED_WITH_LINKS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Srj,
    Srx,
}

impl Format {
    fn of(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()? {
            "srj" => Some(Self::Srj),
            "srx" => Some(Self::Srx),
            _ => None,
        }
    }

    const fn other(self) -> Self {
        match self {
            Self::Srj => Self::Srx,
            Self::Srx => Self::Srj,
        }
    }

    async fn read(self, bytes: &[u8]) -> Result<SparqlResult, String> {
        match self {
            Self::Srj => parse_srj_bounded(bytes).await,
            Self::Srx => parse_srx_bounded(bytes).await,
        }
        .map_err(|error| error.to_string())
    }

    async fn write(self, model: &SparqlResult) -> Result<Vec<u8>, String> {
        match self {
            Self::Srj => write_srj(Vec::new(), model).await,
            Self::Srx => write_srx(Vec::new(), model).await,
        }
        .map_err(|error| error.to_string())
    }

    /// Write `model` in this format, read it back, and require equality.
    async fn round_trip(self, model: &SparqlResult) -> Result<(), String> {
        let written = self.write(model).await?;
        let reread = self.read(&written).await?;
        if &reread == model {
            Ok(())
        } else {
            Err("model changed on round trip".to_owned())
        }
    }
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

    /// Round-trip the file; `Ok(true)` when its header carries links.
    async fn check(&self) -> Result<bool, String> {
        let bytes = std::fs::read(&self.path).map_err(|error| error.to_string())?;
        let model = self.format.read(&bytes).await?;
        self.format.round_trip(&model).await?;
        self.format.other().round_trip(&model).await?;
        let (SparqlResult::Select { head, .. } | SparqlResult::Ask { head, .. }) = &model;
        Ok(!head.links().is_empty())
    }
}

#[tokio::test]
async fn every_w3c_result_file_round_trips() {
    let files = CorpusFile::collect();
    assert_eq!(files.len(), EXPECTED_FILES, "corpus file count");
    assert_eq!(EXPECTED_PASSES + SKIPS.len(), EXPECTED_FILES);

    let mut passes = 0_usize;
    let mut with_links = 0_usize;
    let mut failures = Vec::new(); // bounded: the vendored corpus
    for file in &files {
        let name = file.name();
        let skip = SKIPS.iter().find(|(path, _)| *path == name);
        match (file.check().await, skip) {
            (Ok(linked), None) => {
                passes += 1;
                with_links += usize::from(linked);
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
    let files: Vec<String> = CorpusFile::collect().iter().map(CorpusFile::name).collect(); // bounded: the vendored corpus
    assert_eq!(listed, files, "SHA256SUMS lists exactly the vendored files");
}
