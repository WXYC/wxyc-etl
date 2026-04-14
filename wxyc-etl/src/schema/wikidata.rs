//! Wikidata-cache table and column constants.
//!
//! Matches the schema defined in the 6b plan
//! (`plans/phase-6-etl-conversions/6b-wikidata-cache-import.md`).

pub const ENTITY_TABLE: &str = "entity";
pub const ENTITY_COLUMNS: &[&str] = &["qid", "label", "description", "entity_type"];

pub const DISCOGS_MAPPING_TABLE: &str = "discogs_mapping";
pub const DISCOGS_MAPPING_COLUMNS: &[&str] = &["qid", "property", "discogs_id"];

pub const INFLUENCE_TABLE: &str = "influence";
pub const INFLUENCE_COLUMNS: &[&str] = &["source_qid", "target_qid"];

pub const GENRE_TABLE: &str = "genre";
pub const GENRE_COLUMNS: &[&str] = &["entity_qid", "genre_qid"];

pub const RECORD_LABEL_TABLE: &str = "record_label";
pub const RECORD_LABEL_COLUMNS: &[&str] = &["artist_qid", "label_qid"];

pub const LABEL_HIERARCHY_TABLE: &str = "label_hierarchy";
pub const LABEL_HIERARCHY_COLUMNS: &[&str] = &["child_qid", "parent_qid"];

pub const ENTITY_ALIAS_TABLE: &str = "entity_alias";
pub const ENTITY_ALIAS_COLUMNS: &[&str] = &["qid", "alias"];

pub const OCCUPATION_TABLE: &str = "occupation";
pub const OCCUPATION_COLUMNS: &[&str] = &["entity_qid", "occupation_qid"];

/// All table names in the wikidata-cache schema.
pub const ALL_TABLES: &[&str] = &[
    ENTITY_TABLE,
    DISCOGS_MAPPING_TABLE,
    INFLUENCE_TABLE,
    GENRE_TABLE,
    RECORD_LABEL_TABLE,
    LABEL_HIERARCHY_TABLE,
    ENTITY_ALIAS_TABLE,
    OCCUPATION_TABLE,
];
