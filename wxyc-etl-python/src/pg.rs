use pyo3::prelude::*;

/// Register pg submodule functions.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(to_pg_text_form, m)?)?;
    m.add_function(wrap_pyfunction!(batch_to_pg_text_form, m)?)?;
    Ok(())
}

/// Make `s` acceptable for storage in a PostgreSQL TEXT column.
///
/// Strips U+0000 (NUL) bytes, which the SQL standard forbids in character
/// types. Accepts None (returns "") so Python callers don't need to guard
/// NULL values from DB columns or optional API fields.
///
/// See WX-3.B / WXYC/docs#18 for the strip-at-boundary policy.
#[pyfunction]
fn to_pg_text_form(s: Option<&str>) -> String {
    s.map(|s| wxyc_etl::pg::to_pg_text_form(s).into_owned())
        .unwrap_or_default()
}

/// Apply [`to_pg_text_form`] to each input in one cross-FFI call.
#[pyfunction]
fn batch_to_pg_text_form(items: Vec<String>) -> Vec<String> {
    items
        .iter()
        .map(|s| wxyc_etl::pg::to_pg_text_form(s).into_owned())
        .collect()
}
