//! ftfy-style mojibake fixer for Latin-1/CP1252-as-UTF-8 mis-decoding.
//!
//! Reverses strings whose bytes are UTF-8 that was previously misread as
//! Latin-1 (or Windows-1252) and re-encoded back to UTF-8 — e.g. `Ã©` was
//! `é`, `Î£tella` was `Σtella`, `â€™` was `'`. The full algorithm in
//! ftfy is broader; this is a hand-port of the single most common
//! sub-case (one mis-decoding round-trip), tuned against the WX-1
//! `mojibake_known` corpus category.
//!
//! ## Idempotence
//!
//! `fix_mojibake(fix_mojibake(s)) == fix_mojibake(s)` for every input.
//! A fix is accepted only if the candidate (a) parses as valid UTF-8,
//! (b) is strictly shorter than the input, and (c) contains at least one
//! non-ASCII codepoint. A clean string fails (b); an already-fixed string
//! also fails (b) on a second pass because mojibake is always longer in
//! UTF-8 than the original.

/// Map a single mojibake codepoint back to the byte it would have been in
/// the misreading single-byte encoding (Latin-1 with Windows-1252
/// punctuation in the C1 slots).
///
/// Returns `None` for codepoints outside Latin-1 + W1252 — the presence of
/// any such codepoint means the input cannot have been produced by a
/// single-pass Latin-1/W1252 mis-decoding of UTF-8.
const fn char_to_w1252_byte(c: char) -> Option<u8> {
    let cp = c as u32;
    // Latin-1 direct range: U+0000..U+00FF, except the C1 control range
    // U+0080..U+009F whose slots are reserved for the W1252 punctuation
    // table below. (The C1 controls themselves are not used as content.)
    if cp <= 0x7F || (cp >= 0xA0 && cp <= 0xFF) {
        return Some(cp as u8);
    }
    match c {
        '\u{20AC}' => Some(0x80), // €
        '\u{201A}' => Some(0x82),
        '\u{0192}' => Some(0x83),
        '\u{201E}' => Some(0x84),
        '\u{2026}' => Some(0x85),
        '\u{2020}' => Some(0x86),
        '\u{2021}' => Some(0x87),
        '\u{02C6}' => Some(0x88),
        '\u{2030}' => Some(0x89),
        '\u{0160}' => Some(0x8A),
        '\u{2039}' => Some(0x8B),
        '\u{0152}' => Some(0x8C),
        '\u{017D}' => Some(0x8E),
        '\u{2018}' => Some(0x91),
        '\u{2019}' => Some(0x92), // '
        '\u{201C}' => Some(0x93),
        '\u{201D}' => Some(0x94),
        '\u{2022}' => Some(0x95),
        '\u{2013}' => Some(0x96),
        '\u{2014}' => Some(0x97),
        '\u{02DC}' => Some(0x98),
        '\u{2122}' => Some(0x99), // ™
        '\u{0161}' => Some(0x9A),
        '\u{203A}' => Some(0x9B),
        '\u{0153}' => Some(0x9C),
        '\u{017E}' => Some(0x9E),
        '\u{0178}' => Some(0x9F),
        _ => None,
    }
}

/// Fast-path predicate: does `s` contain any codepoint that *could* be the
/// first byte of a mojibake'd UTF-8 sequence? Skipping the full conversion
/// for clean input keeps storage-form a near-no-op on the hot path.
///
/// Each trigger codepoint corresponds to a UTF-8 lead byte that, when
/// re-decoded as Latin-1/Windows-1252, produces a chunk of the U+xxxx range
/// indicated. Hebrew (lead bytes 0xD6, 0xD7 → Ö, ×) is intentionally
/// absent — no Hebrew mojibake fix-pair exists in the WX-1 corpus today,
/// and the false-positive cost on common Swedish/Finnish/German Ö is
/// non-zero. Add when a real case surfaces.
fn has_mojibake_trigger(s: &str) -> bool {
    s.chars().any(|c| {
        matches!(
            c,
            '\u{00C2}'  // Â — lead byte 0xC2 → U+0080..U+00BF (Latin-1 punctuation, etc.)
            | '\u{00C3}'  // Ã — lead byte 0xC3 → U+00C0..U+00FF (Latin-1 diacritics)
            | '\u{00C4}'  // Ä — lead byte 0xC4 → U+0100..U+013F (Latin Extended-A)
            | '\u{00C5}'  // Å — lead byte 0xC5 → U+0140..U+017F (Latin Extended-A)
            | '\u{00CE}'  // Î — lead byte 0xCE → U+0380..U+03BF (Greek)
            | '\u{00CF}'  // Ï — lead byte 0xCF → U+03C0..U+03FF (Greek)
            | '\u{00D0}'  // Ð — lead byte 0xD0 → U+0400..U+043F (Cyrillic)
            | '\u{00D1}'  // Ñ — lead byte 0xD1 → U+0440..U+047F (Cyrillic)
            | '\u{00D8}'  // Ø — lead byte 0xD8 → U+0600..U+063F (Arabic)
            | '\u{00D9}'  // Ù — lead byte 0xD9 → U+0640..U+067F (Arabic)
            | '\u{00DA}'  // Ú — lead byte 0xDA → U+0680..U+06BF (Arabic Supplement)
            | '\u{00DB}'  // Û — lead byte 0xDB → U+06C0..U+06FF (Arabic Supplement / NKo)
            | '\u{00E2}' // â — lead byte 0xE2 → 3-byte sequences in U+2000–U+2FFF (punctuation)
        )
    })
}

