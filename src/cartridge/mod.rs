mod axrom;
mod bandai_fcg;
mod bnrom_nina;
mod camerica;
#[allow(clippy::module_inception)]
mod cartridge;
mod cnrom;
mod colordreams;
mod common;
mod cprom;
mod gxrom;
mod ines;
mod mapper;
mod mapper_templates;
mod mmc1;
mod mmc2;
mod mmc3;
mod mmc4;
mod mmc5;
mod multicart_15;
mod namco118;
mod namco163;
mod nina_tengen;
mod nrom;
mod rom_db;
mod sunsoft_4;
mod sunsoft_fme7;
#[cfg(test)]
pub mod test_helpers;
mod uxrom;
mod vrc2_vrc4;
mod vrc6;

pub use cartridge::{Cartridge, MirroringMode, RomTvSystem};
#[allow(unused_imports)]
pub use common::{
    BankSwitch, BankedRom, ChrMemory, DEFAULT_CHR_RAM_SIZE, DEFAULT_PRG_RAM_SIZE, PrgRam,
    StateSnapshot,
};
#[allow(unused_imports)]
pub use ines::{ConsoleType, InesHeader, Mirroring, TimingMode, parse_header, parse_rom};
#[allow(unused_imports)]
pub use mapper::{Mapper, MapperCapabilities, MapperContext};
#[allow(unused_imports)]
pub use mapper_templates::{DualBank32Mapper, SimpleBankedPrgMapper, SimpleFixedPrgMapper};
#[allow(unused_imports)]
pub use rom_db::calculate_rom_crc32;
pub(crate) use rom_db::default_arkanoid_on_port;
pub(crate) use rom_db::default_zapper_on_port;
