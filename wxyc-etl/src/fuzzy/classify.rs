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
}
