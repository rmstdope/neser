#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::app_context::AppContext;
    use crate::autorun::{headless_playback::run_headless_playback, load_autorun_file};
    use crate::cartridge::{Cartridge, TimingMode as CartridgeTimingMode};
    use crate::console::{Config, Nes, RamInitMode, TimingMode};

    const MAPPER_AUTORUN_ROOT: &str = "roms/games/mappers";

    fn deterministic_config() -> Config {
        Config {
            ram_init_mode: RamInitMode::Zero,
            ..Default::default()
        }
    }

    fn collect_autorun_files_recursive(dir: &Path) -> Result<Vec<PathBuf>, String> {
        let mut autorun_files = Vec::new();

        for entry in std::fs::read_dir(dir)
            .map_err(|e| format!("Failed to read directory {}: {e}", dir.display()))?
        {
            let entry = entry
                .map_err(|e| format!("Failed to read directory entry in {}: {e}", dir.display()))?;
            let path = entry.path();

            if path.is_dir() {
                autorun_files.extend(collect_autorun_files_recursive(&path)?);
            } else if path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("autorun"))
            {
                autorun_files.push(path);
            }
        }

        autorun_files.sort();
        Ok(autorun_files)
    }

    fn timing_mode_for_cartridge(timing_mode: CartridgeTimingMode) -> TimingMode {
        match timing_mode {
            CartridgeTimingMode::Pal => TimingMode::Pal,
            CartridgeTimingMode::Ntsc => TimingMode::Ntsc,
            CartridgeTimingMode::MultiRegion
            | CartridgeTimingMode::Dendy
            | CartridgeTimingMode::Unknown(_) => TimingMode::Ntsc,
        }
    }

    fn make_nes_for_rom(rom_path: &Path) -> Result<Nes, String> {
        let rom_data = std::fs::read(rom_path)
            .map_err(|e| format!("Failed to read ROM {}: {e}", rom_path.display()))?;
        let cartridge = Cartridge::load_from_file(&rom_data, rom_path, AppContext::new())
            .map_err(|e| format!("Failed to parse ROM {}: {e}", rom_path.display()))?;

        let mut config = deterministic_config();
        config.tv_system = timing_mode_for_cartridge(cartridge.rom_timing_mode());

        let mut nes = Nes::new(AppContext::new_with_config(config));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        Ok(nes)
    }

    fn rom_path_for_autorun(autorun_path: &Path) -> PathBuf {
        autorun_path.with_extension("nes")
    }

    fn verify_single_autorun(autorun_path: &Path) -> Result<(), String> {
        let recording = load_autorun_file(autorun_path)
            .map_err(|e| format!("{}: {e}", autorun_path.display()))?;

        if recording.checkpoints.is_empty() {
            return Err(format!(
                "{}: recording has no checkpoints, cannot verify final CRC",
                autorun_path.display()
            ));
        }
        if recording.frames.is_empty() {
            return Err(format!(
                "{}: recording has no frames",
                autorun_path.display()
            ));
        }

        let last_checkpoint = recording
            .checkpoints
            .last()
            .expect("recording checkpoint list should be non-empty");
        if last_checkpoint.frame_index as usize != recording.frames.len().saturating_sub(1) {
            return Err(format!(
                "{}: final checkpoint must target the last frame",
                autorun_path.display()
            ));
        }

        let rom_path = rom_path_for_autorun(autorun_path);
        if !rom_path.exists() {
            return Err(format!(
                "{}: matching ROM file not found at {}",
                autorun_path.display(),
                rom_path.display()
            ));
        }

        let mut nes =
            make_nes_for_rom(&rom_path).map_err(|e| format!("{}: {e}", rom_path.display()))?;

        let result = run_headless_playback(&mut nes, &recording, None)
            .map_err(|e| format!("{}: playback failed: {e}", autorun_path.display()))?;

        if result.total_checkpoints_verified != recording.checkpoints.len() {
            return Err(format!(
                "{}: not all checkpoints were verified (verified {}, expected {})",
                autorun_path.display(),
                result.total_checkpoints_verified,
                recording.checkpoints.len()
            ));
        }

        if result.crc_mismatches != 0 {
            return Err(format!(
                "{}: CRC mismatch detected ({} mismatches)",
                autorun_path.display(),
                result.crc_mismatches
            ));
        }

        Ok(())
    }

    #[test]
    fn test_mapper_autoruns_verify_all_checkpoints_and_final_crc() {
        let root = Path::new(MAPPER_AUTORUN_ROOT);

        if !root.exists() {
            println!(
                "[autorun verification] Skipping - mapper autorun root does not exist: {}",
                root.display()
            );
            return;
        }

        let autorun_files =
            collect_autorun_files_recursive(root).expect("collect mapper autorun files");

        if autorun_files.is_empty() {
            println!(
                "[autorun verification] Skipping - no .autorun files found under {}",
                root.display()
            );
            return;
        }

        let total = autorun_files.len();
        let mut failures = Vec::new();

        for (index, autorun_path) in autorun_files.into_iter().enumerate() {
            match verify_single_autorun(&autorun_path) {
                Ok(()) => println!(
                    "[autorun verification] ({}/{}) PASS - {}",
                    index + 1,
                    total,
                    autorun_path.display()
                ),
                Err(error) => {
                    println!(
                        "[autorun verification] ({}/{}) FAIL - {}",
                        index + 1,
                        total,
                        autorun_path.display()
                    );
                    failures.push(error);
                }
            }
        }

        assert!(
            failures.is_empty(),
            "autorun verification failures:\n{}",
            failures.join("\n")
        );
    }
}
