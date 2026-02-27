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
mod irem_g101;
mod mapper;
mod mapper12;
mod mapper185;
mod mapper241;
mod mapper242;
mod mapper243;
mod mapper244;
mod mapper245;
mod mapper246;
mod mapper251;
mod mapper254;
mod mapper255;
mod mapper37;
mod mapper42;
mod mapper43;
mod mapper44;
mod mapper45;
mod mapper46;
mod mapper47;
mod mapper48;
mod mapper49;
mod mapper50;
mod mapper51;
mod mapper52;
mod mapper53;
mod mapper56;
mod mapper57;
mod mapper58;
mod mapper59;
mod mapper60;
mod mapper61;
mod mapper62;
mod mapper64;
mod mapper65;
mod mapper67;
mod mapper72;
mod mapper73;
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
mod ntdec_2722;
mod rom_db;
mod sunsoft_4;
mod sunsoft_fme7;
mod super_magic_card;
mod taito_tc0190;
#[cfg(test)]
pub mod test_helpers;
mod uxrom;
mod vrc2_vrc4;
mod vrc6;

pub use cartridge::Cartridge;
#[allow(unused_imports)]
pub use common::{
    BankSwitch, BankedRom, ChrMemory, DEFAULT_CHR_RAM_SIZE, DEFAULT_PRG_RAM_SIZE, PrgRam,
    StateSnapshot,
};
#[allow(unused_imports)]
pub use ines::{ConsoleType, InesHeader, NametableLayout, ParsedRom, TimingMode};
#[allow(unused_imports)]
pub use mapper::{Mapper, MapperCapabilities, MapperContext};
#[allow(unused_imports)]
pub use mapper_templates::{DualBank32Mapper, SimpleBankedPrgMapper, SimpleFixedPrgMapper};
#[allow(unused_imports)]
pub use rom_db::calculate_rom_crc32;
pub(crate) use rom_db::default_arkanoid_on_port;
#[allow(unused_imports)]
pub use rom_db::{RomDb, RomDbEntry};
