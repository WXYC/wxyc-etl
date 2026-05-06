//! WX-2 normalizer charter: the three canonical forms.
//!
//! Charter design lives in `plans/mojibake-prevention/2-normalizer-charter.md`.
//! This module ships the public entry points; the heavy lifting (mojibake
//! repair, fold registry) lives in sibling modules.
//!
//! Currently implemented:
//! - [`to_storage_form`] (WX-2.2.1)
//! - [`to_match_form`] (WX-2.2.2)
//! - [`to_ascii_form`] (WX-2.2.3)

use deunicode::deunicode;
use unicode_categories::UnicodeCategories;
use unicode_normalization::UnicodeNormalization;

use super::folds::apply_folds;
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

/// Equivalence-class form for *comparison*. Caller normalizes both sides
/// of a comparison through this; the return value is **never stored**.
///
/// Pipeline:
/// 1. [`to_storage_form`] — equivalence is computed against canonical bytes.
/// 2. NFKC compatibility composition (folds half-width / full-width,
///    micro-sign U+00B5 → Greek µ, ligatures with compat decomposition).
/// 3. Default Unicode lowercasing (`char::to_lowercase`). Covers Σ → σ,
///    Ё → ё, É → é, etc. Eszett ß is preserved (lowercase identity);
///    if a future need arises, swap in the `caseless` crate for
///    Default Caseless TR#31 case-folding.
/// 4. Strip combining marks **selectively**: NFD-decompose, drop combining
///    marks only when the preceding base codepoint is in Latin or Greek
///    script, then re-NFC. Cyrillic Ё/Й-style script-essential
///    diaeresis / breve survive; Latin é / ñ and Greek ά become e / n / α.
/// 5. Apply WXYC fold registry (see [`super::folds`]): final-sigma ς → σ,
///    æ → ae, œ → oe.
/// 6. Strip Cf format characters **except** U+200D ZWJ (preserved so emoji
///    sequences like 👨‍👩‍👧‍👦 stay intact). LRM/RLM/RLO/PDF and
///    isolate marks all go.
/// 7. Collapse runs of ASCII space (U+0020) to a single space; trim leading
///    and trailing ASCII space. Other whitespace (TAB, etc.) is preserved
///    for round-trip fidelity to the original input.
///
/// Cross-script transliteration (Σ → S, я → ya) is **not** performed here;
/// that is `to_ascii_form`'s job.
///
/// Idempotent: `to_match_form(to_match_form(s)) == to_match_form(s)`.
pub fn to_match_form(s: &str) -> String {
    let storage = to_storage_form(s);
    let nfkc: String = storage.nfkc().collect();
    let lower: String = nfkc.chars().flat_map(char::to_lowercase).collect();
    let stripped = strip_combining_selective(&lower);
    let folded = apply_folds(&stripped);
    let cf_stripped = strip_cf_except_zwj(&folded);
    collapse_and_trim_ascii_space(&cf_stripped)
}

