//! The vendored PeterLemon (krom) Super FX test ROMs
//! (`roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-CHIP-GSU-GSUTest/`), one per GSU
//! opcode group plus a code-cache injection demo. Each ROM runs its GSU program once per case,
//! prints result, flags and PASS/FAIL for every row, and then idles on that screen.
//!
//! Goldens (nr-hab.1): every screen settles by frame 72 and stays unchanged through frame 900.
//! The frame-300 capture of each ROM matched a Mesen2 2.1.1 `--testRunner` capture at the same
//! frame (`--snes.RamPowerOnState=AllZeros`, frame skipping off) at 0 differing pixels, and also
//! matched krom's own screenshot shipped beside the ROM (`GSU<name>.png`, every row PASS) at 0
//! differing pixels. So each CRC below is a screen on which every row reads PASS.
//!
//! Mutation checks: forcing ADD/ADC's overflow flag to 0 fails `gsu_add_passes`,
//! `gsu_adc_passes` and `gsu_cacheinject_passes` (whose injected program is an ADC); leaving
//! cache lines written through `$3100-$32FF` marked empty fails `gsu_cacheinject_passes` alone.
//! The suite pins opcode results and flags as the S-CPU reads them back, not GSU timing: each ROM
//! waits for the GSU with a GO poll.

use super::rom_runner::{RunConfig, assert_rom_screen_crc};

const PETERLEMON_GSU_ROOT: &str =
    "roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-CHIP-GSU-GSUTest";

/// The Mesen2- and screenshot-verified frame (see the module doc).
const SAMPLE_FRAME: u32 = 300;

#[cfg(test)]
mod tests {
    use super::*;

    fn run_gsu_test(name: &str, expected_crc: u32) {
        assert_rom_screen_crc(
            PETERLEMON_GSU_ROOT,
            &format!("{name}/GSU{name}.sfc"),
            "peterlemon_gsu_tests",
            SAMPLE_FRAME,
            expected_crc,
            RunConfig::new(400_000_000, 0),
        );
    }

    #[test]
    fn gsu_adc_passes() {
        run_gsu_test("ADC", 0x1767_E977);
    }

    #[test]
    fn gsu_add_passes() {
        run_gsu_test("ADD", 0x0EA8_F3FF);
    }

    #[test]
    fn gsu_and_passes() {
        run_gsu_test("AND", 0x25E3_A853);
    }

    #[test]
    fn gsu_asr_passes() {
        run_gsu_test("ASR", 0x2220_9526);
    }

    #[test]
    fn gsu_bic_passes() {
        run_gsu_test("BIC", 0x6845_8FAF);
    }

    #[test]
    fn gsu_cacheinject_passes() {
        run_gsu_test("CACHEINJECT", 0x5A71_9C6B);
    }

    #[test]
    fn gsu_cmp_passes() {
        run_gsu_test("CMP", 0xF654_3F6F);
    }

    #[test]
    fn gsu_dec_passes() {
        run_gsu_test("DEC", 0xBD56_8EC2);
    }

    #[test]
    fn gsu_div2_passes() {
        run_gsu_test("DIV2", 0xFBC1_5702);
    }

    #[test]
    fn gsu_fmult_passes() {
        run_gsu_test("FMULT", 0x36EE_59F6);
    }

    #[test]
    fn gsu_hib_passes() {
        run_gsu_test("HIB", 0x9C56_9386);
    }

    #[test]
    fn gsu_ibt_passes() {
        run_gsu_test("IBT", 0x6CCD_70A8);
    }

    #[test]
    fn gsu_inc_passes() {
        run_gsu_test("INC", 0xC275_9DD2);
    }

    #[test]
    fn gsu_iwt_passes() {
        run_gsu_test("IWT", 0x3997_B57B);
    }

    #[test]
    fn gsu_lmult_passes() {
        run_gsu_test("LMULT", 0x8086_9798);
    }

    #[test]
    fn gsu_lob_passes() {
        run_gsu_test("LOB", 0xA1AF_BCA5);
    }

    #[test]
    fn gsu_lsr_passes() {
        run_gsu_test("LSR", 0xAEB6_1962);
    }

    #[test]
    fn gsu_merge_passes() {
        run_gsu_test("MERGE", 0x0B8E_9661);
    }

    #[test]
    fn gsu_move_passes() {
        run_gsu_test("MOVE", 0x3673_B10F);
    }

    #[test]
    fn gsu_moves_passes() {
        run_gsu_test("MOVES", 0x90BF_80B9);
    }

    #[test]
    fn gsu_mult_passes() {
        run_gsu_test("MULT", 0xA41B_3F9F);
    }

    #[test]
    fn gsu_not_passes() {
        run_gsu_test("NOT", 0x2900_EA5D);
    }

    #[test]
    fn gsu_or_passes() {
        run_gsu_test("OR", 0xCDF4_8941);
    }

    #[test]
    fn gsu_rol_passes() {
        run_gsu_test("ROL", 0xB027_8304);
    }

    #[test]
    fn gsu_ror_passes() {
        run_gsu_test("ROR", 0x78C2_933C);
    }

    #[test]
    fn gsu_sbc_passes() {
        run_gsu_test("SBC", 0x9F67_B5C0);
    }

    #[test]
    fn gsu_sex_passes() {
        run_gsu_test("SEX", 0xE110_DB25);
    }

    #[test]
    fn gsu_sub_passes() {
        run_gsu_test("SUB", 0xB987_6FDB);
    }

    #[test]
    fn gsu_swap_passes() {
        run_gsu_test("SWAP", 0xB56D_C5B0);
    }

    #[test]
    fn gsu_umult_passes() {
        run_gsu_test("UMULT", 0x3A27_B9D5);
    }

    #[test]
    fn gsu_xor_passes() {
        run_gsu_test("XOR", 0xC9DF_E9BE);
    }
}
