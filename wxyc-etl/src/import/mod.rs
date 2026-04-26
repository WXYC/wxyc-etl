//! Column mapping and deduplication for CSV/TSV import.
//!
//! Unifies the import patterns from `discogs-cache/scripts/import_csv.py`
//! (`TableConfig`) and `musicbrainz-cache/scripts/import_tsv.py` (`TableSpec`).

mod dedup;
mod mapping;

pub use dedup::ImportDedupSet;
pub use mapping::{ColumnMapping, TransformFn};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_mapping_basic() {
        let mapping = ColumnMapping::new(
            vec!["id".into(), "title".into()],
            vec!["id".into(), "title".into()],
            vec!["id".into()],
            None,
        );
        assert_eq!(mapping.source_columns.len(), 2);
        assert_eq!(mapping.required_columns, vec!["id"]);
        assert_eq!(mapping.source_index("title"), Some(1));
        assert_eq!(mapping.source_index("nonexistent"), None);
    }

    #[test]
    fn column_mapping_with_unique_key() {
        let mapping = ColumnMapping::new(
            vec!["release_id".into(), "artist_name".into(), "extra".into()],
            vec!["release_id".into(), "artist_name".into(), "extra".into()],
            vec!["release_id".into()],
            Some(vec!["release_id".into(), "artist_name".into()]),
        );
        let indices = mapping.unique_key_indices().unwrap();
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn column_mapping_with_transform() {
        let mut mapping =
            ColumnMapping::new(vec!["format".into()], vec!["format".into()], vec![], None);
        mapping.add_transform("format", Box::new(|v| v.map(|s| s.to_uppercase())));
        let transform = mapping.transforms.get("format").unwrap();
        assert_eq!(transform(Some("vinyl")), Some("VINYL".to_string()));
        assert_eq!(transform(None), None);
    }

    #[test]
    fn column_mapping_different_source_and_db_columns() {
        let mapping = ColumnMapping::new(
            vec!["label".into(), "catno".into()],
            vec!["label_name".into(), "catno".into()],
            vec!["label_name".into()],
            None,
        );
        assert_eq!(mapping.source_columns[0], "label");
        assert_eq!(mapping.db_columns[0], "label_name");
    }

    #[test]
    fn dedup_set_basic() {
        let mut dedup = ImportDedupSet::new();
        assert!(dedup.is_empty());
        assert!(dedup.insert(&["1", "Autechre"]));
        assert!(!dedup.insert(&["1", "Autechre"])); // duplicate
        assert!(dedup.insert(&["1", "Stereolab"])); // different key
        assert_eq!(dedup.len(), 2);
    }

    #[test]
    fn dedup_set_single_column_key() {
        let mut dedup = ImportDedupSet::new();
        assert!(dedup.insert(&["42"]));
        assert!(!dedup.insert(&["42"]));
        assert!(dedup.insert(&["43"]));
    }

    #[test]
    fn dedup_set_empty_key() {
        let mut dedup = ImportDedupSet::new();
        assert!(dedup.insert(&[]));
        assert!(!dedup.insert(&[])); // duplicate empty key
    }
}
