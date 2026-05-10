//! Metadata enrichment from TheGamesDB SQLite database.
//!
//! This module reads game metadata (titles, descriptions, genres, images, etc.)
//! from a local copy of TheGamesDB metadata database and matches ROM entries
//! to their metadata using fuzzy title matching.

#[cfg(feature = "native")]
mod db;
#[cfg(feature = "native")]
mod matcher;

// Re-exports used by the ROM browser (native frontend).
// TODO: remove allow(unused_imports) once the ROM browser module consumes these.
#[allow(unused_imports)]
#[cfg(feature = "native")]
pub use db::{GameMetadata, ImageInfo, MetadataDb};
#[allow(unused_imports)]
#[cfg(feature = "native")]
pub use matcher::{TitleMatch, match_title};
