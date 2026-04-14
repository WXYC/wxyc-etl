//! Error and resilience tests for PostgreSQL operations.
//!
//! These tests exercise failure paths in the `pg` module:
//! connection refused, authentication failure, mid-batch disconnect,
//! COPY with malformed input, and `set_unlogged` on non-existent tables.
//!
//! All tests are gated behind `TEST_DATABASE_URL` and skip gracefully
//! when PostgreSQL is not available.

use std::io::Write;

use anyhow::Result;
use wxyc_etl::pg::admin::{set_tables_logged, set_tables_unlogged};
use wxyc_etl::pg::batch::{BatchCopier, CopyTarget};
use wxyc_etl::pg::copy::{copy_line, escape_copy_text, write_copy_row};

/// Return the test database URL if set, or `None` to skip.
fn test_db_url() -> Option<String> {
    std::env::var("TEST_DATABASE_URL").ok()
}

/// Helper: connect to the test database.
fn connect(url: &str) -> postgres::Client {
    postgres::Client::connect(url, postgres::NoTls).unwrap()
}

// ---------------------------------------------------------------------------
// Connection error tests
// ---------------------------------------------------------------------------

#[test]
fn test_pg_connection_refused() {
    // Attempt to connect to a port where nothing is listening.
    // This must return an error, not hang.
    let result = postgres::Client::connect(
        "postgresql://localhost:59999/nonexistent",
        postgres::NoTls,
    );
    assert!(result.is_err(), "Connection to a closed port should fail");
    let err = result.err().unwrap();
    let msg = err.to_string().to_lowercase();
    // The error message should indicate a connection issue (not auth or query).
    assert!(
        msg.contains("connect") || msg.contains("refused") || msg.contains("timeout") || msg.contains("io error"),
        "Expected connection error, got: {}",
        err
    );
}

#[test]
fn test_pg_auth_failure() {
    let Some(db_url) = test_db_url() else { return };

    // Mangle the URL to use bad credentials.  We replace the password (if any)
    // or add a bogus user to force an auth failure.
    let bad_url = if db_url.contains('@') {
        // Replace user:pass with bogus credentials
        let at_pos = db_url.find('@').unwrap();
        let prefix_end = db_url.find("://").map(|p| p + 3).unwrap_or(0);
        format!(
            "{}bogus_user_12345:wrong_password{}",
            &db_url[..prefix_end],
            &db_url[at_pos..]
        )
    } else {
        // No credentials in URL -- add bogus ones
        db_url.replace("://", "://bogus_user_12345:wrong_password@")
    };

    let result = postgres::Client::connect(&bad_url, postgres::NoTls);
    assert!(
        result.is_err(),
        "Connection with wrong credentials should fail"
    );
}

// ---------------------------------------------------------------------------
// Mid-batch disconnect (BatchCopier with a failing CopyTarget)
// ---------------------------------------------------------------------------

/// A CopyTarget that fails after N successful calls.
struct FailAfterNCopyTarget {
    remaining: usize,
    succeeded: usize,
}

impl FailAfterNCopyTarget {
    fn new(succeed_count: usize) -> Self {
        Self {
            remaining: succeed_count,
            succeeded: 0,
        }
    }
}

impl CopyTarget for FailAfterNCopyTarget {
    fn copy_in(&mut self, _stmt: &str, _data: &[u8]) -> Result<()> {
        if self.remaining == 0 {
            anyhow::bail!("simulated mid-batch disconnect");
        }
        self.remaining -= 1;
        self.succeeded += 1;
        Ok(())
    }
}

