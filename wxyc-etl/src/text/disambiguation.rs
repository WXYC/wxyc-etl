//! Discogs artist/label disambiguation-suffix stripping.
//!
//! Promotes [wxyc-etl#147](https://github.com/WXYC/wxyc-etl/issues/147)'s
//! reconciliation of three overlapping `library-metadata-lookup`
//! implementations:
//!
//! - `discogs/matching.py::strip_discogs_suffix` —
//!   `_DISCOGS_SUFFIX_RE = re.compile(r"\s*\(\d+\)$")`. Numeric-only, no
//!   tolerance for whitespace *after* the closing paren.
//! - `clients/streaming/matching.py`'s `_DISCOGS_DISAMBIG_RE =
//!   re.compile(r"\s*\(\d+\)\s*$")`. Same numeric-only shape, but tolerant
//!   of trailing whitespace after `)`. A superset of the pattern above for
//!   every ASCII-digit disambiguator — no real disagreement to adjudicate,
//!   just a widen; `broad = false` mode here takes this (broader) variant.
//!   (Not a literal superset over arbitrary input — see "ASCII-digit
//!   narrowing" below.)
//! - `clients/streaming/matching.py::strip_discogs_disambig` —
//!   `_DISCOGS_DISAMBIG_BROAD_RE = re.compile(
//!   r"\s*\(\s*[A-Za-z0-9][A-Za-z0-9\s.\-]{0,18}\)\s*$")`. Also strips
//!   country/region codes (`Stereolab (UK)`) and short word-form qualifiers
//!   (`Sade (band)`). `broad = true` mode implements this shape
//!   (simplified; see "Simplification" below). `clients/streaming/
//!   matching.py` additionally re-imports `strip_discogs_suffix` from
//!   `discogs.matching` (`# noqa: F401`), so all three strippers coexisted
//!   in that one module's scope before this promotion.
//!
//! ## Simplification (deliberate; wxyc-etl#147)
//!
//! The broad Python regex allows unbounded whitespace padding right after
//! the opening paren, before its mandatory first alphanumeric character
//! (`\(\s*[A-Za-z0-9]...`) — e.g. `"Foo (   UK)"` would technically match.
//! That padding is a regex-authoring artifact, not a real Discogs shape:
//! Discogs disambiguators are always written tight against the paren —
//! `(UK)`, `(2)`, `(band)`. This implementation requires the first content
//! character to sit directly against `(`, dropping that never-exercised
//! padding allowance. Strictly narrower on that one dimension; behaviorally
//! identical for every real Discogs suffix and every case in this module's
//! test table.
//!
//! ## Decision (wxyc-etl#147)
//!
//! `broad` is an explicit caller-chosen parameter, not a heuristic: pick
//! `false` for contexts where a country/word qualifier might legitimately
//! be part of a real name (LML's own `normalize_artist_credit` only strips
//! the numeric form, for exactly this reason), `true` for contexts that
//! want the full Discogs-suffix surface (LML's `strip_discogs_disambig`).
//!
//! ## ASCII-digit narrowing (deliberate; wxyc-etl#147)
//!
//! Python's `\d` inside `_DISCOGS_SUFFIX_RE` and `_DISCOGS_DISAMBIG_RE` is
//! Unicode-aware by default — it matches *any* Unicode decimal-digit
//! codepoint (Nd category), not just ASCII `0`-`9`. This implementation's
//! narrow mode uses [`char::is_ascii_digit`], so `"Foo (٢)"` (Arabic-Indic
//! digit U+0662) was stripped by both LML narrow regexes but is left
//! unchanged here. Real Discogs disambiguators are always ASCII digits
//! (Discogs assigns them programmatically, `(1)`, `(2)`, ...), so this is a
//! superset for every disambiguator shape that actually occurs — not a
//! literal claim about arbitrary Unicode input. (Broad mode's character
//! class has no such gap: `_DISCOGS_DISAMBIG_BROAD_RE`'s `[A-Za-z0-9]` is
//! already an explicit ASCII range in the Python source, matching
//! [`char::is_ascii_alphanumeric`] exactly.)

