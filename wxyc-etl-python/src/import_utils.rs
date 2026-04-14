use std::collections::HashSet;

use pyo3::prelude::*;

/// Register import_utils submodule.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<DedupSet>()?;
    Ok(())
}

/// Tracks seen rows by unique key tuple during CSV import.
///
/// Accepts tuple[str | None, ...] keys via add() and __contains__().
/// None values are converted to "" for dedup purposes.
#[pyclass]
struct DedupSet {
    seen: HashSet<Vec<String>>,
}

#[pymethods]
impl DedupSet {
    #[new]
    fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    /// Add a key tuple. Returns True if the key was new (not a duplicate).
    fn add(&mut self, key: Vec<Option<String>>) -> bool {
        let owned: Vec<String> = key.into_iter().map(|s| s.unwrap_or_default()).collect();
        self.seen.insert(owned)
    }

    /// Check if a key tuple has been seen (via Python's `in` operator).
    fn __contains__(&self, key: Vec<Option<String>>) -> bool {
        let owned: Vec<String> = key.into_iter().map(|s| s.unwrap_or_default()).collect();
        self.seen.contains(&owned)
    }

    fn __len__(&self) -> usize {
        self.seen.len()
    }
}
