// Modules shared between lib.rs and main.rs may have public APIs consumed only
// by the library or test code, producing dead_code warnings in the binary crate.
#![allow(dead_code)]

mod frontends;
mod gb;
mod gba;
mod nes;
mod platform;
mod snes;

use nes::console::{
    CartridgeCatalogOptions, Config, Nes, ParseResult, default_catalog_csv_path,
    refresh_cartridge_catalog,
};
use platform::app_context::AppContext;
use platform::autorun::AutorunFormat;
use platform::debugging::log_info;
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
        config.frontend.cartridge_search_paths.clone(),
        config.frontend.scan_cartridges,
        config.frontend.rebuild_cartridge_catalog,
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

fn convert_autorun_for_rom(rom_path: &str, format: AutorunFormat) -> Result<String, String> {
    use platform::autorun::{AUTORUN_VERSION, autorun_path_for_rom, convert_autorun_file};

    let path = autorun_path_for_rom(&PathBuf::from(rom_path));
    if !path.exists() {
        return Err(format!(
            "No autorun file found for ROM {}: {}",
            rom_path,
            path.display()
        ));
    }

    convert_autorun_file(&path, format, None)?;
    Ok(format!(
        "Converted autorun file to {} format (version {}): {}",
        format,
        AUTORUN_VERSION,
        path.display()
    ))
}

fn trim_autorun_checkpoints_for_rom(
    rom_path: &str,
    checkpoints_to_trim: usize,
    format: AutorunFormat,
) -> Result<String, String> {
    use platform::autorun::{
        autorun_path_for_rom, load_autorun_file, save_autorun_file, trim_recording,
    };
    use std::path::PathBuf;

    let path = autorun_path_for_rom(&PathBuf::from(rom_path));
    let mut file = load_autorun_file(&path, None)?;
    let checkpoints_before = file.checkpoints.len();
    trim_recording(&mut file, checkpoints_to_trim);
    save_autorun_file(&path, &file, format, None)?;

    Ok(format!(
        "Trimmed {} checkpoint(s): {} → {} checkpoints, {} frames remaining",
        checkpoints_before.saturating_sub(file.checkpoints.len()),
        checkpoints_before,
        file.checkpoints.len(),
        file.frames.len(),
    ))
}

