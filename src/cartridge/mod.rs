// Manufacturer sub-modules
mod bandai;
mod camerica;
mod irem;
mod jaleco;
mod konami;
mod namco;
mod nintendo;
mod sachen;
mod sunsoft;
mod taito;
mod tengen;
mod unlicensed;

// Infrastructure modules (shared across all manufacturers)
mod base_mapper;
#[allow(clippy::module_inception)]
mod cartridge;
mod common;
mod cpu_cycle_irq;
mod hardware_type;
mod ines;
mod mapper;
mod mapper_templates;
mod rom_db;
#[cfg(test)]
pub mod test_helpers;

// Re-export sub-module internals used by cross-module references
// so that `crate::cartridge::mmc3::MMC3Mapper` etc. still resolve.
pub(crate) use konami::vrc_irq;
pub(crate) use konami::vrc2_vrc4;
pub(crate) use nintendo::mmc1;
pub(crate) use nintendo::mmc2_mmc4_latch;
pub(crate) use nintendo::mmc3;
pub(crate) use sachen::mapper243;
pub(crate) use unlicensed::mapper45;

// Re-exports used only in test code
#[cfg(test)]
pub(crate) use namco::namco118;
#[cfg(test)]
pub(crate) use namco::namco163;
#[cfg(test)]
pub(crate) use nintendo::nrom;
#[cfg(test)]
pub(crate) use taito::taito_tc0190;

#[allow(unused_imports)]
pub use base_mapper::BaseMapper;
pub use cartridge::Cartridge;
#[cfg(test)]
#[allow(unused_imports)]
pub use common::{BankSwitch, BankedRom};
#[allow(unused_imports)]
pub use common::{ChrMemory, DEFAULT_CHR_RAM_SIZE, DEFAULT_PRG_RAM_SIZE, PrgRam, StateSnapshot};
#[allow(unused_imports)]
pub use hardware_type::HardwareType;
#[allow(unused_imports)]
pub use ines::{ConsoleType, InesHeader, NametableLayout, ParsedRom, RomParseError, TimingMode};
#[allow(unused_imports)]
pub use mapper::{Mapper, MapperCapabilities, MapperContext};
#[allow(unused_imports)]
pub use mapper_templates::{DualBank32Mapper, SimpleBankedPrgMapper, SimpleFixedPrgMapper};
#[allow(unused_imports)]
pub use rom_db::calculate_rom_crc32;
pub(crate) use rom_db::default_arkanoid_on_port;
#[allow(unused_imports)]
pub use rom_db::{RomDb, RomDbEntry};
