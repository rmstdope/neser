mod app_context;
mod apu;
mod autorun;
mod bus;
mod cartridge;
mod console;
mod cpu;
mod debugging;
mod frontend_toasts;
mod input;
mod ppu;
mod rendering;
mod sdl_frontend;

use app_context::AppContext;
use console::{
    ApuChannels, CartridgeCatalogOptions, Config, Nes, ParseResult, SaveState,
    default_catalog_csv_path, log_rom_timing_mode_selection, refresh_cartridge_catalog,
};
use debugging::log_info;
use frontend_toasts::{cartridge_load_toast_message, emulator_timing_toast_message};
use sdl_frontend::{SdlEventLoop, SdlNesAudio};
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

fn cartridge_catalog_startup_config(
    app_context: &Rc<RefCell<AppContext>>,
) -> (Vec<String>, bool, bool) {
    let config = app_context.borrow();
    let config = config.config();
    (
        config.cartridge_search_paths.clone(),
        config.scan_cartridges,
        config.rebuild_cartridge_catalog,
    )
}

fn refresh_startup_cartridge_catalog(app_context: &Rc<RefCell<AppContext>>) {
    let (cartridge_search_paths, scan_cartridges, rebuild_cartridge_catalog) =
        cartridge_catalog_startup_config(app_context);

    if let Some(home) = std::env::var_os("HOME") {
        let home_path = PathBuf::from(home);
        let catalog_path = default_catalog_csv_path(home_path.as_path());
        let mut search_paths: Vec<PathBuf> = cartridge_search_paths
            .into_iter()
            .map(PathBuf::from)
            .collect();
        if search_paths.is_empty() {
            search_paths.push(home_path.join(".neser").join("roms"));
        }
        let mut catalog_options = CartridgeCatalogOptions::new(search_paths, catalog_path);
        catalog_options.scan_enabled = scan_cartridges;
        catalog_options.rebuild_catalog = rebuild_cartridge_catalog;
        if let Err(err) = refresh_cartridge_catalog(&catalog_options) {
            log_info(format!(
                "Warning: failed to refresh cartridge catalog: {err}"
            ));
        }
    }
}

fn convert_autorun_for_rom(rom_path: &str) -> Result<String, String> {
    use autorun::{AUTORUN_VERSION, autorun_path_for_rom, convert_autorun_file};

    let path = autorun_path_for_rom(&PathBuf::from(rom_path));
    if !path.exists() {
        return Err(format!(
            "No autorun file found for ROM {}: {}",
            rom_path,
            path.display()
        ));
    }

    convert_autorun_file(&path)?;
    Ok(format!(
        "Converted autorun file to version {}: {}",
        AUTORUN_VERSION,
        path.display()
    ))
}

fn trim_autorun_checkpoints_for_rom(
    rom_path: &str,
    checkpoints_to_trim: usize,
) -> Result<String, String> {
    use autorun::{autorun_path_for_rom, load_autorun_file, save_autorun_file, trim_recording};
    use std::path::PathBuf;

    let path = autorun_path_for_rom(&PathBuf::from(rom_path));
    let mut file = load_autorun_file(&path)?;
    let checkpoints_before = file.checkpoints.len();
    trim_recording(&mut file, checkpoints_to_trim);
    save_autorun_file(&path, &file)?;

    Ok(format!(
        "Trimmed {} checkpoint(s): {} → {} checkpoints, {} frames remaining",
        checkpoints_before.saturating_sub(file.checkpoints.len()),
        checkpoints_before,
        file.checkpoints.len(),
        file.frames.len(),
    ))
}

