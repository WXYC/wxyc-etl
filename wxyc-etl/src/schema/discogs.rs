//! Discogs-cache table and column constants.
//!
//! Matches the PostgreSQL schema in `discogs-cache/schema/create_database.sql`.

// -- Release tables --

pub const RELEASE_TABLE: &str = "release";
pub const RELEASE_COLUMNS: &[&str] = &[
    "id",
    "title",
    "release_year",
    "country",
    "artwork_url",
    "released",
    "format",
    "master_id",
];

pub const RELEASE_ARTIST_TABLE: &str = "release_artist";
pub const RELEASE_ARTIST_COLUMNS: &[&str] = &[
    "release_id",
    "artist_id",
    "artist_name",
    "extra",
    "role",
];

pub const RELEASE_LABEL_TABLE: &str = "release_label";
pub const RELEASE_LABEL_COLUMNS: &[&str] = &[
    "release_id",
    "label_id",
    "label_name",
    "catno",
];

pub const RELEASE_GENRE_TABLE: &str = "release_genre";
pub const RELEASE_GENRE_COLUMNS: &[&str] = &["release_id", "genre"];

pub const RELEASE_STYLE_TABLE: &str = "release_style";
pub const RELEASE_STYLE_COLUMNS: &[&str] = &["release_id", "style"];

pub const RELEASE_TRACK_TABLE: &str = "release_track";
pub const RELEASE_TRACK_COLUMNS: &[&str] = &[
    "release_id",
    "sequence",
    "position",
    "title",
    "duration",
];

pub const RELEASE_TRACK_ARTIST_TABLE: &str = "release_track_artist";
pub const RELEASE_TRACK_ARTIST_COLUMNS: &[&str] = &[
    "release_id",
    "track_sequence",
    "artist_name",
];

// -- Artist detail tables --

pub const ARTIST_TABLE: &str = "artist";
pub const ARTIST_COLUMNS: &[&str] = &[
    "id",
    "name",
    "profile",
    "image_url",
    "fetched_at",
];

pub const ARTIST_ALIAS_TABLE: &str = "artist_alias";
pub const ARTIST_ALIAS_COLUMNS: &[&str] = &["artist_id", "alias_id", "alias_name"];

pub const ARTIST_NAME_VARIATION_TABLE: &str = "artist_name_variation";
pub const ARTIST_NAME_VARIATION_COLUMNS: &[&str] = &["artist_id", "name"];

pub const ARTIST_MEMBER_TABLE: &str = "artist_member";
pub const ARTIST_MEMBER_COLUMNS: &[&str] = &[
    "artist_id",
    "member_id",
    "member_name",
    "active",
];

pub const ARTIST_URL_TABLE: &str = "artist_url";
pub const ARTIST_URL_COLUMNS: &[&str] = &["artist_id", "url"];

// -- Metadata tables --

pub const CACHE_METADATA_TABLE: &str = "cache_metadata";
pub const CACHE_METADATA_COLUMNS: &[&str] = &[
    "release_id",
    "cached_at",
    "source",
    "last_validated",
];

/// All table names in the discogs-cache schema.
pub const ALL_TABLES: &[&str] = &[
    RELEASE_TABLE,
    RELEASE_ARTIST_TABLE,
    RELEASE_LABEL_TABLE,
    RELEASE_GENRE_TABLE,
    RELEASE_STYLE_TABLE,
    RELEASE_TRACK_TABLE,
    RELEASE_TRACK_ARTIST_TABLE,
    ARTIST_TABLE,
    ARTIST_ALIAS_TABLE,
    ARTIST_NAME_VARIATION_TABLE,
    ARTIST_MEMBER_TABLE,
    ARTIST_URL_TABLE,
    CACHE_METADATA_TABLE,
];
