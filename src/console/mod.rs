mod cartridge_catalog;
mod config;
mod nes;
mod ram_init;
pub mod save_state_io;

use crate::app_context::SharedAppContext;
use crate::debugging::log_info;

pub use crate::autorun::AutorunFormat;
pub use crate::cartridge::TimingMode;
pub use cartridge_catalog::{
    CartridgeCatalogOptions, default_catalog_csv_path, refresh_cartridge_catalog,
};
#[allow(unused_imports)] // Used by frontend features
pub use config::ApuChannels;
#[allow(unused_imports)] // Used by frontend features
pub use config::AutorunMode;
pub use config::Config;
pub use config::ExpansionPort;
pub use config::HardwareMode;
#[allow(unused_imports)] // Used by integration tests and lib consumers
pub use config::HardwareModel;
pub use config::ParseResult;
pub use config::RamInitMode;
pub use nes::Nes;
pub use nes::SaveState;
pub use ram_init::initialize_ram;

pub fn log_hardware_selection(app_context: &SharedAppContext, timing_applied: bool) {
    let binding = app_context.borrow();
    let cfg = binding.config();

    let hardware_desc = match cfg.hardware_mode {
        config::HardwareMode::Famicom => "Famicom".to_string(),
        config::HardwareMode::Nes => match cfg.hardware_model {
            config::HardwareModel::NesNtsc => "NES NTSC".to_string(),
            config::HardwareModel::NesPal => "NES PAL".to_string(),
            config::HardwareModel::Dendy => "NES Dendy".to_string(),
        },
    };

    let source =
        if cfg.hardware_mode == config::HardwareMode::Famicom && !cfg.hardware_mode_explicit {
            "from ROM DB"
        } else if cfg.hardware_mode_explicit || cfg.hardware_model_explicit {
            "from configuration"
        } else if timing_applied {
            "detected from ROM"
        } else {
            "default"
        };

    log_info(format!("Emulating {hardware_desc} ({source})"));
}
