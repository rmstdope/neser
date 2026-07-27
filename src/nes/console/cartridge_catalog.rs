use std::io;
use std::path::{Path, PathBuf};
use std::{collections::BTreeSet, fs};

const CATALOG_HEADER: &str = "path";

#[derive(Debug, Clone)]
pub struct CartridgeCatalogOptions {
    pub search_paths: Vec<PathBuf>,
    pub catalog_csv_path: PathBuf,
    pub scan_enabled: bool,
    pub rebuild_catalog: bool,
}

impl CartridgeCatalogOptions {
    pub fn new(search_paths: Vec<PathBuf>, catalog_csv_path: PathBuf) -> Self {
        Self {
            search_paths,
            catalog_csv_path,
            scan_enabled: true,
            rebuild_catalog: false,
        }
    }
}

pub fn refresh_cartridge_catalog(options: &CartridgeCatalogOptions) -> io::Result<Vec<PathBuf>> {
    let mut catalog_entries = existing_catalog_entries(options)?;

    if options.scan_enabled {
        let discovered = collect_rom_files(&options.search_paths)?;
        catalog_entries.extend(discovered);
    }

    write_catalog_entries(&options.catalog_csv_path, &catalog_entries)?;
    Ok(catalog_entries.into_iter().collect())
}

pub fn default_catalog_csv_path(home_dir: &Path) -> PathBuf {
    home_dir.join(".neser").join("cartridges.csv")
}

fn existing_catalog_entries(options: &CartridgeCatalogOptions) -> io::Result<BTreeSet<PathBuf>> {
    if options.rebuild_catalog {
        return Ok(BTreeSet::new());
    }

    read_catalog_entries(&options.catalog_csv_path).map(|entries| entries.into_iter().collect())
}

fn collect_rom_files(search_paths: &[PathBuf]) -> io::Result<BTreeSet<PathBuf>> {
    let mut files = BTreeSet::new();
    for root in search_paths {
        collect_rom_files_under(root, &mut files)?;
    }
    Ok(files)
}

fn collect_rom_files_under(root: &Path, out: &mut BTreeSet<PathBuf>) -> io::Result<()> {
    let Some(entries) = read_dir_if_exists(root)? else {
        return Ok(());
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
            Err(err) => return Err(err),
        };

        let file_type = metadata.file_type();

        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            collect_rom_files_under(&path, out)?;
        } else if file_type.is_file() && is_rom_file(&path) {
            out.insert(path);
        }
    }

    Ok(())
}

fn read_dir_if_exists(path: &Path) -> io::Result<Option<fs::ReadDir>> {
    match fs::read_dir(path) {
        Ok(entries) => Ok(Some(entries)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err),
    }
}

fn is_rom_file(path: &Path) -> bool {
    // The catalog feeds both the ROM browser (NES, GB, GBC, GBA, and SNES
    // platforms) and the in-emulator cartridge switch dialog (which filters
    // to .nes itself).
    const ROM_EXTENSIONS: [&str; 6] = ["nes", "gb", "gbc", "gba", "sfc", "smc"];
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            ROM_EXTENSIONS
                .iter()
                .any(|rom_ext| ext.eq_ignore_ascii_case(rom_ext))
        })
}

fn read_catalog_entries(path: &Path) -> io::Result<Vec<PathBuf>> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err),
    };

    let entries = content
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect();

    Ok(entries)
}

fn write_catalog_entries(path: &Path, entries: &BTreeSet<PathBuf>) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let content = serialize_catalog_entries(entries);

    fs::write(path, content)
}

