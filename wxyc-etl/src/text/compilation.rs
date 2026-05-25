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
        lower.starts_with(prefix)
            && lower[prefix.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_compilation_artist_cases() {
        // Real-WXYC-artist false-positives the substring rule used to flag:
        // The Soundtrack of Our Lives (Swedish rock), The Various (Australian
        // band), Various Production (UK electronic).
        let cases = [
            ("Various Artists", true),
            ("Various", true), // Discogs canonical V/A credit.
            ("various", true),
            ("VARIOUS", true),
            ("Various Artists-Rock-Y", true), // WXYC filing convention.
            ("V/A", true),
            ("V.A.", true),
            ("Soundtrack", true),
            ("Soundtracks", true),
            ("Compilation", true),
            ("The Soundtrack of Our Lives", false),
            ("Soundtrack of Our Lives", false),
            ("The Various", false),
            ("Various Production", false),
            ("Stereolab", false),
            ("Juana Molina", false),
            ("Cat Power", false),
            ("", false),
        ];
        for (input, expected) in cases {
            assert_eq!(
                is_compilation_artist(input),
                expected,
                "is_compilation_artist({input:?})"
            );
        }
    }
}
