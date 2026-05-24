//! GBA cartridge loading and save-type detection.
//!
//! This module implements:
//!
//! * ROM header parsing ([`header`])
//! * Save-type heuristic detection ([`save_type`])
//! * Save backends: [`sram`], [`eeprom`], [`flash`]
//! * The top-level [`GbaCartridge`] aggregate ([`cartridge`])
//!
//! See `architecture.md` and the GBATek "Cartridges" reference for context:
//! <https://problemkaputt.de/gbatek.htm#gbacartridges>.

#[allow(clippy::module_inception)]
pub mod cartridge;
pub mod eeprom;
pub mod flash;
pub mod header;
pub mod save_type;
pub mod sram;

#[allow(unused_imports)]
pub use cartridge::{
    CartridgeError, GbaCartridge, ROM_MAX_SIZE, SaveBackend, SaveBackendState, load_cartridge,
};
#[allow(unused_imports)]
pub use eeprom::Eeprom;
#[allow(unused_imports)]
pub use flash::Flash;
#[allow(unused_imports)]
pub use header::{GbaHeader, HeaderError, parse_header};
#[allow(unused_imports)]
pub use save_type::{SaveType, detect_save_type};
#[allow(unused_imports)]
pub use sram::Sram;