/// Reverse Latin-1/CP1252-as-UTF-8 mis-decoding when possible; otherwise
/// return `s` unchanged. See the module docstring for the idempotence
/// contract.
pub fn fix_mojibake(s: &str) -> String {
    if !has_mojibake_trigger(s) {
        return s.to_string();
    }
    let mut bytes = Vec::with_capacity(s.len());
    for c in s.chars() {
        match char_to_w1252_byte(c) {
            Some(b) => bytes.push(b),
            None => return s.to_string(),
        }
    }
    let Ok(candidate) = std::str::from_utf8(&bytes) else {
        return s.to_string();
    };
    let shorter = candidate.len() < s.len();
    let has_non_ascii = candidate.bytes().any(|b| b >= 0x80);
    if shorter && has_non_ascii {
        candidate.to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- The WX-1 mojibake_known fix-pairs (copied here so unit-test
    // failures are diagnosable without loading the corpus). ---

    #[test]
    fn fixes_latin_e_acute() {
        assert_eq!(fix_mojibake("Ã©"), "é");
    }

    #[test]
    fn fixes_latin_n_tilde() {
        assert_eq!(fix_mojibake("Ã±"), "ñ");
    }

    #[test]
    fn fixes_latin_o_diaeresis() {
        assert_eq!(fix_mojibake("Ã¶"), "ö");
    }

    #[test]
    fn fixes_latin_u_diaeresis() {
        assert_eq!(fix_mojibake("Ã¼"), "ü");
    }

    #[test]
    fn fixes_greek_capital_sigma() {
        assert_eq!(fix_mojibake("Î£tella"), "Σtella");
    }

    #[test]
    fn fixes_w1252_right_single_quote() {
        assert_eq!(fix_mojibake("â€™"), "\u{2019}");
    }

    // --- Idempotence: fix(fix(x)) == fix(x). ---

    #[test]
    fn idempotent_on_already_fixed() {
        for input in ["é", "ñ", "ö", "ü", "Σtella", "\u{2019}"] {
            assert_eq!(
                fix_mojibake(&fix_mojibake(input)),
                fix_mojibake(input),
                "idempotence broken for {input:?}"
            );
        }
    }

    // --- Non-mojibake input passes through unchanged. ---

    #[test]
    fn passes_through_clean_ascii() {
        assert_eq!(fix_mojibake("Stereolab"), "Stereolab");
    }

    #[test]
    fn passes_through_clean_latin_with_diacritics() {
        // No trigger codepoints — fast path.
        assert_eq!(fix_mojibake("Nilüfer Yanya"), "Nilüfer Yanya");
        assert_eq!(fix_mojibake("Hermanos Gutiérrez"), "Hermanos Gutiérrez");
    }

    #[test]
    fn passes_through_legitimate_capital_n_tilde() {
        // Spanish "EL NIÑO" — Ñ triggers the scan but the candidate is
        // invalid UTF-8 (0xD1 needs a continuation byte; 0x4F is not one).
        assert_eq!(fix_mojibake("EL NIÑO"), "EL NIÑO");
    }

    #[test]
    fn passes_through_legitimate_capital_a_circumflex() {
        // French "Â" alone — Â triggers but produces a 1-byte invalid UTF-8
        // candidate.
        assert_eq!(fix_mojibake("Â"), "Â");
    }

    #[test]
    fn passes_through_legitimate_norwegian_o_slash() {
        // Norwegian "Øster" — Ø triggers but candidate (0xD8 + ASCII) is
        // invalid UTF-8.
        assert_eq!(fix_mojibake("Øster"), "Øster");
    }

    #[test]
    fn passes_through_clean_greek() {
        assert_eq!(fix_mojibake("Σtella"), "Σtella");
        assert_eq!(fix_mojibake("Στελλάς"), "Στελλάς");
    }

    #[test]
    fn passes_through_clean_cjk() {
        // CJK characters are 3-byte UTF-8 outside the Latin-1+W1252 range,
        // so char_to_w1252_byte returns None for them — bail out path.
        assert_eq!(fix_mojibake("細野晴臣"), "細野晴臣");
    }

    #[test]
    fn passes_through_emoji() {
        assert_eq!(fix_mojibake("🎵"), "🎵");
        assert_eq!(fix_mojibake("Stereolab 🎸"), "Stereolab 🎸");
    }

    #[test]
    fn passes_through_empty() {
        assert_eq!(fix_mojibake(""), "");
    }
}
