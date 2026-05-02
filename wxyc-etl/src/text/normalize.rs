//! Artist and title name normalization.
//!
//! Normalizes names using NFKD decomposition with combining character
//! removal, derived from `filter_csv.py:normalize_artist()` in the
//! discogs-cache repo. Diverges in one place: this module also folds the
//! Greek lowercase final-form sigma (U+03C2 ς) to the medial-form sigma
//! (U+03C3 σ) — see `fold_sigma`. The Python implementation does not.

use unicode_categories::UnicodeCategories;
use unicode_normalization::UnicodeNormalization;

/// Normalize an artist name for matching.
///
/// Applies NFKD decomposition, strips combining characters (diacritics),
/// lowercases, folds the Greek final-form sigma (U+03C2 ς → U+03C3 σ),
/// and trims whitespace. The first three steps match this Python:
///
/// ```python
/// nfkd = unicodedata.normalize("NFKD", name)
/// stripped = "".join(c for c in nfkd if not unicodedata.combining(c))
/// return stripped.lower().strip()
/// ```
///
/// The sigma fold is the WXYC/library-metadata-lookup#168 warm-up — see
/// `fold_sigma`.
pub fn normalize_artist_name(name: &str) -> String {
    let result = strip_diacritics_and_lowercase(name);
    let trimmed = result.trim_matches(' ');
    if trimmed.len() == result.len() {
        result
    } else {
        trimmed.to_string()
    }
}

/// Strip diacritics via NFKD decomposition without lowercasing or trimming.
///
/// Useful for title normalization contexts where case preservation is needed.
///
/// Also folds the Greek lowercase final-form sigma (U+03C2 ς) to the
/// medial-form sigma (U+03C3 σ) so positional variants of the same letter
/// hash to the same bucket. Capital sigma (U+03A3 Σ) is preserved here —
/// it is folded only by the lowercasing path in `normalize_artist_name`.
pub fn strip_diacritics(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.nfkd() {
        if !c.is_mark() {
            result.push(fold_sigma(c));
        }
    }
    result
}

/// Normalize a title for matching.
///
/// Uses the same NFKD + combining-character-removal + lowercase + trim as
/// `normalize_artist_name`.
pub fn normalize_title(title: &str) -> String {
    normalize_artist_name(title)
}

/// NFKD decompose, remove combining marks, and lowercase in a single pass.
/// Also folds the Greek lowercase final-form sigma (U+03C2 ς) to the
/// medial-form sigma (U+03C3 σ) — see `fold_sigma`.
fn strip_diacritics_and_lowercase(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.nfkd() {
        if !c.is_mark() {
            for lc in c.to_lowercase() {
                result.push(fold_sigma(lc));
            }
        }
    }
    result
}

