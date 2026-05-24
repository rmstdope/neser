use std::fs;

use crate::platform::debugging::log_info;
use crate::platform::emulator::Console;

/// Saves the current emulator state to disk.
///
/// Shows a toast notification on success or failure.
/// Does nothing if no ROM is loaded or the system has no state path.
pub fn save_state_to_disk(console: &mut Console) {
    let Some(state_path) = console.state_path() else {
        return;
    };

    let bytes = match console.save_state_bytes() {
        Ok(bytes) => bytes,
        Err(err) => {
            log_info(format!("Failed to serialize save-state: {err}"));
            console
                .app_context()
                .borrow_mut()
                .add_toast("Failed to save state");
            return;
        }
    };

    if let Some(parent) = state_path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        log_info(format!("Failed to create save-state directory: {err}"));
        console
            .app_context()
            .borrow_mut()
            .add_toast("Failed to save state");
        return;
    }

    // Atomic write: write to temp file, then rename.
    // On Windows, rename fails if the destination already exists, so we
    // remove it first (best-effort) before the rename.
    let mut tmp_path = state_path.clone();
    tmp_path.set_extension(format!("state.tmp.{}", std::process::id()));

    if let Err(err) = fs::write(&tmp_path, bytes) {
        log_info(format!("Failed to write save-state: {err}"));
        console
            .app_context()
            .borrow_mut()
            .add_toast("Failed to save state");
        return;
    }

    #[cfg(windows)]
    let _ = fs::remove_file(&state_path);

    if let Err(err) = fs::rename(&tmp_path, &state_path) {
        log_info(format!("Failed to finalize save-state: {err}"));
        let _ = fs::remove_file(&tmp_path);
        console
            .app_context()
            .borrow_mut()
            .add_toast("Failed to save state");
        return;
    }

    console.app_context().borrow_mut().add_toast("State saved");
}

