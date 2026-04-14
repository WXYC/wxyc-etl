//! End-to-end pipeline orchestration.

use std::fmt;
use std::thread::JoinHandle;

use anyhow::Result;
use crossbeam_channel::Receiver;
use log::{info, warn};

use std::collections::HashSet;

use super::processor::{process_batch, process_byte_batch};
use super::scanner::{Batch, ByteBatch};
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

    let scanner_result = handle
        .join()
        .map_err(|panic_payload| {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                format!("scanner thread panicked: {}", s)
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                format!("scanner thread panicked: {}", s)
            } else {
                "scanner thread panicked (unknown payload)".to_string()
            };
            anyhow::anyhow!(msg)
        })?;
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

/// Optional deduplication configuration for byte pipelines.
///
/// Extracts an ID from raw bytes *before* the transform runs. If the ID
/// has been seen, the item is skipped and counted as a duplicate.
pub struct DedupConfig<'a, I: Fn(&[u8]) -> Option<u64>> {
    /// Set of previously seen IDs.
    pub seen_ids: &'a mut HashSet<u64>,
    /// Extract an ID from the raw byte slice. Return `None` to skip the item.
    pub id_fn: I,
}

/// Byte-batch pipeline variant with optional deduplication.
///
/// Like [`run_pipeline`] but operates on [`ByteBatch`]es. The transform
/// receives raw byte slices and returns `Option<R>` (None to filter).
/// When a [`DedupConfig`] is provided, IDs are extracted from raw bytes
/// and duplicates are skipped before the transform runs.
pub fn run_byte_pipeline<R, F, O, I>(
    rx: Receiver<ByteBatch>,
    handle: JoinHandle<Result<usize>>,
    transform: F,
    output: &mut O,
    dedup: Option<DedupConfig<'_, I>>,
) -> Result<PipelineStats>
where
    R: Send,
    F: Fn(&[u8]) -> Option<R> + Sync + Send,
    O: PipelineOutput<R>,
    I: Fn(&[u8]) -> Option<u64>,
{
    let mut written = 0usize;
    let mut filtered = 0usize;
    let mut duplicates = 0usize;

    // Unpack dedup config (borrow checker needs the Option to be destructured)
    let mut dedup_state: Option<(&mut HashSet<u64>, &dyn Fn(&[u8]) -> Option<u64>)> =
        None;
    // We need to store the DedupConfig to keep id_fn alive
    let mut dedup_cfg = dedup;
    if let Some(ref mut cfg) = dedup_cfg {
        dedup_state = Some((&mut *cfg.seen_ids, &cfg.id_fn));
    }

    let loop_result: Result<()> = (|| {
        for batch in &rx {
            let results: Vec<Option<R>> = process_byte_batch(&batch, &transform);

            for (i, result) in results.into_iter().enumerate() {
                match result {
                    Some(item) => {
                        if let Some((ref mut seen, id_fn)) = dedup_state {
                            let bytes = batch.get(i);
                            if let Some(id) = id_fn(bytes) {
                                if !seen.insert(id) {
                                    duplicates += 1;
                                    continue;
                                }
                            }
                        }
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

    drop(rx);

    let scanner_result = handle
        .join()
        .map_err(|panic_payload| {
            let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                format!("scanner thread panicked: {}", s)
            } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                format!("scanner thread panicked: {}", s)
            } else {
                "scanner thread panicked (unknown payload)".to_string()
            };
            anyhow::anyhow!(msg)
        })?;
    if let Err(ref e) = loop_result {
        warn!("Byte pipeline processing failed: {}", e);
        return Err(loop_result.unwrap_err());
    }
    let total = scanner_result?;

    let stats = PipelineStats {
        scanned: total,
        written,
        filtered,
        skipped: 0,
        duplicates,
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

    struct StringOutput {
        items: Vec<String>,
    }

    impl StringOutput {
        fn new() -> Self {
            Self { items: Vec::new() }
        }
    }

    impl PipelineOutput<String> for StringOutput {
        fn write_item(&mut self, item: &String) -> Result<()> {
            self.items.push(item.clone());
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
    fn test_run_byte_pipeline() {
        use crate::pipeline::scanner::{start_byte_scanner, ByteBatch};

        let config = BatchConfig {
            batch_size: 2,
            channel_capacity: 4,
        };
        let (rx, handle) = start_byte_scanner(
            |tx| {
                tx.send(ByteBatch::from_slices(&[b"hello", b"world"]))?;
                tx.send(ByteBatch::from_slices(&[b"foo"]))?;
                Ok(3)
            },
            config,
        );

        let mut output = StringOutput::new();
        let no_dedup: Option<DedupConfig<'_, fn(&[u8]) -> Option<u64>>> = None;
        let stats = run_byte_pipeline(
            rx,
            handle,
            |bytes| Some(String::from_utf8_lossy(bytes).to_uppercase()),
            &mut output,
            no_dedup,
        )
        .unwrap();

        assert_eq!(stats.scanned, 3);
        assert_eq!(stats.written, 3);
        assert_eq!(output.items, vec!["HELLO", "WORLD", "FOO"]);
    }

    #[test]
    fn test_run_byte_pipeline_with_dedup() {
        use crate::pipeline::scanner::{start_byte_scanner, ByteBatch};
        use std::collections::HashSet;

        let config = BatchConfig {
            batch_size: 10,
            channel_capacity: 4,
        };
        let (rx, handle) = start_byte_scanner(
            |tx| {
                // "1:hello", "2:world", "1:hello-again" — ID 1 is duplicated
                tx.send(ByteBatch::from_slices(&[
                    b"1:hello",
                    b"2:world",
                    b"1:hello-again",
                ]))?;
                Ok(3)
            },
            config,
        );

        let mut output = StringOutput::new();
        let mut seen = HashSet::new();
        let stats = run_byte_pipeline(
            rx,
            handle,
            |bytes| {
                let s = String::from_utf8_lossy(bytes);
                let (_, val) = s.split_once(':')?;
                Some(val.to_uppercase())
            },
            &mut output,
            Some(DedupConfig {
                seen_ids: &mut seen,
                id_fn: |bytes| {
                    let s = String::from_utf8_lossy(bytes);
                    let (id, _) = s.split_once(':')?;
                    id.parse().ok()
                },
            }),
        )
        .unwrap();

        assert_eq!(stats.scanned, 3);
        assert_eq!(stats.written, 2);
        assert_eq!(stats.duplicates, 1);
        assert_eq!(output.items, vec!["HELLO", "WORLD"]);
    }

    // --- Integration tests: order preservation and realistic transforms ---

    #[test]
    fn test_pipeline_preserves_order_across_batches() {
        // Verify that items are written in the exact order they were scanned,
        // even when processed in parallel across multiple batches.
        let config = BatchConfig {
            batch_size: 3,
            channel_capacity: 4,
        };

        let wxyc_artists = vec![
            "Autechre", "Stereolab", "Cat Power", "Juana Molina",
            "Jessica Pratt", "Chuquimamani-Condori", "Sessa", "Anne Gillis",
            "Father John Misty", "Rafael Toral", "Buck Meek",
        ];
        let expected: Vec<String> = wxyc_artists
            .iter()
            .map(|s| s.to_uppercase())
            .collect();

        let (rx, handle) = start_scanner(
            move |tx| {
                let count = wxyc_artists.len();
                for artist in wxyc_artists {
                    tx.send_item(artist.to_string())?;
                }
                Ok(count)
            },
            config,
        );

        let mut output = StringOutput::new();
        let stats = run_pipeline(
            rx,
            handle,
            |s: &String| Some(s.to_uppercase()),
            &mut output,
        )
        .unwrap();

        assert_eq!(stats.scanned, 11);
        assert_eq!(stats.written, 11);
        assert_eq!(stats.filtered, 0);
        assert_eq!(output.items, expected, "items should be in scan order");
    }

    #[test]
    fn test_pipeline_order_preserved_with_filtering() {
        // Even with filtering, the remaining items should be in order
        let config = BatchConfig {
            batch_size: 2,
            channel_capacity: 4,
        };

        // Send indices 0..10; keep only even ones
        let (rx, handle) = start_scanner(
            |tx| {
                for i in 0..10u32 {
                    tx.send_item(i)?;
                }
                Ok(10)
            },
            config,
        );

        let mut output = MockOutput::new();
        let stats = run_pipeline(
            rx,
            handle,
            |&x| if x % 2 == 0 { Some(x * 100) } else { None },
            &mut output,
        )
        .unwrap();

        assert_eq!(stats.scanned, 10);
        assert_eq!(stats.written, 5);
        assert_eq!(stats.filtered, 5);
        assert_eq!(output.items, vec![0, 200, 400, 600, 800]);
    }

    #[test]
    fn test_pipeline_large_batch_order_preservation() {
        // 1000 items across many small batches to stress-test order preservation
        let n = 1000u32;
        let config = BatchConfig {
            batch_size: 7, // prime-sized batches for uneven splits
            channel_capacity: 4,
        };

        let (rx, handle) = start_scanner(
            move |tx| {
                for i in 0..n {
                    tx.send_item(i)?;
                }
                Ok(n as usize)
            },
            config,
        );

        let mut output = MockOutput::new();
        let stats = run_pipeline(rx, handle, |&x| Some(x), &mut output).unwrap();

        assert_eq!(stats.scanned, n as usize);
        assert_eq!(stats.written, n as usize);

        let expected: Vec<u32> = (0..n).collect();
        assert_eq!(output.items, expected, "1000 items should arrive in order");
    }

    #[test]
    fn test_pipeline_transform_type_conversion() {
        // Transform from one type to another (u32 -> String)
        let config = BatchConfig {
            batch_size: 3,
            channel_capacity: 4,
        };

        let artists = vec!["Autechre", "Stereolab", "Cat Power", "Sessa"];

        let (rx, handle) = start_scanner(
            move |tx| {
                let count = artists.len();
                for (i, artist) in artists.into_iter().enumerate() {
                    tx.send_item((i as u32, artist.to_string()))?;
                }
                Ok(count)
            },
            config,
        );

        let mut output = StringOutput::new();
        let stats = run_pipeline(
            rx,
            handle,
            |(id, name): &(u32, String)| Some(format!("{}:{}", id, name)),
            &mut output,
        )
        .unwrap();

        assert_eq!(stats.written, 4);
        assert_eq!(output.items, vec![
            "0:Autechre", "1:Stereolab", "2:Cat Power", "3:Sessa",
        ]);
    }

    #[test]
    fn test_byte_pipeline_order_preservation() {
        use crate::pipeline::scanner::{start_byte_scanner, ByteBatch};

        let config = BatchConfig {
            batch_size: 2,
            channel_capacity: 4,
        };

        let (rx, handle) = start_byte_scanner(
            |tx| {
                tx.send(ByteBatch::from_slices(&[
                    b"Autechre", b"Stereolab",
                ]))?;
                tx.send(ByteBatch::from_slices(&[
                    b"Cat Power", b"Juana Molina",
                ]))?;
                tx.send(ByteBatch::from_slices(&[
                    b"Sessa",
                ]))?;
                Ok(5)
            },
            config,
        );

        let mut output = StringOutput::new();
        let no_dedup: Option<DedupConfig<'_, fn(&[u8]) -> Option<u64>>> = None;
        let stats = run_byte_pipeline(
            rx,
            handle,
            |bytes| Some(String::from_utf8_lossy(bytes).to_string()),
            &mut output,
            no_dedup,
        )
        .unwrap();

        assert_eq!(stats.scanned, 5);
        assert_eq!(stats.written, 5);
        assert_eq!(
            output.items,
            vec!["Autechre", "Stereolab", "Cat Power", "Juana Molina", "Sessa"],
        );
    }

    #[test]
    fn test_byte_pipeline_filtering_and_dedup_combined() {
        use crate::pipeline::scanner::{start_byte_scanner, ByteBatch};
        use std::collections::HashSet;

        let config = BatchConfig {
            batch_size: 10,
            channel_capacity: 4,
        };

        // Format: "id:artist" -- filter out entries without ':', dedup by ID
        let (rx, handle) = start_byte_scanner(
            |tx| {
                tx.send(ByteBatch::from_slices(&[
                    b"1:Autechre",
                    b"2:Stereolab",
                    b"bad-data",        // no colon -> transform returns None
                    b"1:Autechre Dup",  // duplicate ID 1
                    b"3:Cat Power",
                ]))?;
                Ok(5)
            },
            config,
        );

        let mut output = StringOutput::new();
        let mut seen = HashSet::new();
        let stats = run_byte_pipeline(
            rx,
            handle,
            |bytes| {
                let s = String::from_utf8_lossy(bytes);
                let (_, name) = s.split_once(':')?;
                Some(name.to_string())
            },
            &mut output,
            Some(DedupConfig {
                seen_ids: &mut seen,
                id_fn: |bytes| {
                    let s = String::from_utf8_lossy(bytes);
                    let (id, _) = s.split_once(':')?;
                    id.parse().ok()
                },
            }),
        )
        .unwrap();

        assert_eq!(stats.scanned, 5);
        assert_eq!(stats.written, 3);     // Autechre, Stereolab, Cat Power
        assert_eq!(stats.filtered, 1);    // bad-data
        assert_eq!(stats.duplicates, 1);  // duplicate Autechre
        assert_eq!(output.items, vec!["Autechre", "Stereolab", "Cat Power"]);
    }
}
