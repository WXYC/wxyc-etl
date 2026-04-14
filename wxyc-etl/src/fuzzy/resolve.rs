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
        let result = best_match("completely different", &candidates, jaro_winkler_similarity, 0.9);
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
}
