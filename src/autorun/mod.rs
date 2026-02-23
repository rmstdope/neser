pub use types::{
    AUTORUN_VERSION, AutorunCheckpoint, AutorunFile, AutorunFrame, CHECKPOINT_INTERVAL_FRAMES,
};
pub use utils::{
    autorun_path_for_rom, backup_autorun_file, crc32, load_autorun_file, save_autorun_file,
    trim_recording,
};
pub mod headless_playback;

mod types;
mod utils;
