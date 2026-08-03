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
        run_cputest_screen_crc("ADC/CPUADC.sfc", 94, 0x6BF9_6B41);
    }

    #[test]
    fn cpu_and_passes_all_modes() {
        // Settles at frame 30 on the AND (sr,S),Y page with all rows PASS.
        run_cputest_screen_crc("AND/CPUAND.sfc", 90, 0x2112_519A);
    }

    #[test]
    fn cpu_asl_passes_all_modes() {
        // Settles at frame 10 on the ASL dp,X page with all rows PASS.
        run_cputest_screen_crc("ASL/CPUASL.sfc", 70, 0x4073_9A9D);
    }

    #[test]
    fn cpu_bit_passes_all_modes() {
        // Settles at frame 18 on the TSB dp page with all rows PASS.
        run_cputest_screen_crc("BIT/CPUBIT.sfc", 78, 0x6CE9_EB3F);
    }

    #[test]
    fn cpu_bra_passes_all_branches() {
        // Settles at frame 2 with every branch opcode (BCC..BRL) PASS.
        run_cputest_screen_crc("BRA/CPUBRA.sfc", 62, 0x4A8C_01B5);
    }

    #[test]
    fn cpu_cmp_passes_all_modes() {
        // Settles at frame 42 on the CPY dp page with all rows PASS.
        run_cputest_screen_crc("CMP/CPUCMP.sfc", 102, 0x26A1_36A3);
    }

    #[test]
    fn cpu_dec_passes_all_modes() {
        // Settles at frame 14 on the DEY page with all rows PASS.
        run_cputest_screen_crc("DEC/CPUDEC.sfc", 74, 0x07AB_8290);
    }

    #[test]
    fn cpu_eor_passes_all_modes() {
        // Settles at frame 30 on the EOR (sr,S),Y page with all rows PASS.
        run_cputest_screen_crc("EOR/CPUEOR.sfc", 90, 0x81E6_684A);
    }

    #[test]
    fn cpu_inc_passes_all_modes() {
        // Settles at frame 14 on the INY page with all rows PASS.
        run_cputest_screen_crc("INC/CPUINC.sfc", 74, 0x6CB7_0894);
    }

    #[test]
    fn cpu_jmp_passes_all_jumps() {
        // Settles at frame 2 with every JMP/JML/JSR/JSL variant PASS.
        run_cputest_screen_crc("JMP/CPUJMP.sfc", 62, 0x144B_2E9B);
    }

    #[test]
    fn cpu_ldr_passes_all_modes() {
        // Settles at frame 50 on the LDY dp,X page with all rows PASS.
        run_cputest_screen_crc("LDR/CPULDR.sfc", 110, 0xAB8C_539C);
    }

    #[test]
    fn cpu_lsr_passes_all_modes() {
        // Settles at frame 10 on the LSR dp,X page with all rows PASS.
        run_cputest_screen_crc("LSR/CPULSR.sfc", 70, 0x7512_54E8);
    }

    #[test]
    fn cpu_mov_passes_block_moves() {
        // Settles at frame 6 with the MVP block-move result PASS.
        run_cputest_screen_crc("MOV/CPUMOV.sfc", 66, 0x81B5_1227);
    }

    #[test]
    fn cpu_msc_passes_all_misc_opcodes() {
        // Settles with NOP/WDM/BRK/COP/WAI all PASS and the STP row blank
        // behind the ROM's own "** Please Reset To PASS STP **" prompt: STP
        // halts the CPU, so the ROM cannot reach its own result write without
        // the reset it asks for.
        //
        // Until #3116 this asserted 0x7E06_1BD2, which reported STP as PASS
        // *without* a reset -- only reachable because NESER ran straight
        // through the halt. Re-approved against a fresh Mesen2 headless
        // capture at the same frame (0 px; the old golden differs from Mesen2
        // by 94 px, all inside the STP row's PASS text at x=201..231,
        // y=95..101).
        run_cputest_screen_crc("MSC/CPUMSC.sfc", 63, 0x28F7_F20D);
    }

    #[test]
    fn cpu_ora_passes_all_modes() {
        // Settles at frame 30 on the ORA (sr,S),Y page with all rows PASS.
        run_cputest_screen_crc("ORA/CPUORA.sfc", 90, 0x4174_C6F4);
    }

    #[test]
    fn cpu_phl_passes_all_modes() {
        // Settles at frame 32 on the PLY page with all rows PASS. Previously
        // failed on the PLP page because RDNMI ($4210) bits 6-4 didn't return
        // CPU open bus, leaving V=0 after WaitNMI's `bit.w $4210` (#2975).
        run_cputest_screen_crc("PHL/CPUPHL.sfc", 92, 0x5115_ABA3);
    }

    #[test]
    fn cpu_psr_passes_all_flag_opcodes() {
        // Settles at frame 2 with CLC/CLD/CLI/CLV/REP/SEC/SED/SEI/SEP PASS.
        run_cputest_screen_crc("PSR/CPUPSR.sfc", 62, 0xB38D_AC72);
    }

    #[test]
    fn cpu_ret_passes_all_returns() {
        // Settles at frame 2 with RTI/RTL/RTS all PASS.
        run_cputest_screen_crc("RET/CPURET.sfc", 62, 0x1CE2_2329);
    }

    #[test]
    fn cpu_rol_passes_all_modes() {
        // Settles at frame 10 on the ROL dp,X page with all rows PASS.
        run_cputest_screen_crc("ROL/CPUROL.sfc", 70, 0x8915_928C);
    }

    #[test]
    fn cpu_ror_passes_all_modes() {
        // Settles at frame 10 on the ROR dp,X page with all rows PASS.
        run_cputest_screen_crc("ROR/CPUROR.sfc", 70, 0x2774_35F7);
    }

    #[test]
    fn cpu_sbc_passes_all_modes() {
        // Settles at frame 34 on the SBC (sr,S),Y page with all rows PASS.
        run_cputest_screen_crc("SBC/CPUSBC.sfc", 94, 0xEF44_3E4A);
    }

    #[test]
    fn cpu_str_passes_all_modes() {
        // Settles at frame 48 on the STZ dp,X page with all rows PASS.
        run_cputest_screen_crc("STR/CPUSTR.sfc", 108, 0x467E_881B);
    }

    #[test]
    fn cpu_trn_passes_all_transfers() {
        // Settles at frame 28 on the XCE page with all rows PASS.
        run_cputest_screen_crc("TRN/CPUTRN.sfc", 88, 0x78B3_13EF);
    }
}
