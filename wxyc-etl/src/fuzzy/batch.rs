//! Batch APIs for fuzzy classification and filtering.
//!
//! Array-in/array-out APIs suitable for PyO3 exposure, avoiding per-item
//! Python/Rust boundary crossings.

use std::collections::HashSet;

use rayon::prelude::*;

use super::classify::{classify_release, Classification, ClassifyConfig, LibraryIndex};
use crate::text::normalize_artist_name;

/// Classify N releases in parallel using rayon.
///
/// Replaces ProcessPoolExecutor in `verify_cache.py`. Each (artist, title)
/// pair is normalized and classified against the library index.
pub fn batch_classify_releases(
    artists: &[String],
    titles: &[String],
    library_index: &LibraryIndex,
    config: &ClassifyConfig,
) -> Vec<Classification> {
    artists
        .par_iter()
        .zip(titles.par_iter())
        .map(|(artist, title)| {
            let norm_artist = normalize_artist_name(artist);
            let norm_title = normalize_artist_name(title);
            classify_release(&norm_artist, &norm_title, library_index, config)
        })
        .collect()
}

/// Normalize + set-membership lookup for each artist name.
///
/// Replaces the 11.5M-iteration Python loop in `filter_artists.py`.
/// Returns a boolean vector: `true` if the normalized name is in `library_set`.
pub fn batch_filter_artists(
    artist_names: &[String],
    library_set: &HashSet<String>,
) -> Vec<bool> {
    artist_names
        .iter()
        .map(|name| library_set.contains(&normalize_artist_name(name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_test_index() -> LibraryIndex {
        let pairs = vec![
            ("Juana Molina".to_string(), "DOGA".to_string()),
            ("Stereolab".to_string(), "Aluminum Tunes".to_string()),
            ("Cat Power".to_string(), "Moon Pix".to_string()),
        ];
        LibraryIndex::from_pairs(&pairs)
    }

    // --- batch_classify_releases ---

    #[test]
    fn test_batch_classify_releases() {
        let index = build_test_index();
        let config = ClassifyConfig::default();
        let artists = vec!["Juana Molina".to_string(), "Unknown Artist".to_string()];
        let titles = vec!["DOGA".to_string(), "Nonexistent Album".to_string()];
        let results = batch_classify_releases(&artists, &titles, &index, &config);
        assert_eq!(results[0], Classification::Keep);
        assert_eq!(results[1], Classification::Prune);
    }

    #[test]
    fn test_batch_classify_releases_empty() {
        let index = build_test_index();
        let config = ClassifyConfig::default();
        let results = batch_classify_releases(&[], &[], &index, &config);
        assert!(results.is_empty());
    }

    #[test]
    fn test_batch_classify_releases_parallel_consistency() {
        // Run a larger batch to exercise rayon parallelism
        let index = build_test_index();
        let config = ClassifyConfig::default();
        let artists: Vec<String> = (0..100)
            .map(|i| {
                if i % 3 == 0 { "Juana Molina".into() }
                else if i % 3 == 1 { "Unknown".into() }
                else { "Cat Power".into() }
            })
            .collect();
        let titles: Vec<String> = (0..100)
            .map(|i| {
                if i % 3 == 0 { "DOGA".into() }
                else if i % 3 == 1 { "No Album".into() }
                else { "Moon Pix".into() }
            })
            .collect();
        let results = batch_classify_releases(&artists, &titles, &index, &config);
        assert_eq!(results.len(), 100);
        assert_eq!(results[0], Classification::Keep);
        assert_eq!(results[1], Classification::Prune);
        assert_eq!(results[2], Classification::Keep);
    }

    // --- batch_filter_artists ---

    #[test]
    fn test_batch_filter_artists() {
        let library_set: HashSet<String> = vec![
            "autechre".to_string(),
            "stereolab".to_string(),
        ]
        .into_iter()
        .collect();
        let names = vec![
            "Autechre".to_string(),
            "Unknown".to_string(),
            "Stereolab".to_string(),
        ];
        let results = batch_filter_artists(&names, &library_set);
        assert_eq!(results, vec![true, false, true]);
    }

    #[test]
    fn test_batch_filter_artists_empty() {
        let library_set: HashSet<String> = HashSet::new();
        let results = batch_filter_artists(&[], &library_set);
        assert!(results.is_empty());
    }

    #[test]
    fn test_batch_filter_artists_diacritics() {
        let library_set: HashSet<String> = vec!["bjork".to_string()].into_iter().collect();
        let names = vec!["Björk".to_string(), "bjork".to_string()];
        let results = batch_filter_artists(&names, &library_set);
        assert_eq!(results, vec![true, true]);
    }
}