/// Loads a previously saved emulator state from disk.
///
/// Shows a toast notification on success, failure, or when no save file exists.
/// Does nothing if no ROM is loaded or the system has no state path.
pub fn load_state_from_disk(console: &mut Console) {
    let Some(state_path) = console.state_path() else {
        return;
    };

    let bytes = match fs::read(&state_path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            console
                .app_context()
                .borrow_mut()
                .add_toast("No save state found");
            return;
        }
        Err(err) => {
            log_info(format!("Failed to read save-state: {err}"));
            console
                .app_context()
                .borrow_mut()
                .add_toast("Failed to load state");
            return;
        }
    };

    if let Err(err) = console.load_state_bytes(&bytes) {
        log_info(format!("Failed to restore save-state: {err}"));
        console
            .app_context()
            .borrow_mut()
            .add_toast("Failed to load state");
        return;
    }

    console.app_context().borrow_mut().add_toast("State loaded");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::Cartridge;
    use crate::nes::console::Config;
    use crate::platform::app_context::AppContext;
    use crate::platform::emulator::Console;
    use std::fs;
    use std::path::Path;
    use std::time::Instant;
    use tempfile::TempDir;

    fn setup_console_with_temp_rom(temp_dir: &TempDir) -> Console {
        let rom_path = temp_dir.path().join("test.nes");
        fs::copy("roms/nes/automated_tests/nestest/nestest.nes", &rom_path)
            .expect("Failed to copy test ROM");
        let rom_bytes = fs::read(&rom_path).expect("Failed to read ROM");
        let mut console = Console::new_nes(AppContext::new_with_config(Config::default()));
        let Console::Nes(ref nes) = console else {
            panic!("expected NES console");
        };
        let cart = Cartridge::load_from_file(&rom_bytes, &rom_path, Some(nes.rom_db()))
            .expect("Failed to load ROM");
        let Console::Nes(ref mut nes) = console else {
            unreachable!()
        };
        nes.insert_cartridge(cart);
        console.reset(false);
        console
    }

    fn minimal_valid_gba_rom() -> Vec<u8> {
        use crate::gba::cartridge::header::{
            COMPLEMENT_CHECK_OFFSET, FIXED_BYTE_OFFSET, FIXED_BYTE_VALUE, compute_complement_check,
        };

        let mut rom = vec![0u8; 0xC0];
        rom[FIXED_BYTE_OFFSET] = FIXED_BYTE_VALUE;
        rom[COMPLEMENT_CHECK_OFFSET] = compute_complement_check(&rom);
        rom
    }

    fn setup_gba_console_with_temp_rom(temp_dir: &TempDir) -> Console {
        let rom_path = temp_dir.path().join("test.gba");
        fs::write(&rom_path, minimal_valid_gba_rom()).expect("Failed to write GBA ROM");
        let rom_bytes = fs::read(&rom_path).expect("Failed to read GBA ROM");
        let mut console = Console::new_gba(AppContext::new_with_config(Config::default()));
        console
            .load_rom(&rom_bytes, rom_path.to_str().expect("ROM path is UTF-8"))
            .expect("Failed to load GBA ROM");
        console
    }

    fn assert_visible_toast(console: &Console, expected: &str) {
        let toasts = console
            .app_context()
            .borrow_mut()
            .visible_toasts(Instant::now());
        assert!(
            toasts.iter().any(|t| t.contains(expected)),
            "expected '{expected}' toast, got: {toasts:?}"
        );
    }

    fn assert_state_file_exists(rom_path: &Path) {
        let state_path = rom_path.with_extension("state");
        assert!(
            state_path.exists(),
            "expected save-state file at {}",
            state_path.display()
        );
    }

    #[test]
    fn test_save_state_to_disk_shows_state_saved_toast() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut console = setup_console_with_temp_rom(&temp_dir);

        save_state_to_disk(&mut console);

        let toasts = console
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
        let mut console = setup_console_with_temp_rom(&temp_dir);

        load_state_from_disk(&mut console);

        let toasts = console
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
        let mut console = setup_console_with_temp_rom(&temp_dir);

        save_state_to_disk(&mut console);
        load_state_from_disk(&mut console);

        let toasts = console
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
        let mut console = setup_console_with_temp_rom(&temp_dir);

        // Block the rename by occupying the target state path with a directory.
        let rom_path = temp_dir.path().join("test.nes");
        let state_path = rom_path.with_extension("state");
        fs::create_dir_all(&state_path).expect("Failed to create blocking directory");

        save_state_to_disk(&mut console);

        let toasts = console
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
        let mut console = setup_console_with_temp_rom(&temp_dir);

        // Place a directory at the state path so fs::read returns an EISDIR error.
        let rom_path = temp_dir.path().join("test.nes");
        let state_path = rom_path.with_extension("state");
        fs::create_dir_all(&state_path).expect("Failed to create blocking directory");

        load_state_from_disk(&mut console);

        let toasts = console
            .app_context()
            .borrow_mut()
            .visible_toasts(Instant::now());
        assert!(
            toasts.iter().any(|t| t.contains("Failed to load state")),
            "expected 'Failed to load state' toast on read error, got: {toasts:?}"
        );
    }

    #[test]
    fn test_gba_save_state_to_disk_writes_file_and_shows_state_saved_toast() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut console = setup_gba_console_with_temp_rom(&temp_dir);
        let rom_path = temp_dir.path().join("test.gba");

        save_state_to_disk(&mut console);

        assert_state_file_exists(&rom_path);
        assert_visible_toast(&console, "State saved");
    }

    #[test]
    fn test_gba_load_state_from_disk_shows_no_save_state_found_toast_when_file_absent() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut console = setup_gba_console_with_temp_rom(&temp_dir);

        load_state_from_disk(&mut console);

        assert_visible_toast(&console, "No save state found");
    }

    #[test]
    fn test_gba_load_state_from_disk_shows_state_loaded_toast_when_file_exists() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut console = setup_gba_console_with_temp_rom(&temp_dir);

        save_state_to_disk(&mut console);
        load_state_from_disk(&mut console);

        assert_visible_toast(&console, "State loaded");
    }

    #[test]
    fn test_gba_save_state_to_disk_shows_failed_toast_on_io_error() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut console = setup_gba_console_with_temp_rom(&temp_dir);
        let state_path = temp_dir.path().join("test.state");
        fs::create_dir_all(&state_path).expect("Failed to create blocking directory");

        save_state_to_disk(&mut console);

        assert_visible_toast(&console, "Failed to save state");
    }

    #[test]
    fn test_gba_load_state_from_disk_shows_failed_toast_on_read_error() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let mut console = setup_gba_console_with_temp_rom(&temp_dir);
        let state_path = temp_dir.path().join("test.state");
        fs::create_dir_all(&state_path).expect("Failed to create blocking directory");

        load_state_from_disk(&mut console);

        assert_visible_toast(&console, "Failed to load state");
    }
}
