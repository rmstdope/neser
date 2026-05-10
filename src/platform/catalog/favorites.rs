//! Persistent favorites storage.
//!
//! Favorites are stored as a JSON array of ROM file paths in a configurable
//! location (default: `~/.neser/favorites.json`).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Manages a set of favorited ROM file paths with JSON persistence.
#[derive(Debug)]
pub struct Favorites {
    /// The file where favorites are persisted.
    path: PathBuf,
    /// In-memory set of favorited ROM paths (stored as canonical strings).
    entries: HashSet<String>,
}

impl Favorites {
    /// Load favorites from disk, or create an empty set if the file is missing.
    pub fn load(path: &Path) -> Self {
        let entries = std::fs::read_to_string(path)
            .ok()
            .and_then(|data| serde_json::from_str::<Vec<String>>(&data).ok())
            .unwrap_or_default()
            .into_iter()
            .collect();
        Self {
            path: path.to_path_buf(),
            entries,
        }
    }

    /// Returns `true` if the given ROM path is favorited.
    pub fn contains(&self, rom_path: &Path) -> bool {
        self.entries
            .contains(&rom_path.to_string_lossy().to_string())
    }

    /// Toggle the favorite status of a ROM path. Returns the new status.
    pub fn toggle(&mut self, rom_path: &Path) -> bool {
        let key = rom_path.to_string_lossy().to_string();
        if self.entries.contains(&key) {
            self.entries.remove(&key);
            false
        } else {
            self.entries.insert(key);
            true
        }
    }

    /// Persist the current favorites to disk.
    pub fn save(&self) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create favorites dir: {e}"))?;
        }
        let sorted: Vec<&String> = {
            let mut v: Vec<&String> = self.entries.iter().collect();
            v.sort();
            v
        };
        let json = serde_json::to_string_pretty(&sorted).map_err(|e| format!("JSON error: {e}"))?;
        std::fs::write(&self.path, json).map_err(|e| format!("Write error: {e}"))
    }

    /// Number of favorited entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the favorites set is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_returns_empty_when_file_missing() {
        let fav = Favorites::load(Path::new("/nonexistent/favorites.json"));
        assert!(fav.is_empty());
    }

    #[test]
    fn load_parses_json_array() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp.as_file(), r#"["/roms/a.nes", "/roms/b.nes"]"#).unwrap();
        let fav = Favorites::load(tmp.path());
        assert_eq!(fav.len(), 2);
        assert!(fav.contains(Path::new("/roms/a.nes")));
        assert!(fav.contains(Path::new("/roms/b.nes")));
    }

    #[test]
    fn load_handles_malformed_json() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        write!(tmp.as_file(), "not json at all").unwrap();
        let fav = Favorites::load(tmp.path());
        assert!(fav.is_empty());
    }

    #[test]
    fn toggle_adds_then_removes() {
        let fav_path = tempfile::NamedTempFile::new().unwrap();
        let mut fav = Favorites::load(fav_path.path());
        let rom = Path::new("/roms/test.nes");

        assert!(!fav.contains(rom));
        let added = fav.toggle(rom);
        assert!(added);
        assert!(fav.contains(rom));

        let removed = fav.toggle(rom);
        assert!(!removed);
        assert!(!fav.contains(rom));
    }

    #[test]
    fn save_and_reload_round_trips() {
        let dir = tempfile::TempDir::new().unwrap();
        let fav_path = dir.path().join("favorites.json");

        let mut fav = Favorites::load(&fav_path);
        fav.toggle(Path::new("/roms/zelda.nes"));
        fav.toggle(Path::new("/roms/mario.nes"));
        fav.save().unwrap();

        let reloaded = Favorites::load(&fav_path);
        assert_eq!(reloaded.len(), 2);
        assert!(reloaded.contains(Path::new("/roms/zelda.nes")));
        assert!(reloaded.contains(Path::new("/roms/mario.nes")));
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::TempDir::new().unwrap();
        let fav_path = dir.path().join("sub").join("dir").join("favorites.json");

        let mut fav = Favorites::load(&fav_path);
        fav.toggle(Path::new("/roms/test.nes"));
        fav.save().unwrap();
        assert!(fav_path.exists());
    }

    #[test]
    fn saved_json_is_sorted() {
        let dir = tempfile::TempDir::new().unwrap();
        let fav_path = dir.path().join("favorites.json");

        let mut fav = Favorites::load(&fav_path);
        fav.toggle(Path::new("/roms/zelda.nes"));
        fav.toggle(Path::new("/roms/contra.nes"));
        fav.toggle(Path::new("/roms/mario.nes"));
        fav.save().unwrap();

        let data = std::fs::read_to_string(&fav_path).unwrap();
        let list: Vec<String> = serde_json::from_str(&data).unwrap();
        assert_eq!(
            list,
            vec!["/roms/contra.nes", "/roms/mario.nes", "/roms/zelda.nes"]
        );
    }
}
