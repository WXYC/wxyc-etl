//! Fuzzy string matching and batch classification/resolution.
//!
//! # Quick start
//!
//! ```
//! use wxyc_etl::fuzzy::{
//!     token_set_ratio, token_sort_ratio, jaro_winkler_similarity,
//!     classify_release, batch_classify_releases, batch_fuzzy_resolve,
//!     batch_filter_artists, LibraryIndex, Classification,
//! };
//! ```

pub mod batch;
pub mod classify;
pub mod metrics;
pub mod resolve;

// Convenience re-exports for the most common entry points.
pub use batch::{batch_classify_releases, batch_filter_artists};
pub use classify::{classify_release, Classification, ClassifyConfig, LibraryIndex};
pub use metrics::{jaro_winkler_similarity, levenshtein_ratio, token_set_ratio, token_sort_ratio};
pub use resolve::{batch_fuzzy_resolve, best_match};

#[cfg(test)]
mod tests {
    use super::metrics::{token_set_ratio, token_sort_ratio};

    /// Cross-validation against rapidfuzz reference values.
    ///
    /// Expected values computed with:
    ///   rapidfuzz.fuzz.token_set_ratio(s1, s2) / 100.0
    ///   rapidfuzz.fuzz.token_sort_ratio(s1, s2) / 100.0
    ///
    /// We allow up to 5% tolerance since our Levenshtein normalization may
    /// differ slightly from rapidfuzz's internal implementation. The key
    /// requirement is that the ranking order matches.
    #[test]
    fn test_token_set_ratio_matches_rapidfuzz() {
        let cases: Vec<(&str, &str, f64)> = vec![
            ("juana molina", "juana molina", 1.0),
            ("duke ellington", "ellington duke", 1.0),
            ("fuzzy wuzzy", "fuzzy wuzzy was a bear", 1.0),
            ("juana molina", "molina juana", 1.0),
            ("duke ellington", "duke ellington john coltrane", 1.0),
            ("cat power", "cat power band", 1.0),
            ("abc def", "xyz uvw", 0.142857),
            ("autechre", "aphex twin", 0.333333),
            ("cat power", "dog power", 0.714286),
            ("sessa", "stereolab", 0.428571),
        ];
        for (s1, s2, expected) in cases {
            let actual = token_set_ratio(s1, s2);
            assert!(
                (actual - expected).abs() < 0.05,
                "token_set_ratio({:?}, {:?}): got {:.4}, expected {:.4}",
                s1,
                s2,
                actual,
                expected,
            );
        }
    }

    #[test]
    fn test_token_sort_ratio_matches_rapidfuzz() {
        let cases: Vec<(&str, &str, f64)> = vec![
            ("the beatles", "beatles the", 1.0),
            ("juana molina", "molina juana", 1.0),
            ("cat power", "stereolab", 0.333333),
            ("duke ellington", "duke ellington john coltrane", 0.666667),
            ("cat power", "cat power band", 0.782609),
            ("stereolab", "stereolab reissue", 0.692308),
            ("jessica pratt", "jessica pratt live", 0.838710),
            ("abc def", "xyz uvw", 0.142857),
            ("cat power", "dog power", 0.666667),
        ];
        for (s1, s2, expected) in cases {
            let actual = token_sort_ratio(s1, s2);
            assert!(
                (actual - expected).abs() < 0.05,
                "token_sort_ratio({:?}, {:?}): got {:.4}, expected {:.4}",
                s1,
                s2,
                actual,
                expected,
            );
        }
    }

    /// Verify that ranking order is preserved: if rapidfuzz scores A > B,
    /// our implementation should also score A > B.
    #[test]
    fn test_ranking_order_preserved() {
        // "cat power" vs "dog power" should score higher than "cat power" vs "stereolab"
        let score_similar = token_set_ratio("cat power", "dog power");
        let score_different = token_set_ratio("cat power", "stereolab");
        assert!(
            score_similar > score_different,
            "ranking violated: similar={:.4} should be > different={:.4}",
            score_similar,
            score_different,
        );
    }
}
