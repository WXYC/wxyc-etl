//! Batch operations for PyO3 efficiency.
//!
//! Provides array-in/array-out APIs suitable for exposing via PyO3,
//! avoiding per-item Python/Rust boundary crossings.

use super::filter::ArtistFilter;
use super::forms::{to_ascii_form, to_match_form, to_storage_form};

/// Apply [`to_storage_form`] to each input in one call.
pub fn batch_to_storage_form(inputs: &[String]) -> Vec<String> {
    inputs.iter().map(|s| to_storage_form(s)).collect()
}

/// Apply [`to_match_form`] to each input in one call.
pub fn batch_to_match_form(inputs: &[String]) -> Vec<String> {
    inputs.iter().map(|s| to_match_form(s)).collect()
}

/// Apply [`to_ascii_form`] to each input in one call.
pub fn batch_to_ascii_form(inputs: &[String]) -> Vec<String> {
    inputs.iter().map(|s| to_ascii_form(s)).collect()
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
    fn test_batch_filter_matches() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artists.txt");
        fs::write(&path, "Stereolab\nNilüfer Yanya\n").unwrap();

        let filter = super::super::filter::ArtistFilter::from_file(&path).unwrap();
        let names = vec![
            "Stereolab".into(),
            "Unknown".into(),
            "Nilüfer Yanya".into(),
            "Cat Power".into(),
        ];
        assert_eq!(
            batch_filter(&names, &filter),
            vec![true, false, true, false]
        );
    }

    #[test]
    fn test_batch_to_storage_form_matches_singles() {
        let inputs: Vec<String> = vec!["Stereolab".into(), "  Caf\u{00e9}  ".into()];
        let expected: Vec<String> = inputs.iter().map(|s| to_storage_form(s)).collect();
        assert_eq!(batch_to_storage_form(&inputs), expected);
    }

    #[test]
    fn test_batch_to_match_form_matches_singles() {
        let inputs: Vec<String> = vec!["STEREOLAB".into(), "Nil\u{00fc}fer Yanya".into()];
        let expected: Vec<String> = inputs.iter().map(|s| to_match_form(s)).collect();
        assert_eq!(batch_to_match_form(&inputs), expected);
    }

    #[test]
    fn test_batch_to_ascii_form_matches_singles() {
        let inputs: Vec<String> = vec!["\u{03a3}tella".into(), "Stereolab \u{1f3b8}".into()];
        let expected: Vec<String> = inputs.iter().map(|s| to_ascii_form(s)).collect();
        assert_eq!(batch_to_ascii_form(&inputs), expected);
    }

    #[test]
    fn test_batch_form_helpers_empty() {
        let names: Vec<String> = vec![];
        assert!(batch_to_storage_form(&names).is_empty());
        assert!(batch_to_match_form(&names).is_empty());
        assert!(batch_to_ascii_form(&names).is_empty());
    }

    #[test]
    fn test_batch_filter_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artists.txt");
        fs::write(&path, "Stereolab\n").unwrap();

        let filter = super::super::filter::ArtistFilter::from_file(&path).unwrap();
        let names: Vec<String> = vec![];
        assert_eq!(batch_filter(&names, &filter), Vec::<bool>::new());
    }
}
