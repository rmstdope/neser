//! Image cache implementation.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Connect timeout for cover art downloads.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Read timeout for cover art downloads.
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// Progress callback data for image downloads.
#[derive(Debug, Clone)]
pub struct ImageCacheProgress {
    pub current: usize,
    pub total: usize,
    pub game_title: String,
}

/// Cached image paths for a single game.
#[derive(Debug, Clone, Default)]
pub struct CachedImages {
    pub boxart_path: Option<PathBuf>,
    pub screenshot_paths: Vec<PathBuf>,
}

/// Manages downloading and caching cover art images from TheGamesDB CDN.
pub struct ImageCache {
    cache_dir: PathBuf,
    base_url: String,
    /// Shared HTTP client configured with connection and read timeouts.
    client: reqwest::blocking::Client,
}

impl ImageCache {
    /// Create a new image cache with the given directory and CDN base URL.
    ///
    /// Creates the cache directory if it doesn't exist.
    pub fn new(cache_dir: PathBuf, base_url: String) -> Result<Self, String> {
        fs::create_dir_all(&cache_dir)
            .map_err(|e| format!("Failed to create image cache dir {cache_dir:?}: {e}"))?;
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(READ_TIMEOUT)
            .user_agent(concat!("neser/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| format!("Failed to build HTTP client: {e}"))?;
        Ok(Self {
            cache_dir,
            base_url,
            client,
        })
    }

    /// Return the cache directory path.
    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Check if an image is already cached and return its path.
    pub fn cached_path(&self, filename: &str) -> Option<PathBuf> {
        let path = self.local_path(filename);
        if path.exists() && fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
            Some(path)
        } else {
            None
        }
    }

    /// Get the local filesystem path for a CDN filename.
    ///
    /// The CDN filename may contain subdirectories (e.g. "boxart/front/123-1.jpg"),
    /// which are flattened into the cache directory by replacing '/' with '_'.
    fn local_path(&self, filename: &str) -> PathBuf {
        let safe_name = filename.replace('/', "_");
        self.cache_dir.join(safe_name)
    }

    /// Download a single image from the CDN if not already cached.
    ///
    /// Returns the local path on success, or `None` on download failure.
    pub fn ensure_cached(&self, filename: &str) -> Option<PathBuf> {
        if let Some(path) = self.cached_path(filename) {
            return Some(path);
        }
        self.download(filename)
    }

    /// Download an image from the CDN and save to the cache directory.
    fn download(&self, filename: &str) -> Option<PathBuf> {
        let url = format!("{}{}", self.base_url, filename);
        let response = self.client.get(&url).send().ok()?;

        if !response.status().is_success() {
            return None;
        }

        let bytes = response.bytes().ok()?;
        if bytes.is_empty() {
            return None;
        }

        let local = self.local_path(filename);
        fs::write(&local, &bytes).ok()?;
        Some(local)
    }

    /// Ensure all images for a game are cached.
    ///
    /// Takes the front boxart filename and screenshot filenames from metadata,
    /// downloads any that are missing, and returns the cached paths.
    pub fn ensure_game_images(
        &self,
        boxart_filename: Option<&str>,
        screenshot_filenames: &[&str],
    ) -> CachedImages {
        let boxart_path = boxart_filename.and_then(|f| self.ensure_cached(f));
        let screenshot_paths = screenshot_filenames
            .iter()
            .filter_map(|f| self.ensure_cached(f))
            .collect();
        CachedImages {
            boxart_path,
            screenshot_paths,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn new_creates_cache_directory() {
        let tmp = TempDir::new().unwrap();
        let cache_dir = tmp.path().join("image_cache");
        assert!(!cache_dir.exists());

        let cache = ImageCache::new(cache_dir.clone(), "https://example.com/".to_string());
        assert!(cache.is_ok());
        assert!(cache_dir.exists());
    }

    #[test]
    fn cached_path_returns_none_for_missing_file() {
        let tmp = TempDir::new().unwrap();
        let cache =
            ImageCache::new(tmp.path().to_path_buf(), "https://example.com/".to_string()).unwrap();

        assert!(cache.cached_path("nonexistent.jpg").is_none());
    }

    #[test]
    fn cached_path_returns_some_for_existing_file() {
        let tmp = TempDir::new().unwrap();
        let cache =
            ImageCache::new(tmp.path().to_path_buf(), "https://example.com/".to_string()).unwrap();

        // Create a fake cached file
        let path = tmp.path().join("boxart_front_123-1.jpg");
        fs::write(&path, b"fake image data").unwrap();

        assert_eq!(
            cache.cached_path("boxart/front/123-1.jpg"),
            Some(path.clone())
        );
    }

    #[test]
    fn cached_path_returns_none_for_empty_file() {
        let tmp = TempDir::new().unwrap();
        let cache =
            ImageCache::new(tmp.path().to_path_buf(), "https://example.com/".to_string()).unwrap();

        let path = tmp.path().join("boxart_front_123-1.jpg");
        fs::write(&path, b"").unwrap();

        assert!(cache.cached_path("boxart/front/123-1.jpg").is_none());
    }

    #[test]
    fn local_path_flattens_subdirectories() {
        let tmp = TempDir::new().unwrap();
        let cache =
            ImageCache::new(tmp.path().to_path_buf(), "https://example.com/".to_string()).unwrap();

        let path = cache.local_path("boxart/front/123-1.jpg");
        assert_eq!(path, tmp.path().join("boxart_front_123-1.jpg"));
    }

    #[test]
    fn ensure_cached_returns_existing_path_without_download() {
        let tmp = TempDir::new().unwrap();
        let cache = ImageCache::new(
            tmp.path().to_path_buf(),
            "https://invalid.example.com/".to_string(),
        )
        .unwrap();

        // Pre-populate the cache
        let path = tmp.path().join("boxart_front_5-1.jpg");
        fs::write(&path, b"cached image").unwrap();

        let result = cache.ensure_cached("boxart/front/5-1.jpg");
        assert_eq!(result, Some(path));
    }

    #[test]
    fn ensure_game_images_collects_results() {
        let tmp = TempDir::new().unwrap();
        let cache = ImageCache::new(
            tmp.path().to_path_buf(),
            "https://invalid.example.com/".to_string(),
        )
        .unwrap();

        // Pre-populate boxart
        let boxart = tmp.path().join("boxart_front_5-1.jpg");
        fs::write(&boxart, b"boxart data").unwrap();

        // Pre-populate one screenshot (second will be missing)
        let screen1 = tmp.path().join("screenshot_5-1.jpg");
        fs::write(&screen1, b"screenshot data").unwrap();

        let result = cache.ensure_game_images(
            Some("boxart/front/5-1.jpg"),
            &["screenshot/5-1.jpg", "screenshot/5-2.jpg"],
        );

        assert_eq!(result.boxart_path, Some(boxart));
        assert_eq!(result.screenshot_paths.len(), 1);
        assert_eq!(result.screenshot_paths[0], screen1);
    }

    #[test]
    fn ensure_game_images_handles_no_boxart() {
        let tmp = TempDir::new().unwrap();
        let cache = ImageCache::new(
            tmp.path().to_path_buf(),
            "https://invalid.example.com/".to_string(),
        )
        .unwrap();

        let result = cache.ensure_game_images(None, &[]);
        assert!(result.boxart_path.is_none());
        assert!(result.screenshot_paths.is_empty());
    }
}
