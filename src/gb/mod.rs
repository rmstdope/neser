pub mod apu;
pub mod autorun;
pub mod boot_rom;
pub mod bus;
pub mod cartridge;
pub mod console;
pub mod cpu;
pub mod debugging;
pub mod input;
pub mod model;
pub mod ppu;
pub mod timer;

pub use console::gameboy::GameBoy;
// Re-export DmgModel so downstream library consumers can use `neser::gb::DmgModel`.
#[allow(unused_imports)]
pub use model::DmgModel;

#[cfg(test)]
pub mod integration_tests;
