//! library.db schema constants.
//!
//! Matches the SQLite schema in `discogs-cache/scripts/export_to_sqlite.py`.

pub const LIBRARY_TABLE: &str = "library";

pub const LIBRARY_COLUMNS: &[&str] = &[
    "id",
    "title",
    "artist",
    "call_letters",
    "artist_call_number",
    "release_call_number",
    "genre",
    "format",
    "alternate_artist_name",
];

pub const LIBRARY_DDL: &str = "\
CREATE TABLE library (
    id INTEGER PRIMARY KEY,
    title TEXT,
    artist TEXT,
    call_letters TEXT,
    artist_call_number INTEGER,
    release_call_number INTEGER,
    genre TEXT,
    format TEXT,
    alternate_artist_name TEXT
)";

pub const LIBRARY_FTS_TABLE: &str = "library_fts";

pub const LIBRARY_FTS_DDL: &str = "\
CREATE VIRTUAL TABLE library_fts USING fts5(
    title,
    artist,
    alternate_artist_name,
    content='library',
    content_rowid='id'
)";
