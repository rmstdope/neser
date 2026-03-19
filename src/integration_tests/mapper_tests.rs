#[cfg(test)]
mod tests {
    use std::fs;

    use crate::cartridge::Cartridge;
    use crate::console::{Config, Nes, RamInitMode};
    use crate::integration_tests::rom_test_runner::tests::run_nes_for_frames;
    use crate::{setup_rom_console_test, setup_rom_crc_test, setup_rom_test};

    // bntest — BxROM (mapper 34) and AxROM (mapper 7) function tests
    // Verified output: PRG banks readable as hex digits, nametable mirroring pattern
    setup_rom_crc_test!(
        test_bntest_h,
        "roms/automated_tests/bntest/bntest_h.nes",
        [(300, 3291074823)]
    );
    setup_rom_crc_test!(
        test_bntest_v,
        "roms/automated_tests/bntest/bntest_v.nes",
        [(300, 4160665903)]
    );
    setup_rom_crc_test!(
        test_bntest_aorom,
        "roms/automated_tests/bntest/bntest_aorom.nes",
        [(300, 1193424937)]
    );

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
        let cartridge =
            Cartridge::load_from_file(&rom_data, rom_path, crate::app_context::AppContext::new())
                .expect("mmc5exram ROM should parse");

        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(
            Config::default(),
        ));
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

    fn build_mapper95_test_rom() -> Vec<u8> {
        let prg_rom_banks_16k = 8u8;
        let chr_rom_banks_8k = 1u8;

        let mut rom = Vec::new();
        rom.extend_from_slice(b"NES\x1A");
        rom.push(prg_rom_banks_16k);
        rom.push(chr_rom_banks_8k);
        rom.push(0xF0);
        rom.push(0x50);
        rom.extend_from_slice(&[0u8; 8]);

        let prg_size = prg_rom_banks_16k as usize * 16 * 1024;
        let mut prg = vec![0xEA; prg_size];
        let program: [u8; 33] = [
            0xA9, 0x06, 0x8D, 0x00, 0x80, 0xA9, 0x01, 0x8D, 0x01, 0x80, 0xA9, 0x07, 0x8D, 0x00,
            0x80, 0xA9, 0x02, 0x8D, 0x01, 0x80, 0xA9, 0x00, 0x8D, 0x00, 0x80, 0xA9, 0x20, 0x8D,
            0x01, 0x80, 0x4C, 0x1E, 0x80,
        ];
        prg[0..program.len()].copy_from_slice(&program);

        let reset_vector = 0x8000u16;
        let vector_base = prg_size - 6;
        prg[vector_base..vector_base + 2].copy_from_slice(&reset_vector.to_le_bytes());
        prg[vector_base + 2..vector_base + 4].copy_from_slice(&reset_vector.to_le_bytes());
        prg[vector_base + 4..vector_base + 6].copy_from_slice(&reset_vector.to_le_bytes());

        rom.extend_from_slice(&prg);
        rom.extend(std::iter::repeat_n(
            0u8,
            chr_rom_banks_8k as usize * 8 * 1024,
        ));

        rom
    }

    #[test]
    fn test_mapper95_in_memory_rom_crc_sequence() {
        let rom_path = "in-memory/mapper95-test.nes";
        let rom_data = build_mapper95_test_rom();
        let cartridge =
            Cartridge::load_from_file(&rom_data, rom_path, crate::app_context::AppContext::new())
                .expect("in-memory mapper95 ROM should parse");

        let config = Config {
            ram_init_mode: RamInitMode::Zero,
            ..Default::default()
        };
        let mut nes = Nes::new(crate::app_context::AppContext::new_with_config(config));
        nes.insert_cartridge(cartridge);
        nes.reset(false);

        let expected_crcs = [0xE328388E, 0xE328388E, 0xE328388E];
        for (index, expected_crc) in expected_crcs.iter().enumerate() {
            run_nes_for_frames(&mut nes, 60);
            let crc = nes.get_screen_buffer().crc32();
            assert_eq!(
                crc,
                *expected_crc,
                "unexpected frame CRC at checkpoint {} for in-memory mapper95 ROM",
                index + 1
            );
        }
    }

    #[test]
    fn test_complex_mapper_files_document_limitations() {
        let mapper_registry = fs::read_to_string("src/cartridge/mapper.rs")
            .expect("mapper registry source should be readable");

        let mut mapper_files: Vec<String> = mapper_registry
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if !trimmed.starts_with("use super::") || !trimmed.ends_with("Mapper;") {
                    return None;
                }

                let module_path = trimmed.strip_prefix("use super::")?;
                let module = module_path.split("::").next()?;
                Some(format!("src/cartridge/{}.rs", module))
            })
            .collect();

        mapper_files.sort();
        mapper_files.dedup();

        assert!(
            !mapper_files.is_empty(),
            "expected mapper module list to be non-empty"
        );

        for mapper_file in mapper_files {
            let source = fs::read_to_string(&mapper_file)
                .unwrap_or_else(|error| panic!("failed to read {}: {}", mapper_file, error));

            let has_limitations_docs =
                source.contains("Known Limitations") || source.contains("Known Issues");

            assert!(
                has_limitations_docs,
                "{} is missing a searchable limitations section",
                mapper_file
            );
        }
    }
    // TODO mmc5test_v2

    // TODO Submappers

    // ================================================================
    // VRC6 community test suite (vrc6test)
    // Tests CHR banking and nametable mirroring for Mapper 24 (VRC6a) and Mapper 26 (VRC6b).
    // Reports pass/fail via console text: "All Tests Passed!"
    // ================================================================

    setup_rom_console_test!(
        test_vrc6test24,
        "roms/automated_tests/vrc6test/vrc6test24.nes",
        "ALL TESTS PASSED!"
    );
    setup_rom_console_test!(
        test_vrc6test26,
        "roms/automated_tests/vrc6test/vrc6test26.nes",
        "ALL TESTS PASSED!"
    );

    // ================================================================
    // Mapper Verification Suite — custom test ROMs
    // Tests PRG/CHR banking, nametable mirroring, IRQ, PRG-RAM,
    // and bus conflicts across mappers 0,1,2,3,4,7 with submapper variants.
    // ================================================================
    // Combined Mapper Verification ROMs (all tests per mapper/submapper)
    // ================================================================

    // ================================================================
    // Mapper 0 (NROM), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m000_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m000.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m000_0_combined,
        "roms/automated_tests/mapper_verification/bin/m000.0.nes"
    );

    // ================================================================
    // Mapper 1 (MMC1), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m001_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m001.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m001_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m001.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m001_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m001.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m001_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m001.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m001_0_combined,
        "roms/automated_tests/mapper_verification/bin/m001.0.nes"
    );

    // ================================================================
    // Mapper 1, Submapper 5 (Fixed PRG)
    // ================================================================

    setup_rom_test!(
        test_mv_m001_5_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m001.5_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m001_5_combined,
        "roms/automated_tests/mapper_verification/bin/m001.5.nes"
    );

    // ================================================================
    // Mapper 2 (UxROM), Submapper 0 (Bus Conflicts)
    // ================================================================

    setup_rom_test!(
        test_mv_m002_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m002.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m002_0_bus_conflicts,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m002.0_bus_conflicts.nes"
    );
    setup_rom_test!(
        test_mv_m002_0_combined,
        "roms/automated_tests/mapper_verification/bin/m002.0.nes"
    );

    // ================================================================
    // Mapper 2, Submapper 2 (No Bus Conflicts)
    // ================================================================

    setup_rom_test!(
        test_mv_m002_2_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m002.2_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m002_2_combined,
        "roms/automated_tests/mapper_verification/bin/m002.2.nes"
    );

    // ================================================================
    // Mapper 3 (CNROM), Submapper 0 (Bus Conflicts)
    // ================================================================

    setup_rom_test!(
        test_mv_m003_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m003.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m003_0_bus_conflicts,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m003.0_bus_conflicts.nes"
    );
    setup_rom_test!(
        test_mv_m003_0_combined,
        "roms/automated_tests/mapper_verification/bin/m003.0.nes"
    );

    // ================================================================
    // Mapper 3, Submapper 1 (No Bus Conflicts)
    // ================================================================

    setup_rom_test!(
        test_mv_m003_1_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m003.1_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m003_1_combined,
        "roms/automated_tests/mapper_verification/bin/m003.1.nes"
    );

    // ================================================================
    // Mapper 4 (MMC3), Submapper 0 (Sharp IRQ)
    // ================================================================

    setup_rom_test!(
        test_mv_m004_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m004.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m004_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m004.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m004_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m004.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m004_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m004.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m004_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m004.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m004_0_write_protect,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m004.0_write_protect.nes"
    );
    setup_rom_test!(
        test_mv_m004_0_combined,
        "roms/automated_tests/mapper_verification/bin/m004.0.nes"
    );

    // ================================================================
    // Mapper 4, Submapper 1 (NEC IRQ)
    // ================================================================

    setup_rom_test!(
        test_mv_m004_1_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m004.1_irq.nes"
    );
    setup_rom_test!(
        test_mv_m004_1_write_protect,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m004.1_write_protect.nes"
    );
    setup_rom_test!(
        test_mv_m004_1_combined,
        "roms/automated_tests/mapper_verification/bin/m004.1.nes"
    );

    // ================================================================
    // Mapper 5 (MMC5), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m005_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m005_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m005_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m005_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m005_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m005_0_write_protect,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_write_protect.nes"
    );
    setup_rom_test!(
        test_mv_m005_0_multiplier,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_multiplier.nes"
    );
    setup_rom_test!(
        test_mv_m005_0_exram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_exram.nes"
    );
    setup_rom_test!(
        test_mv_m005_0_combined,
        "roms/automated_tests/mapper_verification/bin/m005.0.nes"
    );

    // ================================================================
    // Mapper 6 (Front Fareast Magic Card), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m006_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m006.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m006_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m006.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m006_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m006.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m006_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m006.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m006_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m006.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m006_0_combined,
        "roms/automated_tests/mapper_verification/bin/m006.0.nes"
    );
    setup_rom_test!(
        test_mv_m006_0_modes,
        "roms/automated_tests/mapper_verification/bin/m006.0_modes.nes"
    );

    // ================================================================
    // Mapper 7 (AxROM), Submapper 0 (Bus Conflicts)
    // ================================================================

    setup_rom_test!(
        test_mv_m007_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m007.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m007_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m007.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m007_0_combined,
        "roms/automated_tests/mapper_verification/bin/m007.0.nes"
    );

    // ================================================================
    // Mapper 7, Submapper 1 (No Bus Conflicts)
    // ================================================================

    setup_rom_test!(
        test_mv_m007_1_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m007.1_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m007_1_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m007.1_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m007_1_combined,
        "roms/automated_tests/mapper_verification/bin/m007.1.nes"
    );

    // ================================================================
    // Mapper 8 (SMC GNROM mode 4), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m008_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m008.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m008_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m008.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m008_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m008.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m008_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m008.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m008_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m008.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m008_0_combined,
        "roms/automated_tests/mapper_verification/bin/m008.0.nes"
    );

    // ================================================================
    // Mapper 9 (MMC2), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m009_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m009.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m009_0_chr_latch,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m009.0_chr_latch.nes"
    );
    setup_rom_test!(
        test_mv_m009_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m009.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m009_0_combined,
        "roms/automated_tests/mapper_verification/bin/m009.0.nes"
    );

    // ================================================================
    // Mapper 10 (MMC4), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m010_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m010.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m010_0_chr_latch,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m010.0_chr_latch.nes"
    );
    setup_rom_test!(
        test_mv_m010_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m010.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m010_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m010.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m010_0_combined,
        "roms/automated_tests/mapper_verification/bin/m010.0.nes"
    );

    // ================================================================
    // Mapper 11 (Color Dreams), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m011_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m011.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m011_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m011.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m011_0_combined,
        "roms/automated_tests/mapper_verification/bin/m011.0.nes"
    );

    // ================================================================
    // Mapper 11 (Color Dreams), Submapper 1 — No Bus Conflicts
    // ================================================================

    setup_rom_test!(
        test_mv_m011_1_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m011.1_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m011_1_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m011.1_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m011_1_combined,
        "roms/automated_tests/mapper_verification/bin/m011.1.nes"
    );

    // ================================================================
    // Mapper 12 (SL-5020B / MMC3 + outer CHR), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m012_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m012.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m012_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m012.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m012_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m012.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m012_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m012.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m012_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m012.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m012_0_write_protect,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m012.0_write_protect.nes"
    );
    setup_rom_test!(
        test_mv_m012_0_combined,
        "roms/automated_tests/mapper_verification/bin/m012.0.nes"
    );

    // ================================================================
    // Mapper 13 (CPROM), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m013_0_chr_ram_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m013.0_chr_ram_banking.nes"
    );
    setup_rom_test!(
        test_mv_m013_0_combined,
        "roms/automated_tests/mapper_verification/bin/m013.0.nes"
    );

    // ================================================================
    // Mapper 14 (SL-1632 / MMC3+VRC2 hybrid), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m014_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m014.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m014_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m014.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m014_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m014.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m014_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m014.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m014_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m014.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m014_0_write_protect,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m014.0_write_protect.nes"
    );
    setup_rom_test!(
        test_mv_m014_0_vrc2,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m014.0_vrc2.nes"
    );
    setup_rom_test!(
        test_mv_m014_0_combined,
        "roms/automated_tests/mapper_verification/bin/m014.0.nes"
    );

    // ================================================================
    // Mapper 15 (K-1029 multicart), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m015_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m015.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m015_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m015.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m015_0_combined,
        "roms/automated_tests/mapper_verification/bin/m015.0.nes"
    );
    setup_rom_test!(
        test_mv_m015_0_mode0,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m015.0_mode0.nes"
    );
    setup_rom_test!(
        test_mv_m015_0_mode2,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m015.0_mode2.nes"
    );
    setup_rom_test!(
        test_mv_m015_0_mode3,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m015.0_mode3.nes"
    );
    setup_rom_test!(
        test_mv_m015_0_modes_combined,
        "roms/automated_tests/mapper_verification/bin/m015.0_modes.nes"
    );

    // ================================================================
    // Mapper 16 (Bandai FCG), Submapper 4 (FCG-1/2)
    // ================================================================

    setup_rom_test!(
        test_mv_m016_4_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m016.4_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m016_4_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m016.4_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m016_4_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m016.4_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m016_4_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m016.4_irq.nes"
    );
    setup_rom_test!(
        test_mv_m016_4_combined,
        "roms/automated_tests/mapper_verification/bin/m016.4.nes"
    );

    // ================================================================
    // Mapper 16 (Bandai FCG), Submapper 5 (LZ93D50)
    // ================================================================

    setup_rom_test!(
        test_mv_m016_5_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m016.5_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m016_5_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m016.5_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m016_5_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m016.5_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m016_5_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m016.5_irq.nes"
    );
    setup_rom_test!(
        test_mv_m016_5_combined,
        "roms/automated_tests/mapper_verification/bin/m016.5.nes"
    );

    // ================================================================
    // Mapper 18 (Jaleco SS 88006), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m018_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m018.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m018_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m018.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m018_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m018.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m018_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m018.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m018_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m018.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m018_0_combined,
        "roms/automated_tests/mapper_verification/bin/m018.0.nes"
    );

    // ================================================================
    // Mapper 19 (Namco 129/163), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m019_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m019.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m019_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m019.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m019_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m019.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m019_0_nt_from_chr,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m019.0_nt_from_chr.nes"
    );
    setup_rom_test!(
        test_mv_m019_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m019.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m019_0_combined,
        "roms/automated_tests/mapper_verification/bin/m019.0.nes"
    );

    // ================================================================
    // Mapper 19 (Namco 129/163), Submapper 2 (no expansion audio)
    // ================================================================

    setup_rom_test!(
        test_mv_m019_2_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m019.2_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m019_2_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m019.2_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m019_2_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m019.2_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m019_2_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m019.2_irq.nes"
    );
    setup_rom_test!(
        test_mv_m019_2_combined,
        "roms/automated_tests/mapper_verification/bin/m019.2.nes"
    );

    // ================================================================
    // Mapper 21 (VRC4a/VRC4c), Submapper 1 (VRC4a)
    // ================================================================

    setup_rom_test!(
        test_mv_m021_1_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m021.1_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m021_1_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m021.1_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m021_1_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m021.1_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m021_1_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m021.1_irq.nes"
    );
    setup_rom_test!(
        test_mv_m021_1_combined,
        "roms/automated_tests/mapper_verification/bin/m021.1.nes"
    );

    // ================================================================
    // Mapper 21 (VRC4a/VRC4c), Submapper 2 (VRC4c)
    // ================================================================

    setup_rom_test!(
        test_mv_m021_2_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m021.2_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m021_2_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m021.2_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m021_2_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m021.2_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m021_2_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m021.2_irq.nes"
    );
    setup_rom_test!(
        test_mv_m021_2_combined,
        "roms/automated_tests/mapper_verification/bin/m021.2.nes"
    );

    // ================================================================
    // Mapper 22 (VRC2a), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m022_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m022.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m022_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m022.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m022_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m022.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m022_0_combined,
        "roms/automated_tests/mapper_verification/bin/m022.0.nes"
    );

    // ================================================================
    // Mapper 23 (VRC4e/VRC4f/VRC2b), Submapper 1 (VRC4f)
    // ================================================================

    setup_rom_test!(
        test_mv_m023_1_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.1_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m023_1_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.1_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m023_1_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.1_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m023_1_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.1_irq.nes"
    );
    setup_rom_test!(
        test_mv_m023_1_combined,
        "roms/automated_tests/mapper_verification/bin/m023.1.nes"
    );

    // ================================================================
    // Mapper 23 (VRC4e/VRC4f/VRC2b), Submapper 2 (VRC4e)
    // ================================================================

    setup_rom_test!(
        test_mv_m023_2_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.2_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m023_2_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.2_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m023_2_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.2_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m023_2_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.2_irq.nes"
    );
    setup_rom_test!(
        test_mv_m023_2_combined,
        "roms/automated_tests/mapper_verification/bin/m023.2.nes"
    );

    // ================================================================
    // Mapper 23 (VRC4e/VRC4f/VRC2b), Submapper 3 (VRC2b)
    // ================================================================

    setup_rom_test!(
        test_mv_m023_3_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.3_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m023_3_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.3_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m023_3_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m023.3_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m023_3_combined,
        "roms/automated_tests/mapper_verification/bin/m023.3.nes"
    );

    // ================================================================
    // Mapper 24 (VRC6a), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m024_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m024.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m024_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m024.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m024_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m024.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m024_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m024.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m024_0_combined,
        "roms/automated_tests/mapper_verification/bin/m024.0.nes"
    );

    // ================================================================
    // Mapper 25 (VRC4b/VRC4d/VRC2c), Submapper 1 (VRC4b)
    // ================================================================

    setup_rom_test!(
        test_mv_m025_1_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.1_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m025_1_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.1_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m025_1_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.1_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m025_1_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.1_irq.nes"
    );
    setup_rom_test!(
        test_mv_m025_1_combined,
        "roms/automated_tests/mapper_verification/bin/m025.1.nes"
    );

    // ================================================================
    // Mapper 25 (VRC4b/VRC4d/VRC2c), Submapper 2 (VRC4d)
    // ================================================================

    setup_rom_test!(
        test_mv_m025_2_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.2_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m025_2_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.2_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m025_2_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.2_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m025_2_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.2_irq.nes"
    );
    setup_rom_test!(
        test_mv_m025_2_combined,
        "roms/automated_tests/mapper_verification/bin/m025.2.nes"
    );

    // ================================================================
    // Mapper 25 (VRC4b/VRC4d/VRC2c), Submapper 3 (VRC2c)
    // ================================================================

    setup_rom_test!(
        test_mv_m025_3_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.3_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m025_3_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.3_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m025_3_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m025.3_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m025_3_combined,
        "roms/automated_tests/mapper_verification/bin/m025.3.nes"
    );

    // ================================================================
    // Mapper 26 (VRC6b), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m026_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m026.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m026_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m026.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m026_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m026.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m026_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m026.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m026_0_combined,
        "roms/automated_tests/mapper_verification/bin/m026.0.nes"
    );

    // ================================================================
    // Mapper 28 (Action 53), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m028_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m028.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m028_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m028.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m028_0_combined,
        "roms/automated_tests/mapper_verification/bin/m028.0.nes"
    );

    // ================================================================
    // Mapper 29 (Sealie Computing), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m029_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m029.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m029_0_chr_ram_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m029.0_chr_ram_banking.nes"
    );
    setup_rom_test!(
        test_mv_m029_0_combined,
        "roms/automated_tests/mapper_verification/bin/m029.0.nes"
    );

    // ================================================================
    // Mapper 30 (UNROM 512), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m030_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m030.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m030_0_chr_ram_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m030.0_chr_ram_banking.nes"
    );
    setup_rom_test!(
        test_mv_m030_0_bus_conflicts,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m030.0_bus_conflicts.nes"
    );
    setup_rom_test!(
        test_mv_m030_0_combined,
        "roms/automated_tests/mapper_verification/bin/m030.0.nes"
    );

    // ================================================================
    // Mapper 30 (UNROM 512), Submapper 1 (flashable, no bus conflicts)
    // ================================================================

    setup_rom_test!(
        test_mv_m030_1_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m030.1_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m030_1_chr_ram_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m030.1_chr_ram_banking.nes"
    );
    setup_rom_test!(
        test_mv_m030_1_combined,
        "roms/automated_tests/mapper_verification/bin/m030.1.nes"
    );

    // ================================================================
    // Mapper 30 (UNROM 512), Submapper 2 (1-screen switchable)
    // ================================================================

    setup_rom_test!(
        test_mv_m030_2_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m030.2_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m030_2_chr_ram_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m030.2_chr_ram_banking.nes"
    );
    setup_rom_test!(
        test_mv_m030_2_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m030.2_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m030_2_combined,
        "roms/automated_tests/mapper_verification/bin/m030.2.nes"
    );

    // ================================================================
    // Mapper 31 (NSF subset), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m031_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m031.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m031_0_combined,
        "roms/automated_tests/mapper_verification/bin/m031.0.nes"
    );

    // ================================================================
    // Mapper 32 (Irem G-101), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m032_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m032.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m032_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m032.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m032_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m032.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m032_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m032.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m032_0_prg_mode,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m032.0_prg_mode.nes"
    );
    setup_rom_test!(
        test_mv_m032_0_combined,
        "roms/automated_tests/mapper_verification/bin/m032.0.nes"
    );

    // ================================================================
    // Mapper 32 (Irem G-101), Submapper 1 (Major League)
    // ================================================================

    setup_rom_test!(
        test_mv_m032_1_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m032.1_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m032_1_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m032.1_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m032_1_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m032.1_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m032_1_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m032.1_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m032_1_combined,
        "roms/automated_tests/mapper_verification/bin/m032.1.nes"
    );

    // ================================================================
    // Mapper 33 (Taito TC0190), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m033_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m033.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m033_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m033.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m033_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m033.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m033_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m033.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m033_0_combined,
        "roms/automated_tests/mapper_verification/bin/m033.0.nes"
    );

    // ================================================================
    // Mapper 34, Submapper 0 (BNROM/NINA-001 combined)
    // ================================================================

    setup_rom_test!(
        test_mv_m034_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m034.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m034_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m034.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m034_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m034.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m034_0_combined,
        "roms/automated_tests/mapper_verification/bin/m034.0.nes"
    );

    // ================================================================
    // Mapper 34, Submapper 1 (NINA-001)
    // ================================================================

    setup_rom_test!(
        test_mv_m034_1_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m034.1_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m034_1_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m034.1_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m034_1_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m034.1_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m034_1_combined,
        "roms/automated_tests/mapper_verification/bin/m034.1.nes"
    );

    // ================================================================
    // Mapper 34, Submapper 2 (BNROM)
    // ================================================================

    setup_rom_test!(
        test_mv_m034_2_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m034.2_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m034_2_combined,
        "roms/automated_tests/mapper_verification/bin/m034.2.nes"
    );

    // ================================================================
    // Mapper 35 (J.Y. Company ASIC), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m035_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m035.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m035_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m035.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m035_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m035.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m035_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m035.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m035_0_multiplier,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m035.0_multiplier.nes"
    );
    setup_rom_test!(
        test_mv_m035_0_combined,
        "roms/automated_tests/mapper_verification/bin/m035.0.nes"
    );

    // ================================================================
    // Mapper 37 (SMB + Tetris + Nintendo World Cup), Submapper 0
    // Uses console verification because $6000-$7FFF is mapper control.
    // ================================================================

    setup_rom_console_test!(
        test_mv_m037_0_block_select,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m037.0_block_select.nes"
    );
    setup_rom_console_test!(
        test_mv_m037_0_combined,
        "roms/automated_tests/mapper_verification/bin/m037.0.nes"
    );

    // ================================================================
    // Mapper 38 (Bit Corp. PCI556), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m038_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m038.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m038_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m038.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m038_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m038.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m038_0_combined,
        "roms/automated_tests/mapper_verification/bin/m038.0.nes"
    );

    // ================================================================
    // Mapper 39 (BNROM clone / Study & Game 32-in-1), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m039_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m039.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m039_0_combined,
        "roms/automated_tests/mapper_verification/bin/m039.0.nes"
    );

    // ================================================================
    // Mapper 40 (NTDEC 2722), Submapper 0
    // Uses console verification because $6000-$7FFF is PRG-ROM.
    // ================================================================

    setup_rom_console_test!(
        test_mv_m040_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m040.0_prg_banking.nes"
    );
    setup_rom_console_test!(
        test_mv_m040_0_combined,
        "roms/automated_tests/mapper_verification/bin/m040.0.nes"
    );

    // ================================================================
    // Mapper 41 (Caltron 6-in-1), Submapper 0
    // Uses console verification because $6000-$67FF is mapper control.
    // ================================================================

    setup_rom_console_test!(
        test_mv_m041_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m041.0_prg_banking.nes"
    );
    setup_rom_console_test!(
        test_mv_m041_0_combined,
        "roms/automated_tests/mapper_verification/bin/m041.0.nes"
    );

    // ================================================================
    // Mapper 42 (Mario Baby / Ai Senshi Nicol), Submapper 0
    // Uses console verification because $6000-$7FFF is PRG-ROM.
    // ================================================================

    setup_rom_console_test!(
        test_mv_m042_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m042.0_prg_banking.nes"
    );
    setup_rom_console_test!(
        test_mv_m042_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m042.0_chr_banking.nes"
    );
    setup_rom_console_test!(
        test_mv_m042_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m042.0_nametable.nes"
    );
    setup_rom_console_test!(
        test_mv_m042_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m042.0_irq.nes"
    );
    setup_rom_console_test!(
        test_mv_m042_0_combined,
        "roms/automated_tests/mapper_verification/bin/m042.0.nes"
    );

    // ================================================================
    // Mapper 43 (TONY-I / YS-612), Submapper 0
    // Uses console verification because $6000-$7FFF is PRG-ROM and control is at $4022.
    // ================================================================

    setup_rom_console_test!(
        test_mv_m043_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m043.0_prg_banking.nes"
    );
    setup_rom_console_test!(
        test_mv_m043_0_combined,
        "roms/automated_tests/mapper_verification/bin/m043.0.nes"
    );

    // ================================================================
    // Mapper 44 (Super Big 7-in-1), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m044_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m044.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m044_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m044.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m044_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m044.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m044_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m044.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m044_0_block_select,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m044.0_block_select.nes"
    );
    setup_rom_test!(
        test_mv_m044_0_combined,
        "roms/automated_tests/mapper_verification/bin/m044.0.nes"
    );

    // ================================================================
    // Mapper 45 (GA23C multicart), Submapper 0
    // Uses console verification because $6000-$6001 are mapper control.
    // ================================================================

    setup_rom_console_test!(
        test_mv_m045_0_block_select,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m045.0_block_select.nes"
    );
    setup_rom_console_test!(
        test_mv_m045_0_combined,
        "roms/automated_tests/mapper_verification/bin/m045.0.nes"
    );

    // ================================================================
    // Mapper 46 (Rumble Station), Submapper 0
    // Uses console verification because $6000-$7FFF is mapper control.
    // ================================================================

    setup_rom_console_test!(
        test_mv_m046_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m046.0_prg_banking.nes"
    );
    setup_rom_console_test!(
        test_mv_m046_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m046.0_chr_banking.nes"
    );
    setup_rom_console_test!(
        test_mv_m046_0_combined,
        "roms/automated_tests/mapper_verification/bin/m046.0.nes"
    );

    // ================================================================
    // Mapper 47 (Super Spike V'Ball + Nintendo World Cup), Submapper 0
    // Uses console verification because $6000-$7FFF is mapper control.
    // ================================================================

    setup_rom_console_test!(
        test_mv_m047_0_block_select,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m047.0_block_select.nes"
    );
    setup_rom_console_test!(
        test_mv_m047_0_combined,
        "roms/automated_tests/mapper_verification/bin/m047.0.nes"
    );

    // ================================================================
    // Mapper 48 (Taito TC0690), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m048_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m048.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m048_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m048.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m048_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m048.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m048_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m048.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m048_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m048.0_prg_ram.nes"
    );
    setup_rom_test!(
        test_mv_m048_0_combined,
        "roms/automated_tests/mapper_verification/bin/m048.0.nes"
    );

    // ================================================================
    // Mapper 64 (RAMBO-1), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m064_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m064.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m064_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m064.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m064_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m064.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m064_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m064.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m064_0_combined,
        "roms/automated_tests/mapper_verification/bin/m064.0.nes"
    );

    // ================================================================
    // Mapper 65 (Irem H3001), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m065_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m065.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m065_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m065.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m065_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m065.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m065_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m065.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m065_0_combined,
        "roms/automated_tests/mapper_verification/bin/m065.0.nes"
    );

    // ================================================================
    // Mapper 67 (Sunsoft 3), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m067_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m067.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m067_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m067.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m067_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m067.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m067_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m067.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m067_0_combined,
        "roms/automated_tests/mapper_verification/bin/m067.0.nes"
    );

    // ================================================================
    // Mapper 75 (VRC1), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m075_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m075.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m075_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m075.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m075_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m075.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m075_0_combined,
        "roms/automated_tests/mapper_verification/bin/m075.0.nes"
    );

    // ================================================================
    // Mapper 119 (TQROM), Submapper 0
    // ================================================================

    setup_rom_test!(
        test_mv_m119_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m119.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m119_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m119.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m119_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m119.0_nametable.nes"
    );
    setup_rom_test!(
        test_mv_m119_0_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m119.0_irq.nes"
    );
    setup_rom_test!(
        test_mv_m119_0_combined,
        "roms/automated_tests/mapper_verification/bin/m119.0.nes"
    );
}
