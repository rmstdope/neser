mod apu;
mod audio;
mod cartridge;
mod cpu;
mod eventloop;
mod input;
mod manual_test_cartridges;
mod mem_controller;
mod nes;
mod ppu;
mod screen_buffer;
mod tracing;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();

    // Show help if requested
    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        println!("NES Emulator");
        println!("\nUsage: neser [OPTIONS]");
        println!("\nOptions:");
        println!("  --pal                 Use PAL TV system (default: NTSC)");
        println!("  --no-audio            Disable audio output");
        println!("  --trace               Enable CPU trace output");
        println!("  --trace-nestest        Enable CPU trace output (nestest.log format)");
        println!("  --trace-ppu            Enable PPU trace output");
        println!("  --trace-apu            Enable APU trace output");
        println!("\nAPU Channel Control (for debugging):");
        println!("  --disable-pulse1      Mute pulse 1 channel");
        println!("  --disable-pulse2      Mute pulse 2 channel");
        println!("  --disable-triangle    Mute triangle channel");
        println!("  --disable-noise       Mute noise channel");
        println!("  --disable-dmc         Mute DMC channel");
        println!("\nExample:");
        println!("  neser --disable-pulse2 --disable-triangle    # Only pulse1, noise, and DMC");
        return Ok(());
    }

    let tv_system = if args.contains(&"--pal".to_string()) {
        nes::TvSystem::Pal
    } else {
        nes::TvSystem::Ntsc
    };
    let no_audio = args.contains(&"--no-audio".to_string());
    let tracing = tracing::Tracing::from_args(&args);

    // Channel enable/disable flags (default: all enabled)
    let pulse1_enabled = !args.contains(&"--disable-pulse1".to_string());
    let pulse2_enabled = !args.contains(&"--disable-pulse2".to_string());
    let triangle_enabled = !args.contains(&"--disable-triangle".to_string());
    let noise_enabled = !args.contains(&"--disable-noise".to_string());
    let dmc_enabled = !args.contains(&"--disable-dmc".to_string());

    // Initialize SDL2
    let sdl_context = sdl2::init()?;
    let mut nes_instance = nes::Nes::new(tv_system);

    // Create audio output (request 44.1 kHz) unless disabled.
    // SDL may open the device at a different rate; always sync the APU to the actual rate
    // to avoid steady underruns.
    let audio = if no_audio {
        None
    } else {
        let audio = audio::NesAudio::new(&sdl_context, 44100)?;
        let actual_rate = audio.actual_sample_rate() as f32;
        nes_instance.apu.borrow_mut().set_sample_rate(actual_rate);
        Some(audio)
    };

    let mut event_loop = eventloop::EventLoop::new(false, tv_system, 4.0, 1.0, audio)?;

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
    // let rom_data = std::fs::read("roms/games/donkey kong.nes")?;
    let rom_data = std::fs::read("roms/games/zelda.nes")?;

    // Manual testing of Blargg
    // let rom_data = std::fs::read("roms/blargg/dmc_tests/status.nes")?;

    // let rom_data = manual_test_cartridges::triangle_only_nrom_128();

    let cart = cartridge::Cartridge::new(&rom_data)?;
    nes_instance.insert_cartridge(cart);
    nes_instance.reset(false);

    // Apply channel enable/disable settings
    {
        let mut apu = nes_instance.apu.borrow_mut();
        apu.set_pulse1_enabled(pulse1_enabled);
        apu.set_pulse2_enabled(pulse2_enabled);
        apu.set_triangle_enabled(triangle_enabled);
        apu.set_noise_enabled(noise_enabled);
        apu.set_dmc_enabled(dmc_enabled);
    }

    event_loop
        .run(&mut nes_instance, tracing)
        .map_err(|e| e.into())
}
