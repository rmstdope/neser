use super::rom_runner::{RunConfig, assert_rom_screen_crc};

const GILYON_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/gilyon";

#[cfg(test)]
mod tests {
    use super::*;

    /// The ROM finishes by looping forever on its final screen (either
    /// "Success" or a frozen "Failed" diagnostic screen), so any frame
    /// comfortably past completion works as the sampling point. It needs
    /// ~87M ticks to reach frame 2000; 400M (the same budget used
    /// throughout blargg_apu_tests.rs) gives comfortable headroom without
    /// letting a hung/regressed run grind for long.
    ///
    /// `cputest.sfc` is bit-identical to the previously vendored
    /// `cputest-full.sfc` (the full 1610-test build; the retired 1107-test
    /// `cputest-basic.sfc` was a strict subset of it), so the golden
    /// carries over unchanged from the old `gilyon_tests/` copy.
    #[test]
    fn cputest_passes_all_1610_tests() {
        // Reaches "Success" at test 0649 (hex), the last of 1610 full tests.
        assert_rom_screen_crc(
            GILYON_ROOT,
            "cputest.sfc",
            "gilyon_cpu_tests",
            2000,
            0xB7EB_715E,
            RunConfig::new(400_000_000, 0),
        );
    }
}
