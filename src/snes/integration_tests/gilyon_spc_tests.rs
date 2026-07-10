use super::rom_runner::{RunConfig, assert_rom_screen_crc};

const GILYON_ROOT: &str = "roms/snes/automated_tests/snes_test_roms/gilyon";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spctest_passes_all_1320_tests() {
        // Reaches "Success" at test 0527 (hex), the last of 1320 SPC-700
        // tests in this upstream build (the previously vendored older build
        // had 1368; the count changed upstream, not in NESER). Needs ~87M
        // ticks to reach frame 2000; 400M (the same budget used throughout
        // blargg_apu_tests.rs) gives comfortable headroom without letting a
        // hung/regressed run grind for long. The Success screen is static
        // (CRC identical at frames 2000 and 2500).
        assert_rom_screen_crc(
            GILYON_ROOT,
            "spctest.sfc",
            "gilyon_spc_tests",
            2000,
            0xE10F_EB9D,
            RunConfig::new(400_000_000, 0),
        );
    }
}
