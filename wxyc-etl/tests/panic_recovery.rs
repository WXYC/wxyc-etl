//! Panic and error recovery tests for the pipeline framework.
//!
//! These tests verify that the pipeline handles failure gracefully:
//! - Scanner thread panics propagate as `Err`, not panics
//! - Writer flush failures return errors cleanly
//! - Receiver drops don't cause scanner deadlocks
//! - Processor errors are collected properly
//!
//! Uses WXYC example artists for test data.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};

use wxyc_etl::pipeline::runner::run_pipeline;
use wxyc_etl::pipeline::scanner::{start_scanner, BatchConfig};
use wxyc_etl::pipeline::writer::PipelineOutput;

/// WXYC example artists used as test data throughout these tests.
const WXYC_ARTISTS: &[&str] = &[
    "Autechre",
    "Prince Jammy",
    "Juana Molina",
    "Stereolab",
    "Cat Power",
    "Jessica Pratt",
    "Chuquimamani-Condori",
    "Duke Ellington & John Coltrane",
    "Sessa",
    "Anne Gillis",
    "Father John Misty",
    "Rafael Toral",
    "Buck Meek",
    "Nourished by Time",
    "For Tracy Hyde",
    "Rochelle Jordan",
    "Large Professor",
];

// ---------------------------------------------------------------------------
// Shared mock outputs
// ---------------------------------------------------------------------------

/// A simple output that collects items into a Vec.
struct CollectingOutput {
    items: Vec<String>,
}

impl CollectingOutput {
    fn new() -> Self {
        Self { items: Vec::new() }
    }
}

impl PipelineOutput<String> for CollectingOutput {
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

/// An output whose `flush()` returns an error after `fail_after` items have
/// been written.
struct FailingFlushOutput {
    items: Vec<String>,
    fail_after: usize,
}

impl FailingFlushOutput {
    fn new(fail_after: usize) -> Self {
        Self {
            items: Vec::new(),
            fail_after,
        }
    }
}

impl PipelineOutput<String> for FailingFlushOutput {
    fn write_item(&mut self, item: &String) -> Result<()> {
        self.items.push(item.clone());
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if self.items.len() >= self.fail_after {
            bail!(
                "flush failed after {} items (simulated disk full)",
                self.items.len()
            );
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        self.flush()
    }
}

/// An output whose `write_item` returns an error after `fail_after` writes.
struct FailingWriteOutput {
    written: usize,
    fail_after: usize,
    items: Vec<String>,
}

impl FailingWriteOutput {
    fn new(fail_after: usize) -> Self {
        Self {
            written: 0,
            fail_after,
            items: Vec::new(),
        }
    }
}

impl PipelineOutput<String> for FailingWriteOutput {
    fn write_item(&mut self, item: &String) -> Result<()> {
        if self.written >= self.fail_after {
            bail!(
                "write_item failed at item {} (simulated I/O error)",
                self.written
            );
        }
        self.items.push(item.clone());
        self.written += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test 1: Scanner thread panic
// ---------------------------------------------------------------------------

/// A scanner that panics after producing some items should cause `run_pipeline`
/// to return an `Err`, not propagate the panic to the caller.
///
/// Currently, `runner.rs` calls `handle.join().unwrap()` which re-panics if
/// the scanner thread panicked. This test documents the desired behavior.
#[test]
fn scanner_panic_returns_error_not_panic() {
    let config = BatchConfig {
        batch_size: 3,
        channel_capacity: 2,
    };

    let (rx, handle) = start_scanner(
        |tx| {
            // Produce a few artists, then panic
            for artist in &WXYC_ARTISTS[..5] {
                tx.send_item(artist.to_string())?;
            }
            panic!("scanner hit corrupt data in Autechre discography");
        },
        config,
    );

    let mut output = CollectingOutput::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_pipeline(rx, handle, |s| Some(s.clone()), &mut output)
    }));

    // The pipeline should return Err, not panic. If this assertion fails,
    // the .unwrap() on handle.join() is propagating the scanner panic.
    assert!(
        result.is_ok(),
        "run_pipeline panicked instead of returning Err — \
         handle.join().unwrap() in runner.rs propagates scanner panics"
    );

