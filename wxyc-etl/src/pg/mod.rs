//! PostgreSQL bulk loading utilities.
//!
//! Provides COPY TEXT escaping, row formatting, buffered batch writing,
//! deduplication tracking, and administrative operations for PostgreSQL
//! bulk imports.

pub mod copy;
pub mod dedup;
