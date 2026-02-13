pub use types::{AutorunFile, AutorunFrame, AUTORUN_VERSION};
pub use utils::{autorun_path_for_rom, crc32, load_autorun_file, save_autorun_file};

mod types;
mod utils;
