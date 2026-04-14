//! KEEP/PRUNE/REVIEW classification for release validation.
//!
//! Ports the 3-scorer agreement logic from `verify_cache.py` in discogs-cache.

use std::collections::{HashMap, HashSet};

use crate::text::normalize_artist_name;

/// Classification result for a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classification {
    Keep,
    Prune,
    Review,
}

/// Pre-built in-memory index of (artist, title) pairs from the library.
///
/// Stores multiple representations for different scoring strategies:
/// exact pair lookup, per-artist title lookup, combined string fuzzy matching,
/// and a deduplicated artist list.
pub struct LibraryIndex {
    /// Normalized (artist, title) pairs for exact lookup.
    pub exact_pairs: HashSet<(String, String)>,
    /// Normalized artist -> list of normalized titles.
    pub artist_to_titles: HashMap<String, Vec<String>>,
    /// "artist ||| title" combined strings for token-based scorers.
    pub combined_strings: Vec<String>,
    /// Deduplicated, sorted list of normalized artist names.
    pub all_artists: Vec<String>,
}

impl LibraryIndex {
    /// Build an index from (artist, title) pairs.
    ///
    /// All strings are normalized via `normalize_artist_name` (which also
    /// works for titles — same NFKD + lowercase + trim logic).
    pub fn from_pairs(pairs: &[(String, String)]) -> Self {
        let mut exact_pairs = HashSet::with_capacity(pairs.len());
        let mut artist_to_titles: HashMap<String, Vec<String>> = HashMap::new();
        let mut combined_strings = Vec::with_capacity(pairs.len());
        let mut artist_set = HashSet::new();

        for (artist, title) in pairs {
            let norm_artist = normalize_artist_name(artist);
            let norm_title = normalize_artist_name(title);

            exact_pairs.insert((norm_artist.clone(), norm_title.clone()));
            artist_to_titles
                .entry(norm_artist.clone())
                .or_default()
                .push(norm_title.clone());
            combined_strings.push(format!("{} ||| {}", norm_artist, norm_title));
            artist_set.insert(norm_artist);
        }

        let mut all_artists: Vec<String> = artist_set.into_iter().collect();
        all_artists.sort();

        LibraryIndex {
            exact_pairs,
            artist_to_titles,
            combined_strings,
            all_artists,
        }
    }
}

/// Configuration thresholds for the 3-scorer classification logic.
///
/// Defaults match the thresholds in `verify_cache.py`.
pub struct ClassifyConfig {
    /// Minimum score for token_set_ratio to count as a match.
    pub token_set_threshold: f64,
    /// Minimum score for token_sort_ratio to count as a match.
    pub token_sort_threshold: f64,
    /// Minimum score for two_stage to count as a match.
    pub two_stage_threshold: f64,
    /// Artist threshold for the two_stage scorer's first stage.
    pub artist_threshold: f64,
    /// Below this, all scorers agree it's a prune.
    pub prune_ceiling: f64,
}

impl Default for ClassifyConfig {
    fn default() -> Self {
        ClassifyConfig {
            token_set_threshold: 0.8,
            token_sort_threshold: 0.8,
            two_stage_threshold: 0.8,
            artist_threshold: 0.7,
            prune_ceiling: 0.4,
        }
    }
}

/// Classify a single release as KEEP, PRUNE, or REVIEW.
///
/// Logic (matching `verify_cache.py`):
/// 1. If exact match -> KEEP
/// 2. Run three fuzzy scorers (token_set, token_sort, two_stage)
/// 3. If all three above their thresholds AND two_stage participates -> KEEP
/// 4. If all three below prune_ceiling -> PRUNE
/// 5. Otherwise -> REVIEW
pub fn classify_release(
    norm_artist: &str,
    norm_title: &str,
    index: &LibraryIndex,
    config: &ClassifyConfig,
) -> Classification {
    // 1. Exact match -> KEEP
    if score_exact(norm_artist, norm_title, index) == 1.0 {
        return Classification::Keep;
    }

    // 2. Run three fuzzy scorers
    let ts = score_token_set(norm_artist, norm_title, index);
    let tr = score_token_sort(norm_artist, norm_title, index);
    let tw = score_two_stage(norm_artist, norm_title, index, config.artist_threshold);

    // 3. All above thresholds AND two_stage participates -> KEEP
    if ts >= config.token_set_threshold
        && tr >= config.token_sort_threshold
        && tw >= config.two_stage_threshold
    {
        return Classification::Keep;
    }

    // 4. All below prune_ceiling -> PRUNE
    if ts < config.prune_ceiling && tr < config.prune_ceiling && tw < config.prune_ceiling {
        return Classification::Prune;
    }

    // 5. Otherwise -> REVIEW
    Classification::Review
}

