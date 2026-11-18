//! Writing 10^5 rows: no single write, no single allocation, and no peak of
//! live heap grows with the row count.
//!
//! This binary holds one test so the counting allocator sees only it.

#![allow(missing_docs)]

use std::alloc::{
    GlobalAlloc,
    Layout,
    System,
};
use std::pin::Pin;
use std::sync::atomic::{
    AtomicBool,
    AtomicUsize,
    Ordering,
};
use std::task::{
    Context,
    Poll,
};

use indexmap::IndexMap;
use sparql_results::{
    BaseDirection,
    CsvWriter,
    ResultHead,
    ResultRow,
    ResultValue,
    SrjWriter,
    SrxWriter,
    TsvWriter,
};
use tokio::io::AsyncWrite;

const SMALL: u32 = 1_000;
const LARGE: u32 = 100_000;

/// Records the largest single allocation and the peak of live heap bytes
/// while recording is on.
struct CountingAllocator {
    recording: AtomicBool,
    largest: AtomicUsize,
    live: AtomicUsize,
    peak: AtomicUsize,
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator {
    recording: AtomicBool::new(false),
    largest: AtomicUsize::new(0),
    live: AtomicUsize::new(0),
    peak: AtomicUsize::new(0),
};

// SAFETY: every call is forwarded unchanged to `System`; only counters change.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.grow(layout.size());
        // SAFETY: forwarded with the caller's layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.shrink(layout.size());
        // SAFETY: forwarded with the caller's pointer and layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        self.shrink(layout.size());
        self.grow(new_size);
        // SAFETY: forwarded with the caller's pointer, layout and size.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

impl CountingAllocator {
    fn grow(&self, size: usize) {
        let live = self.live.fetch_add(size, Ordering::Relaxed) + size;
        if self.recording.load(Ordering::Relaxed) {
            self.largest.fetch_max(size, Ordering::Relaxed);
            self.peak.fetch_max(live, Ordering::Relaxed);
        }
    }

    fn shrink(&self, size: usize) {
        self.live.fetch_sub(size, Ordering::Relaxed);
    }

    /// Start recording; the peak is measured above the current live heap.
    fn start(&self) -> usize {
        let base = self.live.load(Ordering::Relaxed);
        self.largest.store(0, Ordering::Relaxed);
        self.peak.store(base, Ordering::Relaxed);
        self.recording.store(true, Ordering::Relaxed);
        base
    }

    fn stop(&self, base: usize) -> Usage {
        self.recording.store(false, Ordering::Relaxed);
        Usage {
            largest_allocation: self.largest.load(Ordering::Relaxed),
            peak_live: self.peak.load(Ordering::Relaxed).saturating_sub(base),
            largest_write: 0,
            writes: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Usage {
    largest_allocation: usize,
    peak_live: usize,
    largest_write: usize,
    writes: usize,
}

/// Discards bytes, recording the largest single write.
#[derive(Default)]
struct WriteRecorder {
    largest: usize,
    writes: usize,
}

impl AsyncWrite for WriteRecorder {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        self.largest = self.largest.max(buf.len());
        self.writes += 1;
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

/// A fixed-width row, so every row has the same encoded size.
fn row(index: u32) -> ResultRow {
    ResultRow {
        bindings: IndexMap::from([
            (
                "s".to_owned(),
                ResultValue::Uri(format!("http://example.org/row/{index:08}")),
            ),
            (
                "o".to_owned(),
                ResultValue::Literal {
                    value: format!("value {index:08}"),
                    lang: Some("ar".to_owned()),
                    datatype: None,
                    dir: Some(BaseDirection::Rtl),
                },
            ),
        ]),
    }
}

#[derive(Clone, Copy)]
enum Format {
    Srj,
    Srx,
    Csv,
    Tsv,
}

impl Format {
    async fn write(self, rows: u32) -> Usage {
        let head = ResultHead::select(vec!["s".to_owned(), "o".to_owned()]).unwrap();
        let base = ALLOCATOR.start();
        let recorder = match self {
            Self::Srj => {
                let mut writer = SrjWriter::start_select(WriteRecorder::default(), &head)
                    .await
                    .unwrap();
                for index in 0..rows {
                    writer.write_row(&row(index)).await.unwrap();
                }
                writer.finish().await.unwrap()
            }
            Self::Srx => {
                let mut writer = SrxWriter::start_select(WriteRecorder::default(), &head)
                    .await
                    .unwrap();
                for index in 0..rows {
                    writer.write_row(&row(index)).await.unwrap();
                }
                writer.finish().await.unwrap()
            }
            Self::Csv => {
                let mut writer = CsvWriter::start_select(WriteRecorder::default(), &head)
                    .await
                    .unwrap();
                for index in 0..rows {
                    writer.write_row(&row(index)).await.unwrap();
                }
                writer.finish().await.unwrap()
            }
            Self::Tsv => {
                let mut writer = TsvWriter::start_select(WriteRecorder::default(), &head)
                    .await
                    .unwrap();
                for index in 0..rows {
                    writer.write_row(&row(index)).await.unwrap();
                }
                writer.finish().await.unwrap()
            }
        };
        Usage {
            largest_write: recorder.largest,
            writes: recorder.writes,
            ..ALLOCATOR.stop(base)
        }
    }
}

#[test]
fn writes_and_allocations_do_not_grow_with_row_count() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for format in [Format::Srj, Format::Srx, Format::Csv, Format::Tsv] {
        let small = runtime.block_on(format.write(SMALL));
        let large = runtime.block_on(format.write(LARGE));
        assert!(large.writes > small.writes * 50, "{small:?} {large:?}");
        assert_eq!(large.largest_write, small.largest_write, "largest write");
        assert_eq!(
            large.largest_allocation, small.largest_allocation,
            "largest allocation"
        );
        assert_eq!(large.peak_live, small.peak_live, "peak live heap");
        assert!(small.largest_write < 512, "{small:?}");
        assert!(small.peak_live < 16 * 1024, "{small:?}");
    }
}
