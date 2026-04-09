#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use crate::app_context::AppContext;
    use crate::autorun::load_autorun_file;
    use crate::nes::autorun::headless_playback::run_headless_playback;
    use crate::nes::cartridge::{Cartridge, TimingMode as CartridgeTimingMode};
    use crate::nes::console::{Config, HardwareModel, Nes, NesConfig, RamInitMode};

    fn deterministic_config() -> Config {
        Config {
            nes: NesConfig {
                ram_init_mode: RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn hardware_model_for_cartridge(timing_mode: CartridgeTimingMode) -> HardwareModel {
        HardwareModel::from_timing_mode(timing_mode)
    }

    #[allow(dead_code)]
    fn make_nes_for_rom(rom_path: &Path) -> Result<Nes, String> {
        let rom_data = std::fs::read(rom_path)
            .map_err(|e| format!("Failed to read ROM {}: {e}", rom_path.display()))?;
        let cartridge = Cartridge::load_from_file(&rom_data, rom_path, AppContext::new())
            .map_err(|e| format!("Failed to parse ROM {}: {e}", rom_path.display()))?;

        let mut config = deterministic_config();
        config.nes.hardware_model = hardware_model_for_cartridge(cartridge.rom_timing_mode());

        let mut nes = Nes::new(AppContext::new_with_config(config));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        Ok(nes)
    }

    #[allow(dead_code)]
    fn rom_path_for_autorun(autorun_path: &Path) -> PathBuf {
        autorun_path.with_extension("nes")
    }

    #[allow(dead_code)]
    fn verify_single_autorun(autorun_path: &Path) -> Result<(), String> {
        let recording = load_autorun_file(autorun_path, None)
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

    include!(concat!(env!("OUT_DIR"), "/autorun_generated_tests.rs"));
}
