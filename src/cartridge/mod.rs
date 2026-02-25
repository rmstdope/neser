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
mod mapper241;
mod mapper242;
mod mapper244;
mod mapper246;
mod mapper42;
mod mapper44;
mod mapper45;
mod mapper46;
mod mapper47;
mod mapper49;
mod mapper50;
mod mapper51;
mod mapper52;
mod mapper53;
mod mapper56;
mod mapper57;
mod mapper58;
mod mapper60;
mod mapper61;
mod mapper62;
mod mapper64;
mod mapper65;
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
pub use ines::{
    ConsoleType, InesHeader, Mirroring, NametableLayout, TimingMode, parse_header, parse_rom,
};
#[allow(unused_imports)]
pub use mapper::{
    Mapper, MapperAudio, MapperCapabilities, MapperComposable, MapperContext, MapperCore,
    MapperIrq, MapperPpuExtension, MapperStateSnapshot,
};
#[allow(unused_imports)]
pub use mapper_templates::{DualBank32Mapper, SimpleBankedPrgMapper, SimpleFixedPrgMapper};
#[allow(unused_imports)]
pub use rom_db::calculate_rom_crc32;
pub(crate) use rom_db::default_arkanoid_on_port;
pub use rom_db::{RomDb, RomDbEntry};
