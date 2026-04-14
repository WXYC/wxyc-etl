//! End-to-end pipeline orchestration.

use std::fmt;
use std::thread::JoinHandle;

use anyhow::Result;
use crossbeam_channel::Receiver;
use log::{info, warn};

use super::processor::process_batch;
use super::scanner::Batch;
use super::writer::PipelineOutput;

/// Accumulated statistics from a pipeline run.
pub struct PipelineStats {
    /// Total items scanned by the producer.
    pub scanned: usize,
    /// Items written to the output.
    pub written: usize,
    /// Items filtered out by the transform (returned `None`).
    pub filtered: usize,
    /// Items skipped for other reasons (e.g., missing required fields).
    pub skipped: usize,
    /// Duplicate items skipped.
    pub duplicates: usize,
}

impl fmt::Display for PipelineStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Complete: {} scanned, {} written, {} filtered, {} skipped",
            self.scanned, self.written, self.filtered, self.skipped
        )?;
        if self.duplicates > 0 {
            write!(f, ", {} duplicate ids skipped", self.duplicates)?;
        }
        Ok(())
    }
}

/// Consume batches from a scanner, process with rayon, write sequentially.
///
/// The `transform` function returns `Some(result)` for items that should be
/// written, or `None` to filter them out. This matches the pattern from
/// `consume_releases()` in `discogs-xml-converter`.
///
/// The receiver is dropped before joining the scanner handle to prevent the
/// deadlock documented in `main.rs`: if the receiver stays alive after an
/// error, the scanner blocks on `send()` and the join never completes.
pub fn run_pipeline<T, R, F, O>(
    rx: Receiver<Batch<T>>,
    handle: JoinHandle<Result<usize>>,
    transform: F,
    output: &mut O,
) -> Result<PipelineStats>
where
    T: Sync + Send,
    R: Send,
    F: Fn(&T) -> Option<R> + Sync + Send,
    O: PipelineOutput<R>,
{
    let mut written = 0usize;
    let mut filtered = 0usize;

    let loop_result: Result<()> = (|| {
        for batch in &rx {
            let results: Vec<Option<R>> = process_batch(&batch, &transform);

            for result in results {
                match result {
                    Some(item) => {
                        output.write_item(&item)?;
                        written += 1;
                    }
                    None => filtered += 1,
                }
            }
        }

        output.flush()?;
        Ok(())
    })();

    // Drop receiver so scanner's send() unblocks, preventing deadlock.
    drop(rx);

    let scanner_result = handle.join().unwrap();
    if let Err(ref e) = loop_result {
        warn!("Pipeline processing failed: {}", e);
        return Err(loop_result.unwrap_err());
    }
    let total = scanner_result?;

    let stats = PipelineStats {
        scanned: total,
        written,
        filtered,
        skipped: 0,
        duplicates: 0,
    };
    info!("{}", stats);
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::scanner::{start_scanner, BatchConfig};
    use crate::pipeline::writer::PipelineOutput;
    use anyhow::Result;

    struct MockOutput {
        items: Vec<u32>,
    }

    impl MockOutput {
        fn new() -> Self {
            Self { items: Vec::new() }
        }
    }

    impl PipelineOutput<u32> for MockOutput {
        fn write_item(&mut self, item: &u32) -> Result<()> {
            self.items.push(*item);
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_run_pipeline_end_to_end() {
        let config = BatchConfig {
            batch_size: 3,
            channel_capacity: 4,
        };
        let (rx, handle) = start_scanner(
            |tx| {
                for i in 0..5u32 {
                    tx.send_item(i)?;
                }
                Ok(5)
            },
            config,
        );

        let mut output = MockOutput::new();
        let stats = run_pipeline(rx, handle, |&x| Some(x * 10), &mut output).unwrap();

        assert_eq!(stats.scanned, 5);
        assert_eq!(stats.written, 5);
        assert_eq!(stats.filtered, 0);
        assert_eq!(output.items, vec![0, 10, 20, 30, 40]);
    }

    #[test]
    fn test_run_pipeline_with_filtering() {
        let config = BatchConfig {
            batch_size: 3,
            channel_capacity: 4,
        };
        let (rx, handle) = start_scanner(
            |tx| {
                for i in 0..6u32 {
                    tx.send_item(i)?;
                }
                Ok(6)
            },
            config,
        );

        let mut output = MockOutput::new();
        let stats = run_pipeline(
            rx,
            handle,
            |&x| if x % 2 == 0 { Some(x) } else { None },
            &mut output,
        )
        .unwrap();

        assert_eq!(stats.scanned, 6);
        assert_eq!(stats.written, 3);
        assert_eq!(stats.filtered, 3);
        assert_eq!(output.items, vec![0, 2, 4]);
    }

    #[test]
    fn test_run_pipeline_empty_input() {
        let config = BatchConfig::default();
        let (rx, handle) = start_scanner(|_tx| Ok(0), config);

        let mut output = MockOutput::new();
        let stats = run_pipeline(rx, handle, |&x: &u32| Some(x), &mut output).unwrap();

        assert_eq!(stats.scanned, 0);
        assert_eq!(stats.written, 0);
        assert!(output.items.is_empty());
    }

    #[test]
    fn test_pipeline_stats_display() {
        let stats = PipelineStats {
            scanned: 100,
            written: 80,
            filtered: 15,
            skipped: 5,
            duplicates: 0,
        };
        let display = format!("{}", stats);
        assert!(display.contains("100"));
        assert!(display.contains("80"));
        assert!(display.contains("15"));
        assert!(display.contains("5"));
        assert!(!display.contains("duplicate"));
    }

    #[test]
    fn test_pipeline_stats_display_with_duplicates() {
        let stats = PipelineStats {
            scanned: 50,
            written: 40,
            filtered: 5,
            skipped: 2,
            duplicates: 3,
        };
        let display = format!("{}", stats);
        assert!(display.contains("3 duplicate"));
    }
}
