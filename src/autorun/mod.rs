#[allow(unused_imports)] // Used by autorun features
pub use types::{
    AUTORUN_VERSION, AutorunCheckpoint, AutorunFile, AutorunFormat, AutorunFrame, AutorunMode,
    CHECKPOINT_INTERVAL_FRAMES,
};
#[allow(unused_imports)]
pub use utils::{
    BINARY_MAGIC, autorun_path_for_rom, backup_autorun_file, convert_autorun_file, crc32,
    load_autorun_file, save_autorun_file, trim_recording,
};
#[cfg(feature = "native")]
pub mod state;

mod types;
mod utils;
