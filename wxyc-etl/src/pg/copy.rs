//! PostgreSQL COPY TEXT escaping and row formatting.
//!
//! These functions produce data in PostgreSQL's COPY TEXT format:
//! tab-delimited columns, backslash escaping, and `\N` for NULLs.

/// Escape a string for PostgreSQL COPY TEXT format.
///
/// COPY TEXT uses tab-delimited rows with backslash escaping:
/// - `\` -> `\\`
/// - newline -> `\n`
/// - carriage return -> `\r`
/// - tab -> `\t`
pub fn escape_copy_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape a string for PostgreSQL COPY TEXT format, appending directly to a byte buffer.
///
/// This is the zero-allocation counterpart of [`escape_copy_text()`]. Instead of
/// returning a new `String`, it pushes escaped bytes directly into `buf`.
pub fn escape_copy_text_into(buf: &mut Vec<u8>, s: &str) {
    for &b in s.as_bytes() {
        match b {
            b'\\' => buf.extend_from_slice(b"\\\\"),
            b'\n' => buf.extend_from_slice(b"\\n"),
            b'\r' => buf.extend_from_slice(b"\\r"),
            b'\t' => buf.extend_from_slice(b"\\t"),
            _ => buf.push(b),
        }
    }
}

/// Format a value for a COPY TEXT column.
///
/// `None` and empty strings become `\N` (PostgreSQL NULL).
/// Non-empty strings are escaped.
pub fn copy_value(val: Option<&str>) -> String {
    match val {
        None | Some("") => "\\N".to_string(),
        Some(s) => escape_copy_text(s),
    }
}

/// Format a COPY TEXT row from a slice of column values.
///
/// Joins values with tabs and appends a newline.
pub fn copy_line(values: &[Option<&str>]) -> String {
    let mut line = String::new();
    for (i, val) in values.iter().enumerate() {
        if i > 0 {
            line.push('\t');
        }
        line.push_str(&copy_value(*val));
    }
    line.push('\n');
    line
}

/// Trait for accessing image type and URI from different image structs.
///
/// This allows [`pick_artwork_url()`] to be generic over different
/// release image representations.
pub trait ImageRef {
    fn image_type(&self) -> &str;
    fn uri(&self) -> &str;
}

/// Pick the best artwork URL from a slice of images.
///
/// Prefers the first "primary" image; falls back to the first image of any type.
/// Returns `None` if the slice is empty.
pub fn pick_artwork_url<I: ImageRef>(images: &[I]) -> Option<&str> {
    let primary = images
        .iter()
        .find(|img| img.image_type() == "primary")
        .map(|img| img.uri());
    primary.or_else(|| images.first().map(|img| img.uri()))
}

/// Extract a 4-digit year from a Discogs "released" field.
///
/// Accepts formats like "1997-06-16" or "1997". Returns `None` if the
/// string is shorter than 4 characters or doesn't start with 4 digits.
pub fn extract_year(released: &str) -> Option<i16> {
    if released.len() >= 4 && released.as_bytes()[..4].iter().all(|b| b.is_ascii_digit()) {
        released[..4].parse().ok()
    } else {
        None
    }
}

/// Convert an empty string to `None`.
pub fn empty_to_none(s: &str) -> Option<&str> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Write an integer as a COPY TEXT column value directly into a byte buffer.
///
/// Uses [`itoa`] for zero-allocation integer formatting.
pub fn write_copy_int(buf: &mut Vec<u8>, n: impl itoa::Integer) {
    let mut itoa_buf = itoa::Buffer::new();
    buf.extend_from_slice(itoa_buf.format(n).as_bytes());
}

