mod cartridge_catalog;
mod config;
mod nes;
pub mod save_state_io;

use crate::platform::app_context::SharedAppContext;
use crate::platform::debugging::log_info;

pub use crate::nes::cartridge::TimingMode;
pub use crate::platform::config::Config;
pub use crate::platform::config::ParseResult;
pub use crate::platform::config::RamInitMode;
pub use crate::platform::ram_init::initialize_ram;
pub use cartridge_catalog::{
    CartridgeCatalogOptions, default_catalog_csv_path, refresh_cartridge_catalog,
};
#[allow(unused_imports)] // Used by frontend features
pub use config::ApuChannels;
pub(crate) use config::CLI_FLAGS;
pub use config::ExpansionPort;
pub use config::HardwareMode;
#[allow(unused_imports)] // Used by integration tests and lib consumers
pub use config::HardwareModel;
pub use config::NesConfig;
pub use nes::Nes;
pub use nes::SaveState;

pub fn log_hardware_selection(app_context: &SharedAppContext, timing_applied: bool) {
    let binding = app_context.borrow();
    let cfg = binding.config();

    let hardware_desc = match cfg.nes.hardware_mode {
        config::HardwareMode::Famicom => "Famicom".to_string(),
        config::HardwareMode::Nes => match cfg.nes.hardware_model {
            config::HardwareModel::NesNtsc => "NES NTSC".to_string(),
            config::HardwareModel::NesPal => "NES PAL".to_string(),
            config::HardwareModel::Dendy => "NES Dendy".to_string(),
        },
    };

    let source = if cfg.nes.hardware_mode == config::HardwareMode::Famicom
        && !cfg.nes.hardware_mode_explicit
    {
        "from ROM DB"
    } else if cfg.nes.hardware_mode_explicit || cfg.nes.hardware_model_explicit {
        "from configuration"
    } else if timing_applied {
        "detected from ROM"
    } else {
        "default"
    };

    log_info(format!("Emulating {hardware_desc} ({source})"));
}