/// Max length of the parenthetical *content* (excluding the parens) that
/// `broad` mode treats as a disambiguator. Mirrors
/// `_DISCOGS_DISAMBIG_BROAD_RE`'s `[A-Za-z0-9][A-Za-z0-9\s.\-]{0,18}` bound:
/// 1 mandatory character + up to 18 more = 19.
const BROAD_MAX_CONTENT_LEN: usize = 19;

/// Strip a trailing Discogs artist/label disambiguation suffix.
///
/// - `broad = false`: numeric-only — `(2)`, `(22)`, optionally preceded and
///   followed by whitespace.
/// - `broad = true`: also strips a trailing parenthetical whose content is
///   1-19 ASCII letters/digits/spaces/`.`/`-`, starting with a letter or
///   digit — covers Discogs country codes (`(UK)`) and word-form qualifiers
///   (`(band)`, `(rapper)`) in addition to the numeric form.
///
/// Case, diacritics, and the rest of the stem are passed through unchanged
/// — this is a suffix strip, not a match-form fold. Returns the input
/// unchanged when it is empty or no suffix matches at end-of-string.
///
/// Not idempotent in the degenerate case where the *entire* input is the
/// suffix: `strip_discogs_disambiguation("(2)", false) == ""`. This matches
/// every LML variant's `re.sub("", ...)` semantics — unlike
/// `to_identity_match_form`'s trailing-paren strip, none of them special-
/// case "the whole input is bracketed" to avoid producing an empty stem, so
/// this primitive doesn't invent that guard either.
pub fn strip_discogs_disambiguation(name: &str, broad: bool) -> String {
    if name.is_empty() {
        return name.to_string();
    }
    match disambiguator_start(name, broad) {
        Some(start) => name[..start].to_string(),
        None => name.to_string(),
    }
}