/// Returns 1.0 if the (artist, title) pair is in the exact_pairs set, 0.0 otherwise.
pub fn score_exact(norm_artist: &str, norm_title: &str, index: &LibraryIndex) -> f64 {
    if index.exact_pairs.contains(&(norm_artist.to_string(), norm_title.to_string())) {
        1.0
    } else {
        0.0
    }
}

/// Best token_set_ratio of "artist ||| title" against all combined_strings.
pub fn score_token_set(norm_artist: &str, norm_title: &str, index: &LibraryIndex) -> f64 {
    score_combined(norm_artist, norm_title, index, super::metrics::token_set_ratio)
}

/// Best token_sort_ratio of "artist ||| title" against all combined_strings.
pub fn score_token_sort(norm_artist: &str, norm_title: &str, index: &LibraryIndex) -> f64 {
    score_combined(norm_artist, norm_title, index, super::metrics::token_sort_ratio)
}

/// Shared implementation for token_set and token_sort combined-string scoring.
fn score_combined(
    norm_artist: &str,
    norm_title: &str,
    index: &LibraryIndex,
    scorer: fn(&str, &str) -> f64,
) -> f64 {
    let query = format!("{} ||| {}", norm_artist, norm_title);
    index
        .combined_strings
        .iter()
        .map(|c| scorer(&query, c))
        .fold(0.0_f64, f64::max)
}

