//! Shared ETL crate for the WXYC music data pipeline.
//!
//! Provides text normalization, fuzzy matching, PostgreSQL bulk loading,
//! pipeline orchestration, and schema contracts.

pub mod csv_writer;
pub mod fuzzy;
pub mod import;
pub mod parser;
pub mod pg;
pub mod pipeline;
pub mod schema;
pub mod sqlite;
pub mod state;
pub mod text;
