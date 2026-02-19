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
use console::{ApuChannels, Config, Nes, ParseResult, SaveState, log_rom_timing_mode_selection};
use debugging::log_info;
use frontend_toasts::{cartridge_load_toast_message, emulator_timing_toast_message};
use sdl_frontend::{SdlEventLoop, SdlNesAudio};
use std::fs;

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

    let app_context = AppContext::new_with_config(parsed_config);

    // Initialize global tracing state (only active in debug builds)
    let tracing_config = {
        let config = app_context.config();
        config.borrow().tracing
    };
    debugging::init_tracing(tracing_config);

    // Initialize SDL2
    let sdl_context = sdl2::init()?;

    // Create audio output (request 44.1 kHz) unless disabled.
    // SDL may open the device at a different rate; always sync the APU to the actual rate
    // to avoid steady underruns.
    let mut audio_sample_rate = None;
    let audio_enabled = {
        let config = app_context.config();
        config.borrow().audio_enabled
    };
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
    let default_rom_path = "roms/automated_tests/nmi_sync/demo_pal.nes";
    // let default_rom_path = "roms/manual_tests/PaddleTest3/PaddleTest.nes";

    // let rom_data = manual_test_cartridges::triangle_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse1_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse2_only_nrom_128();
    // let rom_data = manual_test_cartridges::noise_only_nrom_128();

    let rom_path = {
        let config = app_context.config();
        config
            .borrow()
            .rom_path
            .clone()
            .unwrap_or_else(|| default_rom_path.to_string())
    };
    let rom_bytes = match fs::read(&rom_path) {
        Ok(bytes) => bytes,
        Err(err) => {
            app_context.add_toast(cartridge_load_toast_message(&rom_path, false));
            return Err(err.into());
        }
    };
    let cart = match cartridge::Cartridge::load_from_file(&rom_bytes, &rom_path, &app_context) {
        Ok(cartridge) => {
            app_context.add_toast(cartridge_load_toast_message(&rom_path, true));
            cartridge
        }
        Err(err) => {
            app_context.add_toast(cartridge_load_toast_message(&rom_path, false));
            return Err(err.into());
        }
    };

    let rom_timing_mode = cart.rom_timing_mode();
    let applied = {
        let config = app_context.config();
        config.borrow_mut().apply_rom_timing_mode(rom_timing_mode)
    };
    log_rom_timing_mode_selection(&app_context, rom_timing_mode, applied);

    let mut nes_instance = Nes::new(app_context.clone());
    nes_instance.insert_cartridge(cart);
    let tv_system = {
        let config = app_context.config();
        config.borrow().tv_system
    };
    app_context.add_toast(emulator_timing_toast_message(tv_system));

    if let Some(actual_rate) = audio_sample_rate {
        nes_instance.apu.borrow_mut().set_sample_rate(actual_rate);
    }

    // Create event loop with headless mode if autorun playback is headless
    let headless = {
        let config = app_context.config();
        config.borrow().autorun_headless
    };
    // In headless autorun/playback, force audio to None so no audio device is required
    let audio_for_frontend = if headless { None } else { audio };
    let mut event_loop =
        SdlEventLoop::new_with_context(headless, audio_for_frontend, app_context.clone())?;

    // Initialize autorun if enabled
    let (autorun_mode, autorun_overwrite, autorun_extend) = {
        let config = app_context.config();
        let config = config.borrow();
        (
            config.autorun_mode,
            config.autorun_overwrite,
            config.autorun_extend,
        )
    };
    if autorun_mode != console::AutorunMode::None {
        event_loop.init_autorun(autorun_mode, &rom_path, autorun_overwrite, autorun_extend)?;
    }

    // Request debugger open if enabled via CLI
    let debugger_enabled = {
        let config = app_context.config();
        config.borrow().debugger_enabled
    };
    if debugger_enabled {
        event_loop.request_debugger_open();
    }

    // Temporary hard-coded breakpoint for debugger development.
    // event_loop.add_breakpoint(0xE486);

    let load_state = {
        let config = app_context.config();
        config.borrow().load_state
    };
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

    // Apply channel enable/disable settings
    {
        let mut apu = nes_instance.apu.borrow_mut();
        let config = app_context.config();
        let config = config.borrow();
        apu.set_pulse1_enabled(config.apu_channels.contains(ApuChannels::PULSE1));
        apu.set_pulse2_enabled(config.apu_channels.contains(ApuChannels::PULSE2));
        apu.set_triangle_enabled(config.apu_channels.contains(ApuChannels::TRIANGLE));
        apu.set_noise_enabled(config.apu_channels.contains(ApuChannels::NOISE));
        apu.set_dmc_enabled(config.apu_channels.contains(ApuChannels::DMC));
    }

    let run_tracing = {
        let config = app_context.config();
        config.borrow().tracing
    };
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
        && let Err(e) = nes_instance.bus.borrow().save_ram()
    {
        log_info(format!("Warning: failed to save RAM: {}", e));
    }

    run_result.map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

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
}