/// Write a COPY TEXT row directly into a byte buffer.
///
/// This is the zero-allocation counterpart of [`copy_line()`]. Values are
/// tab-separated; `None` and empty strings become `\N` (PostgreSQL NULL).
pub fn write_copy_row(buf: &mut Vec<u8>, values: &[Option<&str>]) {
    for (i, val) in values.iter().enumerate() {
        if i > 0 {
            buf.push(b'\t');
        }
        match val {
            None | Some("") => buf.extend_from_slice(b"\\N"),
            Some(s) => escape_copy_text_into(buf, s),
        }
    }
    buf.push(b'\n');
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- escape_copy_text tests --

    #[test]
    fn test_escape_backslash() {
        assert_eq!(escape_copy_text("a\\b"), "a\\\\b");
    }

    #[test]
    fn test_escape_newline() {
        assert_eq!(escape_copy_text("line1\nline2"), "line1\\nline2");
    }

    #[test]
    fn test_escape_tab() {
        assert_eq!(escape_copy_text("col1\tcol2"), "col1\\tcol2");
    }

    #[test]
    fn test_escape_cr() {
        assert_eq!(escape_copy_text("a\rb"), "a\\rb");
    }

    #[test]
    fn test_escape_no_special() {
        assert_eq!(escape_copy_text("plain text"), "plain text");
    }

    #[test]
    fn test_escape_mixed() {
        assert_eq!(
            escape_copy_text("line1\nline2\ttab\\slash"),
            "line1\\nline2\\ttab\\\\slash"
        );
    }

    // -- escape_copy_text_into tests --

    #[test]
    fn test_escape_into_plain() {
        let mut buf = Vec::new();
        escape_copy_text_into(&mut buf, "hello world");
        assert_eq!(buf, b"hello world");
    }

    #[test]
    fn test_escape_into_special_chars() {
        let mut buf = Vec::new();
        escape_copy_text_into(&mut buf, "line1\nline2\ttab\\slash\rret");
        assert_eq!(buf, b"line1\\nline2\\ttab\\\\slash\\rret");
    }

    #[test]
    fn test_escape_into_matches_escape_copy_text() {
        let cases = ["hello", "a\tb", "a\nb", "a\\b", "a\rb", "mix\t\n\\end"];
        for s in &cases {
            let mut buf = Vec::new();
            escape_copy_text_into(&mut buf, s);
            assert_eq!(
                String::from_utf8(buf).unwrap(),
                escape_copy_text(s),
                "Mismatch for input: {:?}",
                s,
            );
        }
    }

    // -- copy_value tests --

    #[test]
    fn test_copy_value_none() {
        assert_eq!(copy_value(None), "\\N");
    }

    #[test]
    fn test_copy_value_empty() {
        assert_eq!(copy_value(Some("")), "\\N");
    }

    #[test]
    fn test_copy_value_normal() {
        assert_eq!(copy_value(Some("hello")), "hello");
    }

    #[test]
    fn test_copy_value_special_chars() {
        assert_eq!(copy_value(Some("a\tb")), "a\\tb");
    }

    // -- copy_line tests --

    #[test]
    fn test_copy_line_all_values() {
        let line = copy_line(&[Some("1001"), Some("Test Title"), Some("US")]);
        assert_eq!(line, "1001\tTest Title\tUS\n");
    }

    #[test]
    fn test_copy_line_with_nulls() {
        let line = copy_line(&[Some("1001"), None, Some("US")]);
        assert_eq!(line, "1001\t\\N\tUS\n");
    }

    #[test]
    fn test_copy_line_empty_string_becomes_null() {
        let line = copy_line(&[Some("1001"), Some(""), Some("US")]);
        assert_eq!(line, "1001\t\\N\tUS\n");
    }

    #[test]
    fn test_copy_line_with_special_chars() {
        let line = copy_line(&[Some("1"), Some("Title with\ttab"), Some("Note\nline2")]);
        assert_eq!(line, "1\tTitle with\\ttab\tNote\\nline2\n");
    }

    // -- write_copy_row tests --

    #[test]
    fn test_write_copy_row_all_values() {
        let mut buf = Vec::new();
        write_copy_row(&mut buf, &[Some("1001"), Some("Test Title"), Some("US")]);
        assert_eq!(buf, b"1001\tTest Title\tUS\n");
    }

    #[test]
    fn test_write_copy_row_with_nulls() {
        let mut buf = Vec::new();
        write_copy_row(&mut buf, &[Some("1001"), None, Some("US")]);
        assert_eq!(buf, b"1001\t\\N\tUS\n");
    }

    #[test]
    fn test_write_copy_row_empty_string_becomes_null() {
        let mut buf = Vec::new();
        write_copy_row(&mut buf, &[Some("1001"), Some(""), Some("US")]);
        assert_eq!(buf, b"1001\t\\N\tUS\n");
    }

    #[test]
    fn test_write_copy_row_with_special_chars() {
        let mut buf = Vec::new();
        write_copy_row(
            &mut buf,
            &[Some("1"), Some("Title with\ttab"), Some("Note\nline2")],
        );
        assert_eq!(buf, b"1\tTitle with\\ttab\tNote\\nline2\n");
    }

    #[test]
    fn test_write_copy_row_matches_copy_line() {
        let test_cases: Vec<Vec<Option<&str>>> = vec![
            vec![Some("1001"), Some("Test"), Some("US")],
            vec![Some("1"), None, Some("value")],
            vec![Some("42"), Some(""), Some("end")],
            vec![Some("1"), Some("tab\there"), Some("nl\nhere")],
        ];
        for values in &test_cases {
            let mut buf = Vec::new();
            write_copy_row(&mut buf, values);
            let expected = copy_line(values);
            assert_eq!(
                String::from_utf8(buf).unwrap(),
                expected,
                "Mismatch for values: {:?}",
                values,
            );
        }
    }

    // -- write_copy_int tests --

    #[test]
    fn test_write_copy_int_u64() {
        let mut buf = Vec::new();
        write_copy_int(&mut buf, 42u64);
        assert_eq!(&buf, b"42");
    }

    #[test]
    fn test_write_copy_int_negative() {
        let mut buf = Vec::new();
        write_copy_int(&mut buf, -7i32);
        assert_eq!(&buf, b"-7");
    }

    #[test]
    fn test_write_copy_int_zero() {
        let mut buf = Vec::new();
        write_copy_int(&mut buf, 0u32);
        assert_eq!(&buf, b"0");
    }

    #[test]
    fn test_write_copy_int_i16() {
        let mut buf = Vec::new();
        write_copy_int(&mut buf, 2001i16);
        assert_eq!(&buf, b"2001");
    }

    // -- extract_year tests --

    #[test]
    fn test_extract_year_full_date() {
        assert_eq!(extract_year("1997-06-16"), Some(1997));
    }

    #[test]
    fn test_extract_year_year_only() {
        assert_eq!(extract_year("2024"), Some(2024));
    }

    #[test]
    fn test_extract_year_empty() {
        assert_eq!(extract_year(""), None);
    }

    #[test]
    fn test_extract_year_non_numeric() {
        assert_eq!(extract_year("Unknown"), None);
    }

    #[test]
    fn test_extract_year_partial_digits() {
        assert_eq!(extract_year("199"), None);
    }

    #[test]
    fn test_extract_year_leading_zeros() {
        assert_eq!(extract_year("0001-01-01"), Some(1));
    }

    // -- empty_to_none tests --

    #[test]
    fn test_empty_to_none_empty() {
        assert_eq!(empty_to_none(""), None);
    }

    #[test]
    fn test_empty_to_none_non_empty() {
        assert_eq!(empty_to_none("hello"), Some("hello"));
    }

    // -- pick_artwork_url tests --

    struct TestImage {
        image_type: &'static str,
        uri: &'static str,
    }

    impl ImageRef for TestImage {
        fn image_type(&self) -> &str {
            self.image_type
        }
        fn uri(&self) -> &str {
            self.uri
        }
    }

    #[test]
    fn test_pick_artwork_prefers_primary() {
        let images = vec![
            TestImage {
                image_type: "secondary",
                uri: "https://img.discogs.com/secondary.jpg",
            },
            TestImage {
                image_type: "primary",
                uri: "https://img.discogs.com/primary.jpg",
            },
        ];
        assert_eq!(
            pick_artwork_url(&images),
            Some("https://img.discogs.com/primary.jpg")
        );
    }

    #[test]
    fn test_pick_artwork_falls_back_to_first() {
        let images = vec![TestImage {
            image_type: "secondary",
            uri: "https://img.discogs.com/first.jpg",
        }];
        assert_eq!(
            pick_artwork_url(&images),
            Some("https://img.discogs.com/first.jpg")
        );
    }

    #[test]
    fn test_pick_artwork_empty() {
        let images: Vec<TestImage> = vec![];
        assert_eq!(pick_artwork_url(&images), None);
    }
}
