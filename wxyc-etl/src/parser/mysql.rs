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

    // String value
    if data[*pos] == b'\'' {
        *pos += 1;
        let mut s = Vec::new();
        while *pos < len {
            let ch = data[*pos];
            if ch == b'\\' && *pos + 1 < len {
                let next = data[*pos + 1];
                match next {
                    b'\'' => s.push(b'\''),
                    b'\\' => s.push(b'\\'),
                    b'n' => s.push(b'\n'),
                    b'r' => s.push(b'\r'),
                    b't' => s.push(b'\t'),
                    b'0' => s.push(0),
                    _ => {
                        s.push(b'\\');
                        s.push(next);
                    }
                }
                *pos += 2;
            } else if ch == b'\'' {
                *pos += 1;
                break;
            } else {
                s.push(ch);
                *pos += 1;
            }
        }
        SqlValue::Str(String::from_utf8_lossy(&s).into_owned())
    }
    // NULL
    else if *pos + 3 < len && &data[*pos..*pos + 4] == b"NULL" {
        *pos += 4;
        SqlValue::Null
    }
    // Number (int or float)
    else if data[*pos].is_ascii_digit() || data[*pos] == b'-' || data[*pos] == b'+' {
        let start = *pos;
        let mut has_dot = false;
        while *pos < len {
            let ch = data[*pos];
            if ch == b'.' {
                has_dot = true;
                *pos += 1;
            } else if ch.is_ascii_digit() || ch == b'-' || ch == b'+' {
                *pos += 1;
            } else {
                break;
            }
        }
        let num_str = std::str::from_utf8(&data[start..*pos]).unwrap_or("0");
        if has_dot {
            SqlValue::Float(num_str.parse().unwrap_or(0.0))
        } else {
            SqlValue::Int(num_str.parse().unwrap_or(0))
        }
    } else {
        // Unknown byte — skip it and return Null
        *pos += 1;
        SqlValue::Null
    }
}

/// Parse the VALUES portion of an INSERT statement into rows of [`SqlValue`].
///
/// `data` should start at the first `(` of the VALUES clause.
pub fn parse_sql_values(data: &[u8]) -> Vec<Vec<SqlValue>> {
    let mut rows = Vec::new();
    let mut i = 0;
    let len = data.len();

    while i < len {
        // Skip to opening paren
        while i < len && data[i] != b'(' {
            if data[i] == b';' {
                return rows;
            }
            i += 1;
        }
        if i >= len {
            break;
        }
        i += 1; // skip '('

        let mut row = Vec::new();

        loop {
            // Skip whitespace
            while i < len && data[i] == b' ' {
                i += 1;
            }
            if i >= len {
                break;
            }

            if data[i] == b')' {
                i += 1;
                break;
            }

            if data[i] == b',' && row.is_empty() {
                i += 1;
                continue;
            }

            let val = parse_single_value(data, &mut i);
            row.push(val);

            // Skip comma between values
            if i < len && data[i] == b',' {
                i += 1;
            }
        }

        if !row.is_empty() {
            rows.push(row);
        }
    }

    rows
}

