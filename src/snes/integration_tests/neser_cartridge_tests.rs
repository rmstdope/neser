//! Base-cartridge verification fixtures (issue #2885): minimal in-code
//! LoROM/HiROM/ExHiROM/copier images loaded through the real cartridge and bus
//! paths, asserting mapping detection, header/region metadata, SRAM
//! size/battery, ROM address translation, and end-to-end battery-SRAM
//! read/write. Enhancement-chip cartridges are out of scope (all fixtures use
//! chipset `$00`/`$02`).
//!
//! The image builder ([`super::cartridge_fixtures::CartFixture`]) is authored
//! from the SNES header spec (fullsnes / nesdev), not from the loader
//! implementation. Fixtures are synthetic and carry no on-disk ROM bytes; see
//! the `neser_cartridge_tests` asset in
//! `roms/snes/automated_tests/manifest.json`.

#[cfg(test)]
mod tests {
    use super::super::cartridge_fixtures::CartFixture;
    use super::super::fixture_rom::FixtureRom;
    use super::super::rom_runner::{RunConfig, RunExitReason, run_rom};
    use crate::platform::app_context::AppContext;
    use crate::platform::config::Config;
    use crate::platform::emulator::Emulator;
    use crate::snes::cartridge::{Cartridge, Mapping, RomSpeed};
    use crate::snes::console::Snes;

    fn load_fixture(image: &[u8], name: &str) -> Snes {
        let mut snes = Snes::new(AppContext::new_with_config(Config::default()));
        snes.load_rom(image, name).expect("load cartridge fixture");
        snes
    }

    // ---- Mapping detection + header/region metadata -----------------------

    #[test]
    fn lorom_fixture_exposes_mapping_and_metadata() {
        let image = CartFixture::new(Mapping::LoRom)
            .chipset(0x02) // ROM + RAM + battery
            .ram_size_field(0x05) // 32 KiB
            .country(0x01) // North America (NTSC)
            .build();
        let cart = Cartridge::from_bytes(&image).expect("cart");
        assert_eq!(cart.mapping(), Mapping::LoRom);
        assert_eq!(cart.title(), "NESER CART FIXTURE");
        assert_eq!(cart.country(), 0x01);
        assert_eq!(cart.sram_size(), 32 * 1024);
        assert!(cart.has_battery());
        assert_eq!(cart.speed(), RomSpeed::Slow);
        assert!(cart.enhancement_chip().is_none());
    }

    #[test]
    fn hirom_fixture_exposes_mapping_and_metadata() {
        let image = CartFixture::new(Mapping::HiRom)
            .country(0x02) // Europe (PAL)
            .build();
        let cart = Cartridge::from_bytes(&image).expect("cart");
        assert_eq!(cart.mapping(), Mapping::HiRom);
        assert_eq!(cart.country(), 0x02);
        assert_eq!(cart.sram_size(), 0);
        assert!(!cart.has_battery());
        assert_eq!(cart.speed(), RomSpeed::Slow);
        assert!(cart.enhancement_chip().is_none());
    }

    #[test]
    fn exhirom_fixture_exposes_mapping_and_metadata() {
        let image = CartFixture::new(Mapping::ExHiRom)
            .chipset(0x02)
            .ram_size_field(0x03) // 8 KiB
            .country(0x00) // Japan (NTSC)
            .build();
        let cart = Cartridge::from_bytes(&image).expect("cart");
        assert_eq!(cart.mapping(), Mapping::ExHiRom);
        assert_eq!(cart.country(), 0x00);
        assert_eq!(cart.sram_size(), 8 * 1024);
        assert!(cart.has_battery());
        assert!(cart.enhancement_chip().is_none());
    }

    // ---- ROM address translation (sentinel read-back) ---------------------

    #[test]
    fn lorom_address_translation_reads_sentinels() {
        // LoROM: $00:8000 -> ROM offset 0, $01:8000 -> ROM offset 0x8000.
        let image = CartFixture::new(Mapping::LoRom)
            .sentinel(0x0000, 0x11)
            .sentinel(0x8000, 0x22)
            .build();
        let snes = load_fixture(&image, "lorom_addr.sfc");
        assert_eq!(snes.read_bus_for_debugger_for_tests(0x00_8000), Some(0x11));
        assert_eq!(snes.read_bus_for_debugger_for_tests(0x01_8000), Some(0x22));
    }

