mod config;
mod nes;
mod ram_init;
mod savestate;

use crate::debugging::log_info;

pub use crate::cartridge::TimingMode;
pub use config::ApuChannels;
pub use config::AutorunMode;
pub use config::Config;
pub use config::ParseResult;
pub use config::RamInitMode;
pub use nes::Nes;
pub use ram_init::initialize_ram;
pub use savestate::{
    ApuState, ArkanoidState, BusState, ControllerStateWrapper, CpuState, DmcState, EnvelopeState,
    FrameCounterState, JoypadState, MapperState, NoiseState, PpuRegisterState, PpuState,
    PpuTimingState, PulseState, SAVESTATE_VERSION, SaveState, SpritesState, TriangleState,
    ZapperState,
};

pub fn log_rom_timing_mode_selection(config: &Config, rom_timing_mode: TimingMode, applied: bool) {
    if !config.tv_system_explicit && !rom_timing_mode.is_ntsc_or_pal() {
        log_info(format!(
            "ROM TV system unknown; using configured {}",
            config.tv_system.as_str()
        ));
    } else if applied {
        log_info(format!(
            "ROM TV system detected as {}; applying timing",
            rom_timing_mode.as_str()
        ));
    }
}
