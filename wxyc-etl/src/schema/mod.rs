//! Schema contracts: table names, column lists, and DDL constants.
//!
//! Provides shared constants for all database schemas consumed by the ETL
//! pipeline: library.db (SQLite), discogs-cache (PostgreSQL), entity store
//! (PostgreSQL), wikidata-cache (PostgreSQL), and musicbrainz-cache (PostgreSQL).

pub mod discogs;
pub mod entity;
pub mod library;
pub mod musicbrainz;
pub mod wikidata;

#[cfg(test)]
mod tests {
    use super::*;

    // -- Library --

    #[test]
    fn library_table_name() {
        assert_eq!(library::LIBRARY_TABLE, "library");
    }

    #[test]
    fn library_columns_contains_required() {
        assert!(library::LIBRARY_COLUMNS.contains(&"artist"));
        assert!(library::LIBRARY_COLUMNS.contains(&"title"));
        assert!(library::LIBRARY_COLUMNS.contains(&"id"));
        assert!(library::LIBRARY_COLUMNS.contains(&"format"));
    }

    #[test]
    fn library_ddl_contains_columns() {
        assert!(library::LIBRARY_DDL.contains("artist"));
        assert!(library::LIBRARY_DDL.contains("title"));
        assert!(library::LIBRARY_DDL.contains("id INTEGER PRIMARY KEY"));
    }

    #[test]
    fn library_fts_ddl() {
        assert!(library::LIBRARY_FTS_DDL.contains("fts5"));
        assert!(library::LIBRARY_FTS_DDL.contains("content='library'"));
    }

    // -- Discogs --

    #[test]
    fn discogs_release_table() {
        assert_eq!(discogs::RELEASE_TABLE, "release");
        assert!(discogs::RELEASE_COLUMNS.contains(&"id"));
        assert!(discogs::RELEASE_COLUMNS.contains(&"title"));
        assert!(discogs::RELEASE_COLUMNS.contains(&"master_id"));
    }

    #[test]
    fn discogs_all_tables_count() {
        assert_eq!(discogs::ALL_TABLES.len(), 13);
    }

    #[test]
    fn discogs_release_artist_columns() {
        assert!(discogs::RELEASE_ARTIST_COLUMNS.contains(&"release_id"));
        assert!(discogs::RELEASE_ARTIST_COLUMNS.contains(&"artist_name"));
    }

    #[test]
    fn discogs_release_label_columns() {
        assert!(discogs::RELEASE_LABEL_COLUMNS.contains(&"label_name"));
        assert!(discogs::RELEASE_LABEL_COLUMNS.contains(&"catno"));
    }

    #[test]
    fn discogs_artist_columns() {
        assert!(discogs::ARTIST_COLUMNS.contains(&"id"));
        assert!(discogs::ARTIST_COLUMNS.contains(&"name"));
        assert!(discogs::ARTIST_COLUMNS.contains(&"profile"));
    }

    // -- Entity --

    #[test]
    fn entity_identity_ddl() {
        assert!(entity::ENTITY_IDENTITY_DDL.contains("library_name"));
        assert!(entity::ENTITY_IDENTITY_DDL.contains("discogs_artist_id"));
        assert!(entity::ENTITY_IDENTITY_DDL.contains("wikidata_qid"));
        assert!(entity::ENTITY_IDENTITY_DDL.contains("musicbrainz_artist_id"));
        assert!(entity::ENTITY_IDENTITY_DDL.contains("reconciliation_status"));
    }

    #[test]
    fn entity_reconciliation_log_ddl() {
        assert!(entity::RECONCILIATION_LOG_DDL.contains("identity_id"));
        assert!(entity::RECONCILIATION_LOG_DDL.contains("confidence"));
        assert!(entity::RECONCILIATION_LOG_DDL.contains("method"));
    }

    #[test]
    fn entity_identity_table_name() {
        assert_eq!(entity::ENTITY_IDENTITY_TABLE, "entity.identity");
    }

    // -- Wikidata --

    #[test]
    fn wikidata_all_tables_count() {
        assert_eq!(wikidata::ALL_TABLES.len(), 8);
    }

    #[test]
    fn wikidata_entity_table() {
        assert_eq!(wikidata::ENTITY_TABLE, "entity");
        assert!(wikidata::ENTITY_COLUMNS.contains(&"qid"));
        assert!(wikidata::ENTITY_COLUMNS.contains(&"entity_type"));
    }

    #[test]
    fn wikidata_discogs_mapping_table() {
        assert_eq!(wikidata::DISCOGS_MAPPING_TABLE, "discogs_mapping");
        assert!(wikidata::DISCOGS_MAPPING_COLUMNS.contains(&"property"));
        assert!(wikidata::DISCOGS_MAPPING_COLUMNS.contains(&"discogs_id"));
    }

    #[test]
    fn wikidata_influence_table() {
        assert!(wikidata::INFLUENCE_COLUMNS.contains(&"source_qid"));
        assert!(wikidata::INFLUENCE_COLUMNS.contains(&"target_qid"));
    }

    #[test]
    fn wikidata_entity_alias_table() {
        assert!(wikidata::ENTITY_ALIAS_COLUMNS.contains(&"qid"));
        assert!(wikidata::ENTITY_ALIAS_COLUMNS.contains(&"alias"));
    }

    // -- MusicBrainz --

    #[test]
    fn musicbrainz_all_tables_count() {
        // mb_area_type, mb_gender, mb_tag, mb_area, mb_country_area,
        // mb_artist, mb_artist_alias, mb_artist_tag, mb_artist_credit,
        // mb_artist_credit_name, mb_release_group, mb_recording, mb_medium, mb_track
        assert_eq!(musicbrainz::ALL_TABLES.len(), 14);
    }

    #[test]
    fn musicbrainz_artist_table() {
        assert_eq!(musicbrainz::MB_ARTIST_TABLE, "mb_artist");
        assert!(musicbrainz::MB_ARTIST_COLUMNS.contains(&"name"));
        assert!(musicbrainz::MB_ARTIST_COLUMNS.contains(&"sort_name"));
        assert!(musicbrainz::MB_ARTIST_COLUMNS.contains(&"comment"));
    }

    #[test]
    fn musicbrainz_recording_table() {
        assert_eq!(musicbrainz::MB_RECORDING_TABLE, "mb_recording");
        assert!(musicbrainz::MB_RECORDING_COLUMNS.contains(&"gid"));
        assert!(musicbrainz::MB_RECORDING_COLUMNS.contains(&"length"));
    }

    #[test]
    fn musicbrainz_track_table() {
        assert_eq!(musicbrainz::MB_TRACK_TABLE, "mb_track");
        assert!(musicbrainz::MB_TRACK_COLUMNS.contains(&"recording"));
        assert!(musicbrainz::MB_TRACK_COLUMNS.contains(&"medium"));
    }

    #[test]
    fn musicbrainz_country_area_table() {
        assert_eq!(musicbrainz::MB_COUNTRY_AREA_TABLE, "mb_country_area");
        assert!(musicbrainz::MB_COUNTRY_AREA_COLUMNS.contains(&"area"));
    }
}
