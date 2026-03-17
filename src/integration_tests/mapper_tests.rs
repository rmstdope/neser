#[cfg(test)]
mod tests {
    use std::fs;

    use crate::cartridge::Cartridge;
    use crate::console::{Config, Nes, RamInitMode};
    use crate::integration_tests::rom_test_runner::tests::run_nes_for_frames;
    use crate::{setup_rom_crc_test, setup_rom_test};

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

    // TODO VRC6

    // ================================================================
    // Mapper Verification Suite — custom test ROMs
    // Tests PRG/CHR banking, nametable mirroring, IRQ, PRG-RAM,
    // and bus conflicts across mappers 0,1,2,3,4,7 with submapper variants.
    // ================================================================
    // Combined Mapper Verification ROMs (all tests per mapper/submapper)
    // ================================================================

    setup_rom_test!(
        test_mv_m000_0_combined,
        "roms/automated_tests/mapper_verification/bin/m000.0.nes"
    );
    setup_rom_test!(
        test_mv_m001_0_combined,
        "roms/automated_tests/mapper_verification/bin/m001.0.nes"
    );
    setup_rom_test!(
        test_mv_m001_5_combined,
        "roms/automated_tests/mapper_verification/bin/m001.5.nes"
    );
    setup_rom_test!(
        test_mv_m002_0_combined,
        "roms/automated_tests/mapper_verification/bin/m002.0.nes"
    );
    setup_rom_test!(
        test_mv_m002_2_combined,
        "roms/automated_tests/mapper_verification/bin/m002.2.nes"
    );
    setup_rom_test!(
        test_mv_m003_0_combined,
        "roms/automated_tests/mapper_verification/bin/m003.0.nes"
    );
    setup_rom_test!(
        test_mv_m003_1_combined,
        "roms/automated_tests/mapper_verification/bin/m003.1.nes"
    );
    setup_rom_test!(
        test_mv_m004_0_combined,
        "roms/automated_tests/mapper_verification/bin/m004.0.nes"
    );
    setup_rom_test!(
        test_mv_m004_1_combined,
        "roms/automated_tests/mapper_verification/bin/m004.1.nes"
    );
    setup_rom_test!(
        test_mv_m007_0_combined,
        "roms/automated_tests/mapper_verification/bin/m007.0.nes"
    );
    setup_rom_test!(
        test_mv_m007_1_combined,
        "roms/automated_tests/mapper_verification/bin/m007.1.nes"
    );

    // ================================================================
    // Single Mapper Verification ROMs (individual test aspects)
    // ================================================================

    // Mapper 0 (NROM)
    setup_rom_test!(
        test_mv_m000_0_prg_ram,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m000.0_prg_ram.nes"
    );

    // Mapper 1 (MMC1), Submapper 0
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

    // Mapper 1, Submapper 5 (Fixed PRG)
    setup_rom_test!(
        test_mv_m001_5_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m001.5_prg_banking.nes"
    );

    // Mapper 2 (UxROM), Submapper 0 (Bus Conflicts)
    setup_rom_test!(
        test_mv_m002_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m002.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m002_0_bus_conflicts,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m002.0_bus_conflicts.nes"
    );

    // Mapper 2, Submapper 2 (No Bus Conflicts)
    setup_rom_test!(
        test_mv_m002_2_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m002.2_prg_banking.nes"
    );

    // Mapper 3 (CNROM), Submapper 0 (Bus Conflicts)
    setup_rom_test!(
        test_mv_m003_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m003.0_chr_banking.nes"
    );
    setup_rom_test!(
        test_mv_m003_0_bus_conflicts,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m003.0_bus_conflicts.nes"
    );

    // Mapper 3, Submapper 1 (No Bus Conflicts)
    setup_rom_test!(
        test_mv_m003_1_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m003.1_chr_banking.nes"
    );

    // Mapper 4 (MMC3), Submapper 0 (Sharp IRQ)
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

