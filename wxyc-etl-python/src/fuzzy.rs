use std::collections::HashSet;

use pyo3::prelude::*;

use wxyc_etl::fuzzy::{
    batch_classify_releases as rust_batch_classify,
    batch_filter_artists as rust_batch_filter,
    resolve, Classification, ClassifyConfig, LibraryIndex,
};

/// Register fuzzy submodule functions.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(jaro_winkler_similarity, m)?)?;
    m.add_function(wrap_pyfunction!(token_set_ratio, m)?)?;
    m.add_function(wrap_pyfunction!(token_sort_ratio, m)?)?;
    m.add_function(wrap_pyfunction!(batch_fuzzy_resolve, m)?)?;
    m.add_function(wrap_pyfunction!(batch_filter_artists, m)?)?;
    m.add_function(wrap_pyfunction!(batch_classify_releases, m)?)?;
    Ok(())
}

/// Compute Jaro-Winkler similarity between two strings.
///
/// Returns a value between 0.0 (no similarity) and 1.0 (identical).
#[pyfunction]
fn jaro_winkler_similarity(a: &str, b: &str) -> f64 {
    wxyc_etl::fuzzy::jaro_winkler_similarity(a, b)
}

/// Token set ratio: compares intersection/remainder token sets.
///
/// Returns a value between 0.0 and 1.0.
#[pyfunction]
fn token_set_ratio(a: &str, b: &str) -> f64 {
    wxyc_etl::fuzzy::token_set_ratio(a, b)
}

/// Token sort ratio: sort tokens alphabetically, then compare.
///
/// Returns a value between 0.0 and 1.0.
#[pyfunction]
fn token_sort_ratio(a: &str, b: &str) -> f64 {
    wxyc_etl::fuzzy::token_sort_ratio(a, b)
}

/// Resolve each name to the best Jaro-Winkler match in `catalog`.
///
/// For each name, finds the top `limit` candidates above `threshold`,
/// then applies the `ambiguity_threshold` guard: if the top two scores
/// differ by less than this margin, the match is rejected (returns None).
///
/// Args:
///     names: List of query strings to resolve.
///     catalog: List of candidate strings to match against.
///     threshold: Minimum Jaro-Winkler similarity to accept (0.0-1.0).
///     limit: Number of top candidates to consider for ambiguity (default 2).
///     ambiguity_threshold: Minimum score gap between top-2 to accept (default 0.02).
///
/// Returns:
///     List of Optional[str] -- the best matching catalog entry or None.
#[pyfunction]
#[pyo3(signature = (names, catalog, threshold, limit=2, ambiguity_threshold=0.02))]
fn batch_fuzzy_resolve(
    names: Vec<String>,
    catalog: Vec<String>,
    threshold: f64,
    limit: usize,
    ambiguity_threshold: f64,
) -> Vec<Option<String>> {
    resolve::batch_fuzzy_resolve(&names, &catalog, threshold, limit, ambiguity_threshold)
}

/// Filter artist names against a library set.
///
/// Returns a list of booleans: True if the normalized name is in `library_set`.
#[pyfunction]
fn batch_filter_artists(artist_names: Vec<String>, library_set: HashSet<String>) -> Vec<bool> {
    rust_batch_filter(&artist_names, &library_set)
}

/// Classify releases as "keep", "prune", or "review" against a library index.
///
/// Args:
///     artists: List of artist names.
///     titles: List of release titles (must be same length as artists).
///     library_pairs: List of (artist, title) tuples forming the library index.
///
/// Returns:
///     List of classification strings: "keep", "prune", or "review".
#[pyfunction]
fn batch_classify_releases(
    artists: Vec<String>,
    titles: Vec<String>,
    library_pairs: Vec<(String, String)>,
) -> PyResult<Vec<String>> {
    if artists.len() != titles.len() {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "artists and titles must have the same length",
        ));
    }
    let index = LibraryIndex::from_pairs(&library_pairs);
    let config = ClassifyConfig::default();
    let results = rust_batch_classify(&artists, &titles, &index, &config);
    Ok(results
        .into_iter()
        .map(|c| match c {
            Classification::Keep => "keep".to_string(),
            Classification::Prune => "prune".to_string(),
            Classification::Review => "review".to_string(),
        })
        .collect())
}
