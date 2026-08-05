//! Shared "ROM path to ready-to-run [`Console`]" loading.
//!
//! Both the native windowed frontend and the headless capture mode need the
//! same startup sequence: detect the system from the file extension, read the
//! ROM, construct the matching [`Console`], hand the ROM to it, and apply any
//! configuration the cartridge header implies. Keeping that in one place stops
//! the two paths from drifting — in particular, a duplicated loader that forgot
//! `apply_rom_timing_mode` would silently capture PAL NES ROMs at NTSC timing.
//!
//! Resetting the console is deliberately left to the caller, so the native path
//! can keep configuring audio before its `reset`.

use crate::platform::app_context::SharedAppContext;
use crate::platform::emulator::{Console, SystemType};
use crate::platform::frontend_toasts::cartridge_load_toast_message;
use std::path::Path;

/// Detect the emulated system from a ROM path's file extension.
///
/// Matching is case-insensitive. Unknown extensions (and paths with no
/// extension at all) fall back to [`SystemType::Nes`], preserving the
/// historical behaviour of the `neser` CLI.
pub fn detect_system_type(path: &str) -> SystemType {
    let extension = Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");

    if extension.eq_ignore_ascii_case("gb") || extension.eq_ignore_ascii_case("gbc") {
        SystemType::GameBoy
    } else if extension.eq_ignore_ascii_case("gba") {
        SystemType::Gba
    } else if extension.eq_ignore_ascii_case("sfc") || extension.eq_ignore_ascii_case("smc") {
        SystemType::Snes
    } else {
        SystemType::Nes
    }
}

/// Read `rom_path` and build the [`Console`] that can run it.
///
/// Adds a cartridge-load toast to `app_context` on both success and failure,
/// and applies the NES ROM's timing mode to the configuration. The returned
/// console has **not** been reset.
pub fn load_console(app_context: &SharedAppContext, rom_path: &str) -> Result<Console, String> {
    let result = build_console(app_context, rom_path);

    // Every load path — success or failure, every system — reports the same
    // toast, so it is raised once here rather than at each exit.
    app_context
        .borrow_mut()
        .add_toast(cartridge_load_toast_message(rom_path, result.is_ok()));

    result
}

/// Read the ROM and construct the console, without raising the load toast.
fn build_console(app_context: &SharedAppContext, rom_path: &str) -> Result<Console, String> {
    let rom_bytes =
        std::fs::read(rom_path).map_err(|err| format!("Failed to read ROM {rom_path}: {err}"))?;

    match detect_system_type(rom_path) {
        SystemType::Nes => build_nes_console(app_context, &rom_bytes, rom_path),
        SystemType::GameBoy => insert_rom(
            Console::new_gameboy(app_context.clone()),
            &rom_bytes,
            rom_path,
        ),
        SystemType::Gba => insert_rom(Console::new_gba(app_context.clone()), &rom_bytes, rom_path),
        SystemType::Snes => {
            insert_rom(Console::new_snes(app_context.clone()), &rom_bytes, rom_path)
        }
    }
}

/// Hand `rom_bytes` to a freshly constructed console.
fn insert_rom(mut console: Console, rom_bytes: &[u8], rom_path: &str) -> Result<Console, String> {
    console.load_rom(rom_bytes, rom_path)?;
    Ok(console)
}

