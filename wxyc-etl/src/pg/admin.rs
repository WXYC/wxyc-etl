//! PostgreSQL administrative operations for bulk imports.
//!
//! Provides `SET UNLOGGED` / `SET LOGGED` table toggles and `VACUUM FULL`
//! for optimizing bulk data loading performance.

use anyhow::{Context, Result};
use log::info;

/// Set tables to UNLOGGED for faster bulk imports (disables WAL).
///
/// **Warning**: UNLOGGED tables are not crash-safe. Data will be lost if
/// the server crashes while tables are in this mode. Always call
/// [`set_tables_logged()`] after the import completes.
///
/// Table names are validated to contain only alphanumeric characters and
/// underscores to prevent SQL injection.
pub fn set_tables_unlogged(client: &mut postgres::Client, tables: &[&str]) -> Result<()> {
    for table in tables {
        validate_table_name(table)?;
        let sql = format!("ALTER TABLE {} SET UNLOGGED", table);
        client
            .execute(&sql, &[])
            .with_context(|| format!("failed to set {} UNLOGGED", table))?;
        info!("Set {} to UNLOGGED", table);
    }
    Ok(())
}

/// Restore tables to LOGGED mode (re-enables WAL durability).
///
/// Table names are validated to contain only alphanumeric characters and
/// underscores to prevent SQL injection.
pub fn set_tables_logged(client: &mut postgres::Client, tables: &[&str]) -> Result<()> {
    for table in tables {
        validate_table_name(table)?;
        let sql = format!("ALTER TABLE {} SET LOGGED", table);
        client
            .execute(&sql, &[])
            .with_context(|| format!("failed to set {} LOGGED", table))?;
        info!("Set {} to LOGGED", table);
    }
    Ok(())
}

/// Run `VACUUM FULL` on tables to reclaim space after bulk deletes/updates.
///
/// Table names are validated to contain only alphanumeric characters and
/// underscores to prevent SQL injection.
pub fn vacuum_full(client: &mut postgres::Client, tables: &[&str]) -> Result<()> {
    for table in tables {
        validate_table_name(table)?;
        let sql = format!("VACUUM FULL {}", table);
        client
            .execute(&sql, &[])
            .with_context(|| format!("failed to VACUUM FULL {}", table))?;
        info!("VACUUM FULL {}", table);
    }
    Ok(())
}

/// Validate that a table name contains only safe characters (alphanumeric + underscore).
fn validate_table_name(name: &str) -> Result<()> {
    if name.is_empty() {
        anyhow::bail!("table name cannot be empty");
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        anyhow::bail!(
            "table name {:?} contains unsafe characters; only alphanumeric and underscore allowed",
            name
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_table_name_valid() {
        assert!(validate_table_name("release").is_ok());
        assert!(validate_table_name("release_artist").is_ok());
        assert!(validate_table_name("table123").is_ok());
    }

    #[test]
    fn test_validate_table_name_empty() {
        assert!(validate_table_name("").is_err());
    }

    #[test]
    fn test_validate_table_name_injection() {
        assert!(validate_table_name("release; DROP TABLE release").is_err());
        assert!(validate_table_name("release--comment").is_err());
        assert!(validate_table_name("public.release").is_err());
    }

    /// Helper to get a test DB connection, or skip the test.
    fn test_db_client() -> Option<postgres::Client> {
        let url = std::env::var("TEST_DATABASE_URL").ok()?;
        postgres::Client::connect(&url, postgres::NoTls).ok()
    }

    #[test]
    fn test_set_unlogged_and_logged() {
        let mut client = match test_db_client() {
            Some(c) => c,
            None => return, // skip if no DB
        };

        // Set up a test table
        client
            .batch_execute(
                "DROP TABLE IF EXISTS _pg_admin_test;
                 CREATE TABLE _pg_admin_test (id integer PRIMARY KEY);",
            )
            .unwrap();

        // Set UNLOGGED
        set_tables_unlogged(&mut client, &["_pg_admin_test"]).unwrap();

        // Verify via pg_class
        let row = client
            .query_one(
                "SELECT relpersistence FROM pg_class WHERE relname = '_pg_admin_test'",
                &[],
            )
            .unwrap();
        assert_eq!(row.get::<_, i8>(0), b'u' as i8);

        // Set LOGGED
        set_tables_logged(&mut client, &["_pg_admin_test"]).unwrap();

        let row = client
            .query_one(
                "SELECT relpersistence FROM pg_class WHERE relname = '_pg_admin_test'",
                &[],
            )
            .unwrap();
        assert_eq!(row.get::<_, i8>(0), b'p' as i8);

        // Clean up
        client.execute("DROP TABLE _pg_admin_test", &[]).unwrap();
    }

    #[test]
    fn test_vacuum_full() {
        let mut client = match test_db_client() {
            Some(c) => c,
            None => return, // skip if no DB
        };

        client
            .batch_execute(
                "DROP TABLE IF EXISTS _pg_vacuum_test;
                 CREATE TABLE _pg_vacuum_test (id integer PRIMARY KEY);
                 INSERT INTO _pg_vacuum_test VALUES (1), (2), (3);
                 DELETE FROM _pg_vacuum_test;",
            )
            .unwrap();

        // Should not error
        vacuum_full(&mut client, &["_pg_vacuum_test"]).unwrap();

        // Clean up
        client.execute("DROP TABLE _pg_vacuum_test", &[]).unwrap();
    }
}
