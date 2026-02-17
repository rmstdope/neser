mod config;
mod nes;
mod ram_init;
mod savestate;

use crate::cartridge::RomTvSystem;
use crate::debugging::log_info;

pub use config::ApuChannels;
pub use config::AutorunMode;
pub use config::Config;
pub use config::ParseResult;
pub use config::RamInitMode;
pub use nes::Nes;
pub use nes::TvSystem;
pub use ram_init::initialize_ram;
pub use savestate::{SAVESTATE_VERSION, SaveState, SpritesState};
// Re-export state types from their respective modules
pub use crate::apu::{ApuState, DmcState, EnvelopeState, FrameCounterState, NoiseState, PulseState, TriangleState};
pub use crate::bus::{BusState, ControllerStateWrapper, MapperState};
pub use crate::cpu::CpuState;
pub use crate::input::{ArkanoidState, JoypadState, ZapperState};
pub use crate::ppu::{PpuRegisterState, PpuState, PpuTimingState};

pub fn log_rom_tv_system_selection(config: &Config, rom_tv_system: RomTvSystem, applied: bool) {
    if !config.tv_system_explicit && rom_tv_system == RomTvSystem::Unknown {
        log_info(format!(
            "ROM TV system unknown; using configured {}",
            config.tv_system.as_str()
        ));
    } else if applied {
        log_info(format!(
            "ROM TV system detected as {}; applying timing",
            rom_tv_system.as_str()
        ));
    }
}
