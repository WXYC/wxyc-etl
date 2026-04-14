//! Shared ETL crate for the WXYC music data pipeline.
//!
//! Provides text normalization, fuzzy matching, PostgreSQL bulk loading,
//! pipeline orchestration, and schema contracts.

pub mod text;
pub mod pg;
pub mod pipeline;
pub mod csv_writer;
pub mod sqlite;
pub mod state;
pub mod import;
pub mod schema;
pub mod fuzzy;
pub mod parser;
