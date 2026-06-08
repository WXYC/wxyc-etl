//! Boundary-safety helpers for PostgreSQL TEXT columns.
//!
//! These are about *transport*, not *normalization* — they make a string
//! *acceptable* to a downstream sink without changing its meaning. The
//! conceptual line vs `text::forms` (storage/match/ascii) is load-bearing
//! and intentionally guarded: don't move boundary-safety helpers into
//! `text::forms`, and don't import normalization into this module.

use std::borrow::Cow;

/// Make `s` acceptable for storage in a PostgreSQL TEXT column.
///
/// Today this strips U+0000 (NUL), which the SQL standard forbids in
/// character types — Postgres rejects rows containing it with
/// `invalid byte sequence for encoding`. Returns `Cow::Borrowed` when
/// the input is already acceptable so the common case never allocates.
///
/// Per the WX-3.B policy ratified on [WXYC/docs#18](https://github.com/WXYC/docs/issues/18),
/// every PG TEXT write boundary calls this helper. U+0000 in metadata is
/// always corruption, never intent.
///
/// Idempotent: `to_pg_text_form(to_pg_text_form(s)) == to_pg_text_form(s)`.
pub fn to_pg_text_form(s: &str) -> Cow<'_, str> {
    if s.contains('\0') {
        Cow::Owned(s.replace('\0', ""))
    } else {
        Cow::Borrowed(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_string_is_borrowed() {
        let out = to_pg_text_form("");
        assert_eq!(out, "");
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn no_nul_returns_borrowed() {
        let input = "Nilüfer Yanya";
        let out = to_pg_text_form(input);
        assert_eq!(out, input);
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn single_nul_returns_owned_with_nul_stripped() {
        let out = to_pg_text_form("Stereo\0lab");
        assert_eq!(out, "Stereolab");
        assert!(matches!(out, Cow::Owned(_)));
    }

    #[test]
    fn leading_nul_stripped() {
        let out = to_pg_text_form("\0Autechre");
        assert_eq!(out, "Autechre");
        assert!(matches!(out, Cow::Owned(_)));
    }

    #[test]
    fn trailing_nul_stripped() {
        let out = to_pg_text_form("Cat Power\0");
        assert_eq!(out, "Cat Power");
        assert!(matches!(out, Cow::Owned(_)));
    }

    #[test]
    fn all_nul_collapses_to_empty() {
        let out = to_pg_text_form("\0\0\0");
        assert_eq!(out, "");
        assert!(matches!(out, Cow::Owned(_)));
    }

    #[test]
    fn idempotent_on_clean_input() {
        let input = "Hermanos Gutiérrez";
        let once = to_pg_text_form(input).into_owned();
        let twice = to_pg_text_form(&once).into_owned();
        assert_eq!(once, twice);
    }

    #[test]
    fn idempotent_on_nul_bearing_input() {
        let once = to_pg_text_form("foo\0bar\0baz").into_owned();
        assert_eq!(once, "foobarbaz");
        let twice = to_pg_text_form(&once).into_owned();
        assert_eq!(twice, "foobarbaz");
    }

    #[test]
    fn preserves_multi_byte_codepoints_around_nul() {
        // Verifies the NUL strip doesn't slice across a UTF-8 codepoint
        // boundary. "Csillagrablók" has multi-byte ó (0xC3 0xB3).
        let out = to_pg_text_form("Csillagrabl\0ók");
        assert_eq!(out, "Csillagrablók");
        assert!(matches!(out, Cow::Owned(_)));
    }

    #[test]
    fn preserves_non_nul_control_bytes() {
        // Only U+0000 is stripped — other control bytes (tab, newline,
        // VT, etc.) are valid TEXT values and pass through unchanged.
        let input = "line1\tcol\nline2\x0bvt";
        let out = to_pg_text_form(input);
        assert_eq!(out, input);
        assert!(matches!(out, Cow::Borrowed(_)));
    }

    #[test]
    fn cross_reference_to_text_forms_contract() {
        // Sanity that to_pg_text_form does NOT do any normalization — case,
        // diacritics, whitespace, ligatures all survive. Normalization is
        // text::forms' job; this is a transport-safety helper.
        let input = "  CAFÉ Tacvba  ";
        let out = to_pg_text_form(input);
        assert_eq!(out, input);
    }
}
