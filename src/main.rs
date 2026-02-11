mod apu;
mod bus;
mod cartridge;
mod console;
mod cpu;
mod debugging;
mod input;
mod ppu;
mod rendering;
mod sdl_frontend;

use console::{ApuChannels, Config, Nes, ParseResult, SaveState, log_rom_tv_system_selection};
use debugging::log_info;
use sdl_frontend::{SdlEventLoop, SdlNesAudio};
use std::fs;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();

    let mut config = match Config::new(&args)? {
        ParseResult::Help => {
            Config::print_help();
            return Ok(());
        }
        ParseResult::Config(c) => c,
    };

    // Initialize global tracing state (only active in debug builds)
    debugging::init_tracing(config.tracing);

    // Initialize SDL2
    let sdl_context = sdl2::init()?;

    // Create audio output (request 44.1 kHz) unless disabled.
    // SDL may open the device at a different rate; always sync the APU to the actual rate
    // to avoid steady underruns.
    let mut audio_sample_rate = None;
    let audio = if !config.audio_enabled {
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
    let default_rom_path = "roms/games/collection/World/Duck Hunt (JUE).nes";
    // let default_rom_path = "roms/manual_tests/PaddleTest3/PaddleTest.nes";

    // let rom_data = manual_test_cartridges::triangle_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse1_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse2_only_nrom_128();
    // let rom_data = manual_test_cartridges::noise_only_nrom_128();

    let rom_path = config.rom_path.as_deref().unwrap_or(default_rom_path);
    let cart = cartridge::Cartridge::load_from_file(rom_path)?;

    let rom_tv_system = cart.rom_tv_system();
    let applied = config.apply_rom_tv_system(rom_tv_system);
    log_rom_tv_system_selection(&config, rom_tv_system, applied);

    let mut nes_instance = Nes::new(config.clone());
    nes_instance.insert_cartridge(cart);

    if let Some(actual_rate) = audio_sample_rate {
        nes_instance.apu.borrow_mut().set_sample_rate(actual_rate);
    }

    let mut event_loop = SdlEventLoop::new(false, audio, &config)?;

    // Request debugger open if enabled via CLI
    if config.debugger_enabled {
        event_loop.request_debugger_open();
    }

    // Temporary hard-coded breakpoint for debugger development.
    // event_loop.add_breakpoint(0xE486);

    if config.load_state {
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
        apu.set_pulse1_enabled(config.apu_channels.contains(ApuChannels::PULSE1));
        apu.set_pulse2_enabled(config.apu_channels.contains(ApuChannels::PULSE2));
        apu.set_triangle_enabled(config.apu_channels.contains(ApuChannels::TRIANGLE));
        apu.set_noise_enabled(config.apu_channels.contains(ApuChannels::NOISE));
        apu.set_dmc_enabled(config.apu_channels.contains(ApuChannels::DMC));
    }

    let run_result = event_loop.run(&mut nes_instance, config.tracing);
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

        let mut event_loop = SdlEventLoop::new(true, None, &config).unwrap();

        if config.debugger_enabled {
            event_loop.request_debugger_open();
        }

        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }
}
