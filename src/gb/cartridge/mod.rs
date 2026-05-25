#[allow(clippy::module_inception)]
mod cartridge;
mod huc1;
mod mbc0;
mod mbc1;
mod mbc2;
mod mbc3;
mod mbc5;
mod mbc7;

#[allow(unused_imports)]
pub use cartridge::{GbCartridge, RomError, has_canonical_nintendo_logo, load_cartridge};
