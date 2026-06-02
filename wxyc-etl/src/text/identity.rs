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
//! Public surface:
//!
//! - [`to_identity_match_form`] — locked-on baseline (steps 4 + 5).
//! - [`to_identity_match_form_title`] — title-side counterpart; same as base
//!   today. Distinct symbol so callers explicitly opt out of the artist-only
//!   `/N` disambiguator strip; future step-6 promotion stays type-safe.
//! - [`to_identity_match_form_with_punctuation`] — opt-in: adds step 6
//!   (punctuation collapse). Ships if the regression report's per-step shift
//!   on this rule is ≤2%; otherwise stays opt-in indefinitely (plan §3.3.4).
//! - [`to_identity_match_form_with_disambiguator_strip`] — opt-in: adds
//!   step 8 (trailing `/N` disambiguator strip). Artists only; titles use
//!   `_title` since `Side A/2` is meaningful disambiguation.
//!
//! Locked-on means: the regression-report `D` per-step shift on steps 4 + 5
//! triggers algorithm refinement, *not* opt-out (plan §3.3.4). Opt-in steps
//! 6 and 8 are exposed under separate function names so callers (and the
//! eventual Postgres analog) audit the choice at the call site.
use unicode_categories::UnicodeCategories;

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
    identity_baseline(s)
}

/// Title counterpart to [`to_identity_match_form`]. Currently identical to the
/// base function — exists as a separate public symbol so:
/// - Callers explicitly type-distinguish artist vs title at the call site,
///   making it impossible to accidentally hand a title to
///   `to_identity_match_form_with_disambiguator_strip` (which would strip
///   `Side A/2` style track-side disambiguators).
/// - When step 6 (punctuation collapse) eventually promotes to locked-on, the
///   title surface remains stable for callers that already typed against this
///   symbol.
///
/// Idempotent.
pub fn to_identity_match_form_title(s: &str) -> String {
    identity_baseline(s)
}

/// [`to_identity_match_form`] + plan §3.3.2 step 6: replace each run of
/// punctuation/symbol characters (anything that is not a Unicode letter,
/// number, or whitespace) with a single ASCII space, then re-collapse and
/// re-trim. Examples:
///
/// - `Godspeed You! Black Emperor` → `godspeed you black emperor`
/// - `M.I.A.` → `m i a`
/// - `+/-` → ``  (entirely punctuation; reduces to empty — caller's fallback)
///
/// Opt-in: gated behind this separately-named function until the regression
/// report shows the per-step shift on real cache data is ≤2%, at which point
/// the project lead may promote it to locked-on. Until then, callers that
/// want this behavior have to opt in explicitly.
///
/// Idempotent.
pub fn to_identity_match_form_with_punctuation(s: &str) -> String {
    let m = to_match_form(s);
    let m = strip_trailing_parens(&m);
    let m = drop_articles(m);
    let m = collapse_punctuation_to_space(&m);
    collapse_and_trim_ascii_space(&m)
}

/// [`to_identity_match_form`] + plan §3.3.2 step 8: strip a trailing
/// `\s*/\d+` (Discogs artist-disambiguator suffix) at end-of-string.
/// Examples:
///
/// - `John Smith /1` → `john smith`
/// - `Various /17` → `various`
/// - `Track 1/12` (no leading whitespace before `/`) → `track 1/12`
///
/// **Artists only.** Discogs uses `/N` to disambiguate same-name artists
/// (`John Smith /1` vs `John Smith /2`). Titles use `/N` for legitimate
/// in-title disambiguation (`Side A/2`, `Track 1/12`). Use
/// [`to_identity_match_form_title`] for title-side identity matching.
///
/// Idempotent.
pub fn to_identity_match_form_with_disambiguator_strip(s: &str) -> String {
    let baseline = identity_baseline(s);
    strip_trailing_disambiguator(&baseline)
}

fn identity_baseline(s: &str) -> String {
    let m = to_match_form(s);
    let m = strip_trailing_parens(&m);
    let m = drop_articles(m);
    collapse_and_trim_ascii_space(&m)
}

/// Replace each run of one or more non-letter, non-number, non-whitespace
/// codepoints with a single ASCII space. Letter/number recognition is
/// Unicode-property-aware: Greek `α`, Cyrillic `я`, Han `細` all qualify
/// as letters. The output may have leading/trailing/runs-of space; the
/// caller is expected to follow with [`collapse_and_trim_ascii_space`].
fn collapse_punctuation_to_space(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_was_replacement = false;
    for c in s.chars() {
        if c.is_letter() || c.is_number() || c.is_whitespace() {
            out.push(c);
            prev_was_replacement = false;
        } else if !prev_was_replacement {
            out.push(' ');
            prev_was_replacement = true;
        }
    }
    out
}