/// Find the byte offset of the first `(` after the VALUES keyword in an INSERT line.
///
/// Returns `None` if the line does not contain a VALUES clause.
pub fn find_values_start(line: &[u8]) -> Option<usize> {
    // Case-insensitive scan for "VALUES"
    let upper: Vec<u8> = line.iter().map(|b| b.to_ascii_uppercase()).collect();
    let pos = upper
        .windows(6)
        .position(|w| w == b"VALUES")?;
    // Find the first '(' after VALUES
    let after_values = pos + 6;
    for j in after_values..line.len() {
        if line[j] == b'(' {
            return Some(j);
        }
    }
    None
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

    // === Cycle 2: parse_single_value — integers and floats ===

    #[test]
    fn test_parse_int() {
        let data = b"42,";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Int(n) => assert_eq!(n, 42),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_negative_int() {
        let data = b"-7,";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Int(n) => assert_eq!(n, -7),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_bigint() {
        let data = b"1710000000000,";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Int(n) => assert_eq!(n, 1710000000000),
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_float() {
        let data = b"3.14,";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Float(f) => assert!((f - 3.14).abs() < 0.001),
            other => panic!("expected Float, got {other:?}"),
        }
    }

    // === Cycle 3: parse_single_value — strings with escapes ===

    #[test]
    fn test_parse_simple_string() {
        let data = b"'hello',";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Str(s) => assert_eq!(s, "hello"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_empty_string() {
        let data = b"'',";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Str(s) => assert_eq!(s, ""),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_string_with_escaped_quote() {
        let data = b"'it\\'s a test',";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Str(s) => assert_eq!(s, "it's a test"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_string_with_escaped_backslash() {
        let data = b"'path\\\\to\\\\file',";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Str(s) => assert_eq!(s, "path\\to\\file"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_string_with_newline() {
        let data = b"'line1\\nline2',";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Str(s) => assert_eq!(s, "line1\nline2"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_string_with_null_byte() {
        let data = b"'has\\0null',";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Str(s) => assert_eq!(s.as_bytes(), b"has\0null"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_string_with_comma() {
        let data = b"'hello, world',";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Str(s) => assert_eq!(s, "hello, world"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn test_parse_string_with_parentheses() {
        let data = b"'(remix)',";
        let mut pos = 0;
        let val = parse_single_value(data, &mut pos);
        match val {
            SqlValue::Str(s) => assert_eq!(s, "(remix)"),
            other => panic!("expected Str, got {other:?}"),
        }
    }

    // === Cycle 4: parse_sql_values — full VALUES parsing ===

    #[test]
    fn test_parse_values_single_row() {
        let data = b"(1,'hello',NULL)";
        let rows = parse_sql_values(data);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 3);
        assert_eq!(rows[0][0], SqlValue::Int(1));
        assert_eq!(rows[0][1], SqlValue::Str("hello".to_string()));
        assert_eq!(rows[0][2], SqlValue::Null);
    }

    #[test]
    fn test_parse_values_multiple_rows() {
        let data = b"(1,'a'),(2,'b'),(3,'c')";
        let rows = parse_sql_values(data);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0][0], SqlValue::Int(1));
        assert_eq!(rows[1][0], SqlValue::Int(2));
        assert_eq!(rows[2][0], SqlValue::Int(3));
    }

    #[test]
    fn test_parse_values_with_semicolon() {
        let data = b"(1,'hello');";
        let rows = parse_sql_values(data);
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn test_parse_values_empty() {
        let data = b"";
        let rows = parse_sql_values(data);
        assert!(rows.is_empty());
    }

    // === Cycle 5: find_values_start ===

    #[test]
    fn test_find_values_start_basic() {
        let line = b"INSERT INTO `table` VALUES (1,'a')";
        let offset = find_values_start(line);
        assert!(offset.is_some());
        assert_eq!(line[offset.unwrap()], b'(');
    }

    #[test]
    fn test_find_values_start_with_columns() {
        let line = b"INSERT INTO `table` (`col1`, `col2`) VALUES (1,'a')";
        let offset = find_values_start(line);
        assert!(offset.is_some());
        // The '(' should be the one after VALUES, not the column list paren
        let data_after = &line[offset.unwrap()..];
        assert!(data_after.starts_with(b"(1,"));
    }

    #[test]
    fn test_find_values_start_case_insensitive() {
        let line = b"INSERT INTO `table` values (1,'a')";
        let offset = find_values_start(line);
        assert!(offset.is_some());
    }

    #[test]
    fn test_find_values_start_no_values() {
        let line = b"CREATE TABLE `table` (id int)";
        let offset = find_values_start(line);
        assert!(offset.is_none());
    }
}
