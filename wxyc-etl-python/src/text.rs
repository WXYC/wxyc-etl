use pyo3::prelude::*;
use std::collections::HashSet;

/// Register text submodule functions.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(normalize_artist_name, m)?)?;
    m.add_function(wrap_pyfunction!(strip_diacritics, m)?)?;
    m.add_function(wrap_pyfunction!(batch_normalize, m)?)?;
    m.add_function(wrap_pyfunction!(is_compilation_artist, m)?)?;
    m.add_function(wrap_pyfunction!(split_artist_name, m)?)?;
    m.add_function(wrap_pyfunction!(split_artist_name_contextual, m)?)?;
    Ok(())
}

/// Normalize an artist name for matching.
///
/// Accepts None (returns "") so Python callers don't need to guard against NULL.
#[pyfunction]
fn normalize_artist_name(name: Option<&str>) -> String {
    match name {
        Some(n) => wxyc_etl::text::normalize_artist_name(n),
        None => String::new(),
    }
}

/// Strip diacritics via NFKD decomposition without lowercasing.
#[pyfunction]
fn strip_diacritics(s: &str) -> String {
    wxyc_etl::text::strip_diacritics(s)
}

/// Normalize a batch of artist names in one call.
#[pyfunction]
fn batch_normalize(names: Vec<String>) -> Vec<String> {
    wxyc_etl::text::batch_normalize(&names)
}

/// Check if an artist name indicates a compilation/soundtrack album.
#[pyfunction]
fn is_compilation_artist(name: &str) -> bool {
    wxyc_etl::text::is_compilation_artist(name)
}

/// Split a combined artist name into individual components (context-free).
///
/// Returns None if the name doesn't appear to be a multi-artist entry.
#[pyfunction]
fn split_artist_name(name: &str) -> Option<Vec<String>> {
    wxyc_etl::text::split_artist_name(name)
}

/// Context-aware artist name splitting.
///
/// Tries context-free splits first, then ampersand splits when at least one
/// component exists in `known_artists` (should contain normalized names).
#[pyfunction]
fn split_artist_name_contextual(
    name: &str,
    known_artists: HashSet<String>,
) -> Option<Vec<String>> {
    wxyc_etl::text::split_artist_name_contextual(name, &known_artists)
}