/// Strip a trailing `\s+/\d+` group. The leading whitespace is **required**:
/// `John Smith /1` strips, `Track 1/12` does not. The original spec's regex
/// (`\s*/\d+$`) is zero-or-more whitespace, but the spec doc itself names
/// `Track 1/12` as a step-8 negative (it survives because the `/` follows a
/// digit with no space). Requiring `\s+` makes the in-the-wild Discogs
/// convention (`Artist /N`, always with a space) match while preserving
/// genuine in-string `X/Y` patterns.
fn strip_trailing_disambiguator(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut end = bytes.len();
    if end == 0 || !bytes[end - 1].is_ascii_digit() {
        return s.to_string();
    }
    while end > 0 && bytes[end - 1].is_ascii_digit() {
        end -= 1;
    }
    if end == 0 || bytes[end - 1] != b'/' {
        return s.to_string();
    }
    let slash = end - 1;
    let mut after_spaces = slash;
    while after_spaces > 0 && bytes[after_spaces - 1] == b' ' {
        after_spaces -= 1;
    }
    if after_spaces == slash {
        // No whitespace before the slash — `Track 1/12` style. Preserve.
        return s.to_string();
    }
    s[..after_spaces].to_string()
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
    if let Some(rest) = strip_leading_article_with_space_suffix(s) {
        return rest.to_string();
    }
    if let Some(stem) = strip_trailing_comma_article(s) {
        return stem.to_string();
    }
    s.to_string()
}

/// Internal helper used by [`drop_articles`]. Only matches the
/// space-suffix form (`"the "`, `"a "`, `"an "`); a bare leading article
/// (`"the"` with no trailing content) is preserved on purpose so that
/// `to_identity_match_form("The")` returns `"the"` rather than `""`.
///
/// The public, broader [`strip_leading_article`] (used by Python callers via
/// the PyO3 binding) does strip bare articles to the empty string. Keep these
/// two helpers separate so a Python parity expectation cannot inadvertently
/// retune the identity-match pipeline.
fn strip_leading_article_with_space_suffix(s: &str) -> Option<&str> {
    for article in ["the ", "a ", "an "] {
        if let Some(rest) = s.strip_prefix(article) {
            return Some(rest);
        }
    }
    None
}

/// Strip a leading article (`the`, `a`, `an`) from a lowercased + trimmed
/// string. Returns the input unchanged when there is no leading article.
///
/// The article must be followed by whitespace OR end-of-string, byte-for-byte
/// matching the Python `^(the|a|an)(\s+|$)` regex used by
/// `library-metadata-lookup`'s reconciler and FTS5 prefix-lookup paths
/// (E3 normalization charter; epic [WXYC/wxyc-etl#73]).
///
/// "Whitespace" here is the exact codepoint set CPython's `re` engine treats
/// as `\s` for `str` patterns: the Unicode `White_Space` property (matched by
/// [`char::is_whitespace`]) plus the four ASCII information separators
/// U+001C..=U+001F (FS, GS, RS, US — a CPython quirk that pre-dates Unicode
/// `White_Space` and survives in `Py_UNICODE_ISSPACE`). `to_match_form` does
/// not strip these — NBSP and the other Zs codepoints with NFKC
/// decompositions to ASCII space are folded away by step 2, but OGHAM SPACE
/// (U+1680), LINE / PARAGRAPH SEPARATOR (U+2028 / U+2029), and the info
/// separators round-trip unchanged and must hash identically on both sides.
///
/// Trailing whitespace after the article is consumed in full
/// (e.g. `"the  beatles"` → `"beatles"`), matching `\s+` greediness. Only the
/// first matching article is stripped; `"the the"` → `"the"`.
///
/// # Contract
///
/// - Input is assumed lowercased and trimmed (matches `to_match_form` output
///   or `str.lower()` on a pre-trimmed user-typed name). Casing is **not**
///   normalized inside; uppercased articles like `"The"` are a no-op.
/// - The article list is intentionally English-only. Plan §3.3.2 reserves
///   Romance-language articles (`le`, `la`, `los`, `las`) for a future
///   extension; when those land here, every consumer of the PyO3 binding
///   inherits them automatically.
///
/// # Relationship to `to_identity_match_form`
///
/// This function is the standalone, public counterpart to the step-5 logic
/// inside [`to_identity_match_form`]. It is **strictly more aggressive** for
/// bare-article inputs: `strip_leading_article("the")` → `""`, whereas
/// `to_identity_match_form("The")` → `"the"`. The latter deliberately
/// preserves bare articles to keep the identity-match fallback ladder from
/// collapsing a stem to empty.
///
/// # Examples
///
/// ```
/// use wxyc_etl::text::strip_leading_article;
///
/// assert_eq!(strip_leading_article("the beatles"), "beatles");
/// assert_eq!(strip_leading_article("a tribe called quest"), "tribe called quest");
/// assert_eq!(strip_leading_article("an albatross"), "albatross");
/// assert_eq!(strip_leading_article("the"), "");
/// assert_eq!(strip_leading_article("theater"), "theater");
/// assert_eq!(strip_leading_article("stereolab"), "stereolab");
/// ```
pub fn strip_leading_article(s: &str) -> &str {
    for article in ["the", "a", "an"] {
        if let Some(rest) = s.strip_prefix(article) {
            if rest.is_empty() {
                return rest;
            }
            if rest.starts_with(is_python_whitespace) {
                return rest.trim_start_matches(is_python_whitespace);
            }
        }
    }
    s
}

