//! Compilation and various-artists detection.

/// Keywords indicating a compilation/soundtrack album.
///
/// Checked as case-insensitive substring matches against artist names.
pub const COMPILATION_KEYWORDS: &[&str] = &[
    "various",
    "soundtrack",
    "compilation",
    "v/a",
    "v.a.",
];

/// Check if an artist name indicates a compilation/soundtrack album.
///
/// Returns `true` if the lowercased artist name contains any compilation
/// keyword as a substring. Matches the behavior of
/// `discogs-cache/lib/matching.py:is_compilation_artist()`.
pub fn is_compilation_artist(artist: &str) -> bool {
    if artist.is_empty() {
        return false;
    }
    let lower = artist.to_lowercase();
    COMPILATION_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_various_artists() {
        assert!(is_compilation_artist("Various Artists"));
    }

    #[test]
    fn test_various_lowercase() {
        assert!(is_compilation_artist("various"));
    }

    #[test]
    fn test_soundtrack() {
        assert!(is_compilation_artist("Soundtrack"));
    }

    #[test]
    fn test_soundtrack_in_phrase() {
        assert!(is_compilation_artist("Original Motion Picture Soundtrack"));
    }

    #[test]
    fn test_v_slash_a() {
        assert!(is_compilation_artist("V/A"));
    }

    #[test]
    fn test_v_dot_a() {
        assert!(is_compilation_artist("v.a."));
    }

    #[test]
    fn test_compilation_keyword() {
        assert!(is_compilation_artist("Compilation Hits"));
    }

    #[test]
    fn test_various_all_caps() {
        assert!(is_compilation_artist("VARIOUS"));
    }

    #[test]
    fn test_stereolab_not_compilation() {
        assert!(!is_compilation_artist("Stereolab"));
    }

    #[test]
    fn test_juana_molina_not_compilation() {
        assert!(!is_compilation_artist("Juana Molina"));
    }

    #[test]
    fn test_cat_power_not_compilation() {
        assert!(!is_compilation_artist("Cat Power"));
    }

    #[test]
    fn test_empty_not_compilation() {
        assert!(!is_compilation_artist(""));
    }
}