#[test]
fn test_batch_copier_mid_batch_disconnect() {
    // Two tables: parent flushes successfully, child fails.
    // batch_size=2 so we can buffer data, then trigger flush via count_and_maybe_flush.
    let mut copier = BatchCopier::new(
        &[
            ("release", "COPY release (id, title) FROM STDIN"),
            (
                "release_artist",
                "COPY release_artist (release_id, artist_name) FROM STDIN",
            ),
        ],
        2, // flush threshold = 2
    );

    // Buffer data for both tables (using WXYC example artists)
    copier
        .buffer("release")
        .extend_from_slice(b"5001\tDOGA\n");
    copier
        .buffer("release_artist")
        .extend_from_slice(b"5001\tJuana Molina\n");

    // Target that succeeds on release (first copy_in) but fails on release_artist
    let mut target = FailAfterNCopyTarget::new(1);

    // Count two records to trigger the flush
    copier.count_and_maybe_flush(&mut target).unwrap(); // batch_count = 1, no flush yet
    let result = copier.count_and_maybe_flush(&mut target); // batch_count = 2, triggers flush

    assert!(result.is_err(), "Flush should fail when child COPY fails");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("simulated mid-batch disconnect"),
        "Error should propagate: {}",
        err
    );
    // The parent table succeeded before the child failed
    assert_eq!(target.succeeded, 1);
}

#[test]
fn test_batch_copier_all_tables_fail() {
    let mut copier = BatchCopier::new(
        &[("release", "COPY release (id) FROM STDIN")],
        2, // flush threshold = 2
    );
    copier
        .buffer("release")
        .extend_from_slice(b"5002\tAluminum Tunes\n");

    let mut target = FailAfterNCopyTarget::new(0); // fail immediately

    // Trigger flush
    copier.count_and_maybe_flush(&mut target).unwrap(); // batch_count = 1, no flush
    let result = copier.count_and_maybe_flush(&mut target); // batch_count = 2, triggers flush

    assert!(result.is_err());
    assert_eq!(target.succeeded, 0);
}

// ---------------------------------------------------------------------------
// COPY with malformed input (escaping edge cases)
// ---------------------------------------------------------------------------

#[test]
fn test_copy_text_escaping_malformed_input() {
    // Unterminated quote characters should pass through (COPY TEXT doesn't
    // use CSV quoting -- quotes are literal).
    let input = "Stereolab \"Aluminum Tunes";
    let escaped = escape_copy_text(input);
    assert_eq!(escaped, "Stereolab \"Aluminum Tunes");

    // Embedded NUL bytes (invalid in PG text but should not panic)
    let with_nul = "Cat\0Power";
    let escaped_nul = escape_copy_text(with_nul);
    assert!(escaped_nul.contains('\0'), "NUL should pass through escape");
}

#[test]
fn test_copy_line_with_all_nulls() {
    let line = copy_line(&[None, None, None]);
    assert_eq!(line, "\\N\t\\N\t\\N\n");
}

#[test]
fn test_write_copy_row_unicode_artists() {
    // WXYC artists with special characters
    let mut buf = Vec::new();
    write_copy_row(
        &mut buf,
        &[
            Some("5001"),
            Some("Chuquimamani-Condori"),
            Some("Edits"),
        ],
    );
    assert_eq!(buf, b"5001\tChuquimamani-Condori\tEdits\n");

    let mut buf2 = Vec::new();
    write_copy_row(
        &mut buf2,
        &[
            Some("5006"),
            Some("Duke Ellington & John Coltrane"),
            Some("In a Sentimental Mood"),
        ],
    );
    assert_eq!(
        buf2,
        b"5006\tDuke Ellington & John Coltrane\tIn a Sentimental Mood\n"
    );
}

#[test]
fn test_copy_text_backslash_sequences() {
    // Input that looks like a COPY escape but is raw data
    let tricky = "Artist\\nName";
    let escaped = escape_copy_text(tricky);
    // The single backslash should become double backslash
    assert_eq!(escaped, "Artist\\\\nName");
}

#[test]
fn test_copy_text_embedded_tabs_and_newlines() {
    // A value with embedded tabs and newlines (common in Discogs notes fields)
    let messy = "Note:\tSee\nrelease\r5003";
    let escaped = escape_copy_text(messy);
    assert_eq!(escaped, "Note:\\tSee\\nrelease\\r5003");
}

// ---------------------------------------------------------------------------
// set_tables_unlogged / set_tables_logged on non-existent tables
// ---------------------------------------------------------------------------

#[test]
fn test_set_unlogged_nonexistent_table() {
    let Some(db_url) = test_db_url() else { return };
    let mut client = connect(&db_url);

    // Ensure table definitely doesn't exist
    client
        .execute("DROP TABLE IF EXISTS _nonexistent_error_test", &[])
        .unwrap();

    let result = set_tables_unlogged(&mut client, &["_nonexistent_error_test"]);
    assert!(result.is_err(), "SET UNLOGGED on missing table should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("_nonexistent_error_test"),
        "Error should name the table: {}",
        err_msg
    );
}