    let pipeline_result = result.unwrap();
    assert!(
        pipeline_result.is_err(),
        "expected Err from panicked scanner, got Ok"
    );
}

/// Verify that partial output written before a scanner panic is not corrupted.
/// Even though the pipeline fails, items that were already written should be
/// intact (important for crash recovery / resumable pipelines).
#[test]
fn scanner_panic_partial_output_is_intact() {
    let config = BatchConfig {
        batch_size: 3,
        channel_capacity: 2,
    };

    let (rx, handle) = start_scanner(
        |tx| {
            for artist in &WXYC_ARTISTS[..6] {
                tx.send_item(artist.to_string())?;
            }
            panic!("scanner hit corrupt XML block");
        },
        config,
    );

    let mut output = CollectingOutput::new();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_pipeline(rx, handle, |s| Some(s.clone()), &mut output)
    }));

    // Whatever was written before the panic should be valid artist names.
    // The partial output may contain anywhere from 0 to 6 items depending
    // on timing, but each item must be one of our WXYC artists.
    for item in &output.items {
        assert!(
            WXYC_ARTISTS.contains(&item.as_str()),
            "corrupted partial output: {:?} is not a WXYC artist",
            item,
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2: Writer flush failure
// ---------------------------------------------------------------------------

/// When `flush()` returns an error, the pipeline should propagate it as `Err`,
/// not panic.
#[test]
fn writer_flush_failure_returns_error() {
    let config = BatchConfig {
        batch_size: 4,
        channel_capacity: 2,
    };

    let (rx, handle) = start_scanner(
        |tx| {
            for artist in WXYC_ARTISTS {
                tx.send_item(artist.to_string())?;
            }
            Ok(WXYC_ARTISTS.len())
        },
        config,
    );

    // flush() will fail after 5 items are written
    let mut output = FailingFlushOutput::new(5);
    let result = run_pipeline(rx, handle, |s| Some(s.clone()), &mut output);

    assert!(result.is_err(), "expected Err from failing flush, got Ok");
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("expected Err but got Ok"),
    };
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("flush failed"),
        "error message should mention flush failure, got: {}",
        err_msg,
    );
}

/// When `write_item()` returns an error mid-batch, the pipeline should stop
/// and return the error. Items written before the failure should be intact.
#[test]
fn writer_write_item_failure_returns_error() {
    let config = BatchConfig {
        batch_size: 4,
        channel_capacity: 2,
    };

    let (rx, handle) = start_scanner(
        |tx| {
            for artist in WXYC_ARTISTS {
                tx.send_item(artist.to_string())?;
            }
            Ok(WXYC_ARTISTS.len())
        },
        config,
    );

    // write_item() will fail after 3 successful writes
    let mut output = FailingWriteOutput::new(3);
    let result = run_pipeline(rx, handle, |s| Some(s.clone()), &mut output);

    assert!(
        result.is_err(),
        "expected Err from failing write_item, got Ok"
    );

    // The 3 items written before failure should be valid WXYC artists
    assert_eq!(output.items.len(), 3);
    for item in &output.items {
        assert!(
            WXYC_ARTISTS.contains(&item.as_str()),
            "corrupted output: {:?}",
            item,
        );
    }
}

// ---------------------------------------------------------------------------
// Test 3: Receiver drop (scanner must not deadlock)
// ---------------------------------------------------------------------------

