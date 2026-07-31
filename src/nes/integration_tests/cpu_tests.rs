#[cfg(test)]
mod tests {
    use std::fs;

    use crate::nes::cartridge::Cartridge;
    use crate::nes::console::{Config, Nes, RamInitMode};
    use crate::nes::input::Button;
    use crate::nes::integration_tests::rom_test_runner::tests::run_nes_for_frames;
    use crate::platform::config::FrontendConfig;
    use crate::{setup_rom_console_crc_test, setup_rom_console_test, setup_rom_test};

    fn deterministic_config() -> Config {
        Config {
            frontend: FrontendConfig {
                ram_init_mode: RamInitMode::Zero,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn load_test_cartridge(rom_data: &[u8], rom_path: &str) -> Cartridge {
        Cartridge::load_from_file(rom_data, rom_path, None).expect("ROM should parse")
    }

    // branch_timing_tests
    setup_rom_console_test!(
        test_branch_timing,
        "roms/nes/automated_tests/branch_timing_tests/1.Branch_Basics.nes"
    );
    setup_rom_console_test!(
        test_backward_branch,
        "roms/nes/automated_tests/branch_timing_tests/2.Backward_Branch.nes"
    );
    setup_rom_console_test!(
        test_forward_branch,
        "roms/nes/automated_tests/branch_timing_tests/3.Forward_Branch.nes"
    );

    // cpu_dummy_reads
    setup_rom_console_test!(
        test_cpu_dummy_reads,
        "roms/nes/automated_tests/cpu_dummy_reads/cpu_dummy_reads.nes"
    );

    // cpu_dummy_writes
    setup_rom_test!(
        test_cpu_dummy_writes_oam,
        "roms/nes/automated_tests/cpu_dummy_writes/cpu_dummy_writes_oam.nes"
    );
    setup_rom_test!(
        test_cpu_dummy_writes_ppumem,
        "roms/nes/automated_tests/cpu_dummy_writes/cpu_dummy_writes_ppumem.nes"
    );

    // cpu_exec_space
    setup_rom_test!(
        test_cpu_exec_space_ppuio,
        "roms/nes/automated_tests/cpu_exec_space/test_cpu_exec_space_ppuio.nes"
    );
    setup_rom_test!(
        test_cpu_exec_space_apu,
        "roms/nes/automated_tests/cpu_exec_space/test_cpu_exec_space_apu.nes"
    );

    // cpu_interrupts_v2
    // setup_rom_test!(
    //     test_cpu_interrupts_v2_cpu_interrupts,
    //     "roms/nes/automated_tests/cpu_interrupts_v2/cpu_interrupts.nes"
    // );
    setup_rom_test!(
        test_cpu_interrupts_v2_cli_latency,
        "roms/nes/automated_tests/cpu_interrupts_v2/rom_singles/1-cli_latency.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_nmi_and_brk,
        "roms/nes/automated_tests/cpu_interrupts_v2/rom_singles/2-nmi_and_brk.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_nmi_and_irq,
        "roms/nes/automated_tests/cpu_interrupts_v2/rom_singles/3-nmi_and_irq.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_irq_and_dma,
        "roms/nes/automated_tests/cpu_interrupts_v2/rom_singles/4-irq_and_dma.nes"
    );
    setup_rom_test!(
        test_cpu_interrupts_v2_branch_delays_irq,
        "roms/nes/automated_tests/cpu_interrupts_v2/rom_singles/5-branch_delays_irq.nes"
    );

    // cpu_reset
    setup_rom_test!(
        test_cpu_reset_ram_after_reset,
        "roms/nes/automated_tests/cpu_reset/ram_after_reset.nes"
    );
    setup_rom_test!(
        test_cpu_reset_reset_registers,
        "roms/nes/automated_tests/cpu_reset/registers.nes"
    );

    // cpu_timing_test6
    setup_rom_console_test!(
        test_cpu_timing_test6,
        "roms/nes/automated_tests/cpu_timing_test6/cpu_timing_test.nes"
    );

    #[test]
    fn test_dma_sync_test_v2() {
        let rom_path = "roms/nes/automated_tests/dma_sync_test_v2/dma_sync_test.nes";
        let rom_data = fs::read(rom_path).expect("DMA Sync Test v2 ROM should load");
        let cartridge = load_test_cartridge(&rom_data, rom_path);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            deterministic_config(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        run_nes_for_frames(&mut nes, 300);

        let binding = nes.ppu();
        let mut ppu = binding.borrow_mut();
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        let palette_data = ppu.read_data();
        // Verify that the palette data read is 0x0F (black)
        assert_eq!(palette_data, 0x0F);
    }

    #[test]
    fn test_dma_sync_test_v2_simulate_failure() {
        let rom_path = "roms/nes/automated_tests/dma_sync_test_v2/dma_sync_test.nes";
        let rom_data = fs::read(rom_path).expect("DMA Sync Test v2 ROM should load");
        let cartridge = load_test_cartridge(&rom_data, rom_path);
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            deterministic_config(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);
        run_nes_for_frames(&mut nes, 150);
        // Halfway in, simulate a right button press
        // This causes the test to fail as the DMA timing is now off
        nes.set_button(1, Button::Right, true);
        run_nes_for_frames(&mut nes, 150);

        let binding = nes.ppu();
        let mut ppu = binding.borrow_mut();
        ppu.write_address(0x3F, false);
        ppu.write_address(0x00, false);
        let palette_data = ppu.read_data();
        // Verify that the palette data read is 0x30 (white)
        assert_eq!(palette_data, 0x30);
    }

    // dmc_dma_during_read4
    setup_rom_console_crc_test!(
        test_dmc_dma_during_read4_2007_read,
        "roms/nes/automated_tests/dmc_dma_during_read4/dma_2007_read.nes",
        300,
        &[0x159A7A8F, 0x5E3DF9C4]
    );
    setup_rom_console_test!(
        test_dmc_dma_during_read4_2007_write,
        "roms/nes/automated_tests/dmc_dma_during_read4/dma_2007_write.nes"
    );
    setup_rom_console_test!(
        test_dmc_dma_during_read4_4016_read,
        "roms/nes/automated_tests/dmc_dma_during_read4/dma_4016_read.nes"
    );
    setup_rom_console_crc_test!(
        test_dmc_dma_during_read4_double_2007_read,
        "roms/nes/automated_tests/dmc_dma_during_read4/double_2007_read.nes",
        300,
        &[0xF018C287, 0xD84F6815] //CRC1 - Mesen, loopyNES, etc., CRC2 - Nintendulator, FCEUX
    );
    setup_rom_console_test!(
        test_dmc_dma_during_read4_read_write_2007,
        "roms/nes/automated_tests/dmc_dma_during_read4/read_write_2007.nes"
    );

    // dpcmletterbox
    #[test]
    fn test_dpcmletterbox() {
        let rom_path = "roms/nes/automated_tests/dpcmletterbox/dpcmletterbox.nes";
        let rom_data = fs::read(rom_path).expect("dpcmletterbox ROM should load");
        let cartridge = load_test_cartridge(&rom_data, rom_path);

        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            deterministic_config(),
        ));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        run_nes_for_frames(&mut nes, 60);

        let crc = nes.get_screen_buffer().crc32();
        // Golden CRC generated from a visually verified correct frame capture using --trace-ppu=1
        assert_eq!(crc, 0x2813_2E95, "unexpected frame CRC for dpcmletterbox");
    }

    // instr_misc
    // setup_rom_test!(
    //     test_instr_misc,
    //     "roms/nes/automated_tests/instr_misc/instr_misc.nes"
    // );
    setup_rom_test!(
        test_instr_misc_01,
        "roms/nes/automated_tests/instr_misc/rom_singles/01-abs_x_wrap.nes"
    );
    setup_rom_test!(
        test_instr_misc_02,
        "roms/nes/automated_tests/instr_misc/rom_singles/02-branch_wrap.nes"
    );
    setup_rom_test!(
        test_instr_misc_03,
        "roms/nes/automated_tests/instr_misc/rom_singles/03-dummy_reads.nes"
    );
    setup_rom_test!(
        test_instr_misc_04,
        "roms/nes/automated_tests/instr_misc/rom_singles/04-dummy_reads_apu.nes"
    );

    // test_instr_v3
    // setup_rom_test!(
    //     test_instr_v3_all_instrs,
    //     "roms/nes/automated_tests/instr_test-v3/all_instrs.nes"
    // );
    // setup_rom_test!(
    //     test_instr_v3_official_only,
    //     "roms/nes/automated_tests/instr_test-v3/official_only.nes"
    // );
    setup_rom_test!(
        test_instr_v3_01_implied,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/01-implied.nes"
    );
    setup_rom_test!(
        test_instr_v3_02_immediate,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/02-immediate.nes"
    );
    setup_rom_test!(
        test_instr_v3_03_zero_page,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/03-zero_page.nes"
    );
    setup_rom_test!(
        test_instr_v3_04_zp_xy,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/04-zp_xy.nes"
    );
    setup_rom_test!(
        test_instr_v3_05_absolute,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/05-absolute.nes"
    );
    setup_rom_test!(
        test_instr_v3_06_abs_xy,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/06-abs_xy.nes"
    );
    setup_rom_test!(
        test_instr_v3_07_ind_x,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/07-ind_x.nes"
    );
    setup_rom_test!(
        test_instr_v3_08_ind_y,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/08-ind_y.nes"
    );
    setup_rom_test!(
        test_instr_v3_09_branches,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/09-branches.nes"
    );
    setup_rom_test!(
        test_instr_v3_10_stack,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/10-stack.nes"
    );
    setup_rom_test!(
        test_instr_v3_11_jmp_jsr,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/11-jmp_jsr.nes"
    );
    setup_rom_test!(
        test_instr_v3_12_rts,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/12-rts.nes"
    );
    setup_rom_test!(
        test_instr_v3_13_rti,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/13-rti.nes"
    );
    setup_rom_test!(
        test_instr_v3_14_brk,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/14-brk.nes"
    );
    setup_rom_test!(
        test_instr_v3_15_special,
        "roms/nes/automated_tests/instr_test-v3/rom_singles/15-special.nes"
    );

    // test_instr_v5
    // setup_rom_test!(
    //     test_instr_v5_all_instrs,
    //     "roms/nes/automated_tests/instr_test-v5/all_instrs.nes"
    // );
    // setup_rom_test!(
    //     test_instr_v5_official_only,
    //     "roms/nes/automated_tests/instr_test-v5/official_only.nes"
    // );
    setup_rom_test!(
        test_instr_v5_01_basics,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/01-basics.nes"
    );
    setup_rom_test!(
        test_instr_v5_02_implied,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/02-implied.nes"
    );
    setup_rom_test!(
        test_instr_v5_03_immediate,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/03-immediate.nes"
    );
    setup_rom_test!(
        test_instr_v5_04_zero_page,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/04-zero_page.nes"
    );
    setup_rom_test!(
        test_instr_v5_05_zp_xy,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/05-zp_xy.nes"
    );
    setup_rom_test!(
        test_instr_v5_06_absolute,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/06-absolute.nes"
    );
    setup_rom_test!(
        test_instr_v5_07_abs_xy,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/07-abs_xy.nes"
    );
    setup_rom_test!(
        test_instr_v5_08_ind_x,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/08-ind_x.nes"
    );
    setup_rom_test!(
        test_instr_v5_09_ind_y,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/09-ind_y.nes"
    );
    setup_rom_test!(
        test_instr_v5_10_branches,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/10-branches.nes"
    );
    setup_rom_test!(
        test_instr_v5_11_stack,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/11-stack.nes"
    );
    setup_rom_test!(
        test_instr_v5_12_jmp_jsr,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/12-jmp_jsr.nes"
    );
    setup_rom_test!(
        test_instr_v5_13_rts,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/13-rts.nes"
    );
    setup_rom_test!(
        test_instr_v5_14_rti,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/14-rti.nes"
    );
    setup_rom_test!(
        test_instr_v5_15_brk,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/15-brk.nes"
    );
    setup_rom_test!(
        test_instr_v5_16_special,
        "roms/nes/automated_tests/instr_test-v5/rom_singles/16-special.nes"
    );

    // instr_timing
    // setup_rom_test!(
    //     test_instr_timing,
    //     "roms/nes/automated_tests/instr_timing/instr_timing.nes"
    // );
    setup_rom_test!(
        test_instr_timing_01,
        "roms/nes/automated_tests/instr_timing/rom_singles/1-instr_timing.nes"
    );
    setup_rom_test!(
        test_instr_timing_02,
        "roms/nes/automated_tests/instr_timing/rom_singles/2-branch_timing.nes"
    );

    // nestest
    #[test]
    fn test_nestest() {
        use crate::platform::debugging::Tracing;

        // Load the golden log from file
        let golden_log = fs::read_to_string("roms/nes/automated_tests/nestest/nestest.log")
            .expect("Failed to load nestest.log - make sure roms/automated_tests/nestest/nestest.log exists");

        // Load the nestest ROM
        let rom_data =
            fs::read("roms/nes/automated_tests/nestest/nestest.nes").expect("Failed to load ROM");
        let cartridge =
            load_test_cartridge(&rom_data, "roms/nes/automated_tests/nestest/nestest.nes");

        // Create NES and insert cartridge
        let mut nes = Nes::new(crate::platform::app_context::AppContext::new_with_config(
            deterministic_config(),
        ));
        nes.insert_cartridge(cartridge);
        nes.cpu_mut().reset(false);
        // nestest automated test starts execution at $C000 (not reset vector $C004)
        nes.cpu_mut().set_pc(0xC000);
        // CPU reset takes 7 cycles, manually sync PPU and CPU cycle counters
        // PPU is already a bit ahead here:
        // - Nes::new runs the PPU for 1 cycle (sprite-0 hit timing offset)
        // - Cpu::reset reads the reset vector via bus reads, which advances the PPU further
        //   due to the CPU master-clock phase model.
        // Ensure the PPU reaches exactly 21 cycles (7 * 3) before the first traced instruction.
        let ppu_cycles_needed = 21u64.saturating_sub(nes.ppu().borrow().total_cycles());
        nes.ppu().borrow_mut().run_ppu_cycles(ppu_cycles_needed);

        for line in golden_log.lines() {
            let expected = line.to_string();
            let actual = nes.trace(&Tracing {
                enabled: true,
                cpu: 0,
                ppu: 0,
                apu: 0,
                mapper: 0,
                nestest: true,
                ..Tracing::default()
            });

            assert_eq!(expected, actual);
            nes.run_cpu_tick();
        }
    }

    // sprdma_and_dmc_dma
    setup_rom_test!(
        test_sprdma_and_dmc_dma,
        "roms/nes/automated_tests/sprdma_and_dmc_dma/sprdma_and_dmc_dma.nes"
    );
    setup_rom_test!(
        test_sprdma_and_dmc_dma_512,
        "roms/nes/automated_tests/sprdma_and_dmc_dma/sprdma_and_dmc_dma_512.nes"
    );
}
