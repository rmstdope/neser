use super::rom_runner::{RunConfig, assert_rom_screen_crc};

const PETERLEMON_CPU_ROOT: &str =
    "roms/snes/automated_tests/snes_test_roms/PeterLemon/SNES-CPUTest-CPU";

#[cfg(test)]
mod tests {
    use super::*;

    /// Each ROM draws one result page per opcode/addressing-mode group and
    /// freezes on the final page (or on the first FAIL row). The golden CRCs
    /// were probed by running each ROM until its screen CRC stayed unchanged
    /// for 600 consecutive frames and manually confirming the settled screen
    /// only reports PASS rows (issue #2974). The sampled frame is that settle
    /// frame plus a 60-frame margin, comfortably inside the verified-stable
    /// window. The 400M tick budget matches the other screen-CRC suites.
    fn run_cputest_screen_crc(file: &str, frames: u32, expected_crc: u32) {
        assert_rom_screen_crc(
            PETERLEMON_CPU_ROOT,
            file,
            "peterlemon_cpu_tests",
            frames,
            expected_crc,
            RunConfig::new(400_000_000, 0),
        );
    }

    #[test]
    fn cpu_adc_passes_all_modes() {
        // Settles at frame 34 on the ADC (sr,S),Y page with all rows PASS.
        run_cputest_screen_crc("ADC/CPUADC.sfc", 94, 0x4BEE_DE02);
    }

    #[test]
    fn cpu_and_passes_all_modes() {
        // Settles at frame 30 on the AND (sr,S),Y page with all rows PASS.
        run_cputest_screen_crc("AND/CPUAND.sfc", 90, 0xA8A0_A1C4);
    }

    #[test]
    fn cpu_asl_passes_all_modes() {
        // Settles at frame 10 on the ASL dp,X page with all rows PASS.
        run_cputest_screen_crc("ASL/CPUASL.sfc", 70, 0xC583_F94E);
    }

    #[test]
    fn cpu_bit_passes_all_modes() {
        // Settles at frame 18 on the TSB dp page with all rows PASS.
        run_cputest_screen_crc("BIT/CPUBIT.sfc", 78, 0xDE68_86CC);
    }

    #[test]
    fn cpu_bra_passes_all_branches() {
        // Settles at frame 2 with every branch opcode (BCC..BRL) PASS.
        run_cputest_screen_crc("BRA/CPUBRA.sfc", 62, 0x8AA7_A0D1);
    }

    #[test]
    fn cpu_cmp_passes_all_modes() {
        // Settles at frame 42 on the CPY dp page with all rows PASS.
        run_cputest_screen_crc("CMP/CPUCMP.sfc", 102, 0x9D3E_4BFF);
    }

    #[test]
    fn cpu_dec_passes_all_modes() {
        // Settles at frame 14 on the DEY page with all rows PASS.
        run_cputest_screen_crc("DEC/CPUDEC.sfc", 74, 0xD52C_E94E);
    }

    #[test]
    fn cpu_eor_passes_all_modes() {
        // Settles at frame 30 on the EOR (sr,S),Y page with all rows PASS.
        run_cputest_screen_crc("EOR/CPUEOR.sfc", 90, 0xAE77_FD05);
    }

    #[test]
    fn cpu_inc_passes_all_modes() {
        // Settles at frame 14 on the INY page with all rows PASS.
        run_cputest_screen_crc("INC/CPUINC.sfc", 74, 0xE2D7_9190);
    }

    #[test]
    fn cpu_jmp_passes_all_jumps() {
        // Settles at frame 2 with every JMP/JML/JSR/JSL variant PASS.
        run_cputest_screen_crc("JMP/CPUJMP.sfc", 62, 0xE740_3CEF);
    }

    #[test]
    fn cpu_ldr_passes_all_modes() {
        // Settles at frame 50 on the LDY dp,X page with all rows PASS.
        run_cputest_screen_crc("LDR/CPULDR.sfc", 110, 0x4AD8_56A6);
    }

    #[test]
    fn cpu_lsr_passes_all_modes() {
        // Settles at frame 10 on the LSR dp,X page with all rows PASS.
        run_cputest_screen_crc("LSR/CPULSR.sfc", 70, 0xEE12_D392);
    }

    #[test]
    fn cpu_mov_passes_block_moves() {
        // Settles at frame 6 with the MVP block-move result PASS.
        run_cputest_screen_crc("MOV/CPUMOV.sfc", 66, 0xF87B_4CA6);
    }

    #[test]
    fn cpu_msc_passes_all_misc_opcodes() {
        // Settles at frame 3 with NOP/WDM/BRK/COP/WAI/STP all PASS. The ROM
        // also prints "** Please Reset To PASS STP **" (a hardware prompt),
        // but the settled screen already reports STP as PASS without a reset.
        run_cputest_screen_crc("MSC/CPUMSC.sfc", 63, 0xAB40_776C);
    }

    #[test]
    fn cpu_ora_passes_all_modes() {
        // Settles at frame 30 on the ORA (sr,S),Y page with all rows PASS.
        run_cputest_screen_crc("ORA/CPUORA.sfc", 90, 0xCE37_E028);
    }

    #[test]
    #[ignore = "PLP (0x28) reports FAIL (BIN,8 result 0x25, NVZC 0011); see issue #2975"]
    fn cpu_phl_passes_all_modes() {
        // Golden CRC unknown until the PLP failure is fixed: the ROM halts at
        // the first FAIL row, currently settling at frame 28 with CRC
        // 0x363F3135 showing "BIN,8 $25 0011 FAIL". Re-probe the settle frame
        // and golden CRC once issue #2975 is resolved.
        run_cputest_screen_crc("PHL/CPUPHL.sfc", 88, 0xFFFF_FFFF);
    }

    #[test]
    fn cpu_psr_passes_all_flag_opcodes() {
        // Settles at frame 2 with CLC/CLD/CLI/CLV/REP/SEC/SED/SEI/SEP PASS.
        run_cputest_screen_crc("PSR/CPUPSR.sfc", 62, 0x0826_2AF4);
    }

    #[test]
    fn cpu_ret_passes_all_returns() {
        // Settles at frame 2 with RTI/RTL/RTS all PASS.
        run_cputest_screen_crc("RET/CPURET.sfc", 62, 0x021D_9680);
    }

    #[test]
    fn cpu_rol_passes_all_modes() {
        // Settles at frame 10 on the ROL dp,X page with all rows PASS.
        run_cputest_screen_crc("ROL/CPUROL.sfc", 70, 0xE888_CD0D);
    }

    #[test]
    fn cpu_ror_passes_all_modes() {
        // Settles at frame 10 on the ROR dp,X page with all rows PASS.
        run_cputest_screen_crc("ROR/CPUROR.sfc", 70, 0xF0E4_1FA6);
    }

    #[test]
    fn cpu_sbc_passes_all_modes() {
        // Settles at frame 34 on the SBC (sr,S),Y page with all rows PASS.
        run_cputest_screen_crc("SBC/CPUSBC.sfc", 94, 0x6CEF_FFAB);
    }

    #[test]
    fn cpu_str_passes_all_modes() {
        // Settles at frame 48 on the STZ dp,X page with all rows PASS.
        run_cputest_screen_crc("STR/CPUSTR.sfc", 108, 0x1FA5_5810);
    }

    #[test]
    fn cpu_trn_passes_all_transfers() {
        // Settles at frame 28 on the XCE page with all rows PASS.
        run_cputest_screen_crc("TRN/CPUTRN.sfc", 88, 0x7068_651F);
    }
}
