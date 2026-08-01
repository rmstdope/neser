//! PAL timing and video-region verification (issue #2888).
//!
//! In-code fixture ROMs, authored from fullsnes (cross-checked against ares
//! and Mesen2) rather than from our implementation, covering the NTSC/PAL
//! differences the SNES actually has -- and, just as importantly, pinning the
//! things that do **not** differ:
//!
//! | Behaviour                    | NTSC          | PAL           |
//! |------------------------------|---------------|---------------|
//! | Scanlines per frame          | 262 (263 int.)| 312 (313 int.)|
//! | Last scanline index          | 261           | 311           |
//! | Master clock                 | 21,477,270 Hz | 21,281,370 Hz |
//! | Refresh rate                 | ~60.099 Hz    | ~50.007 Hz    |
//! | STAT78 `$213F` bit 4         | 0             | 1             |
//! | First VBlank scanline        | 225           | 225           |
//! | ... with SETINI overscan     | 240           | 240           |
//! | Output dimensions            | 256x224/239   | 256x224/239   |
//! | SPC700 clock                 | ~1.025 MHz    | ~1.025 MHz    |
//!
//! The extra 50 PAL scanlines are therefore *all* blanking: the active
//! display, the vblank boundary and the framebuffer are region-independent,
//! and only the frame's total length changes. Because the SPC700 has its own
//! 24.576 MHz crystal while the 65816's master clock drops, the SPC runs
//! ~0.92% fast relative to the CPU on PAL -- which is what the refresh-rate
//! fixture measures through an uploaded SPC program.
//!
//! No committed screen CRCs: where a rendered frame matters, the NTSC and PAL
//! runs are each other's oracle (see `dimensions_and_pixels_are_region_
//! independent`), which needs no approved golden.
//!
//! Fixtures are synthetic and carry no on-disk ROM bytes; see the
//! `neser_pal_tests` asset in `roms/snes/automated_tests/manifest.json`.

#[cfg(test)]
mod tests {
    use super::super::fixture_rom::FixtureRom;
    use super::super::rom_runner::{RESULT_ADDR, RunConfig, RunExitReason, RunResult, run_rom};
    use crate::snes::console::config::SnesHardware;

    /// STAT78: PPU2 status and version. Bit 4 is the frame-rate strap.
    const STAT78: u16 = 0x213F;
    /// PAL/NTSC bit within STAT78 (fullsnes: 0=NTSC/60Hz, 1=PAL/50Hz).
    const STAT78_PAL: u8 = 0x10;

    fn run_pal_fixture(rom: &[u8], name: &str, hardware: Option<SnesHardware>) -> RunResult {
        let mut config = RunConfig::new(0, 40);
        if let Some(hardware) = hardware {
            config = config.with_hardware(hardware);
        }
        run_rom(rom, name, config)
    }

    fn assert_passed(result: &RunResult, name: &str) {
        assert!(
            result.passed && result.exit_reason == RunExitReason::PassMarker,
            "{name}: expected PASS marker, got {result:?}"
        );
    }

    // ---- Group A: region selection and runtime readback -------------------

    /// A fixture that reads STAT78, masks off everything but the frame-rate
    /// bit and reports PASS only when it matches `expected_pal`.
    fn region_readback_rom(country: u8, expected_pal: bool) -> Vec<u8> {
        let mut fixture = FixtureRom::new(b"PAL REGION READBACK");
        fixture.country(country);
        fixture.lda_abs(STAT78);
        fixture.and_imm(STAT78_PAL);
        fixture.branch_fail_if_ne(if expected_pal { STAT78_PAL } else { 0x00 });
        fixture.pass_marker_and_idle();
        fixture.build()
    }

    /// fullsnes "213Fh - STAT78" bit 4: "Frame Rate (PPU2.Pin30) (0=NTSC/60Hz,
    /// 1=PAL/50Hz)". Every country code on both sides of every boundary in the
    /// fullsnes `$FFD9` table, read back the way a real ROM would.
    #[test]
    fn stat78_reports_the_region_selected_by_the_header_country() {
        let cases: [(u8, bool, &str); 12] = [
            (0x00, false, "japan"),
            (0x01, false, "usa"),
            (0x02, true, "europe"),
            (0x06, true, "france-secam"),
            (0x0C, true, "indonesia"),
            (0x0D, false, "south-korea"),
            (0x0E, false, "common-unknown"),
            (0x0F, false, "canada"),
            (0x10, false, "brazil-pal-m-60hz"),
            (0x11, true, "australia"),
            (0x12, false, "other-variation-unknown"),
            (0xFF, false, "out-of-table"),
        ];

        for (country, expected_pal, label) in cases {
            let rom = region_readback_rom(country, expected_pal);
            let result = run_pal_fixture(&rom, &format!("region-{label}.sfc"), None);
            assert_passed(&result, &format!("country {country:#04X} ({label})"));
        }
    }

    /// The `snes-hardware` override outranks the header, in both directions:
    /// a Japanese cartridge forced to PAL must report PAL, and a European
    /// cartridge forced to NTSC must report NTSC.
    #[test]
    fn config_hardware_override_outranks_the_header_country() {
        let forced_pal = region_readback_rom(0x00, true);
        let result = run_pal_fixture(&forced_pal, "override-pal.sfc", Some(SnesHardware::Pal));
        assert_passed(&result, "japan header forced to PAL");

        let forced_ntsc = region_readback_rom(0x02, false);
        let result = run_pal_fixture(&forced_ntsc, "override-ntsc.sfc", Some(SnesHardware::Ntsc));
        assert_passed(&result, "europe header forced to NTSC");
    }

    /// The frame-rate bit must not disturb the rest of STAT78: bits 3-0 are
    /// the 5C78 version (3) and bit 6 the counter-latch flag. A fixture that
    /// masks the version out of a PAL read still sees 3.
    #[test]
    fn stat78_version_field_survives_the_pal_frame_rate_bit() {
        let mut fixture = FixtureRom::new(b"PAL STAT78 VERSION");
        fixture.lda_abs(STAT78);
        fixture.and_imm(0x0F);
        fixture.branch_fail_if_ne(0x03);
        fixture.pass_marker_and_idle();

        let result = run_pal_fixture(
            &fixture.build(),
            "stat78-version.sfc",
            Some(SnesHardware::Pal),
        );
        assert_passed(&result, "PAL STAT78 version field");
    }

    /// The result-block plumbing the measurement fixtures rely on: a fixture
    /// stores a byte behind the marker and the runner hands it back.
    #[test]
    fn fixtures_report_measurements_through_the_result_block() {
        let mut fixture = FixtureRom::new(b"PAL RESULT BLOCK");
        fixture.lda_abs(STAT78);
        fixture.and_imm(STAT78_PAL);
        fixture.sta_long(RESULT_ADDR);
        fixture.pass_marker_and_idle();
        let rom = fixture.build();

        let ntsc = run_pal_fixture(&rom, "result-ntsc.sfc", Some(SnesHardware::Ntsc));
        let pal = run_pal_fixture(&rom, "result-pal.sfc", Some(SnesHardware::Pal));

        assert_passed(&ntsc, "result block NTSC");
        assert_passed(&pal, "result block PAL");
        assert_eq!(ntsc.result_bytes[0], 0x00);
        assert_eq!(pal.result_bytes[0], STAT78_PAL);
    }
}
