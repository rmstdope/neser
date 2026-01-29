#[cfg(test)]
mod tests {
    use std::fs;

    use crate::cartridge::Cartridge;
    use crate::console::{Nes, TvSystem};
    use crate::integration_tests::rom_test_runner::tests::run_nes_for_frames;
    use crate::{setup_rom_console_crc_test, setup_rom_console_test, setup_rom_test};

    // branch_timing_tests
    setup_rom_console_test!(
        test_branch_timing,
        "roms/automated_tests/branch_timing_tests/1.Branch_Basics.nes"
    );
    setup_rom_console_test!(
        test_backward_branch,
        "roms/automated_tests/branch_timing_tests/2.Backward_Branch.nes"
    );
    setup_rom_console_test!(
        test_forward_branch,
        "roms/automated_tests/branch_timing_tests/3.Forward_Branch.nes"
    );

    // cpu_dummy_reads
    setup_rom_console_test!(
        test_cpu_dummy_reads,
        "roms/automated_tests/cpu_dummy_reads/cpu_dummy_reads.nes"
    );

    // cpu_dummy_writes
    setup_rom_test!(
        test_cpu_dummy_writes_oam,
        "roms/automated_tests/cpu_dummy_writes/cpu_dummy_writes_oam.nes"
    );
    setup_rom_test!(
        test_cpu_dummy_writes_ppumem,
        "roms/automated_tests/cpu_dummy_writes/cpu_dummy_writes_ppumem.nes"
    );

    // cpu_exec_space
    setup_rom_test!(
        test_cpu_exec_space_ppuio,
        "roms/automated_tests/cpu_exec_space/test_cpu_exec_space_ppuio.nes"
    );
    setup_rom_test!(
        test_cpu_exec_space_apu,
        "roms/automated_tests/cpu_exec_space/test_cpu_exec_space_apu.nes"
    );

    // cpu_interrupts_v2
    setup_rom_test!(
        test_cpu_interrupts_v2_cpu_interrupts,
        "roms/automated_tests/cpu_interrupts_v2/cpu_interrupts.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_cli_latency,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/1-cli_latency.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_nmi_and_brk,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/2-nmi_and_brk.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_nmi_and_irq,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/3-nmi_and_irq.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_irq_and_dma,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/4-irq_and_dma.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_branch_delays_irq,
        "roms/automated_tests/cpu_interrupts_v2/rom_singles/5-branch_delays_irq.nes"
    );

    // cpu_reset
    setup_rom_test!(
        test_cpu_reset_ram_after_reset,
        "roms/automated_tests/cpu_reset/ram_after_reset.nes"
    );
    setup_rom_test!(
        test_cpu_reset_reset_registers,
        "roms/automated_tests/cpu_reset/registers.nes"
    );

    // cpu_timing_test6
    setup_rom_console_test!(
        test_cpu_timing_test6,
        "roms/automated_tests/cpu_timing_test6/cpu_timing_test.nes"
    );

    // TODO dma_sync_test

    // dmc_dma_during_read4
    setup_rom_console_crc_test!(
        test_dmc_dma_during_read4_2007_read,
        "roms/automated_tests/dmc_dma_during_read4/dma_2007_read.nes",
        300,
        &[0x159A7A8F, 0x5E3DF9C4]
    );
    setup_rom_console_test!(
        test_dmc_dma_during_read4_2007_write,
        "roms/automated_tests/dmc_dma_during_read4/dma_2007_write.nes",
        300
    );
    setup_rom_console_test!(
        test_dmc_dma_during_read4_4016_read,
        "roms/automated_tests/dmc_dma_during_read4/dma_4016_read.nes",
        300
    );
    setup_rom_console_crc_test!(
        test_dmc_dma_during_read4_double_2007_read,
        "roms/automated_tests/dmc_dma_during_read4/double_2007_read.nes",
        300,
        &[0xF018C287, 0xD84F6815] //CRC1 - Mesen, loopyNES, etc., CRC2 - Nintendulator, FCEUX
    );
    setup_rom_console_test!(
        test_dmc_dma_during_read4_read_write_2007,
        "roms/automated_tests/dmc_dma_during_read4/read_write_2007.nes",
        300
    );

