#[cfg(test)]
mod tests {
    use std::fs;

    use crate::cartridge::Cartridge;
    use crate::console::{Nes, TvSystem};
    use crate::integration_tests::rom_test_runner::tests::run_nes_for_frames;
    use crate::setup_rom_test;

    // TODO bntest

    // TODO FME7

    // TODO holydiver

    // TODO Homebrew Mappers

    // MMC3
    setup_rom_test!(
        test_mmc3_test_2_1_clocking,
        "roms/automated_tests/mmc3_test_2/rom_singles/1-clocking.nes"
    );
    setup_rom_test!(
        test_mmc3_test_2_2_details,
        "roms/automated_tests/mmc3_test_2/rom_singles/2-details.nes"
    );
    setup_rom_test!(
        test_mmc3_test_2_3_a12_clocking,
        "roms/automated_tests/mmc3_test_2/rom_singles/3-A12_clocking.nes"
    );
    setup_rom_test!(
        test_mmc3_test_2_4_scanline_timing,
        "roms/automated_tests/mmc3_test_2/rom_singles/4-scanline_timing.nes"
    );
    setup_rom_test!(
        test_mmc3_test_2_5_mmc3,
        "roms/automated_tests/mmc3_test_2/rom_singles/5-MMC3.nes"
    );
    setup_rom_test!(
        test_mmc3_test_2_6_mmc3_alt,
        "roms/automated_tests/mmc3_test_2/rom_singles/6-MMC3_alt.nes"
    );
    // TODO mmc3bigchrram

    // MMC5
    #[test]
    fn test_mmc5_exram_crc_sequence() {
        let rom_path = "roms/automated_tests/exram/mmc5exram.nes";
        let rom_data = fs::read(rom_path).expect("mmc5exram ROM should load");
        let cartridge = Cartridge::new(&rom_data).expect("mmc5exram ROM should parse");

        let mut nes = Nes::new(TvSystem::Ntsc);
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        let expected_crcs = [
            0x90428465, 0x4E2BA407, 0x01ECA2E8, 0x138E5FE2, 0xC7C91CC3, 0xEFBFD0D1, 0xD57CD303,
        ];
        for (index, expected_crc) in expected_crcs.iter().enumerate() {
            run_nes_for_frames(&mut nes, 60);
            let crc = nes.get_screen_buffer().crc32();
            assert_eq!(
                crc,
                *expected_crc,
                "unexpected frame CRC at checkpoint {} for mmc5exram",
                index + 1
            );
        }
    }
    // TODO mmc5test_v2

    // TODO Submappers

    // TODO VRC6
}
