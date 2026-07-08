use super::rom_runner::{RunConfig, assert_rom_screen_crc};

const GILYON_TESTS_SPC_ROOT: &str = "roms/snes/automated_tests/gilyon_tests/spctest";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spctest_passes_all_1368_tests() {
        // Reaches "Success" at test 0557 (hex), the last of 1368 SPC-700
        // tests. Needs ~87M ticks to reach frame 2000; 400M (the same
        // budget used throughout blargg_apu_tests.rs) gives comfortable
        // headroom without letting a hung/regressed run grind for long.
        assert_rom_screen_crc(
            GILYON_TESTS_SPC_ROOT,
            "spctest.sfc",
            "gilyon_spc_tests",
            2000,
            0x87CD_986B,
            RunConfig::new(400_000_000, 0),
        );
    }
}
