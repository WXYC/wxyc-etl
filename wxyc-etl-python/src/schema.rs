use pyo3::prelude::*;

use wxyc_etl::schema::{discogs, entity, library};

/// Register schema submodule functions and constants.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(discogs_tables, m)?)?;
    m.add_function(wrap_pyfunction!(discogs_release_columns, m)?)?;
    m.add_function(wrap_pyfunction!(library_ddl, m)?)?;
    m.add_function(wrap_pyfunction!(library_columns, m)?)?;
    m.add_function(wrap_pyfunction!(entity_identity_ddl, m)?)?;
    m.add_function(wrap_pyfunction!(entity_identity_columns, m)?)?;

    // Module-level constants
    m.add("RELEASE_TABLE", discogs::RELEASE_TABLE)?;
    m.add("RELEASE_ARTIST_TABLE", discogs::RELEASE_ARTIST_TABLE)?;
    m.add("RELEASE_LABEL_TABLE", discogs::RELEASE_LABEL_TABLE)?;
    m.add("RELEASE_GENRE_TABLE", discogs::RELEASE_GENRE_TABLE)?;
    m.add("RELEASE_STYLE_TABLE", discogs::RELEASE_STYLE_TABLE)?;
    m.add("RELEASE_TRACK_TABLE", discogs::RELEASE_TRACK_TABLE)?;
    m.add(
        "RELEASE_TRACK_ARTIST_TABLE",
        discogs::RELEASE_TRACK_ARTIST_TABLE,
    )?;
    m.add("ARTIST_TABLE", discogs::ARTIST_TABLE)?;
    m.add("ARTIST_ALIAS_TABLE", discogs::ARTIST_ALIAS_TABLE)?;
    m.add("CACHE_METADATA_TABLE", discogs::CACHE_METADATA_TABLE)?;
    m.add("LIBRARY_TABLE", library::LIBRARY_TABLE)?;
    m.add("ENTITY_IDENTITY_TABLE", entity::ENTITY_IDENTITY_TABLE)?;
    m.add("RECONCILIATION_LOG_TABLE", entity::RECONCILIATION_LOG_TABLE)?;

    Ok(())
}

/// List of all discogs-cache table names.
#[pyfunction]
fn discogs_tables() -> Vec<String> {
    discogs::ALL_TABLES.iter().map(|s| s.to_string()).collect()
}

/// Release table column names.
#[pyfunction]
fn discogs_release_columns() -> Vec<String> {
    discogs::RELEASE_COLUMNS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Library.db DDL.
#[pyfunction]
fn library_ddl() -> String {
    library::LIBRARY_DDL.to_string()
}

/// Library table column names.
#[pyfunction]
fn library_columns() -> Vec<String> {
    library::LIBRARY_COLUMNS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Entity identity DDL.
#[pyfunction]
fn entity_identity_ddl() -> String {
    entity::ENTITY_IDENTITY_DDL.to_string()
}

/// Entity identity column names.
#[pyfunction]
fn entity_identity_columns() -> Vec<String> {
    entity::ENTITY_IDENTITY_COLUMNS
        .iter()
        .map(|s| s.to_string())
        .collect()
}
