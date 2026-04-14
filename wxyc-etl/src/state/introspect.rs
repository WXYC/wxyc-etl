//! Database introspection for inferring pipeline state.
//!
//! Ported from `discogs-cache/lib/db_introspect.py`. These functions inspect a
//! PostgreSQL database to determine which pipeline steps have already completed.

use anyhow::{Context, Result};

/// Return true if the table exists in the public schema.
pub fn table_exists(client: &mut postgres::Client, table_name: &str) -> Result<bool> {
    let row = client
        .query_one(
            "SELECT EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_schema = 'public' AND table_name = $1
            )",
            &[&table_name],
        )
        .context("checking table existence")?;
    Ok(row.get(0))
}

/// Return true if the table has at least one row.
pub fn table_has_rows(client: &mut postgres::Client, table_name: &str) -> Result<bool> {
    let query = format!("SELECT EXISTS (SELECT 1 FROM {} LIMIT 1)", table_name);
    let row = client
        .query_one(&query, &[])
        .with_context(|| format!("checking if {} has rows", table_name))?;
    Ok(row.get(0))
}

/// Return true if the column exists on the table.
pub fn column_exists(
    client: &mut postgres::Client,
    table_name: &str,
    column_name: &str,
) -> Result<bool> {
    let row = client
        .query_one(
            "SELECT EXISTS (
                SELECT 1 FROM information_schema.columns
                WHERE table_name = $1 AND column_name = $2
            )",
            &[&table_name, &column_name],
        )
        .context("checking column existence")?;
    Ok(row.get(0))
}

/// Return true if the index exists in the public schema.
pub fn index_exists(client: &mut postgres::Client, index_name: &str) -> Result<bool> {
    let row = client
        .query_one(
            "SELECT EXISTS (
                SELECT 1 FROM pg_indexes
                WHERE schemaname = 'public' AND indexname = $1
            )",
            &[&index_name],
        )
        .context("checking index existence")?;
    Ok(row.get(0))
}
