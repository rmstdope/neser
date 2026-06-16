//! SNES cartridge and ROM loading.

mod header;
mod mapping;

#[allow(clippy::module_inception)]
pub mod cartridge;

#[allow(unused_imports)]
pub use cartridge::{Cartridge, CartridgeError, RomSpeed};
#[allow(unused_imports)]
pub use mapping::Mapping;
