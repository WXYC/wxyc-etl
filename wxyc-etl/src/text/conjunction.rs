//! Conjunction fold: normalize the ASCII "&" conjunction to "and".
//!
//! Promotes [wxyc-etl#147](https://github.com/WXYC/wxyc-etl/issues/147)'s
//! reconciliation of three overlapping `library-metadata-lookup`
//! implementations into one shared primitive:
//!
//! - `identity/normalize.py::canonicalize_for_identity_lookup` —
//!   `_AMPERSAND_CONJUNCTION_RE = re.compile(r"\s+&\s+")` -> `" and "`.
//!   Requires whitespace on *both* sides, so glued/typographic ampersands
//!   (`R&B`, `A&E`, `AT&T`) are left alone.
//! - `discogs/matching.py::normalize_for_track_comparison` —
//!   `result.replace("&", " and ")`, a bare literal replace with no
//!   whitespace requirement (folds `R&B` -> `R and B`). This call site
//!   immediately strips all remaining punctuation afterward
//!   (`re.sub(r"[^\w\s]", "", result)`), so the ampersand-specific line
//!   isn't a considered stance on `R&B` specifically — it is a pre-step
//!   that preserves the word "and" before the blanket punctuation strip
//!   nukes everything else.
//! - `clients/streaming/matching.py::normalize_artist_credit` —
//!   `_AMPERSAND_RE = re.compile(r"\s*&\s*")` -> `" and "`, used to
//!   *generate an alternate search variant* (recall-maximizing; a false
//!   fold there just costs one wasted candidate scored against real Discogs
//!   data). Same `R&B` -> `R and B` outcome as the bare replace, but for a
//!   different reason — this call site also tries the reverse fold
//!   (`_AND_RE`, `and` -> `&`) to generate a second variant.
//!
//! ## Decision (wxyc-etl#147)
//!
//! [`fold_conjunctions`] takes the **whitespace-required** behavior
//! (`identity/normalize.py`'s), not the majority-by-count bare/optional-
//! whitespace behavior. Rationale: this function is a single deterministic
//! canonicalization — unlike `normalize_artist_credit`, it does not return a
//! list of variants for a downstream scorer to discard — so an over-eager
//! fold here is a permanent, uncorrectable corruption of genuinely
//! typographic tokens. "R&B" is a real WXYC genre tag, not a hypothetical.
//! See the `fold_conjunctions_decision_table` test below for the full,
//! case-by-case record.
//!
//! Only the `&` -> `and` direction is implemented. LML's `and` -> `&`
//! direction (`normalize_artist_credit`'s `_AND_RE`) exists purely to
//! generate an alternate Discogs *search* query variant; it has no place in
//! a single-output canonicalization primitive and is out of scope here.

/// Fold the "&" conjunction to "and" wherever it is surrounded by Unicode
/// whitespace on both sides; leaves glued/typographic ampersands (`R&B`,
/// `A&E`, `AT&T`, `M&M`) untouched. See the module docs for the reconciled-
/// variant rationale.
///
/// The whitespace run(s) on either side of `&` are consumed along with the
/// `&` itself and replaced by a single ASCII space + `and` + a single ASCII
/// space, so `"Duke Ellington  &  John Coltrane"` (extra interior spaces)
/// folds to `"Duke Ellington and John Coltrane"` rather than leaving
/// doubled spaces behind.
///
/// Case, diacritics, and every other character are passed through
/// unchanged — this is a targeted structural fold, not a case/match-form
/// normalizer, and it does not trim leading/trailing whitespace on the
/// input as a whole (that's [`super::forms::to_match_form`]'s job).
/// Idempotent: the output never contains a whitespace-padded `&` for a
/// second pass to find.
pub fn fold_conjunctions(name: &str) -> String {
    if !name.contains('&') {
        return name.to_string();
    }

    let chars: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len());
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] == '&' {
            let has_leading_ws = out.ends_with(char::is_whitespace);
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            let has_trailing_ws = j > i + 1;

            if has_leading_ws && has_trailing_ws {
                // Trim the whitespace run already emitted before `&`, then
                // emit the canonical " and " in its place.
                while out.ends_with(char::is_whitespace) {
                    out.pop();
                }
                out.push_str(" and ");
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decision table for wxyc-etl#147: each row documents a case where the
    /// three LML variants (see module docs) would have disagreed, plus the
    /// straightforward agreement cases and the Unicode-safety proof.
    #[test]
    fn fold_conjunctions_decision_table() {
        let cases = [
            // Spaced ampersand: all three LML variants agree — fold.
            (
                "Duke Ellington & John Coltrane",
                "Duke Ellington and John Coltrane",
            ),
            // Glued ampersand inside a real genre tag. `identity/normalize.py`
            // requires whitespace on both sides and leaves this alone;
            // `discogs/matching.py`'s bare replace and
            // `clients/streaming/matching.py`'s `\s*&\s*` both fold it to
            // "R and B". DECISION: take the whitespace-required behavior —
            // see the module docs for why.
            ("R&B", "R&B"),
            ("A&E", "A&E"),
            ("AT&T", "AT&T"),
            ("M&M", "M&M"),
            // One-sided padding: consequence of requiring whitespace on
            // *both* sides, not a separate case any LML variant disagreed
            // on — no real Discogs/WXYC name has been observed in this
            // shape, but the rule should be deterministic for it.
            ("Foo &Bar", "Foo &Bar"),
            ("Foo& Bar", "Foo& Bar"),
            // Multiple interior spaces around `&` collapse to a single
            // " and " rather than leaving doubled spaces behind.
            (
                "Duke Ellington  &  John Coltrane",
                "Duke Ellington and John Coltrane",
            ),
            // Multiple conjunctions in one string.
            ("A & B & C", "A and B and C"),
            // No ampersand at all: passthrough, including a case with
            // trailing whitespace this function deliberately does not trim.
            ("Stereolab", "Stereolab"),
            ("Stereolab  ", "Stereolab  "),
            // Diacritic-bearing WXYC-canonical names: proves the char-index
            // scan around the ASCII `&` doesn't corrupt multi-byte UTF-8.
            ("Nilüfer Yanya", "Nilüfer Yanya"),
            ("Hermanos Gutiérrez", "Hermanos Gutiérrez"),
            ("Csillagrablók", "Csillagrablók"),
            // Synthetic pairing of two real WXYC-canonical artists (not an
            // actual credit) to prove the fold and the diacritic
            // passthrough compose correctly in the same input.
            (
                "Csillagrablók & Hermanos Gutiérrez",
                "Csillagrablók and Hermanos Gutiérrez",
            ),
            ("", ""),
        ];
        for (input, expected) in cases {
            assert_eq!(
                fold_conjunctions(input),
                expected,
                "fold_conjunctions({input:?})"
            );
        }
    }

    #[test]
    fn idempotent() {
        for s in [
            "Duke Ellington & John Coltrane",
            "R&B",
            "A & B & C",
            "Nilüfer Yanya",
            "",
        ] {
            let once = fold_conjunctions(s);
            let twice = fold_conjunctions(&once);
            assert_eq!(once, twice, "idempotence broken for {s:?}");
        }
    }
}
