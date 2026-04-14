//! Text normalization, filtering, compilation detection, and artist splitting.
//!
//! # Quick start
//!
//! ```
//! use wxyc_etl::text::{
//!     normalize_artist_name, ArtistFilter, TitleFilter,
//!     is_compilation_artist, split_artist_name, batch_normalize,
//! };
//! ```

pub mod batch;
pub mod compilation;
pub mod filter;
pub mod normalize;
pub mod split;

// Convenience re-exports for the most common entry points.
pub use batch::{batch_filter, batch_normalize};
pub use compilation::{is_compilation_artist, COMPILATION_KEYWORDS};
pub use filter::{ArtistFilter, TitleFilter};
pub use normalize::{normalize_artist_name, normalize_title, strip_diacritics};
pub use split::{split_artist_name, split_artist_name_contextual};