/// Last-resort ASCII reduction. Use only when [`to_match_form`] produced no
/// candidates and the caller wants a maximally lenient cross-script search.
///
/// Pipeline:
/// 1. [`to_match_form`] (gives canonical, NFKC, lowercase, fold-applied input).
/// 2. Strip Symbol-Other codepoints (`So`) — emoji, music symbols, dingbats —
///    *before* transliteration so 🎸 disappears instead of becoming "guitar".
/// 3. WXYC Cyrillic Ё/ё override: deunicode renders these as "Io"/"io" but the
///    common English transliteration is "Yo"/"yo" (Yot, jot). Pre-substitute.
/// 4. [`deunicode::deunicode`]: Latin transliteration via a ~100 KB compiled-in
///    table (Greek Σ → S, Cyrillic я → ya, ц → ts, CJK 繭 → Mao).
/// 5. Lowercase the deunicode output (transliteration restores case).
/// 6. Strip non-ASCII residue (anything deunicode could not transliterate).
/// 7. Collapse runs of ASCII space; trim. TAB and other ASCII control bytes
///    are preserved (same rule as [`to_match_form`]).
///
/// **Lossy in both directions.** `to_ascii_form("繭")` (kanji "cocoon") and
/// `to_ascii_form("Mao")` (the romanization of a person's name) collide on
/// `"mao"`. Search relevance must rank `to_match_form` matches above
/// `to_ascii_form` matches.
pub fn to_ascii_form(s: &str) -> String {
    let matched = to_match_form(s);
    let pre: String = matched
        .chars()
        .filter(|c| !c.is_symbol_other())
        .flat_map(|c| match c {
            '\u{0401}' => "Yo".chars().collect::<Vec<_>>(),
            '\u{0451}' => "yo".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect();
    let translit = deunicode(&pre);
    let lowered: String = translit.chars().flat_map(char::to_lowercase).collect();
    let ascii: String = lowered.chars().filter(|c| c.is_ascii()).collect();
    collapse_and_trim_ascii_space(&ascii)
}

/// NFD-decompose, drop combining marks whose base is Latin or Greek, then
/// re-NFC. Bases in scripts where a diacritic carries lexical weight
/// (Cyrillic Ё/Й, etc.) keep their marks.
fn strip_combining_selective(s: &str) -> String {
    let mut buf = String::with_capacity(s.len());
    let mut prev_base: Option<char> = None;
    for c in s.nfd() {
        if c.is_mark() {
            if matches!(prev_base, Some(b) if is_diacritic_decoration_script(b)) {
                continue;
            }
            buf.push(c);
        } else {
            buf.push(c);
            prev_base = Some(c);
        }
    }
    buf.nfc().collect()
}

/// Scripts where a combining diacritic on a base letter is decoration, not
/// a distinct letter — safe to strip during match-form folding. Latin
/// (incl. all Latin Extended blocks) and Greek (incl. Greek Extended).
///
/// Cyrillic, Hebrew, Arabic, CJK, and supplementary-plane scripts are
/// excluded: their combining marks (Cyrillic diaeresis on Е → Ё; Hebrew
/// niqqud; Arabic tashkil) carry phonemic weight.
fn is_diacritic_decoration_script(c: char) -> bool {
    let cp = c as u32;
    cp <= 0x024F                          // Basic Latin..Latin Extended-B
        || (0x1E00..=0x1EFF).contains(&cp) // Latin Extended Additional
        || (0x2C60..=0x2C7F).contains(&cp) // Latin Extended-C
        || (0xA720..=0xA7FF).contains(&cp) // Latin Extended-D
        || (0xAB30..=0xAB6F).contains(&cp) // Latin Extended-E
        || (0x0370..=0x03FF).contains(&cp) // Greek and Coptic
        || (0x1F00..=0x1FFF).contains(&cp) // Greek Extended
}

/// Drop Unicode Cf (format) characters, except U+200D ZWJ which carries
/// emoji-sequence semantics.
fn strip_cf_except_zwj(s: &str) -> String {
    if !s.chars().any(|c| c != '\u{200D}' && c.is_other_format()) {
        return s.to_string();
    }
    s.chars()
        .filter(|&c| c == '\u{200D}' || !c.is_other_format())
        .collect()
}

/// Collapse runs of ASCII space (U+0020) to a single space; trim leading
/// and trailing ASCII space. Other whitespace (TAB U+0009, etc.) is left
/// alone — `tab\there` remains `tab\there` because TSV column-boundary
/// hazards are intentional probes, not noise to scrub.
fn collapse_and_trim_ascii_space(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = true;
    for c in s.chars() {
        if c == ' ' {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
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

    // --- to_match_form ---

    #[test]
    fn match_form_lowercases_and_strips_latin_diacritics() {
        assert_eq!(to_match_form("Hermanos Gutiérrez"), "hermanos gutierrez");
        assert_eq!(to_match_form("NILÜFER YANYA"), "nilufer yanya");
    }

    #[test]
    fn match_form_folds_greek_capital_sigma_to_medial() {
        assert_eq!(to_match_form("\u{03A3}tella"), "\u{03C3}tella");
    }

    #[test]
    fn match_form_folds_greek_final_sigma_to_medial() {
        // ς and σ must collide on the same bucket (LML#168).
        assert_eq!(to_match_form("Στελλάς"), to_match_form("Στελλάσ"));
        assert_eq!(to_match_form("Στελλάς"), "στελλασ");
    }

    #[test]
    fn match_form_preserves_cyrillic_yo_diaeresis() {
        // Cyrillic Ё must NOT decompose+strip to "е" — script-essential mark.
        assert_eq!(to_match_form("\u{0401}"), "\u{0451}");
    }

    #[test]
    fn match_form_strips_bidi_format_chars() {
        assert_eq!(to_match_form("Hello\u{200E}World"), "helloworld");
        assert_eq!(to_match_form("Hello\u{200F}World"), "helloworld");
        assert_eq!(to_match_form("\u{202E}Reversed\u{202C}"), "reversed");
    }

    #[test]
    fn match_form_preserves_zwj_in_emoji_sequence() {
        // U+200D ZWJ is the only Cf char that survives — emoji integrity.
        let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}\u{200D}\u{1F466}";
        assert_eq!(to_match_form(family), family);
    }

    #[test]
    fn match_form_repairs_mojibake_then_folds() {
        // to_storage_form runs first, so mojibake is fixed before folding.
        assert_eq!(to_match_form("Î£tella"), "\u{03C3}tella");
        assert_eq!(to_match_form("Ã©"), "e");
        assert_eq!(to_match_form("â€™"), "\u{2019}");
    }

    #[test]
    fn match_form_preserves_polish_l_and_norwegian_o() {
        // Non-folds: ł and ø stay (they are letters, not decorated bases).
        assert_eq!(to_match_form("\u{0141}ukasz"), "\u{0142}ukasz");
        assert_eq!(to_match_form("\u{00D8}ster"), "\u{00F8}ster");
    }

    #[test]
    fn match_form_preserves_turkish_dotless_i() {
        assert_eq!(to_match_form("Aşıq Altay"), "asıq altay");
    }

    #[test]
    fn match_form_preserves_cjk_emoji_arabic() {
        assert_eq!(to_match_form("細野晴臣"), "細野晴臣");
        assert_eq!(to_match_form("🎵"), "🎵");
        assert_eq!(to_match_form("فيروز"), "فيروز");
    }

    #[test]
    fn match_form_collapses_runs_of_ascii_space() {
        assert_eq!(to_match_form("  Molchat   Doma  "), "molchat doma");
    }

    #[test]
    fn match_form_preserves_embedded_tab() {
        // TAB is intentional probe data, not whitespace to scrub.
        assert_eq!(to_match_form("tab\there"), "tab\there");
    }

    #[test]
    fn match_form_idempotent() {
        for s in [
            "Stereolab",
            "Hermanos Gutiérrez",
            "Στελλάς",
            "細野晴臣",
            "🎵",
            "Î£tella",
            "Hello\u{200E}World",
            "\u{0401}", // Cyrillic Ё
            "\u{1F468}\u{200D}\u{1F469}",
        ] {
            assert_eq!(
                to_match_form(&to_match_form(s)),
                to_match_form(s),
                "idempotence broken for {s:?}"
            );
        }
    }

    #[test]
    fn match_form_empty() {
        assert_eq!(to_match_form(""), "");
    }

    // --- to_ascii_form ---

    #[test]
    fn ascii_form_lml_168_sigma_to_s() {
        // The motivating case: "Στella" reduces to "stella" so a typed
        // "Stella" can find the canonical entry through the ASCII fallback.
        assert_eq!(to_ascii_form("\u{03A3}tella"), "stella");
        assert_eq!(to_ascii_form("Στελλάς"), "stellas");
    }

    #[test]
    fn ascii_form_cyrillic_yo_overrides_deunicode_io() {
        // deunicode renders Ё/ё as "Io"/"io"; WXYC convention is "Yo"/"yo".
        assert_eq!(to_ascii_form("\u{0401}"), "yo");
        assert_eq!(to_ascii_form("\u{0451}"), "yo");
    }

    #[test]
    fn ascii_form_strips_emoji_instead_of_describing_them() {
        // deunicode would render 🎸 as "guitar"; we strip Symbol-Other first.
        assert_eq!(to_ascii_form("Stereolab \u{1F3B8}"), "stereolab");
        assert_eq!(to_ascii_form("\u{1F3B5}"), "");
    }

    #[test]
    fn ascii_form_transliterates_cyrillic() {
        assert_eq!(to_ascii_form("Молчат Дома"), "molchat doma");
        assert_eq!(to_ascii_form("Аукцыон"), "auktsyon");
    }

    #[test]
    fn ascii_form_strips_latin_diacritics_via_match_form() {
        assert_eq!(to_ascii_form("Hermanos Gutiérrez"), "hermanos gutierrez");
        assert_eq!(to_ascii_form("Sigur Rós"), "sigur ros");
    }

    #[test]
    fn ascii_form_repairs_mojibake_then_transliterates() {
        // Mojibake is fixed by the to_storage_form leg of to_match_form.
        assert_eq!(to_ascii_form("Î£tella"), "stella");
        assert_eq!(to_ascii_form("Ã©"), "e");
    }

    #[test]
    fn ascii_form_preserves_embedded_tab() {
        // Same rule as to_match_form: TAB is intentional probe data.
        assert_eq!(to_ascii_form("tab\there"), "tab\there");
    }

    #[test]
    fn ascii_form_idempotent() {
        for s in [
            "Stereolab",
            "Hermanos Gutiérrez",
            "Στελλάς",
            "Молчат Дома",
            "Î£tella",
            "Stereolab \u{1F3B8}",
            "\u{0401}",
        ] {
            assert_eq!(
                to_ascii_form(&to_ascii_form(s)),
                to_ascii_form(s),
                "idempotence broken for {s:?}"
            );
        }
    }

    #[test]
    fn ascii_form_empty() {
        assert_eq!(to_ascii_form(""), "");
    }
}
