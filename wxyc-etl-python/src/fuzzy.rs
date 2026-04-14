use pyo3::prelude::*;

use wxyc_etl::fuzzy::resolve;

/// Register fuzzy submodule functions.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(batch_fuzzy_resolve, m)?)?;
    m.add_function(wrap_pyfunction!(jaro_winkler_similarity, m)?)?;
    Ok(())
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
///     List of Optional[str] — the best matching catalog entry or None.
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

/// Compute Jaro-Winkler similarity between two strings.
///
/// Returns a value between 0.0 (no similarity) and 1.0 (identical).
#[pyfunction]
fn jaro_winkler_similarity(a: &str, b: &str) -> f64 {
    wxyc_etl::fuzzy::metrics::jaro_winkler_similarity(a, b)
}
