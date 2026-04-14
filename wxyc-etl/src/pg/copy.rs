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
}
