//! Cover art image download and caching.
//!
//! Downloads front boxart and screenshots from TheGamesDB CDN and caches
//! them on disk for the ROM browser. Images are stored in a configurable
//! cache directory (default: `~/.neser/image_cache/`).

#[cfg(feature = "native")]
mod cache;

// Re-exports used by the ROM browser (native frontend).
// TODO: remove allow(unused_imports) once the ROM browser module consumes these.
#[allow(unused_imports)]
#[cfg(feature = "native")]
pub use cache::{CachedImages, ImageCache, ImageCacheProgress};
