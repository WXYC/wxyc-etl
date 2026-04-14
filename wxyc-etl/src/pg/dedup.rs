//! In-memory deduplication trackers for PostgreSQL bulk imports.
//!
//! Prevents duplicate rows when multiple source records map to the same
//! database key (e.g., same artist appearing twice on one release).

use std::collections::HashSet;
use std::hash::Hash;

/// Generic deduplication tracker backed by a [`HashSet`].
///
/// Returns `true` on first insertion of a key, `false` on duplicates.
/// Generalizes the `ArtistDedup`, `LabelDedup`, and `TrackArtistDedup`
/// patterns from `discogs-xml-converter`.
#[derive(Default)]
pub struct DedupSet<K> {
    seen: HashSet<K>,
}

impl<K: Eq + Hash> DedupSet<K> {
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    /// Returns `true` if this is the first occurrence (not a duplicate).
    pub fn insert(&mut self, key: K) -> bool {
        self.seen.insert(key)
    }

    /// Number of unique keys seen so far.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// Returns `true` if no keys have been inserted.
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

/// Dedup tracker for (release_id, artist_name) pairs.
pub type ArtistDedup = DedupSet<(u64, String)>;

/// Dedup tracker for (release_id, label_name) pairs.
pub type LabelDedup = DedupSet<(u64, String)>;

/// Dedup tracker for (release_id, track_sequence, artist_name) tuples.
pub type TrackArtistDedup = DedupSet<(u64, u32, String)>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_first_insert_returns_true() {
        let mut dedup = DedupSet::<(u64, String)>::new();
        assert!(dedup.insert((1, "Autechre".to_string())));
    }

    #[test]
    fn test_duplicate_returns_false() {
        let mut dedup = DedupSet::<(u64, String)>::new();
        dedup.insert((1, "Autechre".to_string()));
        assert!(!dedup.insert((1, "Autechre".to_string())));
    }

    #[test]
    fn test_different_keys_both_true() {
        let mut dedup = DedupSet::<(u64, String)>::new();
        assert!(dedup.insert((1, "Autechre".to_string())));
        assert!(dedup.insert((1, "Stereolab".to_string())));
    }

    #[test]
    fn test_len_and_is_empty() {
        let mut dedup = DedupSet::<u32>::new();
        assert!(dedup.is_empty());
        assert_eq!(dedup.len(), 0);
        dedup.insert(1);
        assert!(!dedup.is_empty());
        assert_eq!(dedup.len(), 1);
        dedup.insert(1); // duplicate
        assert_eq!(dedup.len(), 1);
    }

    #[test]
    fn test_artist_dedup() {
        let mut dedup = ArtistDedup::new();
        assert!(dedup.insert((1001, "Autechre".to_string())));
        assert!(dedup.insert((1001, "Boards of Canada".to_string())));
        assert!(!dedup.insert((1001, "Autechre".to_string())));
        // Same artist, different release
        assert!(dedup.insert((1002, "Autechre".to_string())));
    }

    #[test]
    fn test_track_artist_dedup() {
        let mut dedup = TrackArtistDedup::new();
        assert!(dedup.insert((1001, 1, "Autechre".to_string())));
        assert!(dedup.insert((1001, 2, "Autechre".to_string())));
        assert!(!dedup.insert((1001, 1, "Autechre".to_string())));
        assert!(dedup.insert((1001, 1, "Boards of Canada".to_string())));
    }

    #[test]
    fn test_label_dedup() {
        let mut dedup = LabelDedup::new();
        assert!(dedup.insert((1001, "Warp".to_string())));
        assert!(dedup.insert((1001, "4AD".to_string())));
        assert!(!dedup.insert((1001, "Warp".to_string())));
        assert!(dedup.insert((1002, "Warp".to_string())));
    }
}