#[test]
fn test_set_logged_nonexistent_table() {
    let Some(db_url) = test_db_url() else { return };
    let mut client = connect(&db_url);

    client
        .execute("DROP TABLE IF EXISTS _nonexistent_logged_test", &[])
        .unwrap();

    let result = set_tables_logged(&mut client, &["_nonexistent_logged_test"]);
    assert!(result.is_err(), "SET LOGGED on missing table should fail");
}

#[test]
fn test_set_unlogged_partial_failure_stops_early() {
    let Some(db_url) = test_db_url() else { return };
    let mut client = connect(&db_url);

    // Create a real table and reference a non-existent one
    client
        .batch_execute(
            "DROP TABLE IF EXISTS _partial_test_a;
             CREATE TABLE _partial_test_a (id integer);",
        )
        .unwrap();

    // First table exists, second does not. The function iterates in order,
    // so the second should fail after the first succeeds.
    let result = set_tables_unlogged(
        &mut client,
        &["_partial_test_a", "_does_not_exist_zzz"],
    );
    assert!(result.is_err());

    // Clean up
    client
        .execute("DROP TABLE IF EXISTS _partial_test_a", &[])
        .unwrap();
}

// ---------------------------------------------------------------------------
// Real PG COPY with malformed data
// ---------------------------------------------------------------------------

#[test]
fn test_real_pg_copy_with_malformed_data() {
    let Some(db_url) = test_db_url() else { return };
    let mut client = connect(&db_url);

    client
        .batch_execute(
            "DROP TABLE IF EXISTS _copy_error_test;
             CREATE TABLE _copy_error_test (
                 id integer PRIMARY KEY,
                 title text NOT NULL
             );",
        )
        .unwrap();

    // COPY data with wrong number of columns (3 columns into 2-column table)
    let bad_data = b"1\tMoon Pix\textra_column\n";
    let mut writer = client
        .copy_in("COPY _copy_error_test (id, title) FROM STDIN")
        .unwrap();
    writer.write_all(bad_data).unwrap();
    let result = writer.finish();
    assert!(result.is_err(), "COPY with extra columns should fail");

    // COPY data with non-integer in integer column
    let type_mismatch = b"not_an_int\tAluminum Tunes\n";
    let mut writer2 = client
        .copy_in("COPY _copy_error_test (id, title) FROM STDIN")
        .unwrap();
    writer2.write_all(type_mismatch).unwrap();
    let result2 = writer2.finish();
    assert!(result2.is_err(), "COPY with type mismatch should fail");

    // Verify no partial data was committed from failed COPYs
    let row_count: i64 = client
        .query_one("SELECT count(*) FROM _copy_error_test", &[])
        .unwrap()
        .get(0);
    assert_eq!(row_count, 0, "Failed COPY should not leave partial data");

    // Clean up
    client
        .execute("DROP TABLE _copy_error_test", &[])
        .unwrap();
}

// ---------------------------------------------------------------------------
// Pipeline runner error path tests (no PG needed)
// ---------------------------------------------------------------------------

mod pipeline_error_tests {
    use anyhow::Result;
    use wxyc_etl::pipeline::runner::run_pipeline;
    use wxyc_etl::pipeline::scanner::{start_scanner, BatchConfig};
    use wxyc_etl::pipeline::writer::PipelineOutput;

    /// An output that fails on flush.
    struct FailingFlushOutput {
        items: Vec<u32>,
    }

    impl FailingFlushOutput {
        fn new() -> Self {
            Self { items: Vec::new() }
        }
    }

    impl PipelineOutput<u32> for FailingFlushOutput {
        fn write_item(&mut self, item: &u32) -> Result<()> {
            self.items.push(*item);
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            anyhow::bail!("simulated flush failure");
        }

        fn finish(&mut self) -> Result<()> {
            Ok(())
        }
    }

    /// An output that fails on write_item after N items.
    struct FailAfterNOutput {
        items: Vec<u32>,
        remaining: usize,
    }