/// Byte offset where a trailing disambiguator (plus its leading/trailing
/// whitespace padding) begins, or `None` if the end of `name` doesn't look
/// like one under the given mode.
fn disambiguator_start(name: &str, broad: bool) -> Option<usize> {
    let chars: Vec<(usize, char)> = name.char_indices().collect();
    let mut i = chars.len();

    // Trailing `\s*` after the closing paren. Both LML narrow variants'
    // patterns disagree here (see module docs); this always tolerates it,
    // matching the broader of the two.
    while i > 0 && chars[i - 1].1.is_whitespace() {
        i -= 1;
    }
    if i == 0 || chars[i - 1].1 != ')' {
        return None;
    }
    i -= 1; // consume ')'

    let mut content_len = 0usize;
    let mut first_content_char = None;
    loop {
        if i == 0 {
            return None; // no matching '(' before the start of the string
        }
        let c = chars[i - 1].1;
        if c == '(' {
            break;
        }
        let allowed = if broad {
            c.is_ascii_alphanumeric() || c.is_whitespace() || c == '.' || c == '-'
        } else {
            c.is_ascii_digit()
        };
        if !allowed || (broad && content_len >= BROAD_MAX_CONTENT_LEN) {
            return None;
        }
        first_content_char = Some(c);
        content_len += 1;
        i -= 1;
    }
    if content_len == 0 {
        return None; // "()" — no content, never a valid disambiguator
    }
    // Mandatory first content character (nearest the opening paren) must be
    // alphanumeric. Narrow mode's digits-only set already guarantees this;
    // broad mode's wider set (which also allows space/./-) does not, so
    // this check is load-bearing there.
    if !first_content_char.is_some_and(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    i -= 1; // consume '('

    // Leading `\s*` between the stem and the paren.
    while i > 0 && chars[i - 1].1.is_whitespace() {
        i -= 1;
    }
    Some(chars.get(i).map(|&(b, _)| b).unwrap_or(name.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decision table for wxyc-etl#147.
    #[test]
    fn strip_discogs_disambiguation_decision_table() {
        let cases: &[(&str, bool, &str)] = &[
            // Numeric suffix: all three LML variants agree — strip in both modes.
            ("Plug (2)", false, "Plug"),
            ("Plug (2)", true, "Plug"),
            ("Sessa (22)", false, "Sessa"),
            // Trailing whitespace after the suffix: `discogs/matching.py`'s
            // `_DISCOGS_SUFFIX_RE` (`\s*\(\d+\)$`) would NOT strip this
            // (anchored tight to end-of-string); `clients/streaming/
            // matching.py`'s `_DISCOGS_DISAMBIG_RE` (`\s*\(\d+\)\s*$`)
            // would. DECISION: take the broader, trailing-whitespace-
            // tolerant pattern for `broad = false` too — a superset for
            // ASCII-digit disambiguators (all real Discogs shapes; see the
            // ASCII-digit-narrowing row below for the one input class where
            // that framing doesn't literally hold).
            ("Plug (2)   ", false, "Plug"),
            // ASCII-digit narrowing (see the module docs): Python's `\d` in
            // both narrow LML regexes is Unicode-aware and would strip this
            // Arabic-Indic-digit suffix; this implementation's narrow mode
            // is ASCII-only and leaves it alone. Never observed in a real
            // Discogs disambiguator (they're always ASCII `0`-`9`), so the
            // "superset" framing above holds in practice, not literally.
            ("Foo (\u{0662})", false, "Foo (\u{0662})"),
            // Country/region code: only the broad LML variant
            // (`_DISCOGS_DISAMBIG_BROAD_RE`) strips this; both narrow
            // variants agree it should NOT strip. `broad` is an explicit
            // caller-chosen parameter, not inferred from the content.
            ("Stereolab (UK)", false, "Stereolab (UK)"),
            ("Stereolab (UK)", true, "Stereolab"),
            // Word-form qualifier: same narrow/broad split.
            ("Sade (band)", false, "Sade (band)"),
            ("Sade (band)", true, "Sade"),
            ("Pretty Girls (rapper)", true, "Pretty Girls"),
            // No suffix at all: no-op in both modes.
            ("Stereolab", false, "Stereolab"),
            ("Stereolab", true, "Stereolab"),
            // Empty parenthetical never matches (no content to satisfy the
            // mandatory first-alphanumeric-character requirement).
            ("Foo ()", false, "Foo ()"),
            ("Foo ()", true, "Foo ()"),
            // Degenerate case: the entire input IS the suffix. Every LML
            // variant's `re.sub("", ...)` reduces this to the empty string
            // with no stem-preserving guard (unlike
            // `to_identity_match_form`'s trailing-paren strip); this
            // primitive matches that fidelity instead of inventing a new
            // safety net.
            ("(2)", false, ""),
            ("(UK)", true, ""),
            // Empty input: no-op.
            ("", false, ""),
            ("", true, ""),
            // Diacritic-bearing WXYC-canonical names: proves the
            // char-index scan doesn't corrupt multi-byte UTF-8 on the stem
            // while stripping the (ASCII-only) suffix — or correctly
            // leaving it alone under narrow mode.
            ("Csillagrablók (2)", false, "Csillagrablók"),
            ("Hermanos Gutiérrez (Chile)", true, "Hermanos Gutiérrez"),
            ("Nilüfer Yanya (UK)", false, "Nilüfer Yanya (UK)"),
        ];
        for &(input, broad, expected) in cases {
            assert_eq!(
                strip_discogs_disambiguation(input, broad),
                expected,
                "strip_discogs_disambiguation({input:?}, broad={broad})"
            );
        }
    }

    #[test]
    fn broad_content_length_cap() {
        // Content = "nineteen characters" (19 chars) — exactly the
        // `_DISCOGS_DISAMBIG_BROAD_RE` bound. Strips.
        let boundary_ok = "Foo (nineteen characters)";
        assert_eq!(strip_discogs_disambiguation(boundary_ok, true), "Foo");

        // Content = "nineteen charactersx" (20 chars) — one over the
        // bound. A genuinely long parenthetical aside on a real name is
        // left alone rather than truncated.
        let boundary_over = "Foo (nineteen charactersx)";
        assert_eq!(
            strip_discogs_disambiguation(boundary_over, true),
            boundary_over
        );
    }

    #[test]
    fn idempotent_when_no_further_suffix_remains() {
        for (input, broad) in [
            ("Plug (2)", false),
            ("Stereolab (UK)", true),
            ("Stereolab", false),
            ("", false),
        ] {
            let once = strip_discogs_disambiguation(input, broad);
            let twice = strip_discogs_disambiguation(&once, broad);
            assert_eq!(
                once, twice,
                "idempotence broken for {input:?} broad={broad}"
            );
        }
    }
}
