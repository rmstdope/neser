mod apu;
mod audio;
mod cartridge;
mod config;
mod cpu;
mod debugger;
mod eventloop;
mod input;
mod rendering;
// #[path = "game_verification/manual_test_cartridges.rs"]
// mod manual_test_cartridges;

mod mem_controller;
mod nes;
mod ppu;
mod screen_buffer;
mod tracing;

use config::{Config, ParseResult};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();

    let config = match Config::new(&args)? {
        ParseResult::Help => {
            Config::print_help();
            return Ok(());
        }
        ParseResult::Config(c) => c,
    };

    // Initialize SDL2
    let sdl_context = sdl2::init()?;
    let mut nes_instance = nes::Nes::new(config.tv_system);

    // Create audio output (request 44.1 kHz) unless disabled.
    // SDL may open the device at a different rate; always sync the APU to the actual rate
    // to avoid steady underruns.
    let audio = if !config.audio_enabled {
        None
    } else {
        let audio = audio::NesAudio::new(&sdl_context, 44100)?;
        let actual_rate = audio.actual_sample_rate() as f32;
        nes_instance.apu.borrow_mut().set_sample_rate(actual_rate);
        Some(audio)
    };

    let mut event_loop = eventloop::EventLoop::new(false, audio, &config)?;

    // Request debugger open if enabled via CLI
    if config.debugger_enabled {
        event_loop.request_debugger_open();
    }

    // Temporary hard-coded breakpoint for debugger development.
    // event_loop.add_breakpoint(0xE486);

    // Palette display requiring only scanline-based palette changes,
    // intended to demonstrate the full palette even on less advanced emulators
    // Seems to work ok!
    // let rom_data = std::fs::read("roms/rainwarrior/palette.nes")?;

    // Simple display of any chosen color full-screen
    // Seems to work ok!
    // let rom_data = std::fs::read("roms/rainwarrior/color_test.nes")?;

    // Load game cartridge
    // let rom_data = std::fs::read("roms/games/pac-man.nes")?;
    // let rom_data = std::fs::read("roms/games/Balloon_fight.nes")?;
    // let rom_path = "roms/games/donkey kong.nes";
    let rom_path = "roms/games/Legend of Zelda, The (USA) (Rev 1).nes";
    // let rom_path = "roms/games/Mike Tyson's Punch-Out!! (Japan, USA) (Rev 1).nes";

    // Manual testing of Blargg
    // let rom_path = "roms/nestest.nes";

    // let rom_data = manual_test_cartridges::triangle_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse1_only_nrom_128();
    // let rom_data = manual_test_cartridges::pulse2_only_nrom_128();
    // let rom_data = manual_test_cartridges::noise_only_nrom_128();

    let cart = cartridge::Cartridge::load_from_file(rom_path)?;
    nes_instance.insert_cartridge(cart);

    nes_instance.reset(false);

    // Apply channel enable/disable settings
    {
        let mut apu = nes_instance.apu.borrow_mut();
        apu.set_pulse1_enabled(config.pulse1_enabled);
        apu.set_pulse2_enabled(config.pulse2_enabled);
        apu.set_triangle_enabled(config.triangle_enabled);
        apu.set_noise_enabled(config.noise_enabled);
        apu.set_dmc_enabled(config.dmc_enabled);
    }

    let run_result = event_loop.run(&mut nes_instance, config.tracing);
    // Best-effort save on clean shutdown (Escape/Quit).
    if run_result.is_ok() {
        if let Err(e) = nes_instance.memory.borrow().save_ram() {
            eprintln!("Warning: failed to save RAM: {}", e);
        }
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
        let args = vec!["neser".to_string(), "--start-in-debugger".to_string()];

        let config = match Config::new(&args).unwrap() {
            ParseResult::Config(c) => c,
            ParseResult::Help => panic!("Expected Config"),
        };

        let mut event_loop = crate::eventloop::EventLoop::new(true, None, &config).unwrap();

        if config.debugger_enabled {
            event_loop.request_debugger_open();
        }

        assert!(event_loop.is_paused());
        assert!(event_loop.debugger_open_requested());
    }
}
