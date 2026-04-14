//! Batch-buffered SQLite writer with FTS5 full-text search support.
//!
//! Ported from `discogs-xml-converter/src/sqlite_output.rs`. Provides a generic
//! writer that applies performance pragmas, supports transaction-wrapped batch
//! inserts, and can build FTS5 virtual tables.

mod writer;

pub use writer::{SqliteWriter, SqliteWriterConfig};

#[cfg(test)]
mod tests {
    use super::*;

    fn make_writer(dir: &std::path::Path, batch_size: usize) -> SqliteWriter {
        let config = SqliteWriterConfig {
            db_path: dir.join("test.db"),
            batch_size,
        };
        SqliteWriter::new(config).unwrap()
    }

    #[test]
    fn applies_performance_pragmas() {
        let dir = tempfile::tempdir().unwrap();
        let writer = make_writer(dir.path(), 100);

        let journal: String = writer
            .conn()
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal.to_lowercase(), "wal");

        let sync: i64 = writer
            .conn()
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(sync, 1); // NORMAL = 1

        let cache: i64 = writer
            .conn()
            .query_row("PRAGMA cache_size", [], |row| row.get(0))
            .unwrap();
        assert_eq!(cache, -256000);

        let temp: i64 = writer
            .conn()
            .query_row("PRAGMA temp_store", [], |row| row.get(0))
            .unwrap();
        assert_eq!(temp, 2); // MEMORY = 2
    }

    #[test]
    fn execute_ddl_creates_table() {
        let dir = tempfile::tempdir().unwrap();
        let writer = make_writer(dir.path(), 100);
        writer
            .execute_ddl("CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)")
            .unwrap();

        let count: i64 = writer
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn batch_insert_and_flush() {
        let dir = tempfile::tempdir().unwrap();
        let mut writer = make_writer(dir.path(), 100);
        writer
            .execute_ddl("CREATE TABLE releases (release_id INTEGER PRIMARY KEY, artist TEXT, title TEXT)")
            .unwrap();

        writer
            .insert(
                "INSERT INTO releases (release_id, artist, title) VALUES (?1, ?2, ?3)",
                &[&1i64 as &dyn rusqlite::types::ToSql, &"Autechre", &"Confield"],
            )
            .unwrap();
        writer
            .insert(
                "INSERT INTO releases (release_id, artist, title) VALUES (?1, ?2, ?3)",
                &[&2i64 as &dyn rusqlite::types::ToSql, &"Stereolab", &"Aluminum Tunes"],
            )
            .unwrap();
        writer.flush_batch().unwrap();

        let count: i64 = writer
            .conn()
            .query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(writer.total_written(), 2);
    }

    #[test]
    fn auto_flush_at_batch_size() {
        let dir = tempfile::tempdir().unwrap();
        let mut writer = make_writer(dir.path(), 2);
        writer
            .execute_ddl("CREATE TABLE t (id INTEGER PRIMARY KEY)")
            .unwrap();

        writer
            .insert("INSERT INTO t (id) VALUES (?1)", &[&1i64 as &dyn rusqlite::types::ToSql])
            .unwrap();
        writer
            .insert("INSERT INTO t (id) VALUES (?1)", &[&2i64 as &dyn rusqlite::types::ToSql])
            .unwrap();
        // batch_size=2, should auto-flush after 2nd insert
        assert_eq!(writer.total_written(), 2);

        // 3rd insert starts a new batch
        writer
            .insert("INSERT INTO t (id) VALUES (?1)", &[&3i64 as &dyn rusqlite::types::ToSql])
            .unwrap();
        writer.flush_batch().unwrap();
        assert_eq!(writer.total_written(), 3);
    }

    #[test]
    fn fts5_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let mut writer = make_writer(dir.path(), 100);
        writer
            .execute_ddl(
                "CREATE TABLE releases (
                    release_id INTEGER PRIMARY KEY,
                    artist TEXT NOT NULL,
                    title TEXT NOT NULL
                )",
            )
            .unwrap();

        let sql = "INSERT INTO releases (release_id, artist, title) VALUES (?1, ?2, ?3)";
        writer.insert(sql, &[&1i64 as &dyn rusqlite::types::ToSql, &"Autechre", &"Confield"]).unwrap();
        writer.insert(sql, &[&2i64 as &dyn rusqlite::types::ToSql, &"Stereolab", &"Aluminum Tunes"]).unwrap();
        writer.insert(sql, &[&3i64 as &dyn rusqlite::types::ToSql, &"Cat Power", &"Moon Pix"]).unwrap();
        writer.flush_batch().unwrap();

        writer
            .build_fts5_index(
                "releases_fts",
                "releases",
                "release_id",
                &["artist", "title"],
                "unicode61 remove_diacritics 2",
            )
            .unwrap();

        let count: i64 = writer
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM releases_fts WHERE releases_fts MATCH 'Autechre'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        let count: i64 = writer
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM releases_fts WHERE releases_fts MATCH 'Moon'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
