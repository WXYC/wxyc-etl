//! Compilation and various-artists detection.

/// Anchored leading prefixes that mark an artist credit as a compilation.
///
/// `is_compilation_artist` matches the lowercased input against these as a
/// leading prefix terminated by end-of-string or a non-alphanumeric
/// character. Aligns with the LML#239 decision-record contract
/// (`LIKE 'Various Artists%' OR LIKE 'Soundtracks%'`) and Backend-Service's
/// V/A regex in `jobs/library-etl/job.ts` (`^various\s+artists?…`).
const LEADING_COMPILATION_PREFIXES: &[&str] = &["various artists", "v/a", "v.a", "soundtracks"];

/// Strings that signal a compilation only when they are the entire artist credit.
///
/// Kept exact-only so real bands whose names start with these words ("The
/// Various", "Various Production", "Soundtrack of Our Lives", "Compilation
/// Hits") aren't swept up.
const EXACT_COMPILATION_NAMES: &[&str] = &["various", "soundtrack", "compilation"];

/// Check if an artist name indicates a compilation/soundtrack album.
///
/// Returns `true` when the lowercased artist either equals an entry in
/// [`EXACT_COMPILATION_NAMES`] or starts with an entry in
/// [`LEADING_COMPILATION_PREFIXES`] followed by end-of-string or a
/// non-alphanumeric character.
///
/// The leading-anchored rule excludes WXYC-real artists whose names contain
/// a keyword as a substring (e.g. "The Soundtrack of Our Lives", "The
/// Various"). It does mean inputs like "Original Motion Picture Soundtrack"
/// — where the keyword is trailing rather than leading — are not treated as
/// compilations; that direction is the safer error (one extra real-artist
/// lookup vs. dropping per-source identity for a real band).
pub fn is_compilation_artist(artist: &str) -> bool {
    if artist.is_empty() {
        return false;
    }
    let lower = artist.to_lowercase();
    if EXACT_COMPILATION_NAMES.iter().any(|name| lower == *name) {
        return true;
    }
    LEADING_COMPILATION_PREFIXES.iter().any(|prefix| {
        if !lower.starts_with(prefix) {
            return false;
        }
        match lower[prefix.len()..].chars().next() {
            None => true,
            Some(c) => !c.is_alphanumeric(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Positive cases.

    #[test]
    fn various_artists_is_compilation() {
        assert!(is_compilation_artist("Various Artists"));
    }

    #[test]
    fn various_alone_is_compilation() {
        // Discogs canonical V/A credit.
        assert!(is_compilation_artist("Various"));
    }

    #[test]
    fn various_lowercase_is_compilation() {
        assert!(is_compilation_artist("various"));
    }

    #[test]
    fn various_all_caps_is_compilation() {
        assert!(is_compilation_artist("VARIOUS"));
    }

    #[test]
    fn various_artists_with_wxyc_filing_suffix_is_compilation() {
        // WXYC physical-catalog filing convention.
        assert!(is_compilation_artist("Various Artists-Rock-Y"));
    }

    #[test]
    fn v_slash_a_is_compilation() {
        assert!(is_compilation_artist("V/A"));
    }

    #[test]
    fn v_dot_a_is_compilation() {
        assert!(is_compilation_artist("V.A."));
    }

    #[test]
    fn soundtrack_alone_is_compilation() {
        assert!(is_compilation_artist("Soundtrack"));
    }

    #[test]
    fn soundtracks_plural_is_compilation() {
        assert!(is_compilation_artist("Soundtracks"));
    }

    #[test]
    fn compilation_alone_is_compilation() {
        assert!(is_compilation_artist("Compilation"));
    }

    // Negative cases — real WXYC artists previously misclassified.

    #[test]
    fn the_soundtrack_of_our_lives_is_real_artist() {
        // Swedish rock band — WXYC plays this.
        assert!(!is_compilation_artist("The Soundtrack of Our Lives"));
    }

    #[test]
    fn soundtrack_of_our_lives_is_real_artist() {
        // Same band, alternate credit.
        assert!(!is_compilation_artist("Soundtrack of Our Lives"));
    }

    #[test]
    fn the_various_is_real_artist() {
        // Australian band.
        assert!(!is_compilation_artist("The Various"));
    }

    #[test]
    fn various_production_is_real_artist() {
        // UK electronic act — must not match either rule.
        assert!(!is_compilation_artist("Various Production"));
    }

    // Generic negative cases.

    #[test]
    fn stereolab_is_not_compilation() {
        assert!(!is_compilation_artist("Stereolab"));
    }

    #[test]
    fn juana_molina_is_not_compilation() {
        assert!(!is_compilation_artist("Juana Molina"));
    }

    #[test]
    fn cat_power_is_not_compilation() {
        assert!(!is_compilation_artist("Cat Power"));
    }

    #[test]
    fn empty_is_not_compilation() {
        assert!(!is_compilation_artist(""));
    }
}
