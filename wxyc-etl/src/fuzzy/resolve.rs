//! Batch best-match resolution against a catalog of candidates.
//!
//! Replaces `process.extractOne()` loop in `artist_resolver.py`.

use super::metrics::jaro_winkler_similarity;

/// Find the best-matching candidate above `threshold`.
///
/// Returns `(index, score)` of the best match, or `None` if no candidate
/// meets the threshold.
pub fn best_match(
    query: &str,
    candidates: &[String],
    scorer: fn(&str, &str) -> f64,
    threshold: f64,
) -> Option<(usize, f64)> {
    let mut best_idx = 0;
    let mut best_score = f64::NEG_INFINITY;

    for (i, candidate) in candidates.iter().enumerate() {
        let score = scorer(query, candidate);
        if score > best_score {
            best_score = score;
            best_idx = i;
        }
    }

    if best_score >= threshold {
        Some((best_idx, best_score))
    } else {
        None
    }
}

/// Resolve each name to the best Jaro-Winkler match in `catalog`.
///
/// For each name, finds the top `limit` candidates, then applies the
/// `ambiguity_threshold` guard: if the top two scores differ by less than
/// this margin, the match is rejected (returns `None` for that name).
/// This matches `artist_resolver.py`'s `rfprocess.extract(limit=2)` behavior.
pub fn batch_fuzzy_resolve(
    names: &[String],
    catalog: &[String],
    threshold: f64,
    limit: usize,
    ambiguity_threshold: f64,
) -> Vec<Option<String>> {
    names
        .iter()
        .map(|name| resolve_one(name, catalog, threshold, limit, ambiguity_threshold))
        .collect()
}