/// If the consumer drops the receiver (e.g., due to an early error), the
/// scanner thread must not deadlock on `send()`. It should either detect the
/// closed channel and return an error, or complete normally.
///
/// We use a timeout to detect deadlock: if the scanner hasn't finished within
/// 5 seconds, something is stuck.
#[test]
fn receiver_drop_does_not_deadlock_scanner() {
    let config = BatchConfig {
        batch_size: 2,
        channel_capacity: 1, // tiny capacity to maximize blocking pressure
    };

    let items_sent = Arc::new(AtomicUsize::new(0));
    let items_sent_clone = items_sent.clone();

    let (rx, handle) = start_scanner(
        move |tx| {
            for artist in WXYC_ARTISTS {
                match tx.send_item(artist.to_string()) {
                    Ok(()) => {
                        items_sent_clone.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(_) => {
                        // Channel closed — this is the expected path
                        break;
                    }
                }
            }
            Ok(items_sent_clone.load(Ordering::SeqCst))
        },
        config,
    );

    // Consume one batch, then drop the receiver to simulate early termination
    let _first_batch = rx.recv();
    drop(rx);

    // The scanner thread must not deadlock. Use a polling approach with a
    // total timeout of 5 seconds.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if handle.is_finished() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "scanner thread deadlocked after receiver was dropped \
             ({} items sent before drop)",
            items_sent.load(Ordering::SeqCst),
        );
        std::thread::sleep(Duration::from_millis(50));
    }

    // The join should succeed (the scanner detected the closed channel)
    let join_result = handle.join();
    assert!(
        join_result.is_ok(),
        "scanner thread panicked after receiver drop"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Process batch error (transform returning Err via write_item)
// ---------------------------------------------------------------------------

/// The pipeline's transform function returns `Option<R>`, so it can't directly
/// return an error. However, a transform that produces a "poison" value can
/// trigger a write_item error. This test verifies that a write_item failure
/// caused by a bad transform result is propagated correctly.
#[test]
fn process_batch_error_via_poisoned_transform() {
    let config = BatchConfig {
        batch_size: 4,
        channel_capacity: 2,
    };

    let (rx, handle) = start_scanner(
        |tx| {
            for artist in WXYC_ARTISTS {
                tx.send_item(artist.to_string())?;
            }
            Ok(WXYC_ARTISTS.len())
        },
        config,
    );

    // Output that rejects any item containing "Chuquimamani" (a specific
    // WXYC artist) to simulate a write error on certain data.
    struct RejectingOutput {
        items: Vec<String>,
    }

    impl PipelineOutput<String> for RejectingOutput {
        fn write_item(&mut self, item: &String) -> Result<()> {
            if item.contains("Chuquimamani") {
                bail!("write rejected: invalid character data in '{}'", item);
            }
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

    let mut output = RejectingOutput { items: Vec::new() };
    let result = run_pipeline(rx, handle, |s| Some(s.clone()), &mut output);

    assert!(
        result.is_err(),
        "expected Err when write_item rejects an item"
    );

    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("expected Err but got Ok"),
    };
    let err_msg = format!("{}", err);
    assert!(
        err_msg.contains("Chuquimamani"),
        "error should identify the rejected artist, got: {}",
        err_msg,
    );

    // Items written before the rejection should be valid
    for item in &output.items {
        assert!(
            WXYC_ARTISTS.contains(&item.as_str()),
            "corrupted output: {:?}",
            item,
        );
    }
}

/// The transform can filter items by returning None. Verify that filtered
/// items don't cause errors and are counted correctly.
#[test]
fn transform_filtering_does_not_produce_errors() {
    let config = BatchConfig {
        batch_size: 5,
        channel_capacity: 2,
    };

    let (rx, handle) = start_scanner(
        |tx| {
            for artist in WXYC_ARTISTS {
                tx.send_item(artist.to_string())?;
            }
            Ok(WXYC_ARTISTS.len())
        },
        config,
    );

    let mut output = CollectingOutput::new();

    // Only keep artists whose names are shorter than 15 characters
    let result = run_pipeline(
        rx,
        handle,
        |s| {
            if s.len() < 15 {
                Some(s.clone())
            } else {
                None
            }
        },
        &mut output,
    );

    let stats = result.expect("filtering should not cause pipeline errors");
    assert_eq!(stats.scanned, WXYC_ARTISTS.len());
    assert_eq!(stats.written + stats.filtered, WXYC_ARTISTS.len());
    assert!(stats.filtered > 0, "some artists should have been filtered");

    // Verify every written item is short enough and is a real WXYC artist
    for item in &output.items {
        assert!(item.len() < 15, "filtered item leaked: {:?}", item);
        assert!(
            WXYC_ARTISTS.contains(&item.as_str()),
            "unknown artist: {:?}",
            item,
        );
    }
}
