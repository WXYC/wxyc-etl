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

    // --- Integration tests: schema creation and round-trip data integrity ---

    #[test]
    fn round_trip_wxyc_data() {
        // Insert WXYC example data and verify it can be read back exactly
        let dir = tempfile::tempdir().unwrap();
        let mut writer = make_writer(dir.path(), 100);
        writer.execute_ddl(
            "CREATE TABLE releases (
                release_id INTEGER PRIMARY KEY,
                artist TEXT NOT NULL,
                title TEXT NOT NULL,
                label TEXT,
                country TEXT
            )",
        ).unwrap();

        let wxyc_data: Vec<(i64, &str, &str, &str, &str)> = vec![
            (1001, "Autechre", "Confield", "Warp", "UK"),
            (1002, "Stereolab", "Aluminum Tunes", "Duophonic", "UK"),
            (1003, "Cat Power", "Moon Pix", "Matador Records", "US"),
            (1004, "Juana Molina", "DOGA", "Sonamos", "AR"),
            (1005, "Jessica Pratt", "On Your Own Love Again", "Drag City", "US"),
            (1006, "Chuquimamani-Condori", "Edits", "self-released", "BO"),
            (1007, "Duke Ellington & John Coltrane", "Duke Ellington & John Coltrane", "Impulse Records", "US"),
            (1008, "Sessa", "Pequena Vertigem de Amor", "Mexican Summer", "BR"),
        ];

        let sql = "INSERT INTO releases (release_id, artist, title, label, country) VALUES (?1, ?2, ?3, ?4, ?5)";
        for (id, artist, title, label, country) in &wxyc_data {
            writer.insert(sql, &[
                id as &dyn rusqlite::types::ToSql,
                artist as &dyn rusqlite::types::ToSql,
                title as &dyn rusqlite::types::ToSql,
                label as &dyn rusqlite::types::ToSql,
                country as &dyn rusqlite::types::ToSql,
            ]).unwrap();
        }
        writer.flush_batch().unwrap();

        // Verify count
        let count: i64 = writer.conn()
            .query_row("SELECT COUNT(*) FROM releases", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 8);
        assert_eq!(writer.total_written(), 8);

        // Verify round-trip: read back each row and compare
        let mut stmt = writer.conn()
            .prepare("SELECT release_id, artist, title, label, country FROM releases ORDER BY release_id")
            .unwrap();
        let rows: Vec<(i64, String, String, String, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();

        assert_eq!(rows.len(), 8);
        for (i, (id, artist, title, label, country)) in rows.iter().enumerate() {
            let (exp_id, exp_artist, exp_title, exp_label, exp_country) = wxyc_data[i];
            assert_eq!(*id, exp_id);
            assert_eq!(artist, exp_artist);
            assert_eq!(title, exp_title);
            assert_eq!(label, exp_label);
            assert_eq!(country, exp_country);
        }
    }

    #[test]
    fn multiple_tables_schema() {
        // Create a multi-table schema and verify foreign key-like integrity
        let dir = tempfile::tempdir().unwrap();
        let mut writer = make_writer(dir.path(), 100);
        writer.execute_ddl(
            "CREATE TABLE artists (
                artist_id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            );
            CREATE TABLE releases (
                release_id INTEGER PRIMARY KEY,
                title TEXT NOT NULL,
                artist_id INTEGER NOT NULL
            );
            CREATE TABLE release_tracks (
                track_id INTEGER PRIMARY KEY,
                release_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                position INTEGER NOT NULL
            );",
        ).unwrap();

        // Insert artists
        writer.insert(
            "INSERT INTO artists (artist_id, name) VALUES (?1, ?2)",
            &[&1i64 as &dyn rusqlite::types::ToSql, &"Autechre"],
        ).unwrap();
        writer.insert(
            "INSERT INTO artists (artist_id, name) VALUES (?1, ?2)",
            &[&2i64 as &dyn rusqlite::types::ToSql, &"Stereolab"],
        ).unwrap();

        // Insert releases
        writer.insert(
            "INSERT INTO releases (release_id, title, artist_id) VALUES (?1, ?2, ?3)",
            &[&101i64 as &dyn rusqlite::types::ToSql, &"Confield", &1i64],
        ).unwrap();
        writer.insert(
            "INSERT INTO releases (release_id, title, artist_id) VALUES (?1, ?2, ?3)",
            &[&102i64 as &dyn rusqlite::types::ToSql, &"Aluminum Tunes", &2i64],
        ).unwrap();

        // Insert tracks
        writer.insert(
            "INSERT INTO release_tracks (track_id, release_id, title, position) VALUES (?1, ?2, ?3, ?4)",
            &[&1001i64 as &dyn rusqlite::types::ToSql, &101i64, &"VI Scose Poise", &1i64],
        ).unwrap();
        writer.flush_batch().unwrap();

        // Verify join query works
        let result: (String, String, String) = writer.conn().query_row(
            "SELECT a.name, r.title, t.title
             FROM release_tracks t
             JOIN releases r ON t.release_id = r.release_id
             JOIN artists a ON r.artist_id = a.artist_id
             WHERE t.track_id = 1001",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).unwrap();
        assert_eq!(result.0, "Autechre");
        assert_eq!(result.1, "Confield");
        assert_eq!(result.2, "VI Scose Poise");
    }

    #[test]
    fn fts5_search_wxyc_data() {
        // Comprehensive FTS5 search test with WXYC example data
        let dir = tempfile::tempdir().unwrap();
        let mut writer = make_writer(dir.path(), 100);
        writer.execute_ddl(
            "CREATE TABLE releases (
                release_id INTEGER PRIMARY KEY,
                artist TEXT NOT NULL,
                title TEXT NOT NULL
            )",
        ).unwrap();

        let sql = "INSERT INTO releases (release_id, artist, title) VALUES (?1, ?2, ?3)";
        let entries: Vec<(i64, &str, &str)> = vec![
            (1, "Autechre", "Confield"),
            (2, "Stereolab", "Aluminum Tunes"),
            (3, "Cat Power", "Moon Pix"),
            (4, "Juana Molina", "DOGA"),
            (5, "Jessica Pratt", "On Your Own Love Again"),
            (6, "Father John Misty", "I Love You, Honeybear"),
            (7, "Duke Ellington & John Coltrane", "Duke Ellington & John Coltrane"),
        ];
        for (id, artist, title) in &entries {
            writer.insert(sql, &[
                id as &dyn rusqlite::types::ToSql,
                artist as &dyn rusqlite::types::ToSql,
                title as &dyn rusqlite::types::ToSql,
            ]).unwrap();
        }
        writer.flush_batch().unwrap();

        writer.build_fts5_index(
            "releases_fts",
            "releases",
            "release_id",
            &["artist", "title"],
            "unicode61 remove_diacritics 2",
        ).unwrap();

        // Search by artist name
        let count: i64 = writer.conn().query_row(
            "SELECT COUNT(*) FROM releases_fts WHERE artist MATCH 'Stereolab'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);

        // Search by title
        let count: i64 = writer.conn().query_row(
            "SELECT COUNT(*) FROM releases_fts WHERE title MATCH 'Love'",
            [],
            |row| row.get(0),
        ).unwrap();
        // "On Your Own Love Again" and "I Love You, Honeybear" both match
        assert_eq!(count, 2);

        // Search across all columns
        let count: i64 = writer.conn().query_row(
            "SELECT COUNT(*) FROM releases_fts WHERE releases_fts MATCH 'John'",
            [],
            |row| row.get(0),
        ).unwrap();
        // "Father John Misty" and "Duke Ellington & John Coltrane" (artist + title)
        assert!(count >= 2, "expected at least 2 results for 'John', got {}", count);

        // No results for unknown term
        let count: i64 = writer.conn().query_row(
            "SELECT COUNT(*) FROM releases_fts WHERE releases_fts MATCH 'xyznonexistent'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn batch_size_boundary_conditions() {
        // Test batch_size=1: every insert auto-flushes
        let dir = tempfile::tempdir().unwrap();
        let mut writer = make_writer(dir.path(), 1);
        writer.execute_ddl("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)").unwrap();

        writer.insert(
            "INSERT INTO t (id, name) VALUES (?1, ?2)",
            &[&1i64 as &dyn rusqlite::types::ToSql, &"Autechre"],
        ).unwrap();
        assert_eq!(writer.total_written(), 1, "batch_size=1 should auto-flush after each insert");

        writer.insert(
            "INSERT INTO t (id, name) VALUES (?1, ?2)",
            &[&2i64 as &dyn rusqlite::types::ToSql, &"Stereolab"],
        ).unwrap();
        assert_eq!(writer.total_written(), 2);

        // Verify data
        let count: i64 = writer.conn()
            .query_row("SELECT COUNT(*) FROM t", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn replaces_existing_database() {
        // SqliteWriter::new should remove an existing database file
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create first writer and populate
        {
            let mut writer = make_writer(dir.path(), 100);
            writer.execute_ddl("CREATE TABLE old_table (id INTEGER)").unwrap();
            writer.insert(
                "INSERT INTO old_table (id) VALUES (?1)",
                &[&1i64 as &dyn rusqlite::types::ToSql],
            ).unwrap();
            writer.flush_batch().unwrap();
        }
        assert!(db_path.exists());

        // Create second writer at the same path -- should replace
        let writer2 = make_writer(dir.path(), 100);
        let count: i64 = writer2.conn().query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='old_table'",
            [],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 0, "old table should not exist after replacement");
    }
}
