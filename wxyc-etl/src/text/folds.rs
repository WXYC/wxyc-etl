//! WXYC fold registry — script-aware folds that go beyond what default
//! Unicode case-mapping covers.
//!
//! Charter design: WX-2.1.2 in `plans/mojibake-prevention/2-normalizer-charter.md`.
//! Each entry here is a deliberate, human-reviewed choice; this is the only
//! place in the normalizer where casual cultural decisions live.
//!
//! ## Folds
//!
//! - **U+03C2 ς → U+03C3 σ** (Greek final sigma → medial sigma). Positional
//!   variants of the same letter; default Unicode lowercasing maps capital
//!   Σ → σ but does not collapse ς into σ. Without this, `Στελλάς` and
//!   `Στελλάσ` hash to different match-form buckets.
//! - **U+00E6 æ → "ae"** (Latin small ligature). Treated as the two-letter
//!   sequence its Latin-script readers expect to type. Norwegian / Danish /
//!   Old English usage.
//! - **U+0153 œ → "oe"** (Latin small ligature). Same rationale as æ.
//!   French / Old English usage.
//!
//! ## Non-folds (intentionally preserved)
//!
//! - **U+0142 ł, U+0141 Ł** (Polish). Not a diacritic on a Latin base —
//!   the bar through the L is part of the letter's identity.
//! - **U+00F8 ø, U+00D8 Ø** (Norwegian / Danish). Same: the slash is
//!   part of the letter, not decoration on `o`.
//! - **U+0131 ı, U+0130 İ** (Turkish dotless / dotted i). Distinct letters
//!   in Turkish phonology; folding ı→i would mis-match Turkish names.
//!
//! These non-folds are what callers see. ASCII transliteration that
//! collapses them (ł → l, ø → o, ı → i) belongs in `to_ascii_form`, not here.

/// Apply the WXYC fold registry to `s`. See module docs for the list.
///
/// Pre-scans for trigger codepoints so the common case (no fold needed)
/// returns the input unchanged without an extra allocation.
pub fn apply_folds(s: &str) -> String {
    if !s.chars().any(is_fold_trigger) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{03C2}' => out.push('\u{03C3}'),
            '\u{00E6}' => out.push_str("ae"),
            '\u{0153}' => out.push_str("oe"),
            other => out.push(other),
        }
    }
    out
}

#[inline]
fn is_fold_trigger(c: char) -> bool {
    matches!(c, '\u{03C2}' | '\u{00E6}' | '\u{0153}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_final_sigma_to_medial() {
        assert_eq!(apply_folds("\u{03C2}"), "\u{03C3}");
    }

    #[test]
    fn folds_ae_ligature_to_ae() {
        assert_eq!(apply_folds("\u{00E6}"), "ae");
        assert_eq!(apply_folds("encyclop\u{00E6}dia"), "encyclopaedia");
    }

    #[test]
    fn folds_oe_ligature_to_oe() {
        assert_eq!(apply_folds("\u{0153}"), "oe");
        assert_eq!(apply_folds("c\u{0153}ur"), "coeur");
    }

    #[test]
    fn preserves_polish_l_with_stroke() {
        // Non-fold: ł stays ł.
        assert_eq!(apply_folds("\u{0142}ukasz"), "\u{0142}ukasz");
    }

    #[test]
    fn preserves_norwegian_o_with_stroke() {
        assert_eq!(apply_folds("\u{00F8}ster"), "\u{00F8}ster");
    }

    #[test]
    fn preserves_turkish_dotless_i() {
        assert_eq!(apply_folds("a\u{0131}"), "a\u{0131}");
    }

    #[test]
    fn passes_through_when_no_trigger() {
        // Fast path: no allocation expected, but we only assert equality.
        assert_eq!(apply_folds("stereolab"), "stereolab");
        assert_eq!(apply_folds(""), "");
        assert_eq!(
            apply_folds("\u{03C3}\u{03C4}\u{03B5}\u{03BB}\u{03BB}\u{03B1}"),
            "\u{03C3}\u{03C4}\u{03B5}\u{03BB}\u{03BB}\u{03B1}"
        );
    }

    #[test]
    fn passes_through_medial_sigma() {
        // Medial sigma is the canonical bucket; no trigger.
        assert_eq!(apply_folds("\u{03C3}"), "\u{03C3}");
    }
}