    // Mapper 4, Submapper 1 (NEC IRQ)
    setup_rom_test!(
        test_mv_m004_1_irq,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m004.1_irq.nes"
    );

    // Mapper 7 (AxROM), Submapper 0 (Bus Conflicts)
    setup_rom_test!(
        test_mv_m007_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m007.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m007_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m007.0_nametable.nes"
    );

    // Mapper 7, Submapper 1 (No Bus Conflicts)
    setup_rom_test!(
        test_mv_m007_1_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m007.1_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m007_1_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m007.1_nametable.nes"
    );

    // ================================================================
    // Mapper 5 (MMC5), Submapper 0
    // ================================================================

    // Combined
    setup_rom_test!(
        test_mv_m005_0_combined,
        "roms/automated_tests/mapper_verification/bin/m005.0.nes"
    );

    // Singles
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
        test_mv_m005_0_multiplier,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m005.0_multiplier.nes"
    );

    // ================================================================
    // Mapper 6 (Front Fareast Magic Card), Submapper 0
    // ================================================================

    // Combined
    setup_rom_test!(
        test_mv_m006_0_combined,
        "roms/automated_tests/mapper_verification/bin/m006.0.nes"
    );

    // Singles
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

    // ================================================================
    // Mapper 11 (Color Dreams), Submapper 0
    // ================================================================

    // Combined
    setup_rom_test!(
        test_mv_m011_0_combined,
        "roms/automated_tests/mapper_verification/bin/m011.0.nes"
    );

    // Singles
    setup_rom_test!(
        test_mv_m011_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m011.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m011_0_chr_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m011.0_chr_banking.nes"
    );

    // ================================================================
    // Mapper 13 (CPROM), Submapper 0
    // ================================================================

    // Combined
    setup_rom_test!(
        test_mv_m013_0_combined,
        "roms/automated_tests/mapper_verification/bin/m013.0.nes"
    );

    // Singles
    setup_rom_test!(
        test_mv_m013_0_chr_ram_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m013.0_chr_ram_banking.nes"
    );

    // ================================================================
    // Mapper 9 (MMC2), Submapper 0
    // ================================================================

    // Combined
    setup_rom_test!(
        test_mv_m009_0_combined,
        "roms/automated_tests/mapper_verification/bin/m009.0.nes"
    );

    // Singles
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

    // ================================================================
    // Mapper 10 (MMC4), Submapper 0
    // ================================================================

    // Combined
    setup_rom_test!(
        test_mv_m010_0_combined,
        "roms/automated_tests/mapper_verification/bin/m010.0.nes"
    );

    // Singles
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

    // ============================
    // Mapper 8 (SMC GNROM mode 4)
    // ============================
    setup_rom_test!(
        test_mv_m008_0_combined,
        "roms/automated_tests/mapper_verification/bin/m008.0.nes"
    );
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

    // ============================
    // Mapper 15 (K-1029 multicart)
    // ============================
    setup_rom_test!(
        test_mv_m015_0_combined,
        "roms/automated_tests/mapper_verification/bin/m015.0.nes"
    );
    setup_rom_test!(
        test_mv_m015_0_prg_banking,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m015.0_prg_banking.nes"
    );
    setup_rom_test!(
        test_mv_m015_0_nametable,
        "roms/automated_tests/mapper_verification/bin/rom_singles/m015.0_nametable.nes"
    );

    // ============================
    // Mapper 12 (SL-5020B / MMC3 + outer CHR)
    // ============================
    setup_rom_test!(
        test_mv_m012_0_combined,
        "roms/automated_tests/mapper_verification/bin/m012.0.nes"
    );
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

    // ============================
    // Mapper 14 (SL-1632 / MMC3+VRC2 hybrid)
    // ============================
    setup_rom_test!(
        test_mv_m014_0_combined,
        "roms/automated_tests/mapper_verification/bin/m014.0.nes"
    );
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
}