    #[test]
    fn hirom_address_translation_reads_sentinels() {
        // HiROM: $C0:0000 -> ROM offset 0, $C1:0000 -> ROM offset 0x10000.
        let image = CartFixture::new(Mapping::HiRom)
            .sentinel(0x00000, 0x33)
            .sentinel(0x10000, 0x44)
            .build();
        let snes = load_fixture(&image, "hirom_addr.sfc");
        assert_eq!(snes.read_bus_for_debugger_for_tests(0xC0_0000), Some(0x33));
        assert_eq!(snes.read_bus_for_debugger_for_tests(0xC1_0000), Some(0x44));
    }

    #[test]
    fn exhirom_address_translation_reads_all_bank_regions() {
        // ExHiROM: $C0:0000 -> ROM offset 0 (lower half),
        //          $40:0000 -> ROM offset 0x400000 (upper half),
        //          $00:8000 and $80:8000 (system banks, offset >= $8000) ->
        //          ROM offset 0x8000 (both map identically via bank & 0x3F).
        let image = CartFixture::new(Mapping::ExHiRom)
            .sentinel(0x000000, 0x55)
            .sentinel(0x400000, 0x66)
            .sentinel(0x008000, 0x77)
            .build();
        let snes = load_fixture(&image, "exhirom_addr.sfc");
        assert_eq!(snes.read_bus_for_debugger_for_tests(0xC0_0000), Some(0x55));
        assert_eq!(snes.read_bus_for_debugger_for_tests(0x40_0000), Some(0x66));
        assert_eq!(snes.read_bus_for_debugger_for_tests(0x00_8000), Some(0x77));
        assert_eq!(snes.read_bus_for_debugger_for_tests(0x80_8000), Some(0x77));
    }

    // ---- Copier-header fixture --------------------------------------------

    #[test]
    fn copier_header_hirom_fixture_is_detected_and_stripped() {
        let image = CartFixture::new(Mapping::HiRom)
            .with_copier_header()
            .sentinel(0x0000, 0x77)
            .build();
        // 128 KiB body + 512-byte copier header.
        assert_eq!(image.len(), 0x200 + 0x20000);
        let cart = Cartridge::from_bytes(&image).expect("cart");
        assert_eq!(cart.mapping(), Mapping::HiRom);
        assert_eq!(cart.rom().len(), 0x20000, "copier header must be stripped");
        assert_eq!(cart.rom()[0], 0x77, "stripped body starts at the sentinel");
    }

    // ---- Battery-SRAM read/write within a run (executable) -----------------

    #[test]
    fn battery_sram_read_write_round_trips_within_a_run() {
        // A LoROM battery cart: the CPU writes two distinct bytes to two
        // distinct SRAM addresses, then reads the FIRST back. Reading $70:0000
        // *after* writing $70:1234 defeats an open-bus/MDR false pass -- an
        // unstored read would return the last-written 0xA5, not 0x5A. The ROM
        // self-checks with branch_fail_if_ne, so a PASS marker proves both the
        // SRAM write and read decode paths.
        let mut fixture = FixtureRom::new(b"SRAM RW TEST");
        fixture.with_battery_sram(0x05); // 32 KiB SRAM
        fixture.write_long(0x70_0000, 0x5A);
        fixture.write_long(0x70_1234, 0xA5);
        fixture.lda_long(0x70_0000);
        fixture.branch_fail_if_ne(0x5A);
        fixture.lda_long(0x70_1234);
        fixture.branch_fail_if_ne(0xA5);
        fixture.pass_marker_and_idle();
        let rom = fixture.build();

        let result = run_rom(&rom, "sram_rw.sfc", RunConfig::new(2_000_000, 0));
        assert_eq!(
            result.exit_reason,
            RunExitReason::PassMarker,
            "SRAM read/write self-check should pass; marker={:?} pc={:#06X}",
            result.marker,
            result.pc
        );
        assert!(result.passed);
    }
}
