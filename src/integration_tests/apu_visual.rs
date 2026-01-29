#[cfg(test)]
mod tests {
    use crate::{setup_rom_console_test, setup_rom_test};

    // apu_reset
    setup_rom_test!(
        test_4015_cleared,
        "roms/automated_tests/apu_reset/4015_cleared.nes"
    );
    setup_rom_test!(
        test_4017_timing,
        "roms/automated_tests/apu_reset/4017_timing.nes"
    );
    setup_rom_test!(
        test_4017_written,
        "roms/automated_tests/apu_reset/4017_written.nes"
    );
    setup_rom_test!(
        test_irq_flag_cleared,
        "roms/automated_tests/apu_reset/irq_flag_cleared.nes"
    );
    setup_rom_test!(
        test_len_ctrs_enabled,
        "roms/automated_tests/apu_reset/len_ctrs_enabled.nes"
    );
    setup_rom_test!(
        test_works_immediately,
        "roms/automated_tests/apu_reset/works_immediately.nes"
    );

    // apu_test
    setup_rom_test!(test_apu_test, "roms/automated_tests/apu_test/apu_test.nes");
    setup_rom_test!(
        test_apu_test_1,
        "roms/automated_tests/apu_test/rom_singles/1-len_ctr.nes"
    );
    setup_rom_test!(
        test_apu_test_2,
        "roms/automated_tests/apu_test/rom_singles/2-len_table.nes"
    );
    setup_rom_test!(
        test_apu_test_3,
        "roms/automated_tests/apu_test/rom_singles/3-irq_flag.nes"
    );
    setup_rom_test!(
        test_apu_test_4,
        "roms/automated_tests/apu_test/rom_singles/4-jitter.nes"
    );
    setup_rom_test!(
        test_apu_test_5,
        "roms/automated_tests/apu_test/rom_singles/5-len_timing.nes"
    );
    setup_rom_test!(
        test_apu_test_6,
        "roms/automated_tests/apu_test/rom_singles/6-irq_flag_timing.nes"
    );
    setup_rom_test!(
        test_apu_test_7,
        "roms/automated_tests/apu_test/rom_singles/7-dmc_basics.nes"
    );
    setup_rom_test!(
        test_apu_test_8,
        "roms/automated_tests/apu_test/rom_singles/8-dmc_rates.nes"
    );

    // blargg_apu_2005.07.30
    setup_rom_console_test!(
        test_blargg_apu_01,
        "roms/automated_tests/blargg_apu_2005.07.30/01.len_ctr.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_02,
        "roms/automated_tests/blargg_apu_2005.07.30/02.len_table.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_03,
        "roms/automated_tests/blargg_apu_2005.07.30/03.irq_flag.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_04,
        "roms/automated_tests/blargg_apu_2005.07.30/04.clock_jitter.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_05,
        "roms/automated_tests/blargg_apu_2005.07.30/05.len_timing_mode0.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_06,
        "roms/automated_tests/blargg_apu_2005.07.30/06.len_timing_mode1.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_07,
        "roms/automated_tests/blargg_apu_2005.07.30/07.irq_flag_timing.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_08,
        "roms/automated_tests/blargg_apu_2005.07.30/08.irq_timing.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_09,
        "roms/automated_tests/blargg_apu_2005.07.30/09.reset_timing.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_10,
        "roms/automated_tests/blargg_apu_2005.07.30/10.len_halt_timing.nes"
    );
    setup_rom_console_test!(
        test_blargg_apu_11,
        "roms/automated_tests/blargg_apu_2005.07.30/11.len_reload_timing.nes"
    );

    // TODO pal_apu_tests

    // TODO test_apu_2
}
