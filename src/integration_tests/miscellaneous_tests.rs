#[cfg(test)]
mod tests {
    use crate::setup_rom_console_test;

    // TODO allpads-r9

    // TODO read_joy3
    // setup_rom_console_test!(
    //     test_read_joy3_count_errors,
    //     "roms/automated_tests/read_joy3/count_errors.nes"
    // );

    setup_rom_console_test!(
        test_read_joy3_test_buttons,
        "roms/automated_tests/read_joy3/test_buttons.nes"
    );
}
