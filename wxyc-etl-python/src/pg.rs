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
/// types. `None` passes through as `None` so callers writing through
/// `COALESCE(EXCLUDED.col, table.col)` upserts (or psycopg's COPY `\N`
/// sentinel) continue to skip absent fields rather than overwriting them
/// with the empty string.
///
/// See WX-3.B / WXYC/docs#18 for the strip-at-boundary policy.
#[pyfunction]
fn to_pg_text_form(s: Option<&str>) -> Option<String> {
    s.map(|s| wxyc_etl::pg::to_pg_text_form(s).into_owned())
}

/// Apply [`to_pg_text_form`] to each input in one cross-FFI call.
///
/// Each element is treated independently: `None` entries pass through as
/// `None`, `str` entries get the NUL strip. The output list is the same
/// length as the input.
#[pyfunction]
fn batch_to_pg_text_form(items: Vec<Option<String>>) -> Vec<Option<String>> {
    items
        .into_iter()
        .map(|opt| opt.map(|s| wxyc_etl::pg::to_pg_text_form(&s).into_owned()))
        .collect()
}