fn recalculate_autorun_for_rom(rom_path: &str) -> Result<String, String> {
    use autorun::{
        autorun_path_for_rom, headless_playback::recalculate_checkpoint_crcs_with_progress,
        load_autorun_file, save_autorun_file,
    };
    use cartridge::Cartridge;
    use console::RamInitMode;
    use std::io::{self, Write};

    let path = autorun_path_for_rom(&PathBuf::from(rom_path));
    if !path.exists() {
        return Err(format!(
            "No autorun file found for ROM {}: {}",
            rom_path,
            path.display()
        ));
    }

    let mut file = load_autorun_file(&path)?;
    let rom_bytes =
        fs::read(rom_path).map_err(|e| format!("Failed to read ROM {}: {e}", rom_path))?;

    let config = Config {
        ram_init_mode: RamInitMode::Zero,
        ..Default::default()
    };
    let app_context = AppContext::new_with_config(config);

    let cart = Cartridge::load_from_file(&rom_bytes, rom_path, app_context.clone())
        .map_err(|e| format!("Failed to load cartridge {}: {e}", rom_path))?;
    let mut nes = Nes::new(app_context);
    nes.insert_cartridge(cart);
    nes.reset(false);

    let mut progress_printed = false;
    let updated =
        recalculate_checkpoint_crcs_with_progress(&mut nes, &mut file, None, |done, total| {
            progress_printed = true;
            print!("\rRecalculating checkpoint CRC(s): {done}/{total}");
            let _ = io::stdout().flush();
        })?;

    if progress_printed {
        println!("\n");
    }
    save_autorun_file(&path, &file)?;

    Ok(format!(
        "Recalculated {} checkpoint CRC(s) in {}",
        updated,
        path.display()
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();

    let parsed_config = match Config::new(&args)? {
        ParseResult::Help => {
            Config::print_help();
            return Ok(());
        }
        ParseResult::Config(c) => c,
    };

    let app_context = Rc::new(RefCell::new(AppContext::new_with_config(parsed_config)));

    refresh_startup_cartridge_catalog(&app_context);

    // Handle --trim-checkpoints: modify recording file and exit immediately.
    let trim_checkpoints = app_context.borrow().config().autorun_trim_checkpoints;
    let trim_rom_path = app_context.borrow().config().rom_path.clone();
    if let (Some(checkpoints_to_trim), Some(rom_path)) =
        (trim_checkpoints, trim_rom_path.as_deref())
    {
        let message = trim_autorun_checkpoints_for_rom(rom_path, checkpoints_to_trim)?;
        println!("{message}");
        return Ok(());
    }

    // Handle --convert-autorun: convert recording file format and exit immediately.
    let convert_autorun_requested = app_context.borrow().config().autorun_convert;
    let convert_rom_path = app_context.borrow().config().rom_path.clone();
    if convert_autorun_requested {
        let rom_path =
            convert_rom_path.ok_or_else(|| "--convert-autorun requires a ROM path".to_string())?;
        let message = convert_autorun_for_rom(&rom_path)?;
        println!("{message}");
        return Ok(());
    }

    // Handle --recalculate-autorun: replay and rewrite checkpoint CRCs, then exit.
    let recalculate_autorun_requested = app_context.borrow().config().autorun_recalculate;
    let recalculate_rom_path = app_context.borrow().config().rom_path.clone();
    if recalculate_autorun_requested {
        let rom_path = recalculate_rom_path
            .ok_or_else(|| "--recalculate-autorun requires a ROM path".to_string())?;
        let message = recalculate_autorun_for_rom(&rom_path)?;
        println!("{message}");
        return Ok(());
    }

    // Initialize global tracing state (only active in debug builds)
    let tracing_config = app_context.borrow().config().tracing;
    debugging::init_tracing(tracing_config);

    // Initialize SDL2
    let sdl_context = sdl2::init()?;

    // Create audio output (request 44.1 kHz) unless disabled.
    // SDL may open the device at a different rate; always sync the APU to the actual rate
    // to avoid steady underruns.
    let mut audio_sample_rate = None;
    let audio_enabled = app_context.borrow().config().audio_enabled;
    let audio = if !audio_enabled {
        None
    } else {
        let audio = SdlNesAudio::new(&sdl_context, 44100)?;
        audio_sample_rate = Some(audio.actual_sample_rate() as f32);
        Some(audio)
    };

    // Palette display requiring only scanline-based palette changes,
    // intended to demonstrate the full palette even on less advanced emulators
    // Seems to work ok!
    // let rom_data = std::fs::read("roms/rainwarrior/palette.nes")?;

    // Simple display of any chosen color full-screen
    // Seems to work ok!
    // let rom_data = std::fs::read("roms/rainwarrior/color_test.nes")?;

    // Load game cartridge
    // let default_rom_data = std::fs::read("roms/games/pac-man.nes")?;
    // let default_rom_data = std::fs::read("roms/games/Balloon_fight.nes")?;
    // let default_rom_path = "roms/games/donkey kong.nes";
    // let default_rom_path = "roms/games/Legend of Zelda, The (USA) (Rev 1).nes";
    // let default_rom_path = "roms/games/Mike Tyson's Punch-Out!! (Japan, USA) (Rev 1).nes";
    // let default_rom_path = "roms/games/Castlevania III - Dracula's Curse (USA).nes";
    // let default_rom_path = "roms/games/Akumajyou_Densetsu_(Tr).nes";
    // let default_rom_path = "roms/games/Dragon_Ball_Z_Gaiden_(Tr).nes";
    // let default_rom_path = "roms/games/Super Mario Bros. 3 (USA) (Rev 1).nes";
    // let default_rom_path = "roms/games/Super Chinese 3 (J) [p1].nes";

    // https://sourceforge.net/p/fceultra/bugs/710/
    let default_rom_path = "roms/games/mappers/6/Air Fortress (J) [hFFE].nes";
    // let default_rom_path = "roms/manual_tests/PaddleTest3/PaddleTest.nes";

    // let rom_data = manual_test_cartridges::triangle_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse1_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse2_only_nrom_128();
    // let rom_data = manual_test_cartridges::noise_only_nrom_128();

    let rom_path = app_context
        .borrow()
        .config()
        .rom_path
        .clone()
        .unwrap_or_else(|| default_rom_path.to_string());
    let rom_bytes = match fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            app_context
                .borrow_mut()
                .add_toast(cartridge_load_toast_message(&rom_path, false));
            return Err(err.into());
        }
    };
    let cart =
        match cartridge::Cartridge::load_from_file(&rom_bytes, &rom_path, app_context.clone()) {
            Ok(cartridge) => {
                app_context
                    .borrow_mut()
                    .add_toast(cartridge_load_toast_message(&rom_path, true));
                cartridge
            }
            Err(err) => {
                app_context
                    .borrow_mut()
                    .add_toast(cartridge_load_toast_message(&rom_path, false));
                return Err(err.into());
            }
        };

    let rom_timing_mode = cart.rom_timing_mode();
    let applied = app_context
        .borrow_mut()
        .config_mut()
        .apply_rom_timing_mode(rom_timing_mode);
    log_rom_timing_mode_selection(&app_context, rom_timing_mode, applied);

    let mut nes_instance = Nes::new(app_context.clone());
    nes_instance.insert_cartridge(cart);
    let tv_system = app_context.borrow().config().tv_system;
    app_context
        .borrow_mut()
        .add_toast(emulator_timing_toast_message(tv_system));

    if let Some(actual_rate) = audio_sample_rate {
        nes_instance.apu().borrow_mut().set_sample_rate(actual_rate);
    }

    // Create event loop with headless mode if autorun playback is headless
    let headless = app_context.borrow().config().autorun_headless;
    // In headless autorun/playback, force audio to None so no audio device is required
    let audio_for_frontend = if headless { None } else { audio };
    let mut event_loop =
        SdlEventLoop::new_with_context(headless, audio_for_frontend, app_context.clone())?;

    // Initialize autorun if enabled
    let (autorun_mode, autorun_overwrite, autorun_extend, autorun_from_checkpoint) = {
        let config = app_context.borrow();
        let config = config.config();
        (
            config.autorun_mode,
            config.autorun_overwrite,
            config.autorun_extend,
            config.autorun_from_checkpoint,
        )
    };
    let load_state = app_context.borrow().config().load_state;
    if load_state {
        let state_path = nes_instance
            .state_path()
            .ok_or("No save-state path available for loaded ROM")?;
        let bytes = fs::read(&state_path)?;
        let state = SaveState::from_bytes(&bytes)
            .map_err(|err| format!("Failed to deserialize save-state: {err}"))?;
        nes_instance
            .load_state(&state)
            .map_err(|err| format!("Failed to restore save-state: {err}"))?;
    } else {
        nes_instance.reset(false);
    }

    // Initialize autorun AFTER reset so checkpoint state restore is not overwritten.
    if autorun_mode != console::AutorunMode::None {
        event_loop.init_autorun(
            autorun_mode,
            &rom_path,
            autorun_overwrite,
            autorun_extend,
            autorun_from_checkpoint,
            &mut nes_instance,
        )?;
    }

    // Request debugger open if enabled via CLI
    let debugger_enabled = app_context.borrow().config().debugger_enabled;
    if debugger_enabled {
        event_loop.request_debugger_open();
    }

    // Apply channel enable/disable settings
    {
        let mut apu = nes_instance.apu().borrow_mut();
        let app_context = app_context.borrow();
        let config = app_context.config();
        apu.set_pulse1_enabled(config.apu_channels.contains(ApuChannels::PULSE1));
        apu.set_pulse2_enabled(config.apu_channels.contains(ApuChannels::PULSE2));
        apu.set_triangle_enabled(config.apu_channels.contains(ApuChannels::TRIANGLE));
        apu.set_noise_enabled(config.apu_channels.contains(ApuChannels::NOISE));
        apu.set_dmc_enabled(config.apu_channels.contains(ApuChannels::DMC));
    }

    let run_tracing = app_context.borrow().config().tracing;
    let run_result = event_loop.run(&mut nes_instance, run_tracing);

    // Handle autorun exit codes before save-on-shutdown
    if let Err(ref e) = run_result
        && let Some(exit_code) = e
            .strip_prefix("AUTORUN_EXIT:")
            .and_then(|s| s.parse::<i32>().ok())
    {
        std::process::exit(exit_code);
    }

    // Best-effort save on clean shutdown (Escape/Quit).
    if run_result.is_ok()
        && let Err(e) = nes_instance.bus().borrow().save_ram()
    {
        log_info(format!("Warning: failed to save RAM: {}", e));
    }

    run_result.map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autorun::AUTORUN_VERSION;
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    #[serial]
    fn test_enable_debugger_requests_open_and_pauses_on_start() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(b"").unwrap();

        let args = vec![
            "neser".to_string(),
            "--debugger".to_string(),
            "true".to_string(),
            "--config".to_string(),
            file.path().to_string_lossy().to_string(),
        ];

        let config = match Config::new(&args).unwrap() {
            ParseResult::Config(c) => c,
            ParseResult::Help => panic!("Expected Config"),
        };

        let app_context = AppContext::new_with_config(config.clone());
        let mut event_loop = SdlEventLoop::new(true, None, app_context).unwrap();

        if config.debugger_enabled {
            event_loop.request_debugger_open();
        }

        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }

    #[test]
    fn test_convert_autorun_for_rom_fails_when_autorun_file_missing() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let rom_path = temp_dir.path().join("missing.nes");

        let result = convert_autorun_for_rom(rom_path.to_str().expect("rom path to str"));

        assert!(
            result.is_err(),
            "conversion should fail when corresponding .autorun file is missing"
        );
    }

    #[test]
    fn test_convert_autorun_for_rom_converts_v2_file_to_v3() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let rom_path = temp_dir.path().join("game.nes");
        let autorun_path = rom_path.with_extension("autorun");

        std::fs::write(
            &autorun_path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "version": 2,
                "frames": [
                    {"player1": 0, "player2": 0},
                    {"player1": 0, "player2": 0},
                    {"player1": 1, "player2": 0}
                ],
                "checkpoints": []
            }))
            .expect("serialize v2 file"),
        )
        .expect("write v2 autorun file");

        convert_autorun_for_rom(rom_path.to_str().expect("rom path to str"))
            .expect("convert v2 to v3");

        let converted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&autorun_path).expect("read converted file"))
                .expect("parse converted file");

        assert_eq!(converted["version"], AUTORUN_VERSION);
        assert_eq!(converted["frames"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            converted["frames"][0],
            serde_json::json!({"player1": 0, "player2": 0, "repeat": 2})
        );
    }

    #[test]
    fn test_recalculate_autorun_for_rom_fails_when_autorun_file_missing() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let rom_path = temp_dir.path().join("missing.nes");

        let result = recalculate_autorun_for_rom(rom_path.to_str().expect("rom path to str"));

        assert!(
            result.is_err(),
            "recalculation should fail when corresponding .autorun file is missing"
        );
    }
}
