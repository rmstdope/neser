use std::fs;

use crate::nes::console::{Nes, SaveState};
use crate::platform::debugging::log_info;

/// Saves the current NES state to disk.
///
/// Shows a toast notification on success or failure.
pub fn save_state_to_disk(nes: &mut Nes) {
    let Some(state_path) = nes.state_path() else {
        return;
    };

    let state = nes.save_state();
    let bytes = match state.to_bytes() {
        Ok(bytes) => bytes,
        Err(err) => {
            log_info(format!("Failed to serialize save-state: {err}"));
            nes.app_context()
                .borrow_mut()
                .add_toast("Failed to save state");
            return;
        }
    };

    if let Some(parent) = state_path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        log_info(format!("Failed to create save-state directory: {err}"));
        nes.app_context()
            .borrow_mut()
            .add_toast("Failed to save state");
        return;
    }

    // Atomic write: write to temp file, then rename.
    let mut tmp_path = state_path.clone();
    tmp_path.set_extension(format!("state.tmp.{}", std::process::id()));

    if let Err(err) = fs::write(&tmp_path, bytes) {
        log_info(format!("Failed to write save-state: {err}"));
        nes.app_context()
            .borrow_mut()
            .add_toast("Failed to save state");
        return;
    }

    if let Err(err) = fs::rename(&tmp_path, &state_path) {
        log_info(format!("Failed to finalize save-state: {err}"));
        let _ = fs::remove_file(&tmp_path);
        nes.app_context()
            .borrow_mut()
            .add_toast("Failed to save state");
        return;
    }

    nes.app_context().borrow_mut().add_toast("State saved");
}

/// Loads a previously saved NES state from disk.
///
/// Shows a toast notification on success, failure, or when no save file exists.
pub fn load_state_from_disk(nes: &mut Nes) {
    let Some(state_path) = nes.state_path() else {
        return;
    };

    let bytes = match fs::read(&state_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            nes.app_context()
                .borrow_mut()
                .add_toast("No save state found");
            return;
        }
        Err(err) => {
            log_info(format!("Failed to read save-state: {err}"));
            nes.app_context()
                .borrow_mut()
                .add_toast("Failed to load state");
            return;
        }
    };

    let state = match SaveState::from_bytes(&bytes) {
        Ok(state) => state,
        Err(err) => {
            log_info(format!("Failed to deserialize save-state: {err}"));
            nes.app_context()
                .borrow_mut()
                .add_toast("Failed to load state");
            return;
        }
    };

    if let Err(err) = nes.load_state(&state) {
        log_info(format!("Failed to restore save-state: {err}"));
        nes.app_context()
            .borrow_mut()
            .add_toast("Failed to load state");
        return;
    }

    nes.app_context().borrow_mut().add_toast("State loaded");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::Cartridge;
    use crate::nes::console::Config;
    use crate::platform::app_context::AppContext;
    use std::fs;
    use std::time::Instant;
    use tempfile::TempDir;

    fn setup_nes_with_temp_rom(temp_dir: &TempDir) -> Nes {
        let rom_path = temp_dir.path().join("test.nes");
        fs::copy("roms/automated_tests/nestest/nestest.nes", &rom_path)
            .expect("Failed to copy test ROM");
        let rom_bytes = fs::read(&rom_path).expect("Failed to read ROM");
        let mut nes = Nes::new(AppContext::new_with_config(Config::default()));
        let cart = Cartridge::load_from_file(&rom_bytes, &rom_path, Some(nes.rom_db()))
            .expect("Failed to load ROM");
        nes.insert_cartridge(cart);
        nes.reset(false);
        nes
    }

    #[test]
    fn test_save_state_to_disk_shows_state_saved_toast() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut nes = setup_nes_with_temp_rom(&temp_dir);

        save_state_to_disk(&mut nes);

        let toasts = nes
            .app_context()
            .borrow_mut()
            .visible_toasts(Instant::now());
        assert!(
            toasts.iter().any(|t| t.contains("State saved")),
            "expected 'State saved' toast after successful save, got: {toasts:?}"
        );
    }

    #[test]
    fn test_load_state_from_disk_shows_no_save_state_found_toast_when_file_absent() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut nes = setup_nes_with_temp_rom(&temp_dir);

        load_state_from_disk(&mut nes);

        let toasts = nes
            .app_context()
            .borrow_mut()
            .visible_toasts(Instant::now());
        assert!(
            toasts.iter().any(|t| t.contains("No save state found")),
            "expected 'No save state found' toast, got: {toasts:?}"
        );
    }

    #[test]
    fn test_load_state_from_disk_shows_state_loaded_toast_when_file_exists() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut nes = setup_nes_with_temp_rom(&temp_dir);

        save_state_to_disk(&mut nes);
        load_state_from_disk(&mut nes);

        let toasts = nes
            .app_context()
            .borrow_mut()
            .visible_toasts(Instant::now());
        assert!(
            toasts.iter().any(|t| t.contains("State loaded")),
            "expected 'State loaded' toast after successful load, got: {toasts:?}"
        );
    }

    #[test]
    fn test_save_state_to_disk_shows_failed_toast_on_io_error() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut nes = setup_nes_with_temp_rom(&temp_dir);

        // Block the rename by occupying the target state path with a directory.
        let rom_path = temp_dir.path().join("test.nes");
        let state_path = rom_path.with_extension("state");
        fs::create_dir_all(&state_path).expect("Failed to create blocking directory");

        save_state_to_disk(&mut nes);

        let toasts = nes
            .app_context()
            .borrow_mut()
            .visible_toasts(Instant::now());
        assert!(
            toasts.iter().any(|t| t.contains("Failed to save state")),
            "expected 'Failed to save state' toast on I/O error, got: {toasts:?}"
        );
    }

    #[test]
    fn test_load_state_from_disk_shows_failed_toast_on_read_error() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut nes = setup_nes_with_temp_rom(&temp_dir);

        // Place a directory at the state path so fs::read returns an EISDIR error.
        let rom_path = temp_dir.path().join("test.nes");
        let state_path = rom_path.with_extension("state");
        fs::create_dir_all(&state_path).expect("Failed to create blocking directory");

        load_state_from_disk(&mut nes);

        let toasts = nes
            .app_context()
            .borrow_mut()
            .visible_toasts(Instant::now());
        assert!(
            toasts.iter().any(|t| t.contains("Failed to load state")),
            "expected 'Failed to load state' toast on read error, got: {toasts:?}"
        );
    }
}
