//! MySQL dump file parser.
//!
//! Parses INSERT statements from MySQL dump files, extracting rows as
//! vectors of [`SqlValue`]. Uses memory-mapped file access for efficient
//! processing of large dump files.
//!
//! Ported from `semantic-index/rust/sql-parser/`, with PyO3 bindings
//! removed. The core functions operate on `&[u8]` and `&Path` and return
//! Rust types. PyO3 bindings are provided separately by `wxyc-etl-python`.

use std::path::Path;

use anyhow::{Context, Result};
use memchr::memchr;
use memmap2::Mmap;

/// A parsed SQL value from an INSERT statement.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Int(i64),
    Float(f64),
    Str(String),
}

/// Parse a single SQL value starting at position `pos`, advancing `pos` past the value.
pub fn parse_single_value(data: &[u8], pos: &mut usize) -> SqlValue {
    let len = data.len();
    if *pos >= len {
        return SqlValue::Null;
    }

    // NULL
    if *pos + 3 < len && &data[*pos..*pos + 4] == b"NULL" {
        *pos += 4;
        SqlValue::Null
    } else {
        todo!()
    }
}

/// Parse the VALUES portion of an INSERT statement into rows of [`SqlValue`].
///
/// `data` should start at the first `(` of the VALUES clause.
pub fn parse_sql_values(data: &[u8]) -> Vec<Vec<SqlValue>> {
    todo!()
}

/// Find the byte offset of the first `(` after the VALUES keyword in an INSERT line.
///
/// Returns `None` if the line does not contain a VALUES clause.
pub fn find_values_start(line: &[u8]) -> Option<usize> {
    todo!()
}

/// Load all rows for a given table from a MySQL dump file.
///
/// Memory-maps the file for zero-copy access. Scans line-by-line for
/// `INSERT INTO \`table_name\`` statements, parses the VALUES, and returns
/// all rows.
pub fn load_table_rows(path: &Path, table_name: &str) -> Result<Vec<Vec<SqlValue>>> {
    todo!()
}

/// Streaming variant of [`load_table_rows`] that invokes `callback` per row
/// instead of collecting all rows into memory.
///
/// Returns the number of rows processed.
pub fn iter_table_rows<F>(path: &Path, table_name: &str, callback: F) -> Result<usize>
where
    F: FnMut(Vec<SqlValue>),
{
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Cycle 1: parse_single_value — NULL ===

    #[test]
    fn test_parse_null() {
        let data = b"NULL,";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        assert!(matches!(val, SqlValue::Null));
        assert_eq!(pos, 4);
    }
}
