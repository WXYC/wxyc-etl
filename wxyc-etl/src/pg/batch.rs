//! Buffered COPY with FK-ordered flush.
//!
//! [`BatchCopier`] manages multiple per-table byte buffers and flushes them
//! in a specified order (FK parent before children) when a batch threshold
//! is reached.

use std::collections::HashMap;
use std::io::Write;

use anyhow::Result;
use log::info;

/// Trait abstracting the `COPY ... FROM STDIN` operation.
///
/// Implement this for a real PostgreSQL connection or use the test mock.
pub trait CopyTarget {
    /// Execute a COPY operation, writing `data` into the table specified by `stmt`.
    ///
    /// `stmt` is a full COPY statement like `COPY release (id, title) FROM STDIN`.
    fn copy_in(&mut self, stmt: &str, data: &[u8]) -> Result<()>;
}

/// Implementation of [`CopyTarget`] for a real PostgreSQL connection.
impl CopyTarget for postgres::Client {
    fn copy_in(&mut self, stmt: &str, data: &[u8]) -> Result<()> {
        let mut writer = self.copy_in(stmt)?;
        writer.write_all(data)?;
        writer.finish()?;
        Ok(())
    }
}

/// A named byte buffer for one table's COPY data.
pub struct CopyBuffer {
    /// The full COPY statement (e.g., `COPY release (id, title) FROM STDIN`).
    pub stmt: String,
    /// Accumulated COPY TEXT data.
    pub data: Vec<u8>,
}