/// CPython's `\s` for `str` patterns: Unicode `White_Space` ∪ U+001C..=U+001F.
fn is_python_whitespace(c: char) -> bool {
    c.is_whitespace() || matches!(c, '\u{1C}'..='\u{1F}')
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

    // --- public strip_leading_article (PyO3-exposed; e3-normalization#133) ---

    #[test]
    fn pub_strip_drops_the_prefix() {
        assert_eq!(strip_leading_article("the beatles"), "beatles");
    }

    #[test]
    fn pub_strip_drops_a_prefix() {
        assert_eq!(
            strip_leading_article("a tribe called quest"),
            "tribe called quest"
        );
    }

    #[test]
    fn pub_strip_drops_an_prefix() {
        assert_eq!(strip_leading_article("an albatross"), "albatross");
    }

    #[test]
    fn pub_strip_drops_bare_the_to_empty() {
        // Bare article (EOS match) — diverges from drop_articles, which preserves
        // bare articles in identity-matching context.
        assert_eq!(strip_leading_article("the"), "");
    }

    #[test]
    fn pub_strip_drops_bare_a_to_empty() {
        assert_eq!(strip_leading_article("a"), "");
    }

    #[test]
    fn pub_strip_drops_bare_an_to_empty() {
        assert_eq!(strip_leading_article("an"), "");
    }

    #[test]
    fn pub_strip_no_op_on_prefix_substring() {
        // No word boundary after the article.
        assert_eq!(strip_leading_article("theater"), "theater");
        assert_eq!(
            strip_leading_article("thee silver mt zion"),
            "thee silver mt zion"
        );
        assert_eq!(strip_leading_article("animal"), "animal");
        assert_eq!(strip_leading_article("apple"), "apple");
    }

    #[test]
    fn pub_strip_no_op_on_no_article() {
        assert_eq!(strip_leading_article("stereolab"), "stereolab");
        assert_eq!(strip_leading_article("juana molina"), "juana molina");
    }

    #[test]
    fn pub_strip_no_op_on_empty() {
        assert_eq!(strip_leading_article(""), "");
    }

    #[test]
    fn pub_strip_consumes_multiple_spaces_after_article() {
        assert_eq!(strip_leading_article("the  beatles"), "beatles");
        assert_eq!(strip_leading_article("a\ttribe"), "tribe");
    }

    #[test]
    fn pub_strip_consumes_only_first_article() {
        // `the the` → strip one leading `the ` → `the`.
        assert_eq!(strip_leading_article("the the"), "the");
    }

    #[test]
    fn pub_strip_matches_python_re_whitespace() {
        // Mirrors CPython's `\s` for str patterns: White_Space ∪ U+001C..=U+001F.
        // NBSP folds away under NFKC inside `to_match_form` and never reaches
        // here from that path, but the standalone helper takes pre-normalized
        // input from other paths too — pin it here so the contract is explicit.
        assert_eq!(strip_leading_article("the\u{a0}beatles"), "beatles");
        assert_eq!(strip_leading_article("the\u{1680}beatles"), "beatles");
        assert_eq!(strip_leading_article("the\u{2028}beatles"), "beatles");
        assert_eq!(strip_leading_article("the\u{2029}beatles"), "beatles");
        // The four ASCII info separators are CPython's historical `\s` quirk.
        assert_eq!(strip_leading_article("the\u{1c}beatles"), "beatles");
        assert_eq!(strip_leading_article("the\u{1d}beatles"), "beatles");
        assert_eq!(strip_leading_article("the\u{1e}beatles"), "beatles");
        assert_eq!(strip_leading_article("the\u{1f}beatles"), "beatles");
    }

    #[test]
    fn pub_strip_consumes_mixed_whitespace_run() {
        // `\s+` greediness across heterogeneous codepoints.
        assert_eq!(
            strip_leading_article("the \u{a0}\t\u{1680}beatles"),
            "beatles"
        );
    }

    #[test]
    fn pub_strip_preserves_input_when_not_lowercased() {
        // Documented Python contract: input is assumed lowercased + trimmed.
        // The helper does not normalize; an uppercased article is a no-op.
        assert_eq!(strip_leading_article("The Beatles"), "The Beatles");
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

    // --- title variant: same as base today ---

    #[test]
    fn title_variant_matches_base() {
        for s in [
            "Stereolab",
            "The Sun Also Rises",
            "Foo (Live)",
            "Bar, The",
            "",
        ] {
            assert_eq!(
                to_identity_match_form_title(s),
                to_identity_match_form(s),
                "title variant diverged from base for {s:?}"
            );
        }
    }

    // --- step 6: punctuation collapse (with_punctuation variant) ---

    #[test]
    fn punctuation_collapses_dots() {
        assert_eq!(to_identity_match_form_with_punctuation("M.I.A."), "m i a");
    }

    #[test]
    fn punctuation_collapses_excitement() {
        assert_eq!(
            to_identity_match_form_with_punctuation("Godspeed You! Black Emperor"),
            "godspeed you black emperor"
        );
    }

    #[test]
    fn punctuation_collapses_run_of_punctuation_to_single_space() {
        // Per spec: replace each run of punctuation chars with one ASCII space.
        assert_eq!(to_identity_match_form_with_punctuation("!!!"), "");
        assert_eq!(to_identity_match_form_with_punctuation("+/-"), "");
        assert_eq!(to_identity_match_form_with_punctuation("R.E.M."), "r e m");
    }

    #[test]
    fn punctuation_preserves_letters_and_numbers() {
        assert_eq!(
            to_identity_match_form_with_punctuation("10,000 Maniacs"),
            "10 000 maniacs"
        );
    }

    #[test]
    fn punctuation_preserves_unicode_letters() {
        // Greek/Cyrillic/Han letters survive the punctuation pass.
        assert_eq!(
            to_identity_match_form_with_punctuation("Στελλάς"),
            "στελλασ"
        );
    }

    #[test]
    fn punctuation_runs_after_paren_strip() {
        // Trailing-paren strip happens first, so `(Live)` is gone before step 6
        // sees the input. The remaining punctuation is what gets collapsed.
        assert_eq!(
            to_identity_match_form_with_punctuation("Foo!Bar (Live)"),
            "foo bar"
        );
    }

    #[test]
    fn punctuation_after_article_drop() {
        assert_eq!(
            to_identity_match_form_with_punctuation("The M.I.A."),
            "m i a"
        );
    }

    #[test]
    fn punctuation_idempotent() {
        for s in [
            "M.I.A.",
            "10,000 Maniacs",
            "Godspeed You! Black Emperor",
            "Foo!Bar (Live)",
            "",
        ] {
            let once = to_identity_match_form_with_punctuation(s);
            let twice = to_identity_match_form_with_punctuation(&once);
            assert_eq!(once, twice, "idempotence broken for {s:?}");
        }
    }

    // --- step 8: trailing /N disambiguator strip ---

    #[test]
    fn disambiguator_strips_one_digit() {
        assert_eq!(
            to_identity_match_form_with_disambiguator_strip("John Smith /1"),
            "john smith"
        );
    }

    #[test]
    fn disambiguator_strips_multi_digit() {
        assert_eq!(
            to_identity_match_form_with_disambiguator_strip("Various /17"),
            "various"
        );
    }

    #[test]
    fn disambiguator_requires_leading_space_before_slash() {
        // `Track 1/12` has no whitespace before `/`; preserved.
        assert_eq!(
            to_identity_match_form_with_disambiguator_strip("Track 1/12"),
            "track 1/12"
        );
    }

    #[test]
    fn disambiguator_does_not_strip_trailing_letter() {
        assert_eq!(
            to_identity_match_form_with_disambiguator_strip("Side A/B"),
            "side a/b"
        );
    }

    #[test]
    fn disambiguator_strips_after_paren_and_article_drops() {
        // Pipeline order: to_match_form → parens → article → re-collapse → /N.
        assert_eq!(
            to_identity_match_form_with_disambiguator_strip("The John Smith /3 (1995)"),
            "john smith"
        );
    }

    #[test]
    fn disambiguator_no_match_passes_through() {
        assert_eq!(
            to_identity_match_form_with_disambiguator_strip("Stereolab"),
            "stereolab"
        );
    }

    #[test]
    fn disambiguator_idempotent() {
        for s in [
            "John Smith /1",
            "Various /17",
            "Track 1/12",
            "The John Smith /3 (1995)",
            "Stereolab",
            "",
        ] {
            let once = to_identity_match_form_with_disambiguator_strip(s);
            let twice = to_identity_match_form_with_disambiguator_strip(&once);
            assert_eq!(once, twice, "idempotence broken for {s:?}");
        }
    }
}
