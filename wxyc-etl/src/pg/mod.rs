//! PostgreSQL bulk loading utilities.
//!
//! Provides COPY TEXT escaping, row formatting, buffered batch writing,
//! deduplication tracking, and administrative operations for PostgreSQL
//! bulk imports.
//!
//! # Quick start
//!
//! ```ignore
//! use wxyc_etl::pg::{escape_copy_text, write_copy_row, write_copy_int, BatchCopier, DedupSet, extract_year};
//! ```

pub mod admin;
pub mod batch;
pub mod copy;
pub mod dedup;

// Re-export commonly used items at the pg module level.
pub use admin::{set_tables_logged, set_tables_unlogged, vacuum_full};
pub use batch::{BatchCopier, CopyBuffer, CopyTarget};
pub use copy::{
    copy_line, copy_value, empty_to_none, escape_copy_text, escape_copy_text_into, extract_year,
    pick_artwork_url, write_copy_int, write_copy_row, ImageRef,
};
pub use dedup::{ArtistDedup, DedupSet, LabelDedup, TrackArtistDedup};