/// Fold the Greek lowercase final-form sigma (U+03C2 ς) to the medial-form
/// sigma (U+03C3 σ). These are positional variants of the same letter and
/// must hash to the same normalized bucket; default Unicode case mapping
/// does not collapse them.
///
/// This is the WX-2 "warm-up" — it patches the existing normalizer ahead of
/// the broader normalizer-charter migration in WXYC/docs#16, where this fold
/// will move into `to_match_form`. See WXYC/library-metadata-lookup#168.
#[inline]
fn fold_sigma(c: char) -> char {
    if c == '\u{03C2}' {
        '\u{03C3}'
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- normalize_artist_name: 11 parity tests from discogs-xml-converter/src/filter.rs ---

    #[test]
    fn test_normalize_lowercase() {
        assert_eq!(normalize_artist_name("Stereolab"), "stereolab");
    }

    #[test]
    fn test_normalize_strip_spaces() {
        assert_eq!(normalize_artist_name("  Stereolab  "), "stereolab");
    }

    #[test]
    fn test_normalize_all_caps() {
        assert_eq!(normalize_artist_name("STEREOLAB"), "stereolab");
    }

    #[test]
    fn test_normalize_mixed_case_strip() {
        assert_eq!(normalize_artist_name("  Mixed Case  "), "mixed case");
    }

    #[test]
    fn test_normalize_empty() {
        assert_eq!(normalize_artist_name(""), "");
    }

    #[test]
    fn test_normalize_nilufer_yanya() {
        assert_eq!(normalize_artist_name("Nilüfer Yanya"), "nilufer yanya");
    }

    #[test]
    fn test_normalize_csillagrablok() {
        assert_eq!(normalize_artist_name("Csillagrablók"), "csillagrablok");
    }

    #[test]
    fn test_normalize_motorhead() {
        assert_eq!(normalize_artist_name("Motörhead"), "motorhead");
    }

    #[test]
    fn test_normalize_husker_du() {
        assert_eq!(normalize_artist_name("Hüsker Dü"), "husker du");
    }

    #[test]
    fn test_normalize_hermanos_gutierrez() {
        assert_eq!(
            normalize_artist_name("Hermanos Gutiérrez"),
            "hermanos gutierrez"
        );
    }

    #[test]
    fn test_normalize_zoe() {
        assert_eq!(normalize_artist_name("Zoé"), "zoe");
    }

    // --- trim optimization paths ---

    #[test]
    fn test_normalize_trim_no_extra_alloc() {
        assert_eq!(normalize_artist_name("Stereolab"), "stereolab");
    }

    #[test]
    fn test_normalize_trim_leading_space() {
        assert_eq!(normalize_artist_name("  Nilüfer Yanya"), "nilufer yanya");
    }

    #[test]
    fn test_normalize_trim_trailing_space() {
        assert_eq!(
            normalize_artist_name("Hermanos Gutiérrez  "),
            "hermanos gutierrez"
        );
    }

    // --- strip_diacritics: preserves case and whitespace ---

    #[test]
    fn test_strip_diacritics_preserves_case() {
        assert_eq!(strip_diacritics("Nilüfer Yanya"), "Nilufer Yanya");
    }

    #[test]
    fn test_strip_diacritics_preserves_whitespace() {
        assert_eq!(
            strip_diacritics("  Hermanos Gutiérrez  "),
            "  Hermanos Gutierrez  "
        );
    }

    #[test]
    fn test_strip_diacritics_no_change() {
        assert_eq!(strip_diacritics("Stereolab"), "Stereolab");
    }

    #[test]
    fn test_strip_diacritics_empty() {
        assert_eq!(strip_diacritics(""), "");
    }

    // --- normalize_title delegates to same logic ---

    #[test]
    fn test_normalize_title_lowercase_and_diacritics() {
        assert_eq!(normalize_title("Hermanos Gutiérrez"), "hermanos gutierrez");
        assert_eq!(normalize_title("  Sugar Hill  "), "sugar hill");
        assert_eq!(normalize_title("Aluminum Tunes"), "aluminum tunes");
    }

    // --- Greek sigma fold (WXYC/library-metadata-lookup#168) ---
    // Final-form sigma U+03C2 (ς) and medial-form sigma U+03C3 (σ) are
    // positional variants of the same letter and must hash to the same
    // normalized bucket.

    #[test]
    fn test_normalize_folds_final_sigma_to_medial() {
        assert_eq!(normalize_artist_name("\u{03C2}"), "\u{03C3}");
    }

    #[test]
    fn test_normalize_capital_sigma_lowercases_to_medial() {
        assert_eq!(normalize_artist_name("\u{03A3}"), "\u{03C3}");
    }

    #[test]
    fn test_normalize_greek_word_sigma_variants_collide() {
        let final_form = "Στελλάς";
        let medial_form = "Στελλάσ";
        assert_eq!(
            normalize_artist_name(final_form),
            normalize_artist_name(medial_form)
        );
    }

    #[test]
    fn test_strip_diacritics_folds_final_sigma_to_medial() {
        assert_eq!(strip_diacritics("\u{03C2}"), "\u{03C3}");
    }

    #[test]
    fn test_strip_diacritics_preserves_capital_sigma() {
        assert_eq!(strip_diacritics("\u{03A3}"), "\u{03A3}");
    }
}
