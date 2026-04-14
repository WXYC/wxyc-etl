//! Batch operations for PyO3 efficiency.
//!
//! Provides array-in/array-out APIs suitable for exposing via PyO3,
//! avoiding per-item Python/Rust boundary crossings.

use super::filter::ArtistFilter;
use super::normalize::normalize_artist_name;

/// Normalize a batch of artist names in one call.
pub fn batch_normalize(names: &[String]) -> Vec<String> {
    names.iter().map(|n| normalize_artist_name(n)).collect()
}

/// Normalize and check membership for a batch of names.
///
/// Returns a boolean vector where `true` means the corresponding name
/// matched the artist filter.
pub fn batch_filter(names: &[String], filter: &ArtistFilter) -> Vec<bool> {
    names
        .iter()
        .map(|n| filter.matches_any(std::iter::once(n.as_str())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_batch_normalize_basic() {
        let names = vec!["Björk".into(), "Radiohead".into(), "Sigur Rós".into()];
        assert_eq!(
            batch_normalize(&names),
            vec!["bjork", "radiohead", "sigur ros"]
        );
    }

    #[test]
    fn test_batch_normalize_empty() {
        let names: Vec<String> = vec![];
        assert_eq!(batch_normalize(&names), Vec::<String>::new());
    }

    #[test]
    fn test_batch_filter_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artists.txt");
        fs::write(&path, "Radiohead\nBjörk\n").unwrap();

        let filter = super::super::filter::ArtistFilter::from_file(&path).unwrap();
        let names = vec![
            "Radiohead".into(),
            "Unknown".into(),
            "Björk".into(),
            "Cat Power".into(),
        ];
        assert_eq!(
            batch_filter(&names, &filter),
            vec![true, false, true, false]
        );
    }

    #[test]
    fn test_batch_filter_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artists.txt");
        fs::write(&path, "Radiohead\n").unwrap();

        let filter = super::super::filter::ArtistFilter::from_file(&path).unwrap();
        let names: Vec<String> = vec![];
        assert_eq!(batch_filter(&names, &filter), Vec::<bool>::new());
    }
}