fn serialize_catalog_entries(entries: &BTreeSet<PathBuf>) -> String {
    let mut content = String::from(CATALOG_HEADER);
    content.push('\n');

    for entry in entries {
        content.push_str(&entry.to_string_lossy());
        content.push('\n');
    }

    content
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_file(path: &Path) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent dir");
        }
        fs::write(path, b"NES\x1A").expect("write test file");
    }

    fn read_csv_lines(path: &Path) -> Vec<String> {
        fs::read_to_string(path)
            .expect("catalog CSV should exist")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn given_nested_search_paths_when_refreshing_then_catalog_contains_only_nes_files() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("roms");
        write_file(&root.join("games/a.nes"));
        write_file(&root.join("games/sub/b.NES"));
        write_file(&root.join("games/notes.txt"));

        let catalog_path = temp.path().join("cartridges.csv");
        let options = CartridgeCatalogOptions::new(vec![root], catalog_path.clone());

        let entries = refresh_cartridge_catalog(&options).expect("refresh should succeed");
        let mut csv_lines = read_csv_lines(&catalog_path);
        csv_lines.sort();

        assert_eq!(entries.len(), 2);
        assert!(csv_lines.iter().any(|line| line == "path"));
        assert!(csv_lines.iter().any(|line| line.ends_with("games/a.nes")));
        assert!(
            csv_lines
                .iter()
                .any(|line| line.ends_with("games/sub/b.NES"))
        );
        assert!(!csv_lines.iter().any(|line| line.ends_with("notes.txt")));
    }

    #[test]
    fn given_existing_catalog_when_refreshing_then_new_files_are_added_without_duplicates() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("roms");
        write_file(&root.join("old.nes"));
        write_file(&root.join("new.nes"));

        let old_path = root.join("old.nes");
        let catalog_path = temp.path().join("cartridges.csv");
        fs::write(&catalog_path, format!("path\n{}\n", old_path.display())).expect("seed csv");

        let options = CartridgeCatalogOptions::new(vec![root], catalog_path.clone());
        let _ = refresh_cartridge_catalog(&options).expect("refresh should succeed");

        let csv_lines = read_csv_lines(&catalog_path);
        let old_count = csv_lines
            .iter()
            .filter(|line| line.ends_with("old.nes"))
            .count();
        let new_count = csv_lines
            .iter()
            .filter(|line| line.ends_with("new.nes"))
            .count();
        assert_eq!(old_count, 1);
        assert_eq!(new_count, 1);
    }

    #[test]
    fn given_rebuild_enabled_when_refreshing_then_existing_catalog_is_recreated() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("roms");
        write_file(&root.join("fresh.nes"));

        let stale_path = root.join("stale.nes");
        let catalog_path = temp.path().join("cartridges.csv");
        fs::write(&catalog_path, format!("path\n{}\n", stale_path.display())).expect("seed csv");

        let mut options = CartridgeCatalogOptions::new(vec![root], catalog_path.clone());
        options.rebuild_catalog = true;

        let _ = refresh_cartridge_catalog(&options).expect("refresh should succeed");

        let csv_lines = read_csv_lines(&catalog_path);
        assert!(csv_lines.iter().any(|line| line.ends_with("fresh.nes")));
        assert!(!csv_lines.iter().any(|line| line.ends_with("stale.nes")));
    }

    #[test]
    fn given_scan_disabled_when_refreshing_then_catalog_keeps_existing_entries() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("roms");
        write_file(&root.join("newly_found.nes"));

        let existing = root.join("existing.nes");
        let catalog_path = temp.path().join("cartridges.csv");
        fs::write(&catalog_path, format!("path\n{}\n", existing.display())).expect("seed csv");

        let mut options = CartridgeCatalogOptions::new(vec![root], catalog_path.clone());
        options.scan_enabled = false;

        let entries = refresh_cartridge_catalog(&options).expect("refresh should succeed");

        assert_eq!(entries, vec![existing]);
        let csv_lines = read_csv_lines(&catalog_path);
        assert_eq!(csv_lines.len(), 2);
        assert!(csv_lines.iter().any(|line| line.ends_with("existing.nes")));
        assert!(
            !csv_lines
                .iter()
                .any(|line| line.ends_with("newly_found.nes"))
        );
    }

    #[test]
    #[cfg(unix)]
    fn given_symlinked_directory_loop_when_refreshing_then_scan_skips_symlink() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("roms");
        write_file(&root.join("games/a.nes"));
        fs::create_dir_all(root.join("games/sub")).expect("create nested directory");
        symlink(&root, root.join("games/sub/loop")).expect("create symlink loop");

        let catalog_path = temp.path().join("cartridges.csv");
        let options = CartridgeCatalogOptions::new(vec![root], catalog_path);

        let entries = refresh_cartridge_catalog(&options).expect("refresh should succeed");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].ends_with("games/a.nes"));
    }

    #[test]
    fn default_catalog_path_points_to_dot_neser_cartridges_csv() {
        let home = PathBuf::from("/tmp/demo-home");
        let path = default_catalog_csv_path(&home);
        assert_eq!(path, home.join(".neser").join("cartridges.csv"));
    }

    // The scan must discover ROMs for every platform the ROM browser can
    // display (NES, GB, GBC, GBA, SNES) — the browser reads the same catalog
    // CSV as the in-emulator cartridge switch dialog.
    #[test]
    fn is_rom_file_accepts_all_browser_platform_extensions() {
        assert!(is_rom_file(Path::new("game.nes")));
        assert!(is_rom_file(Path::new("game.NES")));
        assert!(is_rom_file(Path::new("game.gb")));
        assert!(is_rom_file(Path::new("game.GB")));
        assert!(is_rom_file(Path::new("game.gbc")));
        assert!(is_rom_file(Path::new("game.GBC")));
        assert!(is_rom_file(Path::new("game.gba")));
        assert!(is_rom_file(Path::new("game.GBA")));
        assert!(is_rom_file(Path::new("game.sfc")));
        assert!(is_rom_file(Path::new("game.SFC")));
        assert!(is_rom_file(Path::new("game.smc")));
        assert!(is_rom_file(Path::new("game.SMC")));
        assert!(!is_rom_file(Path::new("game.txt")));
        assert!(!is_rom_file(Path::new("game.zip")));
    }

    #[test]
    fn scan_discovers_all_browser_platform_files() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path().join("roms");
        write_file(&root.join("nes_game.nes"));
        write_file(&root.join("gb_game.gb"));
        write_file(&root.join("gbc_game.gbc"));
        write_file(&root.join("gba_game.gba"));
        write_file(&root.join("snes_game.sfc"));
        write_file(&root.join("snes_game2.smc"));
        write_file(&root.join("notes.txt"));

        let catalog_path = temp.path().join("cartridges.csv");
        let options = CartridgeCatalogOptions::new(vec![root], catalog_path);

        let entries = refresh_cartridge_catalog(&options).expect("refresh should succeed");
        assert_eq!(entries.len(), 6);
        assert!(entries.iter().any(|p| p.ends_with("nes_game.nes")));
        assert!(entries.iter().any(|p| p.ends_with("gb_game.gb")));
        assert!(entries.iter().any(|p| p.ends_with("gbc_game.gbc")));
        assert!(entries.iter().any(|p| p.ends_with("gba_game.gba")));
        assert!(entries.iter().any(|p| p.ends_with("snes_game.sfc")));
        assert!(entries.iter().any(|p| p.ends_with("snes_game2.smc")));
        assert!(!entries.iter().any(|p| p.ends_with("notes.txt")));
    }
}