fn recalculate_autorun_for_rom(rom_path: &str, format: AutorunFormat) -> Result<String, String> {
    use nes::autorun::headless_playback::recalculate_checkpoint_crcs_with_progress;
    use nes::cartridge::Cartridge;
    use nes::console::RamInitMode;
    use platform::autorun::{autorun_path_for_rom, load_autorun_file, save_autorun_file};
    use platform::config::FrontendConfig;
    use std::io::{self, Write};

    let path = autorun_path_for_rom(&PathBuf::from(rom_path));
    if !path.exists() {
        return Err(format!(
            "No autorun file found for ROM {}: {}",
            rom_path,
            path.display()
        ));
    }

    let mut file = load_autorun_file(&path, None)?;
    let rom_bytes =
        fs::read(rom_path).map_err(|e| format!("Failed to read ROM {}: {e}", rom_path))?;

    let config = Config {
        frontend: FrontendConfig {
            ram_init_mode: RamInitMode::Zero,
            ..Default::default()
        },
        ..Default::default()
    };
    let app_context = AppContext::new_with_config(config);

    let mut nes = Nes::new(app_context);
    let cart = Cartridge::load_from_file(&rom_bytes, rom_path, Some(nes.rom_db()))
        .map_err(|e| format!("Failed to load cartridge {}: {e}", rom_path))?;
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
    save_autorun_file(&path, &file, format, None)?;

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
        ParseResult::Version => {
            println!("neser {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        ParseResult::Config(c) => *c,
    };

    let app_context = Rc::new(RefCell::new(AppContext::new_with_config(parsed_config)));

    // Handle --tui: launch the interactive TUI ROM browser and exit.
    // Must be checked before refresh_startup_cartridge_catalog so the catalog
    // is not scanned twice (run_tui does its own scan).
    #[cfg(feature = "tui")]
    if app_context.borrow().config().frontend.tui_mode {
        let (search_paths, _, rebuild) = cartridge_catalog_startup_config(&app_context);
        let include_unofficial = app_context
            .borrow()
            .config()
            .frontend
            .include_unofficial_roms;
        return frontends::tui::run_tui(&search_paths, rebuild, include_unofficial);
    }

    refresh_startup_cartridge_catalog(&app_context);

    // Handle --trim-checkpoints: modify recording file and exit immediately.
    let trim_checkpoints = app_context
        .borrow()
        .config()
        .frontend
        .autorun_trim_checkpoints;
    let trim_rom_path = app_context.borrow().config().frontend.rom_path.clone();
    let trim_format = app_context.borrow().config().frontend.autorun_format;
    if let (Some(checkpoints_to_trim), Some(rom_path)) =
        (trim_checkpoints, trim_rom_path.as_deref())
    {
        let message = trim_autorun_checkpoints_for_rom(rom_path, checkpoints_to_trim, trim_format)?;
        println!("{message}");
        return Ok(());
    }

    // Handle --convert-autorun: convert recording file format and exit immediately.
    let convert_autorun_requested = app_context.borrow().config().frontend.autorun_convert;
    let convert_rom_path = app_context.borrow().config().frontend.rom_path.clone();
    let convert_format = app_context.borrow().config().frontend.autorun_format;
    if convert_autorun_requested {
        let rom_path =
            convert_rom_path.ok_or_else(|| "--convert-autorun requires a ROM path".to_string())?;
        let message = convert_autorun_for_rom(&rom_path, convert_format)?;
        println!("{message}");
        return Ok(());
    }

    // Handle --recalculate-autorun: replay and rewrite checkpoint CRCs, then exit.
    let recalculate_autorun_requested = app_context.borrow().config().frontend.autorun_recalculate;
    let recalculate_rom_path = app_context.borrow().config().frontend.rom_path.clone();
    let recalculate_format = app_context.borrow().config().frontend.autorun_format;
    if recalculate_autorun_requested {
        let rom_path = recalculate_rom_path
            .ok_or_else(|| "--recalculate-autorun requires a ROM path".to_string())?;
        let message = recalculate_autorun_for_rom(&rom_path, recalculate_format)?;
        println!("{message}");
        return Ok(());
    }

    // Initialize global tracing state (only active in debug builds)
    let tracing_config = app_context.borrow().config().frontend.tracing;
    platform::debugging::init_tracing(tracing_config);

    // Handle --headless: capture a frame to PNG and exit.
    //
    // Deliberately above both `native` cfg blocks below, not inside them.
    // Capturing needs no window, so a build with `frontend` but without
    // `native` (so without winit/glutin/egui/cpal) still captures, while the
    // same binary run without --headless exits with "No frontend feature
    // enabled". Moving this into the native cfg would silently remove that.
    if platform::headless_capture::run_if_requested(&app_context)? {
        return Ok(());
    }

    #[cfg(feature = "native")]
    {
        run_native_frontend(app_context)?;
    }

    #[cfg(not(feature = "native"))]
    {
        eprintln!("No frontend feature enabled. Enable the 'native' feature.");
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(feature = "native")]
fn run_native_frontend(
    app_context: Rc<RefCell<AppContext>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use frontends::native::rom_browser::{BrowserResult, RomBrowserApp};

    let rom_path = app_context.borrow().config().frontend.rom_path.clone();

    if let Some(rom_path) = rom_path {
        // ROM path provided via CLI — go straight to emulation (no return to browser).
        run_native_emulator(app_context, &rom_path, None)
    } else {
        // No ROM path — launch the ROM browser in a loop.
        // After emulation ends, return to the browser for another selection.
        let mut event_loop = frontends::native::create_event_loop()?;
        let mut browser = RomBrowserApp::new(app_context.clone());
        loop {
            match browser.run(&mut event_loop)? {
                BrowserResult::RomSelected(path) => {
                    let rom_path = path.to_string_lossy().to_string();
                    // Run the emulator; when it exits, loop back to the browser.
                    if let Err(e) =
                        run_native_emulator(app_context.clone(), &rom_path, Some(&mut event_loop))
                    {
                        crate::platform::debugging::log_info(format!("Emulator error: {e}"));
                    }
                }
                BrowserResult::Closed => return Ok(()),
            }
        }
    }
}

/// Load and run a ROM in the native emulator event loop.
#[cfg(feature = "native")]
fn run_native_emulator(
    app_context: Rc<RefCell<AppContext>>,
    rom_path: &str,
    event_loop: Option<&mut winit::event_loop::EventLoop<()>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use frontends::native::{NativeAudio, NativeEventLoop};
    use platform::audio::EmulatorAudio;

    // Read autorun config up front
    let (
        autorun_mode,
        autorun_headless,
        autorun_overwrite,
        autorun_extend,
        autorun_from_checkpoint,
        autorun_format,
    ) = {
        let config = app_context.borrow();
        let config = config.config();
        (
            config.frontend.autorun_mode,
            config.frontend.autorun_headless,
            config.frontend.autorun_overwrite,
            config.frontend.autorun_extend,
            config.frontend.autorun_from_checkpoint,
            config.frontend.autorun_format,
        )
    };

    // Headless autorun is only supported in playback mode because
    // record/extend have no guaranteed termination condition.
    let headless = autorun_headless && autorun_mode == platform::autorun::AutorunMode::Playback;

    // Create audio output unless disabled or headless.
    let mut audio_sample_rate = None;
    let (audio_enabled, audio_buffer_ms, configured_sample_rate) = {
        let config = app_context.borrow();
        let frontend = &config.config().frontend;
        (
            frontend.audio_enabled,
            frontend.audio_buffer_ms,
            frontend.audio_sample_rate,
        )
    };
    let audio = if !audio_enabled || headless {
        None
    } else {
        let audio = NativeAudio::new(configured_sample_rate as i32, audio_buffer_ms)?;
        audio_sample_rate = Some(audio.actual_sample_rate() as f32);
        Some(audio)
    };

    let mut console = platform::rom_loader::load_console(&app_context, rom_path)?;

    if let Some(actual_rate) = audio_sample_rate {
        console.set_audio_sample_rate(actual_rate);
    }

    console.reset(false);

    let tracing = app_context.borrow().config().frontend.tracing;
    let mut native_loop =
        NativeEventLoop::new(app_context.clone(), console, audio, tracing, headless);

    // Initialize autorun AFTER reset so checkpoint state restore is not overwritten.
    if autorun_mode != platform::autorun::AutorunMode::None {
        native_loop.init_autorun(
            autorun_mode,
            rom_path,
            autorun_overwrite,
            autorun_extend,
            autorun_from_checkpoint,
            autorun_format,
        )?;
    }

    let run_result = if let Some(el) = event_loop {
        native_loop.run_with_event_loop(el)
    } else {
        native_loop.run()
    };

    // Handle autorun exit codes
    if let Err(ref e) = run_result
        && let Some(exit_code) = e
            .strip_prefix("AUTORUN_EXIT:")
            .and_then(|s| s.parse::<i32>().ok())
    {
        std::process::exit(exit_code);
    }

    run_result.map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::autorun::AUTORUN_VERSION;
    use tempfile::TempDir;

    #[test]
    fn test_convert_autorun_for_rom_fails_when_autorun_file_missing() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let rom_path = temp_dir.path().join("missing.nes");

        let result = convert_autorun_for_rom(
            rom_path.to_str().expect("rom path to str"),
            AutorunFormat::default(),
        );

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

        convert_autorun_for_rom(
            rom_path.to_str().expect("rom path to str"),
            AutorunFormat::Json,
        )
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

        let result = recalculate_autorun_for_rom(
            rom_path.to_str().expect("rom path to str"),
            AutorunFormat::default(),
        );

        assert!(
            result.is_err(),
            "recalculation should fail when corresponding .autorun file is missing"
        );
    }
}
