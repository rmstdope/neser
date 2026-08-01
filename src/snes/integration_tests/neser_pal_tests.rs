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

    // ---- Group B: scanline count and frame timing -------------------------

    /// NMITIMEN `$4200` bits 5-4 = 2: "IRQ at V=V (H=0)" (fullsnes "4200h
    /// NMITIMEN"). Bit 4 stays clear so the H counter is ignored.
    const NMITIMEN_V_IRQ: u8 = 0x20;
    const VTIMEL: u16 = 0x4209;
    const VTIMEH: u16 = 0x420A;
    /// TIMEUP `$4211` bit 7 is the H/V-timer IRQ flag; reading acknowledges.
    const TIMEUP: u16 = 0x4211;
    /// HVBJOY `$4212` bit 7 is the VBlank flag.
    const HVBJOY: u16 = 0x4212;
    /// OPVCT `$213D`: latched vertical counter, read low byte then high bit.
    const OPVCT: u16 = 0x213D;
    /// SLHV `$2137`: a read strobes the H/V counter latch.
    const SLHV: u16 = 0x2137;

    /// A fixture that arms a V-IRQ at scanline `line` and spins until it
    /// fires. If the scanline does not exist on the console it is running on,
    /// the IRQ never fires and the fixture runs out the runner's frame budget
    /// instead of reaching the PASS marker.
    fn virq_at_line_rom(line: u16) -> Vec<u8> {
        let mut fixture = FixtureRom::new(b"PAL VIRQ LINE");
        fixture.store_imm_abs(VTIMEL, (line & 0xFF) as u8);
        fixture.store_imm_abs(VTIMEH, ((line >> 8) & 0x01) as u8);
        fixture.store_imm_abs(0x4200, NMITIMEN_V_IRQ);
        fixture.lda_abs(TIMEUP); // acknowledge any stale flag
        let poll = fixture.pos();
        fixture.lda_abs(TIMEUP);
        fixture.and_imm(0x80);
        fixture.beq_to(poll);
        fixture.pass_marker_and_idle();
        fixture.build()
    }

    fn virq_fires(line: u16, hardware: SnesHardware) -> bool {
        let label = format!("virq-{line}-{hardware:?}.sfc");
        let result = run_pal_fixture(&virq_at_line_rom(line), &label, Some(hardware));
        match result.exit_reason {
            RunExitReason::PassMarker => true,
            RunExitReason::FrameLimit => false,
            other => panic!("{label}: unexpected exit {other:?} ({result:?})"),
        }
    }

    /// A V-IRQ can only fire on a scanline the console actually generates, so
    /// arming one is a direct probe for "does this scanline exist?". NTSC runs
    /// 262 scanlines (last index 261) and PAL 312 (last index 311), both
    /// non-interlaced -- fullsnes "SNES Timing"; Mesen2
    /// `SnesPpu::UpdateNmiScanline` uses the same 261/311 last-line indices.
    #[test]
    fn v_irq_reaches_only_the_scanlines_its_region_generates() {
        // Inside the visible area: exists on both.
        assert!(virq_fires(200, SnesHardware::Ntsc));
        assert!(virq_fires(200, SnesHardware::Pal));

        // NTSC's last scanline: exists on both.
        assert!(virq_fires(261, SnesHardware::Ntsc));
        assert!(virq_fires(261, SnesHardware::Pal));

        // One past NTSC's last scanline: PAL only.
        assert!(!virq_fires(262, SnesHardware::Ntsc));
        assert!(virq_fires(262, SnesHardware::Pal));

        // PAL's last scanline: PAL only.
        assert!(!virq_fires(311, SnesHardware::Ntsc));
        assert!(virq_fires(311, SnesHardware::Pal));

        // Past PAL's last scanline: neither.
        assert!(!virq_fires(312, SnesHardware::Ntsc));
        assert!(!virq_fires(312, SnesHardware::Pal));
    }

    /// A fixture that waits for the VBlank flag's rising edge, latches the H/V
    /// counters there and publishes OPVCT (low byte, then high bit) to the
    /// result block. `setini` is written before probing so the caller can turn
    /// on 239-line overscan.
    fn vblank_start_probe_rom(setini: u8) -> Vec<u8> {
        let mut fixture = FixtureRom::new(b"PAL VBLANK START");
        fixture.store_imm_abs(0x2133, setini);

        // Wait for VBlank low, then for its rising edge, so the latch below
        // always lands on the first VBlank scanline rather than mid-VBlank.
        let wait_low = fixture.pos();
        fixture.lda_abs(HVBJOY);
        fixture.and_imm(0x80);
        fixture.bne_to(wait_low);
        let wait_high = fixture.pos();
        fixture.lda_abs(HVBJOY);
        fixture.and_imm(0x80);
        fixture.beq_to(wait_high);

        // STAT78 resets the OPHCT/OPVCT read flip-flops, SLHV latches the
        // counters, then OPVCT reads low byte and high bit in that order.
        fixture.lda_abs(STAT78);
        fixture.lda_abs(SLHV);
        fixture.lda_abs(OPVCT);
        fixture.sta_long(RESULT_ADDR);
        fixture.lda_abs(OPVCT);
        fixture.and_imm(0x01);
        fixture.sta_long(RESULT_ADDR + 1);
        fixture.pass_marker_and_idle();
        fixture.build()
    }

    fn probe_vblank_start_line(setini: u8, hardware: SnesHardware) -> u16 {
        let label = format!("vblank-start-{setini:02X}-{hardware:?}.sfc");
        let result = run_pal_fixture(&vblank_start_probe_rom(setini), &label, Some(hardware));
        assert_passed(&result, &label);
        u16::from(result.result_bytes[0]) | (u16::from(result.result_bytes[1]) << 8)
    }

    /// VBlank starts at scanline 225 (240 with overscan) on **both** consoles:
    /// PAL's extra 50 scanlines are all blanking, appended after the active
    /// area rather than extending it. Mesen2 `SnesPpu::UpdateNmiScanline` sets
    /// `_vblankStartScanline = _state.OverscanMode ? 240 : 225` with no region
    /// term at all, while only the last-scanline index is region-dependent.
    #[test]
    fn vblank_starts_on_the_same_scanline_in_both_regions() {
        assert_eq!(probe_vblank_start_line(0x00, SnesHardware::Ntsc), 225);
        assert_eq!(probe_vblank_start_line(0x00, SnesHardware::Pal), 225);

        // SETINI bit 2 = 239-line overscan.
        assert_eq!(probe_vblank_start_line(0x04, SnesHardware::Ntsc), 240);
        assert_eq!(probe_vblank_start_line(0x04, SnesHardware::Pal), 240);
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
