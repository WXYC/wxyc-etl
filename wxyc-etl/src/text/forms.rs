//! WX-2 normalizer charter: the three canonical forms.
//!
//! Charter design lives in `plans/mojibake-prevention/2-normalizer-charter.md`.
//! This module ships the public entry points; the heavy lifting (mojibake
//! repair, fold registry) lives in sibling modules.
//!
//! Currently implemented:
//! - [`to_storage_form`] (WX-2.2.1)
//!
//! Planned:
//! - `to_match_form` (WX-2.2.2)
//! - `to_ascii_form` (WX-2.2.3)

use unicode_normalization::UnicodeNormalization;

use super::mojibake::fix_mojibake;

/// Canonical bytes to *store*. Caller should write the return value to
/// the catalog/database/whatever durable surface they own.
///
/// Pipeline: ftfy-style mojibake repair, then NFC normalization, then
/// trim leading/trailing ASCII spaces. Preserves case, scripts,
/// diacritics, and internal whitespace.
///
/// NFC is included because two byte forms of the same visual string
/// (precomposed `é` vs decomposed `e\u{301}`) would otherwise both end
/// up in the catalog as byte-distinct rows. Catalog deduplication is
/// the load-bearing reason; `to_match_form`'s NFKC fold can canonicalize
/// at compare time but cannot undo the duplicate write.
///
/// Idempotent: `to_storage_form(to_storage_form(s)) == to_storage_form(s)`.
pub fn to_storage_form(s: &str) -> String {
    let fixed = fix_mojibake(s);
    let nfc: String = fixed.nfc().collect();
    let trimmed = nfc.trim_matches(' ');
    if trimmed.len() == nfc.len() {
        nfc
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotent_on_clean_input() {
        for s in ["Stereolab", "Nilüfer Yanya", "Στελλάς", "細野晴臣", "🎵"] {
            assert_eq!(
                to_storage_form(&to_storage_form(s)),
                to_storage_form(s),
                "idempotence broken for {s:?}"
            );
        }
    }

    #[test]
    fn idempotent_on_mojibake_input() {
        for s in ["Ã©", "Î£tella", "â€™"] {
            assert_eq!(to_storage_form(&to_storage_form(s)), to_storage_form(s));
        }
    }

    #[test]
    fn trims_ascii_whitespace() {
        assert_eq!(to_storage_form("  Stereolab  "), "Stereolab");
    }

    #[test]
    fn preserves_internal_whitespace() {
        assert_eq!(to_storage_form("Molchat  Doma"), "Molchat  Doma");
    }

    #[test]
    fn nfc_input_is_passthrough() {
        // Already-NFC café (U+00E9). Storage form is a no-op.
        let nfc = "caf\u{00E9}";
        assert_eq!(to_storage_form(nfc), nfc);
    }

    #[test]
    fn nfd_input_canonicalizes_to_nfc() {
        // Decomposed café (U+0065 U+0301) collapses to precomposed (U+00E9)
        // so two byte forms of the same visual string don't both land in the
        // catalog as separate rows.
        let nfd = "cafe\u{0301}";
        let nfc = "caf\u{00E9}";
        assert_eq!(to_storage_form(nfd), nfc);
    }

    #[test]
    fn nfd_n_tilde_canonicalizes_to_nfc() {
        let nfd = "n\u{0303}";
        let nfc = "\u{00F1}";
        assert_eq!(to_storage_form(nfd), nfc);
    }
}
