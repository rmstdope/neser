mod config;
mod nes;
mod savestate;

use crate::cartridge::RomTvSystem;
use crate::debugging::log_info;

pub use config::ApuChannels;
pub use config::AutorunMode;
pub use config::Config;
pub use config::ParseResult;
pub use nes::Nes;
pub use nes::TvSystem;
pub use savestate::{
    ApuState, ArkanoidState, BusState, ControllerStateWrapper, CpuState, DmcState, EnvelopeState,
    FrameCounterState, JoypadState, MapperState, NoiseState, PpuRegisterState, PpuState,
    PpuTimingState, PulseState, SAVESTATE_VERSION, SaveState, SpritesState, TriangleState,
    ZapperState,
};

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
