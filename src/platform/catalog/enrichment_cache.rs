//! Persistent cache for ROM enrichment results.
//!
//! Stores metadata matching and image cache results keyed by ROM path so that
//! subsequent startups can skip re-matching and re-downloading for already-
//! processed ROMs. The cache is stored as a JSON file (default location:
//! `~/.neser/enrichment_cache.json`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Cached enrichment data for a single ROM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedEnrichment {
    /// TheGamesDB game ID matched via fuzzy title matching.
    pub metadata_game_id: Option<i64>,
    /// Genre names from TheGamesDB.
    pub genres: Vec<String>,
    /// Game overview/description.
    pub overview: Option<String>,
    /// Release date string.
    pub release_date: Option<String>,
    /// Number of players.
    pub players: Option<u32>,
    /// Content rating.
    pub rating: Option<String>,
    /// Path to cached front boxart image.
    pub boxart_path: Option<PathBuf>,
    /// Paths to cached screenshot images.
    pub screenshot_paths: Vec<PathBuf>,
}

/// Persistent cache mapping ROM paths to their enrichment results.
#[derive(Debug, Serialize, Deserialize)]
pub struct EnrichmentCache {
    entries: HashMap<PathBuf, CachedEnrichment>,
    #[serde(skip)]
    path: PathBuf,
}

impl EnrichmentCache {
    /// Load from disk, or create an empty cache if the file doesn't exist.
    pub fn load(path: &Path) -> Self {
        let mut cache: Self = std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| Self {
                entries: HashMap::new(),
                path: PathBuf::new(),
            });
        cache.path = path.to_path_buf();
        cache
    }

    /// Create an empty cache bound to the given path (ignores any existing file).
    pub fn load_empty(path: &Path) -> Self {
        Self {
            entries: HashMap::new(),
            path: path.to_path_buf(),
        }
    }

    /// Look up cached enrichment for a ROM path.
    pub fn get(&self, rom_path: &Path) -> Option<&CachedEnrichment> {
        self.entries.get(rom_path)
    }

    /// Insert or update enrichment data for a ROM path.
    pub fn insert(&mut self, rom_path: PathBuf, data: CachedEnrichment) {
        self.entries.insert(rom_path, data);
    }

    /// Save the cache to disk. Errors are logged but not propagated.
    pub fn save(&self) {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_string(&self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&self.path, json) {
                    eprintln!("Failed to write enrichment cache: {e}");
                }
            }
            Err(e) => eprintln!("Failed to serialize enrichment cache: {e}"),
        }
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove entries whose ROM paths no longer exist on disk.
    pub fn prune_missing(&mut self) {
        self.entries.retain(|path, _| path.exists());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_enrichment(game_id: Option<i64>) -> CachedEnrichment {
        CachedEnrichment {
            metadata_game_id: game_id,
            genres: vec!["Action".to_string()],
            overview: Some("A game".to_string()),
            release_date: Some("1990-01-01".to_string()),
            players: Some(1),
            rating: Some("E".to_string()),
            boxart_path: Some(PathBuf::from("/cache/boxart.jpg")),
            screenshot_paths: vec![PathBuf::from("/cache/screen1.jpg")],
        }
    }

    #[test]
    fn load_returns_empty_cache_when_file_missing() {
        let dir = TempDir::new().unwrap();
        let cache = EnrichmentCache::load(&dir.path().join("nonexistent.json"));
        assert!(cache.is_empty());
    }

    #[test]
    fn insert_and_get() {
        let dir = TempDir::new().unwrap();
        let mut cache = EnrichmentCache::load(&dir.path().join("cache.json"));
        let rom = PathBuf::from("/roms/game.nes");
        cache.insert(rom.clone(), make_enrichment(Some(42)));

        let cached = cache.get(&rom).unwrap();
        assert_eq!(cached.metadata_game_id, Some(42));
        assert_eq!(cached.genres, vec!["Action"]);
    }

    #[test]
    fn save_and_reload() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cache.json");

        let mut cache = EnrichmentCache::load(&path);
        cache.insert(PathBuf::from("/roms/game.nes"), make_enrichment(Some(42)));
        cache.save();

        let loaded = EnrichmentCache::load(&path);
        assert_eq!(loaded.len(), 1);
        let cached = loaded.get(Path::new("/roms/game.nes")).unwrap();
        assert_eq!(cached.metadata_game_id, Some(42));
    }

    #[test]
    fn get_returns_none_for_unknown_path() {
        let dir = TempDir::new().unwrap();
        let cache = EnrichmentCache::load(&dir.path().join("cache.json"));
        assert!(cache.get(Path::new("/roms/unknown.nes")).is_none());
    }

    #[test]
    fn prune_missing_removes_entries_with_nonexistent_paths() {
        let dir = TempDir::new().unwrap();
        let mut cache = EnrichmentCache::load(&dir.path().join("cache.json"));

        // Insert entry for a file that exists and one that doesn't.
        let existing = dir.path().join("real.nes");
        std::fs::write(&existing, b"NES").unwrap();
        cache.insert(existing.clone(), make_enrichment(Some(1)));
        cache.insert(
            PathBuf::from("/nonexistent/game.nes"),
            make_enrichment(Some(2)),
        );

        assert_eq!(cache.len(), 2);
        cache.prune_missing();
        assert_eq!(cache.len(), 1);
        assert!(cache.get(&existing).is_some());
    }

    #[test]
    fn load_handles_corrupt_json_gracefully() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("cache.json");
        std::fs::write(&path, b"not valid json {{{").unwrap();

        let cache = EnrichmentCache::load(&path);
        assert!(cache.is_empty());
    }
}
