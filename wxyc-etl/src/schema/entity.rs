//! Entity store schema constants.
//!
//! Matches the `entity.identity` DDL specified in the WXYC wiki's
//! `cache-databases.md` (Cross-cutting section). These constants are
//! placeholders to be finalized when the entity-resolution pipeline lands
//! (Phase 5 of `metadata-consolidation-2026.md`); that implementation's DDL
//! is the source of truth.

pub const ENTITY_IDENTITY_TABLE: &str = "entity.identity";

pub const ENTITY_IDENTITY_COLUMNS: &[&str] = &[
    "id",
    "library_name",
    "discogs_artist_id",
    "wikidata_qid",
    "musicbrainz_artist_id",
    "spotify_artist_id",
    "apple_music_artist_id",
    "bandcamp_id",
    "reconciliation_status",
    "created_at",
    "updated_at",
];

pub const ENTITY_IDENTITY_DDL: &str = "\
CREATE SCHEMA IF NOT EXISTS entity;

CREATE TABLE entity.identity (
    id SERIAL PRIMARY KEY,
    library_name TEXT NOT NULL UNIQUE,
    discogs_artist_id INTEGER,
    wikidata_qid TEXT,
    musicbrainz_artist_id TEXT,
    spotify_artist_id TEXT,
    apple_music_artist_id TEXT,
    bandcamp_id TEXT,
    reconciliation_status TEXT NOT NULL DEFAULT 'unreconciled',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
)";

pub const RECONCILIATION_LOG_TABLE: &str = "entity.reconciliation_log";

pub const RECONCILIATION_LOG_COLUMNS: &[&str] = &[
    "id",
    "identity_id",
    "source",
    "external_id",
    "confidence",
    "method",
    "created_at",
];

pub const RECONCILIATION_LOG_DDL: &str = "\
CREATE TABLE entity.reconciliation_log (
    id SERIAL PRIMARY KEY,
    identity_id INTEGER NOT NULL REFERENCES entity.identity(id),
    source TEXT NOT NULL,
    external_id TEXT NOT NULL,
    confidence REAL,
    method TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
)";
