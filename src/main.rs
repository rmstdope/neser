mod apu;
mod audio;
mod cartridge;
mod cpu;
mod eventloop;
mod joypad;
mod mapper;
mod mem_controller;
mod nes;
mod opcode;
mod ppu_modules;
mod screen_buffer;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments
    let args: Vec<String> = std::env::args().collect();
    let tv_system = if args.contains(&"-pal".to_string()) {
        nes::TvSystem::Pal
    } else {
        nes::TvSystem::Ntsc
    };
    let no_audio = args.contains(&"--no-audio".to_string());

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

    // OADM Read test - PASS
    // let rom_data = std::fs::read("roms/oam_read.nes")?;

    // OAM Stress test - PASS
    // let rom_data = std::fs::read("roms/oam_stress.nes")?;

    // let rom_data = std::fs::read("roms/cpu_interrupts.nes")?;

    // Palette test - shows timing issues due to PPU timing limitations
    // let rom_data = std::fs::read("roms/palette.nes")?;

    // Color test - Run when input is implemented
    // let rom_data = std::fs::read("roms/color_test.nes")?;

    // NTSC Torture test - Run when input is implemented
    // let rom_data = std::fs::read("roms/ntsc_torture.nes")?;

    // Load game cartridge
    let rom_data = std::fs::read("roms/games/pac-man.nes")?;
    // let rom_data = std::fs::read("roms/games/Balloon_fight.nes")?;
    // let rom_data = std::fs::read("roms/games/donkey kong.nes")?;

    // Unknown status
    // let rom_data = std::fs::read("roms/full_nes_palette.nes")?;
    // let rom_data = std::fs::read("roms/nmi_sync/demo_ntsc.nes")?;
    let cart = cartridge::Cartridge::new(&rom_data)?;
    nes_instance.insert_cartridge(cart);
    nes_instance.reset();

    event_loop
        .run(&mut nes_instance, false)
        .map_err(|e| e.into())
}