/// Resolve a single name against the catalog.
fn resolve_one(
    name: &str,
    catalog: &[String],
    threshold: f64,
    limit: usize,
    ambiguity_threshold: f64,
) -> Option<String> {
    if catalog.is_empty() {
        return None;
    }

    // Collect top-N scores
    let mut scored: Vec<(usize, f64)> = catalog
        .iter()
        .enumerate()
        .map(|(i, c)| (i, jaro_winkler_similarity(name, c)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    scored.truncate(limit);

    let (best_idx, best_score) = scored[0];
    if best_score < threshold {
        return None;
    }

    // Ambiguity guard: reject if top-2 scores are too close
    if scored.len() >= 2 {
        let second_score = scored[1].1;
        if (best_score - second_score).abs() < ambiguity_threshold {
            return None;
        }
    }

    Some(catalog[best_idx].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_best_match_finds_closest() {
        let candidates = vec![
            "Autechre".to_string(),
            "Stereolab".to_string(),
            "Cat Power".to_string(),
        ];
        let result = best_match("Autechre", &candidates, jaro_winkler_similarity, 0.8);
        assert_eq!(result, Some((0, 1.0)));
    }

    #[test]
    fn test_best_match_below_threshold_returns_none() {
        let candidates = vec!["Autechre".to_string(), "Stereolab".to_string()];
        let result = best_match(
            "completely different",
            &candidates,
            jaro_winkler_similarity,
            0.9,
        );
        assert!(result.is_none());
    }

    #[test]
    fn test_best_match_empty_candidates() {
        let candidates: Vec<String> = vec![];
        let result = best_match("anything", &candidates, jaro_winkler_similarity, 0.5);
        assert!(result.is_none());
    }

    #[test]
    fn test_batch_fuzzy_resolve_basic() {
        let names = vec![
            "autechre".to_string(),
            "stereolab".to_string(),
            "unknown xyz".to_string(),
        ];
        let catalog = vec![
            "Autechre".to_string(),
            "Stereolab".to_string(),
            "Cat Power".to_string(),
        ];
        let results = batch_fuzzy_resolve(&names, &catalog, 0.8, 2, 0.02);
        assert_eq!(results[0], Some("Autechre".to_string()));
        assert_eq!(results[1], Some("Stereolab".to_string()));
        assert_eq!(results[2], None);
    }

    #[test]
    fn test_batch_fuzzy_resolve_ambiguity_rejection() {
        // "Smith" vs "Smyth" — very similar to the query "smith"
        let names = vec!["smith".to_string()];
        let catalog = vec!["Smith".to_string(), "Smyth".to_string()];
        let results = batch_fuzzy_resolve(&names, &catalog, 0.5, 2, 1.0);
        // With a huge ambiguity threshold of 1.0, any match where top-2 exist
        // should be rejected because the gap can't exceed 1.0 when both scores
        // are > 0.5 (they'd need to differ by more than 1.0, which is impossible)
        assert_eq!(results[0], None);
    }

    #[test]
    fn test_batch_fuzzy_resolve_empty_names() {
        let catalog = vec!["Autechre".to_string()];
        let results = batch_fuzzy_resolve(&[], &catalog, 0.8, 2, 0.02);
        assert!(results.is_empty());
    }

    #[test]
    fn test_batch_fuzzy_resolve_empty_catalog() {
        let names = vec!["Autechre".to_string()];
        let results = batch_fuzzy_resolve(&names, &[], 0.8, 2, 0.02);
        assert_eq!(results, vec![None]);
    }

    // --- Integration tests: full WXYC artist catalog ---

    /// Full WXYC example artist catalog for resolve integration tests.
    fn wxyc_catalog() -> Vec<String> {
        vec![
            "Autechre".to_string(),
            "Prince Jammy".to_string(),
            "Juana Molina".to_string(),
            "Stereolab".to_string(),
            "Cat Power".to_string(),
            "Jessica Pratt".to_string(),
            "Chuquimamani-Condori".to_string(),
            "Duke Ellington & John Coltrane".to_string(),
            "Sessa".to_string(),
            "Anne Gillis".to_string(),
            "Father John Misty".to_string(),
            "Rafael Toral".to_string(),
            "Buck Meek".to_string(),
            "Nourished by Time".to_string(),
            "For Tracy Hyde".to_string(),
            "Rochelle Jordan".to_string(),
            "Large Professor".to_string(),
        ]
    }

    #[test]
    fn test_resolve_wxyc_exact_names() {
        let catalog = wxyc_catalog();
        let names = vec![
            "Autechre".to_string(),
            "Stereolab".to_string(),
            "Cat Power".to_string(),
            "Jessica Pratt".to_string(),
            "Chuquimamani-Condori".to_string(),
            "Duke Ellington & John Coltrane".to_string(),
        ];
        let results = batch_fuzzy_resolve(&names, &catalog, 0.8, 2, 0.02);
        assert_eq!(results[0], Some("Autechre".to_string()));
        assert_eq!(results[1], Some("Stereolab".to_string()));
        assert_eq!(results[2], Some("Cat Power".to_string()));
        assert_eq!(results[3], Some("Jessica Pratt".to_string()));
        assert_eq!(results[4], Some("Chuquimamani-Condori".to_string()));
        assert_eq!(
            results[5],
            Some("Duke Ellington & John Coltrane".to_string())
        );
    }

    #[test]
    fn test_resolve_wxyc_unknown_names() {
        let catalog = wxyc_catalog();
        let names = vec![
            "completely unknown band".to_string(),
            "zzzzz".to_string(),
            "xylophone ensemble".to_string(),
        ];
        let results = batch_fuzzy_resolve(&names, &catalog, 0.8, 2, 0.02);
        for (i, result) in results.iter().enumerate() {
            assert!(
                result.is_none(),
                "expected None for unknown name {:?}, got {:?}",
                names[i],
                result,
            );
        }
    }

    #[test]
    fn test_resolve_wxyc_close_misspellings() {
        let catalog = wxyc_catalog();
        let names = vec![
            "Autechree".to_string(),    // extra 'e'
            "Stereolabb".to_string(),   // extra 'b'
            "Jessica Prat".to_string(), // missing 't'
        ];
        let results = batch_fuzzy_resolve(&names, &catalog, 0.8, 2, 0.02);
        // Close misspellings should resolve to the correct artist
        assert_eq!(results[0], Some("Autechre".to_string()));
        assert_eq!(results[1], Some("Stereolab".to_string()));
        assert_eq!(results[2], Some("Jessica Pratt".to_string()));
    }

    #[test]
    fn test_resolve_wxyc_ambiguity_with_close_candidates() {
        // Create a catalog with deliberately close names to test ambiguity
        let catalog = vec![
            "Cat Power".to_string(),
            "Cat Powder".to_string(), // very similar
            "Stereolab".to_string(),
        ];
        // With a tight ambiguity_threshold, "Cat Powe" should be rejected
        // because "Cat Power" and "Cat Powder" score very similarly
        let names = vec!["Cat Powe".to_string()];
        let results = batch_fuzzy_resolve(&names, &catalog, 0.7, 2, 0.05);
        // The two "Cat Po..." candidates should be close enough to trigger
        // the ambiguity guard (their scores against "Cat Powe" differ by < 0.05)
        assert_eq!(results[0], None, "ambiguous match should be rejected");
    }

    #[test]
    fn test_resolve_wxyc_limit_1_no_ambiguity_guard() {
        // With limit=1, only one candidate is considered so the ambiguity
        // guard never fires
        let catalog = vec!["Cat Power".to_string(), "Cat Powder".to_string()];
        let names = vec!["Cat Power".to_string()];
        let results = batch_fuzzy_resolve(&names, &catalog, 0.8, 1, 0.05);
        assert_eq!(results[0], Some("Cat Power".to_string()));
    }

    #[test]
    fn test_best_match_wxyc_scorers() {
        use super::super::metrics::{token_set_ratio, token_sort_ratio};

        let catalog = wxyc_catalog();

        // Jaro-Winkler: exact match
        let jw_result = best_match("Autechre", &catalog, jaro_winkler_similarity, 0.8);
        assert!(jw_result.is_some());
        let (idx, score) = jw_result.unwrap();
        assert_eq!(catalog[idx], "Autechre");
        assert!((score - 1.0).abs() < 0.01);

        // token_set_ratio: reordered tokens should still match well
        let ts_result = best_match(
            "ellington duke john coltrane",
            &catalog,
            token_set_ratio,
            0.7,
        );
        assert!(ts_result.is_some(), "reordered tokens should match");

        // token_sort_ratio: sorted comparison
        let tr_result = best_match("Pratt Jessica", &catalog, token_sort_ratio, 0.7);
        assert!(tr_result.is_some(), "reversed name should match");
        let (idx, _) = tr_result.unwrap();
        assert_eq!(catalog[idx], "Jessica Pratt");
    }
}
