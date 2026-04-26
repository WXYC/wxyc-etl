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

/// Tokenize a string: split on whitespace and lowercase each token.
fn tokenize(s: &str) -> Vec<String> {
    s.split_whitespace().map(|t| t.to_lowercase()).collect()
}

/// Sort tokens alphabetically and join with a single space.
fn sorted_tokens(s: &str) -> String {
    let mut tokens = tokenize(s);
    tokens.sort();
    tokens.join(" ")
}

/// Join token parts, skipping empty strings.
fn join_nonempty(parts: &[&str]) -> String {
    parts
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

/// SequenceMatcher-like ratio: `2 * LCS_length / (len1 + len2)`.
///
/// Matches the base ratio used by rapidfuzz's `fuzz.ratio()` (indel
/// normalized similarity). This is different from Levenshtein ratio
/// because substitutions cost 2 (delete + insert) rather than 1.
fn fuzz_ratio(s1: &str, s2: &str) -> f64 {
    let a: Vec<char> = s1.chars().collect();
    let b: Vec<char> = s2.chars().collect();
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let lcs = lcs_length(&a, &b);
    (2 * lcs) as f64 / total as f64
}

/// Compute length of the longest common subsequence using O(n) space DP.
fn lcs_length(a: &[char], b: &[char]) -> usize {
    let mut prev = vec![0usize; b.len() + 1];
    for &ca in a {
        let mut diag = 0;
        for (j, &cb) in b.iter().enumerate() {
            let old = prev[j + 1];
            if ca == cb {
                prev[j + 1] = diag + 1;
            } else {
                prev[j + 1] = prev[j + 1].max(prev[j]);
            }
            diag = old;
        }
    }
    *prev.last().unwrap_or(&0)
}

/// Token set ratio: compares intersection/remainder token sets.
///
/// Port of rapidfuzz's `fuzz.token_set_ratio`. Tokenizes both strings,
/// computes the intersection and remainders, then returns the max
/// fuzz_ratio across three comparison pairs. Returns 0.0–1.0.
pub fn token_set_ratio(s1: &str, s2: &str) -> f64 {
    use std::collections::BTreeSet;

    let t1: BTreeSet<String> = tokenize(s1).into_iter().collect();
    let t2: BTreeSet<String> = tokenize(s2).into_iter().collect();

    if t1.is_empty() && t2.is_empty() {
        return 1.0;
    }
    if t1.is_empty() || t2.is_empty() {
        return 0.0;
    }

    let intersection: BTreeSet<_> = t1.intersection(&t2).cloned().collect();
    let diff1: BTreeSet<_> = t1.difference(&t2).cloned().collect();
    let diff2: BTreeSet<_> = t2.difference(&t1).cloned().collect();

    let inter_str: String = intersection.iter().cloned().collect::<Vec<_>>().join(" ");
    let diff1_str: String = diff1.into_iter().collect::<Vec<_>>().join(" ");
    let diff2_str: String = diff2.into_iter().collect::<Vec<_>>().join(" ");

    let combined1 = join_nonempty(&[&inter_str, &diff1_str]);
    let combined2 = join_nonempty(&[&inter_str, &diff2_str]);

    let r1 = fuzz_ratio(&inter_str, &combined1);
    let r2 = fuzz_ratio(&inter_str, &combined2);
    let r3 = fuzz_ratio(&combined1, &combined2);

    r1.max(r2).max(r3)
}

/// Token sort ratio: sort tokens alphabetically, then compare.
///
/// Port of rapidfuzz's `fuzz.token_sort_ratio`. Returns 0.0–1.0.
pub fn token_sort_ratio(s1: &str, s2: &str) -> f64 {
    let a = sorted_tokens(s1);
    let b = sorted_tokens(s2);
    fuzz_ratio(&a, &b)
}

/// Jaro-Winkler similarity. Thin wrapper around `strsim::jaro_winkler`.
/// Returns 0.0–1.0.
pub fn jaro_winkler_similarity(s1: &str, s2: &str) -> f64 {
    strsim::jaro_winkler(s1, s2)
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

    // --- token_set_ratio ---

    #[test]
    fn test_token_set_ratio_exact_match() {
        let score = token_set_ratio("the beatles", "the beatles");
        assert!((score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_token_set_ratio_reordered() {
        let score = token_set_ratio("duke ellington", "ellington duke");
        assert!(score > 0.9);
    }

    #[test]
    fn test_token_set_ratio_subset() {
        // "fuzzy wuzzy" vs "fuzzy wuzzy was a bear" — superset tokens -> high score
        let score = token_set_ratio("fuzzy wuzzy", "fuzzy wuzzy was a bear");
        assert!(score > 0.8);
    }

    #[test]
    fn test_token_set_ratio_no_overlap() {
        let score = token_set_ratio("abc def", "xyz uvw");
        assert!(score < 0.5);
    }

    #[test]
    fn test_token_set_ratio_empty() {
        assert_eq!(token_set_ratio("", ""), 1.0);
    }

    #[test]
    fn test_token_set_ratio_one_empty() {
        let score = token_set_ratio("hello", "");
        assert!(score < 0.01);
    }

    // --- token_sort_ratio ---

    #[test]
    fn test_token_sort_ratio_same_words() {
        let score = token_sort_ratio("the beatles", "beatles the");
        assert!((score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_token_sort_ratio_different_words() {
        let score = token_sort_ratio("cat power", "stereolab");
        assert!(score < 0.5);
    }

    #[test]
    fn test_token_sort_ratio_empty() {
        assert_eq!(token_sort_ratio("", ""), 1.0);
    }

    // --- jaro_winkler_similarity ---

    #[test]
    fn test_jaro_winkler_identical() {
        assert!((jaro_winkler_similarity("stereolab", "stereolab") - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_jaro_winkler_similar_prefix() {
        // Jaro-Winkler boosts strings matching at the beginning
        let score = jaro_winkler_similarity("martha", "marhta");
        assert!(score > 0.9);
    }

    #[test]
    fn test_jaro_winkler_empty() {
        assert_eq!(jaro_winkler_similarity("", ""), 1.0);
    }

    #[test]
    fn test_jaro_winkler_completely_different() {
        let score = jaro_winkler_similarity("abc", "xyz");
        assert!(score < 0.5);
    }
}
