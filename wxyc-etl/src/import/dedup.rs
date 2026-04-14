use std::collections::HashSet;

/// Tracks seen rows by unique key tuple during CSV import.
///
/// Generalizes the `seen: set[tuple]` pattern from `import_csv.py`.
pub struct ImportDedupSet {
    seen: HashSet<Vec<String>>,
}

impl ImportDedupSet {
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    /// Insert a key. Returns `true` if the key was not already present (i.e., not a duplicate).
    pub fn insert(&mut self, key: &[&str]) -> bool {
        let owned: Vec<String> = key.iter().map(|s| s.to_string()).collect();
        self.seen.insert(owned)
    }

    /// Return the number of unique keys seen so far.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    /// Return true if no keys have been inserted.
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

impl Default for ImportDedupSet {
    fn default() -> Self {
        Self::new()
    }
}
