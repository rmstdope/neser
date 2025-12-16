mod apu;
mod audio;
mod cartridge;
mod cpu;
mod eventloop;
mod input;
mod mem_controller;
mod nes;
mod ppu;
mod screen_buffer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();

    // Show help if requested
    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        println!("NES Emulator");
        println!("\nUsage: neser [OPTIONS]");
        println!("\nOptions:");
        println!("  -pal                  Use PAL TV system (default: NTSC)");
        println!("  --no-audio            Disable audio output");
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

    let tv_system = if args.contains(&"-pal".to_string()) {
        nes::TvSystem::Pal
    } else {
        nes::TvSystem::Ntsc
    };
    let no_audio = args.contains(&"--no-audio".to_string());

    // Channel enable/disable flags (default: all enabled)
    let pulse1_enabled = !args.contains(&"--disable-pulse1".to_string());
    let pulse2_enabled = !args.contains(&"--disable-pulse2".to_string());
    let triangle_enabled = !args.contains(&"--disable-triangle".to_string());
    let noise_enabled = !args.contains(&"--disable-noise".to_string());
    let dmc_enabled = !args.contains(&"--disable-dmc".to_string());

    // Initialize SDL2
    let sdl_context = sdl2::init()?;

    // Create audio output (44.1 kHz) unless disabled
    let audio = if no_audio {
        None
    } else {
        Some(audio::NesAudio::new(&sdl_context, 44100)?)
    };

    let mut event_loop = eventloop::EventLoop::new(false, tv_system, 4.0, 1.0, audio)?;
    let mut nes_instance = nes::Nes::new(tv_system);

    // Palette display requiring only scanline-based palette changes,
    // intended to demonstrate the full palette even on less advanced emulators
    // Seems to work ok!
    // let rom_data = std::fs::read("roms/rainwarrior/palette.nes")?;

    // Simple display of any chosen color full-screen
    // Seems to work ok!
    // let rom_data = std::fs::read("roms/rainwarrior/color_test.nes")?;

    // NTSC Torture Test displays visual patterns to demonstrate NTSC signal artifacts
    // Shows a but for x < 8 pixels on each scanline where the leftmost 8 pixels are incorrect
    // but otherwise seems to work well
    // let rom_data = std::fs::read("roms/rainwarrior/ntsc_torture.nes")?;

    // Verifies NMI timing by creating a specific pattern on the screen (NTSC & PAL versions)
    // Not working ok right now. NMI sync seems off.
    // let rom_data = std::fs::read("roms/nmi_sync/demo_ntsc.nes")?;

    // let rom_data = std::fs::read("roms/cpu_interrupts.nes")?;

    // Load game cartridge
    // let rom_data = std::fs::read("roms/games/pac-man.nes")?;
    // let rom_data = std::fs::read("roms/games/Balloon_fight.nes")?;
    // let rom_data = std::fs::read("roms/games/donkey kong.nes")?;
    // let rom_data = std::fs::read("roms/games/zelda.nes")?;

    // Unknown status
    // let rom_data = std::fs::read("roms/full_nes_palette.nes")?;
    // let rom_data = std::fs::read("roms/nmi_sync/demo_ntsc.nes")?;
    // let rom_data = std::fs::read("roms/blargg/4015_cleared.nes")?;
    let rom_data = std::fs::read("roms/blargg/ppu_open_bus/ppu_open_bus.nes")?;
    let cart = cartridge::Cartridge::new(&rom_data)?;
    nes_instance.insert_cartridge(cart);
    nes_instance.reset();

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
        .run(&mut nes_instance, false)
        .map_err(|e| e.into())
}
