//! String distance metrics for fuzzy matching.
//!
//! Provides `levenshtein_ratio`, `token_set_ratio`, `token_sort_ratio`, and
//! `jaro_winkler_similarity` — all returning 0.0–1.0 similarity scores.

/// Normalized Levenshtein ratio: 1.0 means identical, 0.0 means completely different.
///
/// Computed as `1.0 - (edit_distance / max_length)`.
pub fn levenshtein_ratio(s1: &str, s2: &str) -> f64 {
    let max_len = s1.len().max(s2.len());
    if max_len == 0 {
        return 1.0;
    }
    let dist = strsim::levenshtein(s1, s2);
    1.0 - (dist as f64) / (max_len as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_ratio_identical() {
        assert_eq!(levenshtein_ratio("hello", "hello"), 1.0);
    }

    #[test]
    fn test_levenshtein_ratio_empty() {
        assert_eq!(levenshtein_ratio("", ""), 1.0);
    }

    #[test]
    fn test_levenshtein_ratio_completely_different() {
        let ratio = levenshtein_ratio("abc", "xyz");
        assert!(ratio < 0.1);
    }

    #[test]
    fn test_levenshtein_ratio_one_edit() {
        // "kitten" vs "sitten" — 1 substitution, length 6 -> ratio = 5/6 ≈ 0.833
        let ratio = levenshtein_ratio("kitten", "sitten");
        assert!((ratio - 5.0 / 6.0).abs() < 0.01);
    }

    #[test]
    fn test_levenshtein_ratio_one_empty() {
        assert_eq!(levenshtein_ratio("hello", ""), 0.0);
        assert_eq!(levenshtein_ratio("", "hello"), 0.0);
    }
}
