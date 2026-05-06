use pyo3::exceptions::PyDeprecationWarning;
use pyo3::prelude::*;
use std::collections::HashSet;
use std::ffi::CString;

/// Register text submodule functions.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(normalize_artist_name, m)?)?;
    m.add_function(wrap_pyfunction!(strip_diacritics, m)?)?;
    m.add_function(wrap_pyfunction!(batch_normalize, m)?)?;
    m.add_function(wrap_pyfunction!(is_compilation_artist, m)?)?;
    m.add_function(wrap_pyfunction!(split_artist_name, m)?)?;
    m.add_function(wrap_pyfunction!(split_artist_name_contextual, m)?)?;
    m.add_function(wrap_pyfunction!(to_storage_form, m)?)?;
    m.add_function(wrap_pyfunction!(to_match_form, m)?)?;
    m.add_function(wrap_pyfunction!(to_ascii_form, m)?)?;
    m.add_function(wrap_pyfunction!(batch_to_storage_form, m)?)?;
    m.add_function(wrap_pyfunction!(batch_to_match_form, m)?)?;
    m.add_function(wrap_pyfunction!(batch_to_ascii_form, m)?)?;
    Ok(())
}

/// Fire a Python `DeprecationWarning` whose stacklevel points at the caller's
/// frame, not at this helper. Used by the WX-2.2.5 legacy normalizer wrappers.
fn warn_legacy(py: Python<'_>, name: &str, replacement: &str) -> PyResult<()> {
    let msg = CString::new(format!(
        "wxyc_etl.text.{name} is deprecated; use {replacement} instead. \
         See plans/mojibake-prevention/2-normalizer-charter.md for migration guidance."
    ))
    .expect("deprecation message contains no NUL bytes");
    PyErr::warn(py, &py.get_type::<PyDeprecationWarning>(), &msg, 2)
}

/// Normalize an artist name for matching.
///
/// Accepts None (returns "") so Python callers don't need to guard against NULL.
///
/// Deprecated: use `to_match_form` (or `to_storage_form` / `to_ascii_form`) instead.
#[pyfunction]
fn normalize_artist_name(py: Python<'_>, name: Option<&str>) -> PyResult<String> {
    warn_legacy(py, "normalize_artist_name", "to_match_form")?;
    #[allow(deprecated)]
    Ok(match name {
        Some(n) => wxyc_etl::text::normalize_artist_name(n),
        None => String::new(),
    })
}

/// Strip diacritics via NFKD decomposition without lowercasing.
///
/// Deprecated: use `to_match_form` (or `to_storage_form` / `to_ascii_form`) instead.
#[pyfunction]
fn strip_diacritics(py: Python<'_>, s: &str) -> PyResult<String> {
    warn_legacy(py, "strip_diacritics", "to_match_form")?;
    #[allow(deprecated)]
    Ok(wxyc_etl::text::strip_diacritics(s))
}

/// Normalize a batch of artist names in one call.
///
/// Deprecated: use `batch_to_match_form` (or `batch_to_storage_form` / `batch_to_ascii_form`) instead.
#[pyfunction]
fn batch_normalize(py: Python<'_>, names: Vec<String>) -> PyResult<Vec<String>> {
    warn_legacy(py, "batch_normalize", "batch_to_match_form")?;
    #[allow(deprecated)]
    Ok(wxyc_etl::text::batch_normalize(&names))
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
fn split_artist_name_contextual(name: &str, known_artists: HashSet<String>) -> Option<Vec<String>> {
    wxyc_etl::text::split_artist_name_contextual(name, &known_artists)
}

/// WX-2 storage form: mojibake fix + NFC + trim.
///
/// Accepts None (returns "") so Python callers don't need to guard NULL values
/// from DB columns or optional API fields.
#[pyfunction]
fn to_storage_form(s: Option<&str>) -> String {
    s.map(wxyc_etl::text::to_storage_form).unwrap_or_default()
}

/// WX-2 match form: NFKC + lowercase + selective combining-strip + folds + Cf-strip.
///
/// Accepts None (returns "") so Python callers don't need to guard NULL values
/// from DB columns or optional API fields.
#[pyfunction]
fn to_match_form(s: Option<&str>) -> String {
    s.map(wxyc_etl::text::to_match_form).unwrap_or_default()
}

/// WX-2 ASCII form: match form + emoji-strip + Ё override + deunicode + ASCII-only.
///
/// Accepts None (returns "") so Python callers don't need to guard NULL values
/// from DB columns or optional API fields.
#[pyfunction]
fn to_ascii_form(s: Option<&str>) -> String {
    s.map(wxyc_etl::text::to_ascii_form).unwrap_or_default()
}

/// Apply [`to_storage_form`] to each input in one cross-FFI call.
#[pyfunction]
fn batch_to_storage_form(inputs: Vec<String>) -> Vec<String> {
    wxyc_etl::text::batch_to_storage_form(&inputs)
}

/// Apply [`to_match_form`] to each input in one cross-FFI call.
#[pyfunction]
fn batch_to_match_form(inputs: Vec<String>) -> Vec<String> {
    wxyc_etl::text::batch_to_match_form(&inputs)
}

/// Apply [`to_ascii_form`] to each input in one cross-FFI call.
#[pyfunction]
fn batch_to_ascii_form(inputs: Vec<String>) -> Vec<String> {
    wxyc_etl::text::batch_to_ascii_form(&inputs)
}
