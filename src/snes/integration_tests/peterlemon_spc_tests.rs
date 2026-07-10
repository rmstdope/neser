use super::rom_runner::{RunConfig, assert_rom_screen_crc};

const PETERLEMON_SPC_ROOT: &str =
    "roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-CPUTest-SPC700";

#[cfg(test)]
mod tests {
    use super::*;

    /// Each ROM uploads an SPC700 test program, draws one result page per
    /// opcode group, and freezes on the final page (or on the first FAIL
    /// row). The golden CRCs were probed by running each ROM until its screen
    /// CRC stayed unchanged for 600 consecutive frames and manually
    /// confirming the settled screen only reports PASS rows (issue #2974).
    /// The sampled frame is that settle frame plus a 60-frame margin,
    /// comfortably inside the verified-stable window. These ROMs settle much
    /// later than the CPU suite (up to ~1700 frames) because every SPC700
    /// test round-trips through the APU ports; the 400M tick budget still
    /// leaves ample headroom.
    fn run_spctest_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        assert_rom_screen_crc(
            PETERLEMON_SPC_ROOT,
            file,
            "peterlemon_spc_tests",
            frames,
            expected_crc,
            RunConfig::new(400_000_000, 0),
        );
    }

    #[test]
    fn spc700_adc_passes_all_modes() {
        // Settles at frame 1579 on the ADW dp page with all rows PASS.
        run_spctest_screen_crc("ADC/SPC700ADC.sfc", 1639, 0x794D_0629);
    }

    #[test]
    fn spc700_and_passes_all_modes() {
        // Settles at frame 1702 on the AND !addr:bit page with all rows PASS.
        run_spctest_screen_crc("AND/SPC700AND.sfc", 1762, 0xD41B_379C);
    }

    #[test]
    fn spc700_dec_passes_all_modes() {
        // Settles at frame 840 on the DEW dp page with all rows PASS.
        run_spctest_screen_crc("DEC/SPC700DEC.sfc", 900, 0x5B09_3146);
    }

    #[test]
    fn spc700_eor_passes_all_modes() {
        // Settles at frame 1579 on the EOR addr:bit page with all rows PASS.
        run_spctest_screen_crc("EOR/SPC700EOR.sfc", 1639, 0x8A56_FC0D);
    }

    #[test]
    fn spc700_inc_passes_all_modes() {
        // Settles at frame 840 on the INW dp page with all rows PASS.
        run_spctest_screen_crc("INC/SPC700INC.sfc", 900, 0x907F_E928);
    }

    #[test]
    fn spc700_ora_passes_all_modes() {
        // Settles at frame 1702 on the ORC !addr:bit page with all rows PASS.
        run_spctest_screen_crc("ORA/SPC700ORA.sfc", 1762, 0x41E1_5C34);
    }

    #[test]
    fn spc700_sbc_passes_all_modes() {
        // Settles at frame 1579 on the SBW dp page with all rows PASS.
        run_spctest_screen_crc("SBC/SPC700SBC.sfc", 1639, 0x85DA_3CA3);
    }
}