impl CopyBuffer {
    pub fn new(stmt: impl Into<String>) -> Self {
        Self {
            stmt: stmt.into(),
            data: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Manages multiple [`CopyBuffer`]s and flushes them in FK-safe order.
///
/// Tables are flushed in the order they were registered (parent tables
/// should be registered before child tables).
pub struct BatchCopier {
    /// Table names in flush order (FK parent first).
    table_order: Vec<String>,
    /// Per-table buffers, keyed by table name.
    buffers: HashMap<String, CopyBuffer>,
    /// Number of records buffered since last flush.
    batch_count: usize,
    /// Flush threshold.
    batch_size: usize,
    /// Total records flushed.
    total_written: usize,
}

impl BatchCopier {
    /// Create a new `BatchCopier` with the given table names (in FK order)
    /// and their COPY statements.
    ///
    /// `tables` is a slice of `(table_name, copy_stmt)` pairs. Tables are
    /// flushed in the order provided.
    pub fn new(tables: &[(&str, &str)], batch_size: usize) -> Self {
        let table_order: Vec<String> = tables.iter().map(|(name, _)| name.to_string()).collect();
        let buffers: HashMap<String, CopyBuffer> = tables
            .iter()
            .map(|(name, stmt)| (name.to_string(), CopyBuffer::new(*stmt)))
            .collect();
        Self {
            table_order,
            buffers,
            batch_count: 0,
            batch_size,
            total_written: 0,
        }
    }

    /// Get a mutable reference to a table's data buffer for writing COPY rows.
    ///
    /// # Panics
    ///
    /// Panics if `table` was not registered in [`BatchCopier::new()`].
    pub fn buffer(&mut self, table: &str) -> &mut Vec<u8> {
        &mut self
            .buffers
            .get_mut(table)
            .unwrap_or_else(|| panic!("unknown table: {}", table))
            .data
    }

    /// Increment the batch counter and flush if the threshold is reached.
    pub fn count_and_maybe_flush(&mut self, target: &mut impl CopyTarget) -> Result<()> {
        self.batch_count += 1;
        if self.batch_count >= self.batch_size {
            self.flush(target)?;
        }
        Ok(())
    }

    /// Flush all buffers to the target in FK order. No-op if nothing is buffered.
    pub fn flush(&mut self, target: &mut impl CopyTarget) -> Result<()> {
        if self.batch_count == 0 {
            return Ok(());
        }

        for table_name in &self.table_order {
            let buf = self.buffers.get_mut(table_name).unwrap();
            if !buf.is_empty() {
                target.copy_in(&buf.stmt, &buf.data)?;
                buf.data.clear();
            }
        }

        self.total_written += self.batch_count;
        info!(
            "Flushed {} records ({} total)",
            self.batch_count, self.total_written
        );
        self.batch_count = 0;

        Ok(())
    }

    /// Number of records flushed so far (not counting current unflushed batch).
    pub fn total_written(&self) -> usize {
        self.total_written
    }

    /// Number of records in the current unflushed batch.
    pub fn batch_count(&self) -> usize {
        self.batch_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock [`CopyTarget`] that records operations in order.
    struct MockCopyTarget {
        operations: Vec<(String, Vec<u8>)>,
    }

    impl MockCopyTarget {
        fn new() -> Self {
            Self {
                operations: Vec::new(),
            }
        }
    }

    impl CopyTarget for MockCopyTarget {
        fn copy_in(&mut self, stmt: &str, data: &[u8]) -> Result<()> {
            self.operations.push((stmt.to_string(), data.to_vec()));
            Ok(())
        }
    }

    #[test]
    fn test_flush_order() {
        let mut copier = BatchCopier::new(
            &[
                ("release", "COPY release FROM STDIN"),
                ("release_artist", "COPY release_artist FROM STDIN"),
                ("release_label", "COPY release_label FROM STDIN"),
            ],
            100,
        );

        // Write to child tables first, then parent
        copier
            .buffer("release_artist")
            .extend_from_slice(b"child data\n");
        copier
            .buffer("release")
            .extend_from_slice(b"parent data\n");
        copier.batch_count = 1; // simulate one record

        let mut target = MockCopyTarget::new();
        copier.flush(&mut target).unwrap();

        // Parent should be flushed first regardless of write order
        assert_eq!(target.operations.len(), 2);
        assert_eq!(target.operations[0].0, "COPY release FROM STDIN");
        assert_eq!(target.operations[0].1, b"parent data\n");
        assert_eq!(target.operations[1].0, "COPY release_artist FROM STDIN");
        assert_eq!(target.operations[1].1, b"child data\n");
    }

    #[test]
    fn test_flush_skips_empty_buffers() {
        let mut copier = BatchCopier::new(
            &[
                ("release", "COPY release FROM STDIN"),
                ("release_artist", "COPY release_artist FROM STDIN"),
            ],
            100,
        );

        // Only write to release, not release_artist
        copier
            .buffer("release")
            .extend_from_slice(b"data\n");
        copier.batch_count = 1;

        let mut target = MockCopyTarget::new();
        copier.flush(&mut target).unwrap();

        assert_eq!(target.operations.len(), 1);
        assert_eq!(target.operations[0].0, "COPY release FROM STDIN");
    }

    #[test]
    fn test_flush_noop_when_empty() {
        let mut copier = BatchCopier::new(
            &[("release", "COPY release FROM STDIN")],
            100,
        );

        let mut target = MockCopyTarget::new();
        copier.flush(&mut target).unwrap();

        assert!(target.operations.is_empty());
    }

    #[test]
    fn test_count_and_maybe_flush() {
        let mut copier = BatchCopier::new(
            &[("release", "COPY release FROM STDIN")],
            2,
        );

        let mut target = MockCopyTarget::new();

        copier.buffer("release").extend_from_slice(b"row1\n");
        copier.count_and_maybe_flush(&mut target).unwrap();
        assert_eq!(copier.total_written(), 0); // not yet at threshold

        copier.buffer("release").extend_from_slice(b"row2\n");
        copier.count_and_maybe_flush(&mut target).unwrap();
        assert_eq!(copier.total_written(), 2); // flushed at threshold

        assert_eq!(target.operations.len(), 1);
        assert_eq!(target.operations[0].1, b"row1\nrow2\n");
    }

    #[test]
    fn test_total_written_accumulates() {
        let mut copier = BatchCopier::new(
            &[("release", "COPY release FROM STDIN")],
            1,
        );

        let mut target = MockCopyTarget::new();

        for i in 0..5 {
            copier
                .buffer("release")
                .extend_from_slice(format!("row{}\n", i).as_bytes());
            copier.count_and_maybe_flush(&mut target).unwrap();
        }

        assert_eq!(copier.total_written(), 5);
        assert_eq!(target.operations.len(), 5);
    }

    #[test]
    #[should_panic(expected = "unknown table")]
    fn test_buffer_panics_on_unknown_table() {
        let mut copier = BatchCopier::new(
            &[("release", "COPY release FROM STDIN")],
            100,
        );
        copier.buffer("nonexistent");
    }

    #[test]
    fn test_buffers_cleared_after_flush() {
        let mut copier = BatchCopier::new(
            &[("release", "COPY release FROM STDIN")],
            1,
        );

        let mut target = MockCopyTarget::new();

        copier.buffer("release").extend_from_slice(b"row1\n");
        copier.count_and_maybe_flush(&mut target).unwrap();

        // Buffer should be cleared after flush
        assert!(copier.buffer("release").is_empty());
        assert_eq!(copier.batch_count(), 0);
    }
}
