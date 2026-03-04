mod cartridge_catalog;
mod config;
mod nes;
mod ram_init;

use crate::app_context::SharedAppContext;
use crate::debugging::log_info;

pub use crate::cartridge::TimingMode;
pub use cartridge_catalog::{
    CartridgeCatalogOptions, default_catalog_csv_path, refresh_cartridge_catalog,
};
pub use config::ApuChannels;
pub use config::AutorunMode;
pub use config::Config;
pub use config::ParseResult;
pub use config::RamInitMode;
pub use nes::Nes;
pub use nes::{SAVESTATE_VERSION, SaveState, SaveStateError};
pub use ram_init::initialize_ram;

// Re-export state types from their component modules for backward compatibility
pub use crate::apu::{ApuState, DmcState, EnvelopeState, FrameCounterState, NoiseState, PulseState, TriangleState};
pub use crate::bus::{BusState, ControllerStateWrapper, MapperState};
pub use crate::cpu::CpuState;
pub use crate::input::{ArkanoidState, JoypadState, ZapperState};
pub use crate::ppu::{PpuRegisterState, PpuState, PpuTimingState, SpritesState};

pub fn log_rom_timing_mode_selection(
    app_context: &SharedAppContext,
    rom_timing_mode: TimingMode,
    applied: bool,
) {
    let binding = app_context.borrow();
    let config = binding.config();
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
