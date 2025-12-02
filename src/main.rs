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
    // snake::run()
    let mut event_loop = eventloop::EventLoop::new(false, nes::TvSystem::Ntsc, 2.0, 0.01)?;
    let mut nes_instance = nes::Nes::new(nes::TvSystem::Ntsc);

    // Load the snake.nes cartridge
    let rom_data = std::fs::read("roms/games/pac-man.nes")?;
    let cart = cartridge::Cartridge::new(&rom_data)?;
    nes_instance.insert_cartridge(cart);
    nes_instance.reset();

    event_loop.run(&mut nes_instance).map_err(|e| e.into())
}
