#[allow(clippy::module_inception)]
mod cartridge;
mod mbc0;
mod mbc1;
mod mbc2;
mod mbc3;
mod mbc5;

pub use cartridge::{GbCartridge, RomError, load_cartridge};
