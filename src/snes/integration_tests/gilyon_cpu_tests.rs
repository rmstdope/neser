use super::rom_runner::{RunConfig, assert_rom_screen_crc};

const GILYON_TESTS_CPU_ROOT: &str = "roms/snes/automated_tests/gilyon_tests/cputest";

#[cfg(test)]
mod tests {
    use super::*;

    /// Both ROMs finish by looping forever on their final screen (either
    /// "Success" or a frozen "Failed" diagnostic screen), so any frame
    /// comfortably past completion works as the sampling point. Both need
    /// ~87M ticks to reach frame 2000; 400M (the same budget used
    /// throughout blargg_apu_tests.rs) gives comfortable headroom without
    /// letting a hung/regressed run grind for long.
    fn run_cputest_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        assert_rom_screen_crc(
            GILYON_TESTS_CPU_ROOT,
            file,
            frames,
            expected_crc,
            RunConfig::new(400_000_000, 0),
        );
    }

    #[test]
    fn cputest_basic_passes_all_1107_tests() {
        // Reaches "Success" at test 0452 (hex), the last of 1107 basic tests.
        run_cputest_screen_crc("cputest-basic.sfc", 2000, 0xB4FA_650E);
    }

    #[test]
    fn cputest_full_passes_all_1610_tests() {
        // Reaches "Success" at test 0649 (hex), the last of 1610 full tests.
        run_cputest_screen_crc("cputest-full.sfc", 2000, 0xB7EB_715E);
    }
}
