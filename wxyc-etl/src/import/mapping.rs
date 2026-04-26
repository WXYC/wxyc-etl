use std::collections::HashMap;

/// A function that transforms a column value during import.
pub type TransformFn = Box<dyn Fn(Option<&str>) -> Option<String>>;

/// Describes how to map source columns (CSV/TSV) to database columns.
///
/// Unifies the column mapping pattern from `discogs-cache/scripts/import_csv.py`
/// `TableConfig` and `musicbrainz-cache/scripts/import_tsv.py` `TableSpec`.
pub struct ColumnMapping {
    /// Column names to read from the source file.
    pub source_columns: Vec<String>,
    /// Corresponding database column names.
    pub db_columns: Vec<String>,
    /// Columns that must not be NULL.
    pub required_columns: Vec<String>,
    /// Columns forming the dedup key (if any).
    pub unique_key: Option<Vec<String>>,
    /// Per-column value transforms, keyed by source column name.
    pub transforms: HashMap<String, TransformFn>,
}

impl ColumnMapping {
    /// Create a new column mapping.
    pub fn new(
        source_columns: Vec<String>,
        db_columns: Vec<String>,
        required_columns: Vec<String>,
        unique_key: Option<Vec<String>>,
    ) -> Self {
        Self {
            source_columns,
            db_columns,
            required_columns,
            unique_key,
            transforms: HashMap::new(),
        }
    }

    /// Add a per-column transform function.
    pub fn add_transform(&mut self, column: &str, transform: TransformFn) {
        self.transforms.insert(column.to_string(), transform);
    }

    /// Return the index of a source column by name, or None.
    pub fn source_index(&self, name: &str) -> Option<usize> {
        self.source_columns.iter().position(|c| c == name)
    }

    /// Return the indices of the unique key columns in the source columns list.
    pub fn unique_key_indices(&self) -> Option<Vec<usize>> {
        self.unique_key
            .as_ref()
            .map(|keys| keys.iter().filter_map(|k| self.source_index(k)).collect())
    }
}
