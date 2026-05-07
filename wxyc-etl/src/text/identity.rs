//! Cross-cache-identity match form: the layered normalizer used to resolve
//! `library_id ↔ discogs_master_id ↔ MBID ↔ Wikidata Q-id` joins.
//!
//! Spec: [`docs/normalization.md`](../../../docs/normalization.md).
//! Plan §3.3.2 algorithm; canonical entry point is [`to_identity_match_form`].
//!
//! Strictly more aggressive than [`super::forms::to_match_form`]: anything
//! `to_match_form` collapses, this also collapses. The reverse is not true —
//! identity matching deliberately drops `(Remastered 2019)`-style trailing
//! parens and leading articles that `to_match_form` preserves for FTS5 /
//! prefix-lookup users.
//!
//! This module ships the locked-on baseline (steps 4 + 5 from plan §3.3.2).
//! The opt-in step-6 (punctuation) and step-8 (`/N` disambiguator) variants
//! and the full parity matrix ship in a follow-up PR; the regression-report
//! harness ships in a third.
//!
//! Locked-on means: the regression-report `D` per-step shift on these rules
//! triggers algorithm refinement, *not* opt-out (plan §3.3.4).
use super::forms::{collapse_and_trim_ascii_space, to_match_form};

/// Identity-match form. The canonical entry point for cross-cache identity
/// matching across the four caches (Backend, Discogs, MusicBrainz, Wikidata)
/// and the LML reconciler.
///
/// Pipeline:
/// 1. [`to_match_form`] — covers plan §3.3.2 steps 1-3 + 7 (NFKC, lowercase,
///    selective combining-mark strip, sigma/æ/œ folds, Cf-strip, ASCII space
///    collapse) plus mojibake repair from `to_storage_form`.
/// 2. Strip a trailing parenthetical or bracketed group:
///    `foo (remastered 2019)` → `foo`; `foo [live]` → `foo`. Plan §3.3.2
///    step 4. Applied once to the outermost group; `foo (live) [remastered]`
///    becomes `foo (live)` after one pass — sufficient for the failure
///    modes documented in the plan and parity matrix. Empty-content
///    parens (`foo ()`) and bracket-only inputs (`(foo)`) are negatives:
///    the former strips, the latter is left untouched (it would otherwise
///    reduce to empty and force the `norm_artist_fallback` path).
/// 3. Drop a leading article (`the |a |an `) or the Discogs comma form
///    (`, the` / `, a` / `, an` at end-of-string after a non-empty stem).
///    Plan §3.3.2 step 5. Both positions are handled in one pass; only
///    one match is consumed.
/// 4. Re-collapse ASCII space and re-trim. Strictly cosmetic — covers the
///    case where step 2 or 3 left a stray leading/trailing space.
///
/// Empty-output handling: callers (cache builders, reconciler) are
/// responsible for the plan §3.3.3 fallback ladder (option B: re-run
/// without locked-on edits, then return original input verbatim with
/// `norm_artist_fallback=true`). This function returns the empty string
/// for any input that reduces to zero non-whitespace characters; it does
/// not auto-restore.
///
/// Idempotent on every input: `to_identity_match_form(to_identity_match_form(s)) == to_identity_match_form(s)`.
pub fn to_identity_match_form(s: &str) -> String {
    let m = to_match_form(s);
    let m = strip_trailing_parens(&m);
    let m = drop_articles(m);
    collapse_and_trim_ascii_space(&m)
}

/// Strip a single trailing `(...)` or `[...]` group plus any preceding ASCII
/// space. Returns the input unchanged when:
/// - There is no trailing close-bracket
/// - The matching open-bracket would be at index 0 (i.e. the entire input is
///   bracketed; stripping would erase the stem and force the fallback path)
/// - The brackets are unbalanced (no matching open-bracket found)
fn strip_trailing_parens(s: &str) -> &str {
    let trimmed = s.trim_end_matches(' ');
    let bytes = trimmed.as_bytes();
    let last = match bytes.last() {
        Some(&b) => b,
        None => return s,
    };
    let (open, close) = match last {
        b')' => (b'(', b')'),
        b']' => (b'[', b']'),
        _ => return s,
    };

    let mut depth: u32 = 0;
    let mut open_idx: Option<usize> = None;
    for (i, &b) in bytes.iter().enumerate().rev() {
        if b == close {
            depth += 1;
        } else if b == open {
            depth -= 1;
            if depth == 0 {
                open_idx = Some(i);
                break;
            }
        }
    }

    let Some(idx) = open_idx else { return s };
    if idx == 0 {
        return s;
    }
    let stem = &trimmed[..idx];
    stem.trim_end_matches(' ')
}

/// Drop a leading article (`the `, `a `, `an `) or the trailing Discogs
/// comma form (`, the`, `, a`, `, an` at end-of-string with a non-empty
/// stem). At most one match is consumed; calling twice is a no-op.
///
/// The leading-form pattern requires the article to be followed by ASCII
/// space (or end-of-string); `theater` does not match `the`.
///
/// The comma-form pattern requires `, ` (comma-space) before the article
/// and end-of-string after it; `Beatles, The` matches but
/// `Beatles, the Best Of` does not.
fn drop_articles(s: &str) -> String {
    if let Some(rest) = strip_leading_article(s) {
        return rest.to_string();
    }
    if let Some(stem) = strip_trailing_comma_article(s) {
        return stem.to_string();
    }
    s.to_string()
}

fn strip_leading_article(s: &str) -> Option<&str> {
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = s.strip_prefix(article) {
            return Some(rest);
        }
    }
    None
}

