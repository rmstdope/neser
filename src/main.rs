mod cartridge;
mod cpu;
mod eventloop;
mod mem_controller;
mod nes;
mod opcode;
mod ppu;
mod screen_buffer;
mod snake;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Check for -pal flag
    let args: Vec<String> = std::env::args().collect();
    let tv_system = if args.contains(&"-pal".to_string()) {
        nes::TvSystem::Pal
    } else {
        nes::TvSystem::Ntsc
    };

    // snake::run()
    let mut event_loop = eventloop::EventLoop::new(false, tv_system, 2.0, 0.01)?;
    let mut nes_instance = nes::Nes::new(tv_system);

    // Load the snake.nes cartridge
    // let rom_data = std::fs::read("roms/games/pac-man.nes")?;
    // let rom_data = std::fs::read("roms/color_test.nes")?;
    let rom_data = std::fs::read("roms/full_nes_palette.nes")?;
    let cart = cartridge::Cartridge::new(&rom_data)?;
    nes_instance.insert_cartridge(cart);
    nes_instance.reset();

    event_loop.run(&mut nes_instance).map_err(|e| e.into())
}
