#[allow(unused_imports)] // SDL-only items; unused until native autorun support
pub use types::{
    AUTORUN_VERSION, AutorunCheckpoint, AutorunFile, AutorunFormat, AutorunFrame,
    CHECKPOINT_INTERVAL_FRAMES,
};
#[allow(unused_imports)]
pub use utils::{
    BINARY_MAGIC, autorun_path_for_rom, backup_autorun_file, convert_autorun_file, crc32,
    load_autorun_file, save_autorun_file, trim_recording,
};
pub mod headless_playback;
#[cfg(any(feature = "sdl", feature = "native"))]
pub mod state;

mod types;
mod utils;
