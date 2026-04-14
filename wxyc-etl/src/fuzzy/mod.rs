//! Fuzzy string matching and batch classification/resolution.
//!
//! # Quick start
//!
//! ```
//! use wxyc_etl::fuzzy::metrics::{
//!     levenshtein_ratio, token_set_ratio, token_sort_ratio, jaro_winkler_similarity,
//! };
//! ```

pub mod classify;
pub mod metrics;
pub mod resolve;
