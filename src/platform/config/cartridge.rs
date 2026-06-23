//! Cartridge catalog and metadata/image-cache path configuration.

use super::FrontendConfig;
use super::cli::{
    has_negation_flag, parse_bool, parse_bool_arg, parse_cli_string_arg, parse_search_paths,
};

/// Apply cartridge catalog and metadata-path command-line flags.
pub(super) fn apply_args(cfg: &mut FrontendConfig, args: &[String]) -> Result<(), String> {
    if let Some(paths) = parse_cli_string_arg(args, "--cartridge-search-paths") {
        cfg.cartridge_search_paths = parse_search_paths(&paths);
    }

    if let Some(scan) = parse_bool_arg(args, "--scan-cartridges")? {
        cfg.scan_cartridges = scan;
    }
    if has_negation_flag(args, &["--no-scan-cartridges"]) {
        cfg.scan_cartridges = false;
    }

    if args.iter().any(|arg| arg == "--rebuild-cartridge-catalog") {
        cfg.rebuild_cartridge_catalog = true;
    }

    if let Some(path) = parse_cli_string_arg(args, "--metadata-db-path") {
        cfg.metadata_db_path = Some(path);
    }

    if let Some(path) = parse_cli_string_arg(args, "--image-cache-path") {
        cfg.image_cache_path = Some(path);
    }

    if let Some(include) = parse_bool_arg(args, "--include-unofficial-roms")? {
        cfg.include_unofficial_roms = include;
    }

    Ok(())
}

/// Apply a cartridge/metadata config-file key/value. Returns `true` if handled.
pub(super) fn apply_config_value(
    cfg: &mut FrontendConfig,
    key: &str,
    value: &str,
) -> Result<bool, String> {
    match key {
        "cartridge_search_paths" => {
            cfg.cartridge_search_paths = parse_search_paths(value);
        }
        "scan_cartridges" => {
            if let Ok(scan) = parse_bool(value) {
                cfg.scan_cartridges = scan;
            }
        }
        "rebuild_cartridge_catalog" => {
            if let Ok(rebuild) = parse_bool(value) {
                cfg.rebuild_cartridge_catalog = rebuild;
            }
        }
        "metadata_db_path" => {
            cfg.metadata_db_path = Some(value.to_string());
        }
        "image_cache_path" => {
            cfg.image_cache_path = Some(value.to_string());
        }
        "include_unofficial_roms" => {
            if let Ok(include) = parse_bool(value) {
                cfg.include_unofficial_roms = include;
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

impl FrontendConfig {
    /// Resolve the metadata database path, falling back to the default.
    ///
    /// Returns the configured path, or `~/.neser/metadata.db` if not set.
    pub fn resolved_metadata_db_path(&self) -> std::path::PathBuf {
        if let Some(ref p) = self.metadata_db_path {
            std::path::PathBuf::from(p)
        } else {
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_default();
            home.join(".neser").join("metadata.db")
        }
    }

    /// Resolve the image cache directory path, falling back to the default.
    ///
    /// Returns the configured path, or `~/.neser/image_cache/` if not set.
    pub fn resolved_image_cache_path(&self) -> std::path::PathBuf {
        if let Some(ref p) = self.image_cache_path {
            std::path::PathBuf::from(p)
        } else {
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_default();
            home.join(".neser").join("image_cache")
        }
    }

    /// Resolve the favorites file path.
    ///
    /// Returns `~/.neser/favorites.json`.
    pub fn resolved_favorites_path(&self) -> std::path::PathBuf {
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        home.join(".neser").join("favorites.json")
    }
}