fn strip_trailing_comma_article(s: &str) -> Option<&str> {
    for article in [", the", ", a", ", an"] {
        if let Some(stem) = s.strip_suffix(article) {
            if !stem.is_empty() {
                return Some(stem);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- step 4: trailing paren strip ---

    #[test]
    fn strips_trailing_parens() {
        assert_eq!(to_identity_match_form("Foo (Remastered 2019)"), "foo");
    }

    #[test]
    fn strips_trailing_brackets() {
        assert_eq!(to_identity_match_form("Foo [Live]"), "foo");
    }

    #[test]
    fn strips_outermost_only_in_one_pass() {
        // Documented behavior: a second trailing group survives.
        assert_eq!(
            to_identity_match_form("Foo (Live) [Remastered]"),
            "foo (live)"
        );
    }

    #[test]
    fn strips_empty_parens() {
        assert_eq!(to_identity_match_form("Foo ()"), "foo");
    }

    #[test]
    fn does_not_strip_when_only_parens() {
        // (Foo) would otherwise reduce to empty.
        assert_eq!(to_identity_match_form("(Foo)"), "(foo)");
    }

    #[test]
    fn does_not_strip_unbalanced_close() {
        assert_eq!(to_identity_match_form("Foo )"), "foo )");
    }

    #[test]
    fn does_not_strip_paren_in_middle() {
        assert_eq!(
            to_identity_match_form("Sigur (the band) Rós"),
            "sigur (the band) ros"
        );
    }

    #[test]
    fn handles_nested_parens() {
        assert_eq!(to_identity_match_form("Foo (Live (1999) Edition)"), "foo");
    }

    // --- step 5: leading article drop ---

    #[test]
    fn drops_the() {
        assert_eq!(to_identity_match_form("The Beatles"), "beatles");
    }

    #[test]
    fn drops_a() {
        assert_eq!(
            to_identity_match_form("A Tribe Called Quest"),
            "tribe called quest"
        );
    }

    #[test]
    fn drops_an() {
        assert_eq!(to_identity_match_form("An Albatross"), "albatross");
    }

    #[test]
    fn drops_only_first_article() {
        // `The The` should drop one `the` only.
        assert_eq!(to_identity_match_form("The The"), "the");
    }

    #[test]
    fn does_not_drop_when_article_is_a_prefix_substring() {
        assert_eq!(to_identity_match_form("Theater"), "theater");
        assert_eq!(to_identity_match_form("Anal"), "anal");
        assert_eq!(to_identity_match_form("Apple"), "apple");
    }

    #[test]
    fn drops_comma_form_the() {
        // Discogs convention: `Beatles, The` — leading article moved to end.
        assert_eq!(to_identity_match_form("Beatles, The"), "beatles");
    }

    #[test]
    fn drops_comma_form_a() {
        assert_eq!(
            to_identity_match_form("Tribe Called Quest, A"),
            "tribe called quest"
        );
    }

    #[test]
    fn drops_comma_form_an() {
        assert_eq!(to_identity_match_form("Albatross, An"), "albatross");
    }

    #[test]
    fn does_not_drop_comma_article_when_more_follows() {
        assert_eq!(
            to_identity_match_form("Beatles, the Best Of"),
            "beatles, the best of"
        );
    }

    // --- composed steps ---

    #[test]
    fn drops_article_then_strips_parens() {
        assert_eq!(
            to_identity_match_form("The Beatles (Remastered)"),
            "beatles"
        );
    }

    #[test]
    fn strips_parens_then_drops_article_after_recollapse() {
        // After paren strip, the leading "the " is exposed only if it was
        // already there before — paren strip from end doesn't expose new
        // articles. This test pins composition order.
        assert_eq!(
            to_identity_match_form("The Foo Fighters (1995)"),
            "foo fighters"
        );
    }

    // --- inherited from to_match_form: sigma + diacritics ---

    #[test]
    fn folds_diacritics_via_to_match_form() {
        assert_eq!(
            to_identity_match_form("The Hermanos Gutiérrez"),
            "hermanos gutierrez"
        );
    }

    #[test]
    fn folds_final_sigma_via_to_match_form() {
        assert_eq!(
            to_identity_match_form("Στελλάς"),
            to_identity_match_form("Στελλάσ")
        );
    }

    #[test]
    fn repairs_mojibake_via_to_match_form() {
        // to_match_form runs to_storage_form first, fixing mojibake before fold.
        assert_eq!(to_identity_match_form("The Î£tella"), "\u{03c3}tella");
    }

    // --- empty / whitespace edges ---

    #[test]
    fn empty_input() {
        assert_eq!(to_identity_match_form(""), "");
    }

    #[test]
    fn whitespace_only_input() {
        assert_eq!(to_identity_match_form("   "), "");
    }

    #[test]
    fn only_article() {
        // `The` alone: leading-article pattern requires trailing space, so
        // a bare article is preserved.
        assert_eq!(to_identity_match_form("The"), "the");
    }

    // --- idempotence ---

    #[test]
    fn idempotent() {
        for s in [
            "Stereolab",
            "The Beatles",
            "An Albatross",
            "A Tribe Called Quest",
            "Beatles, The",
            "Foo (Remastered 2019)",
            "The Foo Fighters (1995)",
            "Hermanos Gutiérrez",
            "Στελλάς",
            "Î£tella",
            "",
        ] {
            assert_eq!(
                to_identity_match_form(&to_identity_match_form(s)),
                to_identity_match_form(s),
                "idempotence broken for {s:?}"
            );
        }
    }
}
