use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::Connection;

/// Configuration for a `SqliteWriter`.
pub struct SqliteWriterConfig {
    pub db_path: PathBuf,
    pub batch_size: usize,
}

/// Batch-buffered SQLite writer with FTS5 support.
///
/// Ported from `discogs-xml-converter/src/sqlite_output.rs`. Applies performance
/// pragmas (WAL, NORMAL sync, 256 MB cache, memory temp store), supports
/// transaction-wrapped batch inserts, and can build FTS5 virtual tables.
pub struct SqliteWriter {
    conn: Connection,
    batch_size: usize,
    total_written: usize,
    in_transaction: bool,
    batch_count: usize,
}

impl SqliteWriter {
    /// Create or replace the database at `config.db_path` and apply performance pragmas.
    pub fn new(config: SqliteWriterConfig) -> Result<Self> {
        if config.db_path.exists() {
            std::fs::remove_file(&config.db_path).with_context(|| {
                format!(
                    "removing existing database: {}",
                    config.db_path.display()
                )
            })?;
        }

        let conn = Connection::open(&config.db_path).with_context(|| {
            format!("creating SQLite database: {}", config.db_path.display())
        })?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -256000;
             PRAGMA temp_store = MEMORY;",
        )
        .context("applying performance pragmas")?;

        Ok(Self {
            conn,
            batch_size: config.batch_size,
            total_written: 0,
            in_transaction: false,
            batch_count: 0,
        })
    }

    /// Execute schema creation SQL (CREATE TABLE, etc.).
    pub fn execute_ddl(&self, sql: &str) -> Result<()> {
        self.conn
            .execute_batch(sql)
            .with_context(|| "executing DDL".to_string())
    }

    /// Return a reference to the underlying connection for direct queries.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Begin a batch transaction. Rows inserted via the returned `SqliteBatch`
    /// accumulate until `flush_batch()` commits them.
    pub fn begin_batch(&mut self) -> Result<()> {
        if !self.in_transaction {
            self.conn
                .execute_batch("BEGIN TRANSACTION")
                .context("beginning batch transaction")?;
            self.in_transaction = true;
            self.batch_count = 0;
        }
        Ok(())
    }

    /// Insert a row within the current batch. Automatically flushes when
    /// `batch_size` rows have been inserted.
    pub fn insert(&mut self, sql: &str, params: &[&dyn rusqlite::types::ToSql]) -> Result<()> {
        if !self.in_transaction {
            self.begin_batch()?;
        }
        self.conn
            .execute(sql, params)
            .with_context(|| "inserting row".to_string())?;
        self.batch_count += 1;

        if self.batch_count >= self.batch_size {
            self.flush_batch()?;
        }
        Ok(())
    }

    /// Commit the current batch transaction.
    pub fn flush_batch(&mut self) -> Result<()> {
        if self.in_transaction {
            self.conn
                .execute_batch("COMMIT")
                .context("committing batch")?;
            self.total_written += self.batch_count;
            self.in_transaction = false;
            self.batch_count = 0;
        }
        Ok(())
    }

    /// Build an FTS5 virtual table and rebuild its index from a content table.
    pub fn build_fts5_index(
        &self,
        table: &str,
        content_table: &str,
        content_rowid: &str,
        columns: &[&str],
        tokenizer: &str,
    ) -> Result<()> {
        let cols = columns.join(",\n                ");
        let sql = format!(
            "CREATE VIRTUAL TABLE {table} USING fts5(
                {cols},
                content={content_table},
                content_rowid={content_rowid},
                tokenize='{tokenizer}'
            );
            INSERT INTO {table}({table}) VALUES('rebuild');",
        );
        self.conn
            .execute_batch(&sql)
            .with_context(|| format!("building FTS5 index {table}"))
    }

    /// Return the total number of rows written (across all flushed batches).
    pub fn total_written(&self) -> usize {
        self.total_written
    }
}
