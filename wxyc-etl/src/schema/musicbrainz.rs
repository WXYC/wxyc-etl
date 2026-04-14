//! MusicBrainz-cache table and column constants.
//!
//! Matches the PostgreSQL schema in `musicbrainz-cache/schema/create_database.sql`.

pub const MB_AREA_TYPE_TABLE: &str = "mb_area_type";
pub const MB_AREA_TYPE_COLUMNS: &[&str] = &["id", "name"];

pub const MB_GENDER_TABLE: &str = "mb_gender";
pub const MB_GENDER_COLUMNS: &[&str] = &["id", "name"];

pub const MB_TAG_TABLE: &str = "mb_tag";
pub const MB_TAG_COLUMNS: &[&str] = &["id", "name"];

pub const MB_AREA_TABLE: &str = "mb_area";
pub const MB_AREA_COLUMNS: &[&str] = &["id", "name", "type"];

pub const MB_COUNTRY_AREA_TABLE: &str = "mb_country_area";
pub const MB_COUNTRY_AREA_COLUMNS: &[&str] = &["area"];

pub const MB_ARTIST_TABLE: &str = "mb_artist";
pub const MB_ARTIST_COLUMNS: &[&str] = &[
    "id",
    "name",
    "sort_name",
    "type",
    "area",
    "gender",
    "begin_area",
    "comment",
];

pub const MB_ARTIST_ALIAS_TABLE: &str = "mb_artist_alias";
pub const MB_ARTIST_ALIAS_COLUMNS: &[&str] = &[
    "id",
    "artist",
    "name",
    "sort_name",
    "locale",
    "type",
    "primary_for_locale",
];

pub const MB_ARTIST_TAG_TABLE: &str = "mb_artist_tag";
pub const MB_ARTIST_TAG_COLUMNS: &[&str] = &["artist", "tag", "count"];

pub const MB_ARTIST_CREDIT_TABLE: &str = "mb_artist_credit";
pub const MB_ARTIST_CREDIT_COLUMNS: &[&str] = &["id", "name", "artist_count"];

pub const MB_ARTIST_CREDIT_NAME_TABLE: &str = "mb_artist_credit_name";
pub const MB_ARTIST_CREDIT_NAME_COLUMNS: &[&str] = &[
    "artist_credit",
    "position",
    "artist",
    "name",
    "join_phrase",
];

pub const MB_RELEASE_GROUP_TABLE: &str = "mb_release_group";
pub const MB_RELEASE_GROUP_COLUMNS: &[&str] = &["id", "name", "artist_credit", "type"];

pub const MB_RECORDING_TABLE: &str = "mb_recording";
pub const MB_RECORDING_COLUMNS: &[&str] = &["id", "gid", "name", "artist_credit", "length"];

pub const MB_MEDIUM_TABLE: &str = "mb_medium";
pub const MB_MEDIUM_COLUMNS: &[&str] = &["id", "release", "position", "format"];

pub const MB_TRACK_TABLE: &str = "mb_track";
pub const MB_TRACK_COLUMNS: &[&str] = &[
    "id",
    "recording",
    "medium",
    "position",
    "name",
    "artist_credit",
    "length",
];

/// All table names in the musicbrainz-cache schema.
pub const ALL_TABLES: &[&str] = &[
    MB_AREA_TYPE_TABLE,
    MB_GENDER_TABLE,
    MB_TAG_TABLE,
    MB_AREA_TABLE,
    MB_COUNTRY_AREA_TABLE,
    MB_ARTIST_TABLE,
    MB_ARTIST_ALIAS_TABLE,
    MB_ARTIST_TAG_TABLE,
    MB_ARTIST_CREDIT_TABLE,
    MB_ARTIST_CREDIT_NAME_TABLE,
    MB_RELEASE_GROUP_TABLE,
    MB_RECORDING_TABLE,
    MB_MEDIUM_TABLE,
    MB_TRACK_TABLE,
];