    // dpcmletterbox
    #[test]
    fn test_dpcmletterbox() {
        let rom_path = "roms/automated_tests/dpcmletterbox/dpcmletterbox.nes";
        let rom_data = fs::read(rom_path).expect("dpcmletterbox ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("dpcmletterbox ROM should parse");

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        run_nes_for_frames(&mut nes, 60);

        let crc = nes.get_screen_buffer().crc32();
        // Golden CRC generated from a visually verified correct frame capture using --trace-ppu=1
        assert_eq!(crc, 0x2813_2E95, "unexpected frame CRC for dpcmletterbox");
    }

    // instr_misc
    setup_rom_test!(
        test_instr_misc,
        "roms/automated_tests/instr_misc/instr_misc.nes"
    );
    setup_rom_test!(
        test_instr_misc_01,
        "roms/automated_tests/instr_misc/rom_singles/01-abs_x_wrap.nes"
    );
    setup_rom_test!(
        test_instr_misc_02,
        "roms/automated_tests/instr_misc/rom_singles/02-branch_wrap.nes"
    );
    setup_rom_test!(
        test_instr_misc_03,
        "roms/automated_tests/instr_misc/rom_singles/03-dummy_reads.nes"
    );
    setup_rom_test!(
        test_instr_misc_04,
        "roms/automated_tests/instr_misc/rom_singles/04-dummy_reads_apu.nes"
    );

    // test_instr_v3
    setup_rom_test!(
        test_instr_v3_all_instrs,
        "roms/automated_tests/instr_test-v3/all_instrs.nes"
    );
    setup_rom_test!(
        test_instr_v3_official_only,
        "roms/automated_tests/instr_test-v3/official_only.nes"
    );
    setup_rom_test!(
        test_instr_v3_01_implied,
        "roms/automated_tests/instr_test-v3/rom_singles/01-implied.nes"
    );
    setup_rom_test!(
        test_instr_v3_02_immediate,
        "roms/automated_tests/instr_test-v3/rom_singles/02-immediate.nes"
    );
    setup_rom_test!(
        test_instr_v3_03_zero_page,
        "roms/automated_tests/instr_test-v3/rom_singles/03-zero_page.nes"
    );
    setup_rom_test!(
        test_instr_v3_04_zp_xy,
        "roms/automated_tests/instr_test-v3/rom_singles/04-zp_xy.nes"
    );
    setup_rom_test!(
        test_instr_v3_05_absolute,
        "roms/automated_tests/instr_test-v3/rom_singles/05-absolute.nes"
    );
    setup_rom_test!(
        test_instr_v3_06_abs_xy,
        "roms/automated_tests/instr_test-v3/rom_singles/06-abs_xy.nes"
    );
    setup_rom_test!(
        test_instr_v3_07_ind_x,
        "roms/automated_tests/instr_test-v3/rom_singles/07-ind_x.nes"
    );
    setup_rom_test!(
        test_instr_v3_08_ind_y,
        "roms/automated_tests/instr_test-v3/rom_singles/08-ind_y.nes"
    );
    setup_rom_test!(
        test_instr_v3_09_branches,
        "roms/automated_tests/instr_test-v3/rom_singles/09-branches.nes"
    );
    setup_rom_test!(
        test_instr_v3_10_stack,
        "roms/automated_tests/instr_test-v3/rom_singles/10-stack.nes"
    );
    setup_rom_test!(
        test_instr_v3_11_jmp_jsr,
        "roms/automated_tests/instr_test-v3/rom_singles/11-jmp_jsr.nes"
    );
    setup_rom_test!(
        test_instr_v3_12_rts,
        "roms/automated_tests/instr_test-v3/rom_singles/12-rts.nes"
    );
    setup_rom_test!(
        test_instr_v3_13_rti,
        "roms/automated_tests/instr_test-v3/rom_singles/13-rti.nes"
    );
    setup_rom_test!(
        test_instr_v3_14_brk,
        "roms/automated_tests/instr_test-v3/rom_singles/14-brk.nes"
    );
    setup_rom_test!(
        test_instr_v3_15_special,
        "roms/automated_tests/instr_test-v3/rom_singles/15-special.nes"
    );

    // test_instr_v5
    setup_rom_test!(
        test_instr_v5_all_instrs,
        "roms/automated_tests/instr_test-v5/all_instrs.nes"
    );
    setup_rom_test!(
        test_instr_v5_official_only,
        "roms/automated_tests/instr_test-v5/official_only.nes"
    );
    setup_rom_test!(
        test_instr_v5_01_basics,
        "roms/automated_tests/instr_test-v5/rom_singles/01-basics.nes"
    );
    setup_rom_test!(
        test_instr_v5_02_implied,
        "roms/automated_tests/instr_test-v5/rom_singles/02-implied.nes"
    );
    setup_rom_test!(
        test_instr_v5_03_immediate,
        "roms/automated_tests/instr_test-v5/rom_singles/03-immediate.nes"
    );
    setup_rom_test!(
        test_instr_v5_04_zero_page,
        "roms/automated_tests/instr_test-v5/rom_singles/04-zero_page.nes"
    );
    setup_rom_test!(
        test_instr_v5_05_zp_xy,
        "roms/automated_tests/instr_test-v5/rom_singles/05-zp_xy.nes"
    );
    setup_rom_test!(
        test_instr_v5_06_absolute,
        "roms/automated_tests/instr_test-v5/rom_singles/06-absolute.nes"
    );
    setup_rom_test!(
        test_instr_v5_07_abs_xy,
        "roms/automated_tests/instr_test-v5/rom_singles/07-abs_xy.nes"
    );
    setup_rom_test!(
        test_instr_v5_08_ind_x,
        "roms/automated_tests/instr_test-v5/rom_singles/08-ind_x.nes"
    );
    setup_rom_test!(
        test_instr_v5_09_ind_y,
        "roms/automated_tests/instr_test-v5/rom_singles/09-ind_y.nes"
    );
    setup_rom_test!(
        test_instr_v5_10_branches,
        "roms/automated_tests/instr_test-v5/rom_singles/10-branches.nes"
    );
    setup_rom_test!(
        test_instr_v5_11_stack,
        "roms/automated_tests/instr_test-v5/rom_singles/11-stack.nes"
    );
    setup_rom_test!(
        test_instr_v5_12_jmp_jsr,
        "roms/automated_tests/instr_test-v5/rom_singles/12-jmp_jsr.nes"
    );
    setup_rom_test!(
        test_instr_v5_13_rts,
        "roms/automated_tests/instr_test-v5/rom_singles/13-rts.nes"
    );
    setup_rom_test!(
        test_instr_v5_14_rti,
        "roms/automated_tests/instr_test-v5/rom_singles/14-rti.nes"
    );
    setup_rom_test!(
        test_instr_v5_15_brk,
        "roms/automated_tests/instr_test-v5/rom_singles/15-brk.nes"
    );
    setup_rom_test!(
        test_instr_v5_16_special,
        "roms/automated_tests/instr_test-v5/rom_singles/16-special.nes"
    );

    // instr_timing
    setup_rom_test!(
        test_instr_timing,
        "roms/automated_tests/instr_timing/instr_timing.nes"
    );
    setup_rom_test!(
        test_instr_timing_01,
        "roms/automated_tests/instr_timing/rom_singles/1-instr_timing.nes"
    );
    setup_rom_test!(
        test_instr_timing_02,
        "roms/automated_tests/instr_timing/rom_singles/2-branch_timing.nes"
    );

    // nestest
    // TODO Move nestest from nes.rs here

    // sprdma_and_dmc_dma
    setup_rom_test!(
        test_sprdma_and_dmc_dma,
        "roms/automated_tests/sprdma_and_dmc_dma/sprdma_and_dmc_dma.nes"
    );
    setup_rom_test!(
        test_sprdma_and_dmc_dma_512,
        "roms/automated_tests/sprdma_and_dmc_dma/sprdma_and_dmc_dma_512.nes"
    );
}
