#[cfg(test)]
mod tests {
    use crate::{setup_rom_console_test, setup_rom_test};

    // blargg_nes_cpu_test5
    setup_rom_console_test!(
        test_blargg_nes_cpu_test5_cpu,
        "roms/automated_tests/blargg_nes_cpu_test5/cpu.nes",
        "ALL TESTS COMPLETE"
    );

    setup_rom_console_test!(
        test_blargg_nes_cpu_test5_official,
        "roms/automated_tests/blargg_nes_cpu_test5/official.nes",
        "ALL TESTS COMPLETE"
    );

    // mmc3_irq_tests
    setup_rom_console_test!(
        test_mmc3_irq_tests_1_clocking,
        "roms/automated_tests/mmc3_irq_tests/1.Clocking.nes"
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_2_details,
        "roms/automated_tests/mmc3_irq_tests/2.Details.nes"
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_3_a12_clocking,
        "roms/automated_tests/mmc3_irq_tests/3.A12_clocking.nes"
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_4_scanline_timing,
        "roms/automated_tests/mmc3_irq_tests/4.Scanline_timing.nes"
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_5_rev_a,
        "roms/automated_tests/mmc3_irq_tests/5.MMC3_rev_A.nes"
    );
    setup_rom_console_test!(
        test_mmc3_irq_tests_6_rev_b,
        "roms/automated_tests/mmc3_irq_tests/6.MMC3_rev_B.nes"
    );

    // MMC3
    setup_rom_test!(
        test_mmc3_test_1_clocking,
        "roms/automated_tests/mmc3_test/1-clocking.nes"
    );
    setup_rom_test!(
        test_mmc3_test_2_details,
        "roms/automated_tests/mmc3_test/2-details.nes"
    );
    setup_rom_test!(
        test_mmc3_test_3_a12_clocking,
        "roms/automated_tests/mmc3_test/3-A12_clocking.nes"
    );
    setup_rom_test!(
        test_mmc3_test_4_scanline_timing,
        "roms/automated_tests/mmc3_test/4-scanline_timing.nes"
    );
    setup_rom_test!(
        test_mmc3_test_5_mmc3,
        "roms/automated_tests/mmc3_test/5-MMC3.nes"
    );
    setup_rom_test!(
        test_mmc3_test_6_mmc3_alt,
        "roms/automated_tests/mmc3_test/6-MMC6.nes"
    );

    // nes_instr_test
    setup_rom_test!(
        test_nes_instr_01_implied,
        "roms/automated_tests/nes_instr_test/rom_singles/01-implied.nes"
    );
    setup_rom_test!(
        test_nes_instr_02_immediate,
        "roms/automated_tests/nes_instr_test/rom_singles/02-immediate.nes"
    );
    setup_rom_test!(
        test_nes_instr_03_zero_page,
        "roms/automated_tests/nes_instr_test/rom_singles/03-zero_page.nes"
    );
    setup_rom_test!(
        test_nes_instr_04_zp_xy,
        "roms/automated_tests/nes_instr_test/rom_singles/04-zp_xy.nes"
    );
    setup_rom_test!(
        test_nes_instr_05_absolute,
        "roms/automated_tests/nes_instr_test/rom_singles/05-absolute.nes"
    );
    setup_rom_test!(
        test_nes_instr_06_abs_xy,
        "roms/automated_tests/nes_instr_test/rom_singles/06-abs_xy.nes"
    );
    setup_rom_test!(
        test_nes_instr_07_ind_x,
        "roms/automated_tests/nes_instr_test/rom_singles/07-ind_x.nes"
    );
    setup_rom_test!(
        test_nes_instr_08_ind_y,
        "roms/automated_tests/nes_instr_test/rom_singles/08-ind_y.nes"
    );
    setup_rom_test!(
        test_nes_instr_09_branches,
        "roms/automated_tests/nes_instr_test/rom_singles/09-branches.nes"
    );
    setup_rom_test!(
        test_nes_instr_10_stack,
        "roms/automated_tests/nes_instr_test/rom_singles/10-stack.nes"
    );
    setup_rom_test!(
        test_nes_instr_11_special,
        "roms/automated_tests/nes_instr_test/rom_singles/11-special.nes"
    );

    // sprite_hit_tests_2005
    // These are included even though ppu_sprite_hit_tests are included
    // as e.g. 09.timing-basics.nes found an issue that was not found earlier.
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_01_basics,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/01.basics.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_02_alignment,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/02.alignment.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_03_corners,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/03.corners.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_04_flip,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/04.flip.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_05_left_clip,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/05.left_clip.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_06_right_edge,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/06.right_edge.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_07_screen_bottom,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/07.screen_bottom.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_08_double_height,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/08.double_height.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_09_timing_basics,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/09.timing_basics.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_10_timing_order,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/10.timing_order.nes"
    );
    setup_rom_console_test!(
        test_sprite_hit_tests_2005_10_05_11_edge_timing,
        "roms/automated_tests/sprite_hit_tests_2005.10.05/11.edge_timing.nes"
    );

    // sprite_overflow_tests
    setup_rom_console_test!(
        test_sprite_overflow_tests_1_basics,
        "roms/automated_tests/sprite_overflow_tests/1.Basics.nes"
    );
    setup_rom_console_test!(
        test_sprite_overflow_tests_2_details,
        "roms/automated_tests/sprite_overflow_tests/2.Details.nes"
    );
    setup_rom_console_test!(
        test_sprite_overflow_tests_3_timing,
        "roms/automated_tests/sprite_overflow_tests/3.Timing.nes"
    );
    setup_rom_console_test!(
        test_sprite_overflow_tests_4_obscure,
        "roms/automated_tests/sprite_overflow_tests/4.Obscure.nes"
    );
    setup_rom_console_test!(
        test_sprite_overflow_tests_5_emulator,
        "roms/automated_tests/sprite_overflow_tests/5.Emulator.nes"
    );

    // vbl_nmi_timing
    setup_rom_console_test!(
        test_vbl_nmi_timing_frame_basics,
        "roms/automated_tests/vbl_nmi_timing/1.frame_basics.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_vbl_timing,
        "roms/automated_tests/vbl_nmi_timing/2.vbl_timing.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_even_odd_frames,
        "roms/automated_tests/vbl_nmi_timing/3.even_odd_frames.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_vbl_clear_timing,
        "roms/automated_tests/vbl_nmi_timing/4.vbl_clear_timing.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_nmi_suppression,
        "roms/automated_tests/vbl_nmi_timing/5.nmi_suppression.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_nmi_disable,
        "roms/automated_tests/vbl_nmi_timing/6.nmi_disable.nes"
    );
    setup_rom_console_test!(
        test_vbl_nmi_timing_nmi_timing,
        "roms/automated_tests/vbl_nmi_timing/7.nmi_timing.nes"
    );
}