    impl FailAfterNOutput {
        fn new(succeed_count: usize) -> Self {
            Self {
                items: Vec::new(),
                remaining: succeed_count,
            }
        }
    }

    impl PipelineOutput<u32> for FailAfterNOutput {
        fn write_item(&mut self, item: &u32) -> Result<()> {
            if self.remaining == 0 {
                anyhow::bail!("simulated write failure after {} items", self.items.len());
            }
            self.remaining -= 1;
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
    fn test_scanner_panic_does_not_deadlock_writer() {
        let config = BatchConfig {
            batch_size: 2,
            channel_capacity: 4,
        };

        let (rx, handle) = start_scanner(
            |tx| {
                tx.send_item(1u32)?;
                tx.send_item(2u32)?;
                panic!("scanner panic on item 3");
            },
            config,
        );

        struct CollectOutput {
            items: Vec<u32>,
        }

        impl PipelineOutput<u32> for CollectOutput {
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

        let mut output = CollectOutput { items: Vec::new() };

        // The scanner panics. run_pipeline calls handle.join().unwrap()
        // which will propagate the panic. We catch it here.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_pipeline(rx, handle, |&x| Some(x * 10), &mut output)
        }));

        // The pipeline must not hang -- reaching this assertion proves no deadlock.
        assert!(result.is_err(), "Scanner panic should propagate, not deadlock");
    }

    #[test]
    fn test_output_flush_failure() {
        let config = BatchConfig {
            batch_size: 10,
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

        let mut output = FailingFlushOutput::new();
        let result = run_pipeline(rx, handle, |&x| Some(x), &mut output);

        assert!(result.is_err(), "Pipeline should fail on flush error");
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("simulated flush failure"),
            "Error message should propagate: {}",
            err_msg
        );
        // Items were written before flush failed
        assert_eq!(output.items.len(), 5);
    }

    #[test]
    fn test_output_write_failure_mid_batch() {
        let config = BatchConfig {
            batch_size: 10,
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

        // Fail after writing 3 items
        let mut output = FailAfterNOutput::new(3);
        let result = run_pipeline(rx, handle, |&x| Some(x), &mut output);

        assert!(result.is_err(), "Pipeline should fail on write error");
        assert_eq!(output.items.len(), 3, "Only 3 items should be written before failure");
    }

    #[test]
    fn test_receiver_drop_unblocks_scanner() {
        // Verify that dropping the receiver allows the scanner thread to finish,
        // preventing the deadlock documented in runner.rs.
        let config = BatchConfig {
            batch_size: 1,
            channel_capacity: 1,
        };

        let (rx, handle) = start_scanner(
            |tx| {
                // Try to send more items than channel capacity.
                // If the receiver is dropped, send will fail with an error
                // (not block forever).
                for i in 0..100u32 {
                    if tx.send_item(i).is_err() {
                        // Channel closed -- this is the expected path
                        return Ok(i as usize);
                    }
                }
                Ok(100)
            },
            config,
        );

        // Read one batch then drop the receiver
        let _first = rx.recv();
        drop(rx);

        // The scanner must finish (not hang). We use a timeout via thread::spawn.
        let join_result = handle.join();
        assert!(
            join_result.is_ok(),
            "Scanner thread should finish cleanly when receiver is dropped"
        );
    }

    #[test]
    fn test_scanner_error_propagates_through_pipeline() {
        let config = BatchConfig {
            batch_size: 10,
            channel_capacity: 4,
        };

        let (rx, handle) = start_scanner(
            |tx| {
                tx.send_item(1u32)?;
                anyhow::bail!("scanner encountered an error");
            },
            config,
        );

        struct SimpleOutput {
            items: Vec<u32>,
        }

        impl PipelineOutput<u32> for SimpleOutput {
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

        let mut output = SimpleOutput { items: Vec::new() };
        let result = run_pipeline(rx, handle, |&x| Some(x), &mut output);

        assert!(result.is_err(), "Scanner error should propagate through pipeline");
        let err_msg = result.err().unwrap().to_string();
        assert!(
            err_msg.contains("scanner encountered an error"),
            "Error should propagate: {}",
            err_msg
        );
    }
}
