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
}