/// Build a NES console, applying the cartridge header's timing mode.
///
/// The NES path is separate because the cartridge must be parsed *before* the
/// console exists: its timing mode feeds the configuration that
/// `Console::new_nes` then reads.
fn build_nes_console(
    app_context: &SharedAppContext,
    rom_bytes: &[u8],
    rom_path: &str,
) -> Result<Console, String> {
    let rom_db = crate::nes::cartridge::load_rom_db();
    let cartridge =
        crate::nes::cartridge::Cartridge::load_from_file(rom_bytes, rom_path, Some(&rom_db))
            .map_err(|err| format!("Failed to load cartridge {rom_path}: {err}"))?;

    app_context
        .borrow_mut()
        .config_mut()
        .apply_rom_timing_mode(cartridge.rom_timing_mode());

    let mut console = Console::new_nes(app_context.clone());
    console
        .as_nes_mut()
        .expect("Console::new_nes always returns the Nes variant")
        .insert_cartridge(cartridge);

    Ok(console)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::console::{Config, RamInitMode};
    use crate::platform::app_context::AppContext;
    use crate::platform::config::FrontendConfig;
    use crate::platform::test_roms::{
        minimal_gb_rom, minimal_gba_rom, minimal_nes_rom, minimal_snes_rom,
    };
    use std::cell::RefCell;
    use std::rc::Rc;
    use tempfile::TempDir;

    fn make_app_context() -> SharedAppContext {
        let config = Config {
            frontend: FrontendConfig {
                ram_init_mode: RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        };
        Rc::new(RefCell::new(AppContext::new_with_config(config)))
    }

    /// Write `bytes` to a uniquely named file with `extension` inside `dir`.
    fn write_rom(dir: &TempDir, extension: &str, bytes: &[u8]) -> String {
        let path = dir.path().join(format!("game.{extension}"));
        std::fs::write(&path, bytes).expect("write ROM fixture");
        path.to_string_lossy().into_owned()
    }

    // --- detect_system_type (moved from main.rs) ---

    #[test]
    fn detect_system_type_gb_extension_returns_gameboy() {
        assert_eq!(detect_system_type("tetris.gb"), SystemType::GameBoy);
    }

    #[test]
    fn detect_system_type_gbc_extension_returns_gameboy() {
        assert_eq!(detect_system_type("game.gbc"), SystemType::GameBoy);
    }

    #[test]
    fn detect_system_type_uppercase_gbc_returns_gameboy() {
        assert_eq!(detect_system_type("GAME.GBC"), SystemType::GameBoy);
    }

    #[test]
    fn detect_system_type_uppercase_gb_returns_gameboy() {
        assert_eq!(detect_system_type("TETRIS.GB"), SystemType::GameBoy);
    }

    #[test]
    fn detect_system_type_nes_extension_returns_nes() {
        assert_eq!(detect_system_type("cpu.nes"), SystemType::Nes);
    }

    #[test]
    fn detect_system_type_gba_extension_returns_gba() {
        assert_eq!(detect_system_type("zelda.gba"), SystemType::Gba);
    }

    #[test]
    fn detect_system_type_sfc_extension_returns_snes() {
        assert_eq!(detect_system_type("game.sfc"), SystemType::Snes);
    }

    #[test]
    fn detect_system_type_smc_extension_returns_snes() {
        assert_eq!(detect_system_type("game.smc"), SystemType::Snes);
    }

    #[test]
    fn detect_system_type_uppercase_sfc_returns_snes() {
        assert_eq!(detect_system_type("GAME.SFC"), SystemType::Snes);
    }

    #[test]
    fn detect_system_type_uppercase_smc_returns_snes() {
        assert_eq!(detect_system_type("GAME.SMC"), SystemType::Snes);
    }

    #[test]
    fn detect_system_type_unknown_extension_falls_back_to_nes() {
        assert_eq!(detect_system_type("rom.unknown"), SystemType::Nes);
    }

    #[test]
    fn detect_system_type_no_extension_falls_back_to_nes() {
        assert_eq!(detect_system_type("noext"), SystemType::Nes);
    }

    // --- load_console ---

    #[test]
    fn load_console_builds_a_nes_console_for_an_ines_rom() {
        // Given an NROM image on disk
        let dir = TempDir::new().expect("create temp dir");
        let rom_path = write_rom(&dir, "nes", &minimal_nes_rom(false));
        let app_context = make_app_context();

        // When it is loaded
        let console = load_console(&app_context, &rom_path).expect("NES ROM should load");

        // Then a NES console is returned, ready to be reset
        assert_eq!(console.system_type(), SystemType::Nes);
    }

    #[test]
    fn load_console_builds_a_gameboy_console_for_a_gb_rom() {
        let dir = TempDir::new().expect("create temp dir");
        let rom_path = write_rom(&dir, "gb", &minimal_gb_rom());
        let app_context = make_app_context();

        let console = load_console(&app_context, &rom_path).expect("GB ROM should load");

        assert_eq!(console.system_type(), SystemType::GameBoy);
    }

    #[test]
    fn load_console_builds_a_gba_console_for_a_gba_rom() {
        let dir = TempDir::new().expect("create temp dir");
        let rom_path = write_rom(&dir, "gba", &minimal_gba_rom());
        let app_context = make_app_context();

        let console = load_console(&app_context, &rom_path).expect("GBA ROM should load");

        assert_eq!(console.system_type(), SystemType::Gba);
    }

    #[test]
    fn load_console_builds_a_snes_console_for_an_sfc_rom() {
        let dir = TempDir::new().expect("create temp dir");
        let rom_path = write_rom(&dir, "sfc", &minimal_snes_rom());
        let app_context = make_app_context();

        let console = load_console(&app_context, &rom_path).expect("SNES ROM should load");

        assert_eq!(console.system_type(), SystemType::Snes);
    }

    #[test]
    fn load_console_applies_the_nes_rom_timing_mode_to_the_configuration() {
        // Given a PAL-flagged NROM image, and a config left at its NTSC default
        let dir = TempDir::new().expect("create temp dir");
        let rom_path = write_rom(&dir, "nes", &minimal_nes_rom(true));
        let app_context = make_app_context();
        let model_before = app_context.borrow().config().nes.hardware_model;

        // When it is loaded
        load_console(&app_context, &rom_path).expect("PAL NES ROM should load");

        // Then the cartridge's timing mode was applied. Without this the
        // headless capture path would run PAL ROMs at NTSC timing while the
        // windowed frontend ran them correctly.
        let model_after = app_context.borrow().config().nes.hardware_model;
        assert_ne!(
            model_after, model_before,
            "expected the PAL header to change the hardware model"
        );
    }

    #[test]
    fn load_console_reports_a_missing_rom_file() {
        // Given a path with no file behind it
        let dir = TempDir::new().expect("create temp dir");
        let missing = dir.path().join("absent.nes");
        let app_context = make_app_context();

        // When it is loaded
        let result = load_console(&app_context, &missing.to_string_lossy());

        // Then the error names the path so the CLI message is actionable.
        // `Console` has no `Debug` impl, so destructure rather than expect_err.
        let Err(error) = result else {
            panic!("a missing ROM should not load");
        };
        assert!(
            error.contains("absent.nes"),
            "expected the path in {error:?}"
        );
    }

    #[test]
    fn load_console_reports_an_invalid_cartridge() {
        // Given a .nes file that is not a valid iNES image
        let dir = TempDir::new().expect("create temp dir");
        let rom_path = write_rom(&dir, "nes", b"definitely not a ROM");
        let app_context = make_app_context();

        // When it is loaded
        let result = load_console(&app_context, &rom_path);

        // Then it fails rather than producing an unusable console
        assert!(result.is_err(), "an invalid cartridge should not load");
    }

    #[test]
    fn load_console_adds_a_cartridge_load_toast() {
        // Given a valid ROM
        let dir = TempDir::new().expect("create temp dir");
        let rom_path = write_rom(&dir, "nes", &minimal_nes_rom(false));
        let app_context = make_app_context();

        // When it is loaded
        load_console(&app_context, &rom_path).expect("NES ROM should load");

        // Then the user-visible load notification was recorded, matching what
        // the windowed frontend shows today.
        let expected = cartridge_load_toast_message(&rom_path, true);
        let toasts = app_context
            .borrow_mut()
            .visible_toasts(std::time::Instant::now());
        assert!(
            toasts.contains(&expected),
            "expected {expected:?} among {toasts:?}"
        );
    }
}
