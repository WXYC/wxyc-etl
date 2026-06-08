//! Text normalization, filtering, compilation detection, and artist splitting.
//!
//! # Quick start
//!
//! ```
//! use wxyc_etl::text::{
//!     to_match_form, ArtistFilter, TitleFilter,
//!     is_compilation_artist, split_artist_name, batch_to_match_form,
//! };
//! ```

pub mod batch;
pub mod compilation;
pub mod filter;
pub mod folds;
pub mod forms;
pub mod identity;
pub mod mojibake;
pub mod split;

// Convenience re-exports for the most common entry points.
pub use batch::{batch_filter, batch_to_ascii_form, batch_to_match_form, batch_to_storage_form};
pub use compilation::is_compilation_artist;
pub use filter::{ArtistFilter, TitleFilter};
pub use forms::{to_ascii_form, to_match_form, to_storage_form};
pub use identity::{
    strip_leading_article, to_identity_match_form, to_identity_match_form_title,
    to_identity_match_form_with_disambiguator_strip, to_identity_match_form_with_punctuation,
};
pub use mojibake::fix_mojibake;
pub use split::{split_artist_name, split_artist_name_contextual};
