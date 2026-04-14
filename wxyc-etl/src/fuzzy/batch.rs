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
        let artists = vec!["Juana Molina".to_string(), "xyz".to_string()];
        let titles = vec!["DOGA".to_string(), "qr".to_string()];
        let results = batch_classify_releases(&artists, &titles, &index, &config);
        assert_eq!(results[0], Classification::Keep);
        assert_ne!(results[1], Classification::Keep);
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
                else if i % 3 == 1 { "xyz".into() }
                else { "Cat Power".into() }
            })
            .collect();
        let titles: Vec<String> = (0..100)
            .map(|i| {
                if i % 3 == 0 { "DOGA".into() }
                else if i % 3 == 1 { "qr".into() }
                else { "Moon Pix".into() }
            })
            .collect();
        let results = batch_classify_releases(&artists, &titles, &index, &config);
        assert_eq!(results.len(), 100);
        assert_eq!(results[0], Classification::Keep);
        assert_ne!(results[1], Classification::Keep);
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

    // --- Integration tests: full WXYC example data ---

    /// Build a richer library index from canonical WXYC example data.
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
    fn test_batch_classify_wxyc_keep_prune_review() {
        let index = build_wxyc_index();
        let config = ClassifyConfig::default();

        let artists = vec![
            "Juana Molina".to_string(),       // exact match -> KEEP
            "zzzzz unknown".to_string(),      // no match -> NOT KEEP
            "Cat Power".to_string(),          // exact match -> KEEP
            "Autechre".to_string(),           // exact match -> KEEP
            "xylophone ensemble".to_string(), // no match -> NOT KEEP
        ];
        let titles = vec![
            "DOGA".to_string(),
            "fake album".to_string(),
            "Moon Pix".to_string(),
            "Confield".to_string(),
            "debut".to_string(),
        ];

        let results = batch_classify_releases(&artists, &titles, &index, &config);
        assert_eq!(results.len(), 5);
        assert_eq!(results[0], Classification::Keep, "Juana Molina / DOGA should be KEEP");
        assert_ne!(results[1], Classification::Keep, "unknown should NOT be KEEP");
        assert_eq!(results[2], Classification::Keep, "Cat Power / Moon Pix should be KEEP");
        assert_eq!(results[3], Classification::Keep, "Autechre / Confield should be KEEP");
        assert_ne!(results[4], Classification::Keep, "unknown should NOT be KEEP");
    }

    #[test]
    fn test_batch_classify_all_keep() {
        let index = build_wxyc_index();
        let config = ClassifyConfig::default();

        let artists = vec![
            "Stereolab".to_string(),
            "Jessica Pratt".to_string(),
            "Buck Meek".to_string(),
        ];
        let titles = vec![
            "Aluminum Tunes".to_string(),
            "On Your Own Love Again".to_string(),
            "Gasoline".to_string(),
        ];

        let results = batch_classify_releases(&artists, &titles, &index, &config);
        assert!(
            results.iter().all(|c| *c == Classification::Keep),
            "all canonical entries should be KEEP, got {:?}",
            results,
        );
    }

    #[test]
    fn test_batch_classify_all_not_keep() {
        let index = build_wxyc_index();
        let config = ClassifyConfig::default();

        let artists = vec![
            "completely fake".to_string(),
            "another fake".to_string(),
            "not real at all".to_string(),
        ];
        let titles = vec![
            "no such album".to_string(),
            "also fake".to_string(),
            "made up".to_string(),
        ];

        let results = batch_classify_releases(&artists, &titles, &index, &config);
        assert!(
            results.iter().all(|c| *c != Classification::Keep),
            "all unknown entries should NOT be KEEP, got {:?}",
            results,
        );
    }

    #[test]
    fn test_batch_classify_parallel_consistency_1000() {
        // Run 1000+ items to exercise rayon parallelism and verify determinism
        let index = build_wxyc_index();
        let config = ClassifyConfig::default();

        let wxyc_entries: Vec<(&str, &str)> = vec![
            ("Autechre", "Confield"),
            ("Juana Molina", "DOGA"),
            ("Stereolab", "Aluminum Tunes"),
            ("Cat Power", "Moon Pix"),
            ("Jessica Pratt", "On Your Own Love Again"),
        ];

        let n = 1050;
        let artists: Vec<String> = (0..n)
            .map(|i| wxyc_entries[i % wxyc_entries.len()].0.to_string())
            .collect();
        let titles: Vec<String> = (0..n)
            .map(|i| wxyc_entries[i % wxyc_entries.len()].1.to_string())
            .collect();

        let results = batch_classify_releases(&artists, &titles, &index, &config);
        assert_eq!(results.len(), n);

        // All should be KEEP since they're all exact matches
        for (i, result) in results.iter().enumerate() {
            assert_eq!(
                *result,
                Classification::Keep,
                "item {} ({} / {}) should be KEEP",
                i,
                artists[i],
                titles[i],
            );
        }

        // Run again to verify determinism
        let results2 = batch_classify_releases(&artists, &titles, &index, &config);
        assert_eq!(results, results2, "parallel classification should be deterministic");
    }

    #[test]
    fn test_batch_filter_artists_wxyc_data() {
        let library_set: HashSet<String> = vec![
            "autechre".to_string(),
            "stereolab".to_string(),
            "cat power".to_string(),
            "juana molina".to_string(),
            "jessica pratt".to_string(),
            "chuquimamani-condori".to_string(),
            "sessa".to_string(),
        ]
        .into_iter()
        .collect();

        let names = vec![
            "Autechre".to_string(),
            "Stereolab".to_string(),
            "Unknown Artist".to_string(),
            "Juana Molina".to_string(),
            "Fake Band".to_string(),
            "Chuquimamani-Condori".to_string(),
        ];

        let results = batch_filter_artists(&names, &library_set);
        assert_eq!(results, vec![true, true, false, true, false, true]);
    }
}