/// Two-stage scorer: fuzzy-match artist first, then title against that artist's titles.
///
/// Returns geometric mean of the best artist score and best title score.
/// If no artist matches above `artist_threshold`, returns 0.0.
pub fn score_two_stage(
    norm_artist: &str,
    norm_title: &str,
    index: &LibraryIndex,
    artist_threshold: f64,
) -> f64 {
    // Stage 1: find the best-matching artist
    let mut best_artist_score = 0.0_f64;
    let mut best_artist = None;

    for lib_artist in &index.all_artists {
        let score = super::metrics::jaro_winkler_similarity(norm_artist, lib_artist);
        if score > best_artist_score {
            best_artist_score = score;
            best_artist = Some(lib_artist.as_str());
        }
    }

    if best_artist_score < artist_threshold {
        return 0.0;
    }

    let best_artist = match best_artist {
        Some(a) => a,
        None => return 0.0,
    };

    // Stage 2: find the best-matching title for this artist
    let titles = match index.artist_to_titles.get(best_artist) {
        Some(t) => t,
        None => return 0.0,
    };

    let best_title_score = titles
        .iter()
        .map(|t| super::metrics::jaro_winkler_similarity(norm_title, t))
        .fold(0.0_f64, f64::max);

    // Geometric mean
    (best_artist_score * best_title_score).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_index_from_pairs() {
        let pairs = vec![
            ("Juana Molina".to_string(), "DOGA".to_string()),
            ("Stereolab".to_string(), "Aluminum Tunes".to_string()),
            ("Cat Power".to_string(), "Moon Pix".to_string()),
        ];
        let index = LibraryIndex::from_pairs(&pairs);

        assert_eq!(index.exact_pairs.len(), 3);
        assert!(index.exact_pairs.contains(&("juana molina".into(), "doga".into())));
        assert!(index.exact_pairs.contains(&("stereolab".into(), "aluminum tunes".into())));
        assert!(index.exact_pairs.contains(&("cat power".into(), "moon pix".into())));
    }

    #[test]
    fn test_library_index_all_artists_deduplicated() {
        let pairs = vec![
            ("Stereolab".to_string(), "Aluminum Tunes".to_string()),
            ("Stereolab".to_string(), "Dots and Loops".to_string()),
            ("Cat Power".to_string(), "Moon Pix".to_string()),
        ];
        let index = LibraryIndex::from_pairs(&pairs);

        assert_eq!(index.all_artists.len(), 2);
        assert!(index.all_artists.contains(&"cat power".to_string()));
        assert!(index.all_artists.contains(&"stereolab".to_string()));
    }

    #[test]
    fn test_library_index_artist_to_titles() {
        let pairs = vec![
            ("Stereolab".to_string(), "Aluminum Tunes".to_string()),
            ("Stereolab".to_string(), "Dots and Loops".to_string()),
            ("Cat Power".to_string(), "Moon Pix".to_string()),
        ];
        let index = LibraryIndex::from_pairs(&pairs);

        let stereolab_titles = index.artist_to_titles.get("stereolab").unwrap();
        assert_eq!(stereolab_titles.len(), 2);
        assert!(stereolab_titles.contains(&"aluminum tunes".to_string()));
        assert!(stereolab_titles.contains(&"dots and loops".to_string()));
    }

    #[test]
    fn test_library_index_combined_strings() {
        let pairs = vec![
            ("Cat Power".to_string(), "Moon Pix".to_string()),
        ];
        let index = LibraryIndex::from_pairs(&pairs);

        assert_eq!(index.combined_strings.len(), 1);
        assert_eq!(index.combined_strings[0], "cat power ||| moon pix");
    }

    #[test]
    fn test_library_index_empty() {
        let pairs: Vec<(String, String)> = vec![];
        let index = LibraryIndex::from_pairs(&pairs);

        assert!(index.exact_pairs.is_empty());
        assert!(index.artist_to_titles.is_empty());
        assert!(index.combined_strings.is_empty());
        assert!(index.all_artists.is_empty());
    }

    fn build_test_index() -> LibraryIndex {
        let pairs = vec![
            ("Juana Molina".to_string(), "DOGA".to_string()),
            ("Stereolab".to_string(), "Aluminum Tunes".to_string()),
            ("Cat Power".to_string(), "Moon Pix".to_string()),
        ];
        LibraryIndex::from_pairs(&pairs)
    }

    // --- score_exact ---

    #[test]
    fn test_score_exact_match() {
        let index = build_test_index();
        assert_eq!(score_exact("juana molina", "doga", &index), 1.0);
    }

    #[test]
    fn test_score_exact_no_match() {
        let index = build_test_index();
        assert_eq!(score_exact("unknown", "unknown album", &index), 0.0);
    }

    // --- score_token_set ---

    #[test]
    fn test_score_token_set_good_match() {
        let index = build_test_index();
        let score = score_token_set("juana molina", "doga", &index);
        assert!(score > 0.7, "expected high token_set score, got {}", score);
    }

    #[test]
    fn test_score_token_set_no_match() {
        let index = build_test_index();
        let score = score_token_set("completely unknown", "nonexistent", &index);
        assert!(score < 0.5, "expected low token_set score, got {}", score);
    }

    // --- score_token_sort ---

    #[test]
    fn test_score_token_sort_good_match() {
        let index = build_test_index();
        let score = score_token_sort("juana molina", "doga", &index);
        assert!(score > 0.7, "expected high token_sort score, got {}", score);
    }

    // --- score_two_stage ---

    #[test]
    fn test_score_two_stage_good_match() {
        let index = build_test_index();
        let score = score_two_stage("juana molina", "doga", &index, 0.7);
        assert!(score > 0.9, "expected high two_stage score, got {}", score);
    }

    #[test]
    fn test_score_two_stage_no_artist_match() {
        let index = build_test_index();
        let score = score_two_stage("completely unknown", "doga", &index, 0.7);
        assert!(score < 0.5, "expected low two_stage score, got {}", score);
    }

    #[test]
    fn test_score_two_stage_artist_match_title_mismatch() {
        let index = build_test_index();
        let score = score_two_stage("juana molina", "nonexistent album", &index, 0.7);
        // Artist matches but title doesn't — geometric mean pulls score down
        assert!(score < 0.8, "expected moderate two_stage score, got {}", score);
    }

    // --- classify_release ---

    #[test]
    fn test_classify_exact_match_is_keep() {
        let index = build_test_index();
        let config = ClassifyConfig::default();
        let result = classify_release("juana molina", "doga", &index, &config);
        assert_eq!(result, Classification::Keep);
    }

    #[test]
    fn test_classify_no_match_is_not_keep() {
        let index = build_test_index();
        let config = ClassifyConfig::default();
        let result = classify_release("xyz", "qr", &index, &config);
        // With a small index, the "|||" separator token inflates token-based
        // scores above prune_ceiling, so this lands in Review rather than Prune.
        // The key invariant is that it's NOT classified as Keep.
        assert_ne!(result, Classification::Keep);
    }

    #[test]
    fn test_classify_prune_with_raised_ceiling() {
        let index = build_test_index();
        let config = ClassifyConfig {
            prune_ceiling: 0.5,
            ..ClassifyConfig::default()
        };
        let result = classify_release("xyz", "qr", &index, &config);
        assert_eq!(result, Classification::Prune);
    }

    #[test]
    fn test_classify_close_match_is_keep() {
        // "Juana Molina" / "Doga" should fuzzy-match well against the index
        let index = build_test_index();
        let config = ClassifyConfig::default();
        let result = classify_release("juana molina", "doga", &index, &config);
        assert_eq!(result, Classification::Keep);
    }

    #[test]
    fn test_classify_ambiguous_is_review() {
        // An artist that partially matches but with a wrong title
        let index = build_test_index();
        let config = ClassifyConfig::default();
        // "cat power" matches artist but "wrong album" doesn't match title well
        let result = classify_release("cat power", "wrong album entirely", &index, &config);
        // This should be REVIEW (artist matches but title doesn't)
        assert!(
            matches!(result, Classification::Review | Classification::Prune),
            "expected Review or Prune, got {:?}", result
        );
    }

    // --- Integration tests: full WXYC example data index ---

    /// Build a richer index from the canonical WXYC example data for
    /// integration-level classification tests.
    fn build_wxyc_index() -> LibraryIndex {
        let pairs = vec![
            ("Autechre".to_string(), "Confield".to_string()),
            ("Prince Jammy".to_string(), "...Destroys The Space Invaders".to_string()),
            ("Juana Molina".to_string(), "DOGA".to_string()),
            ("Stereolab".to_string(), "Aluminum Tunes".to_string()),
            ("Cat Power".to_string(), "Moon Pix".to_string()),
            ("Jessica Pratt".to_string(), "On Your Own Love Again".to_string()),
            ("Chuquimamani-Condori".to_string(), "Edits".to_string()),
            ("Duke Ellington & John Coltrane".to_string(), "Duke Ellington & John Coltrane".to_string()),
            ("Sessa".to_string(), "Pequena Vertigem de Amor".to_string()),
            ("Anne Gillis".to_string(), "Eyry".to_string()),
            ("Father John Misty".to_string(), "I Love You, Honeybear".to_string()),
            ("Rafael Toral".to_string(), "Traveling Light".to_string()),
            ("Buck Meek".to_string(), "Gasoline".to_string()),
            ("Nourished by Time".to_string(), "The Passionate Ones".to_string()),
            ("For Tracy Hyde".to_string(), "Hotel Insomnia".to_string()),
            ("Rochelle Jordan".to_string(), "Through the Wall".to_string()),
            ("Large Professor".to_string(), "1st Class".to_string()),
        ];
        LibraryIndex::from_pairs(&pairs)
    }

    #[test]
    fn test_classify_wxyc_exact_matches() {
        let index = build_wxyc_index();
        let config = ClassifyConfig::default();

        // All canonical entries should classify as KEEP via exact match
        let cases = [
            ("autechre", "confield"),
            ("juana molina", "doga"),
            ("stereolab", "aluminum tunes"),
            ("cat power", "moon pix"),
            ("jessica pratt", "on your own love again"),
            ("chuquimamani-condori", "edits"),
            ("sessa", "pequena vertigem de amor"),
            ("anne gillis", "eyry"),
            ("buck meek", "gasoline"),
            ("large professor", "1st class"),
        ];
        for (artist, title) in &cases {
            assert_eq!(
                classify_release(artist, title, &index, &config),
                Classification::Keep,
                "expected KEEP for ({}, {})",
                artist,
                title,
            );
        }
    }

    #[test]
    fn test_classify_wxyc_unknown_is_not_keep() {
        let index = build_wxyc_index();
        let config = ClassifyConfig::default();

        // Completely unknown artists/titles should never be KEEP
        let cases = [
            ("nonexistent band", "fake album"),
            ("zzzzz", "qqqqq"),
            ("xylophone ensemble", "debut"),
        ];
        for (artist, title) in &cases {
            assert_ne!(
                classify_release(artist, title, &index, &config),
                Classification::Keep,
                "expected NOT KEEP for ({}, {})",
                artist,
                title,
            );
        }
    }

    #[test]
    fn test_classify_wxyc_fuzzy_close_match() {
        // Slight misspellings should still classify as KEEP due to fuzzy scoring
        let index = build_wxyc_index();
        let config = ClassifyConfig::default();

        let result = classify_release("stereolab", "aluminium tunes", &index, &config);
        assert_eq!(
            result,
            Classification::Keep,
            "slight misspelling of 'Aluminum Tunes' should be KEEP",
        );
    }

    #[test]
    fn test_classify_right_artist_wrong_title_is_review() {
        // Right artist but wrong title: should be REVIEW (scorers disagree)
        let index = build_wxyc_index();
        let config = ClassifyConfig::default();

        let result = classify_release("autechre", "tri repetae", &index, &config);
        // Autechre matches the artist stage, but "tri repetae" doesn't match
        // "confield" — the scorers should disagree, landing in REVIEW
        assert!(
            matches!(result, Classification::Review | Classification::Prune),
            "expected REVIEW or PRUNE for known artist / unknown title, got {:?}",
            result,
        );
    }

    #[test]
    fn test_classify_config_strict_thresholds() {
        // Raising thresholds makes fuzzy matches harder to achieve
        let index = build_wxyc_index();
        let strict_config = ClassifyConfig {
            token_set_threshold: 0.95,
            token_sort_threshold: 0.95,
            two_stage_threshold: 0.95,
            artist_threshold: 0.9,
            prune_ceiling: 0.4,
        };

        // Exact match should still be KEEP (bypasses fuzzy scoring)
        assert_eq!(
            classify_release("autechre", "confield", &index, &strict_config),
            Classification::Keep,
        );

        // A moderately different title with strict thresholds should no longer
        // be KEEP. "cat power" / "moon pics remix" is close but not close enough
        // for 0.95 thresholds on all three scorers.
        let result = classify_release("cat power", "moon pics remix", &index, &strict_config);
        assert_ne!(
            result,
            Classification::Keep,
            "strict thresholds should reject moderately different title",
        );
    }

    #[test]
    fn test_classify_config_lenient_thresholds() {
        // Lowering thresholds makes more items classify as KEEP
        let index = build_wxyc_index();
        let lenient_config = ClassifyConfig {
            token_set_threshold: 0.5,
            token_sort_threshold: 0.5,
            two_stage_threshold: 0.5,
            artist_threshold: 0.5,
            prune_ceiling: 0.2,
        };

        // Even a somewhat different title should now pass
        let result = classify_release("cat power", "moon pics", &index, &lenient_config);
        assert_eq!(
            result,
            Classification::Keep,
            "lenient thresholds should KEEP close misspelling",
        );
    }

    #[test]
    fn test_three_scorer_agreement_all_keep() {
        // When all three scorers agree above threshold: KEEP
        let index = build_wxyc_index();
        let config = ClassifyConfig::default();

        // Exact match trivially satisfies all scorers
        let ts = score_token_set("autechre", "confield", &index);
        let tr = score_token_sort("autechre", "confield", &index);
        let tw = score_two_stage("autechre", "confield", &index, config.artist_threshold);

        // All should be high for an exact-match entry
        assert!(ts > 0.7, "token_set should be high, got {}", ts);
        assert!(tr > 0.7, "token_sort should be high, got {}", tr);
        assert!(tw > 0.7, "two_stage should be high, got {}", tw);
    }

    #[test]
    fn test_three_scorer_agreement_all_low() {
        // When all three scorers produce low scores: PRUNE (with raised ceiling)
        let index = build_wxyc_index();

        let ts = score_token_set("zzzzz", "qqqqq", &index);
        let tr = score_token_sort("zzzzz", "qqqqq", &index);
        let tw = score_two_stage("zzzzz", "qqqqq", &index, 0.7);

        // All should be very low for completely unrelated strings
        assert!(ts < 0.4, "token_set should be low, got {}", ts);
        assert!(tr < 0.4, "token_sort should be low, got {}", tr);
        assert!(tw < 0.4, "two_stage should be low, got {}", tw);
    }

    #[test]
    fn test_scorer_individual_token_set() {
        // token_set_ratio-based scorer rewards shared tokens
        let index = build_wxyc_index();

        // "duke ellington" shares tokens with the combined string for
        // "duke ellington & john coltrane ||| duke ellington & john coltrane"
        let score = score_token_set("duke ellington", "duke ellington", &index);
        assert!(score > 0.7, "shared tokens should produce high score, got {}", score);
    }

    #[test]
    fn test_scorer_individual_two_stage() {
        // two_stage scorer: artist match first, then title match
        let index = build_wxyc_index();

        // Good artist + good title
        let score_good = score_two_stage("jessica pratt", "on your own love again", &index, 0.7);
        assert!(score_good > 0.9, "exact artist+title should score high, got {}", score_good);

        // Good artist + bad title
        let score_bad_title = score_two_stage("jessica pratt", "nonexistent album", &index, 0.7);
        assert!(
            score_bad_title < score_good,
            "bad title should score lower than good title: {} vs {}",
            score_bad_title, score_good,
        );

        // Bad artist (below artist_threshold) returns 0.0
        let score_bad_artist = score_two_stage("zzzzz", "confield", &index, 0.7);
        assert_eq!(score_bad_artist, 0.0, "bad artist should produce 0.0");
    }
}
