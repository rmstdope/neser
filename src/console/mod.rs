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
pub use config::HardwareModel;
pub use config::ParseResult;
pub use config::RamInitMode;
pub use nes::Nes;
pub use nes::SaveState;
pub use ram_init::initialize_ram;

pub fn log_rom_timing_mode_selection(
    app_context: &SharedAppContext,
    rom_timing_mode: TimingMode,
    applied: bool,
) {
    let binding = app_context.borrow();
    let config = binding.config();
    if !config.hardware_model_explicit && !rom_timing_mode.is_ntsc_or_pal() {
        log_info(format!(
            "ROM TV system unknown; using configured {}",
            config.hardware_model.as_str()
        ));
    } else if applied {
        log_info(format!(
            "ROM TV system detected as {}; applying timing",
            rom_timing_mode.as_str()
        ));
    }
}
