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
    use super::super::rom_runner::{
        InputEvent, RESULT_ADDR, RunConfig, RunExitReason, RunResult, run_rom,
    };
    use crate::snes::console::config::SnesHardware;
    use crate::snes::input::SnesButton;

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

    // ---- Group C: refresh rate and the SPC clock ratio ---------------------

    /// SPC700 program that counts free-running loop iterations for as long as
    /// the 65816 lets it, and publishes the 16-bit total on demand. Assembled
    /// by hand from the SPC700 opcode table (fullsnes "SPC700 CPU"):
    ///
    /// ```text
    /// wait_start:                      ; $F7 is port 3 (CPU -> SPC)
    ///   E4 F7      MOV A,$F7
    ///   68 3C      CMP A,#$3C          ; start token
    ///   D0 FA      BNE wait_start
    ///   E8 00      MOV A,#$00          ; zero the 16-bit counter at $10/$11
    ///   C4 10      MOV $10,A
    ///   C4 11      MOV $11,A
    /// loop:
    ///   E4 F7      MOV A,$F7
    ///   68 5A      CMP A,#$5A          ; stop token
    ///   F0 08      BEQ stop
    ///   AB 10      INC $10
    ///   D0 F6      BNE loop
    ///   AB 11      INC $11
    ///   2F F2      BRA loop
    /// stop:
    ///   E4 10      MOV A,$10
    ///   C4 F4      MOV $F4,A           ; counter low  -> CPU $2140
    ///   E4 11      MOV A,$11
    ///   C4 F5      MOV $F5,A           ; counter high -> CPU $2141
    ///   E8 A5      MOV A,#$A5
    ///   C4 F6      MOV $F6,A           ; "published"  -> CPU $2142
    /// spin:
    ///   2F FE      BRA spin
    /// ```
    ///
    /// The start token matters: after the upload handshake port 3 still holds
    /// the entry address's high byte, so the counter must not begin until the
    /// CPU has aligned it to a frame boundary.
    #[rustfmt::skip]
    const SPC_FRAME_COUNTER: [u8; 40] = [
        0xE4, 0xF7, 0x68, 0x3C, 0xD0, 0xFA,
        0xE8, 0x00, 0xC4, 0x10, 0xC4, 0x11,
        0xE4, 0xF7, 0x68, 0x5A, 0xF0, 0x08,
        0xAB, 0x10, 0xD0, 0xF6, 0xAB, 0x11, 0x2F, 0xF2,
        0xE4, 0x10, 0xC4, 0xF4, 0xE4, 0x11, 0xC4, 0xF5,
        0xE8, 0xA5, 0xC4, 0xF6, 0x2F, 0xFE,
    ];
    const SPC_ORIGIN: u16 = 0x0200;
    const SPC_START_TOKEN: u8 = 0x3C;
    const SPC_STOP_TOKEN: u8 = 0x5A;
    const SPC_PUBLISHED: u8 = 0xA5;
    /// Frames the SPC counter is left running for. Long enough that the ~0.9%
    /// clock-ratio term is thousands of counts wide, short enough that both
    /// regions' counters stay inside 16 bits.
    const MEASURED_FRAMES: u8 = 20;

    /// Emits a wait for the VBlank flag's next rising edge (low first, so a
    /// mid-VBlank start still lands on a real edge).
    fn emit_wait_vblank_edge(fixture: &mut FixtureRom) {
        let wait_low = fixture.pos();
        fixture.lda_abs(HVBJOY);
        fixture.and_imm(0x80);
        fixture.bne_to(wait_low);
        let wait_high = fixture.pos();
        fixture.lda_abs(HVBJOY);
        fixture.and_imm(0x80);
        fixture.beq_to(wait_high);
    }

    /// Uploads [`SPC_FRAME_COUNTER`], runs it across exactly
    /// [`MEASURED_FRAMES`] frames, and publishes the 16-bit count to the
    /// result block.
    fn spc_frame_counter_rom() -> Vec<u8> {
        let mut fixture = FixtureRom::new(b"PAL SPC RATE");
        fixture.upload_spc_program(SPC_ORIGIN, SPC_ORIGIN, &SPC_FRAME_COUNTER);

        // Align the measurement window to a frame boundary before starting.
        emit_wait_vblank_edge(&mut fixture);
        fixture.store_imm_abs(0x2143, SPC_START_TOKEN);

        fixture.ldx_imm(0x00);
        let frame_loop = fixture.pos();
        emit_wait_vblank_edge(&mut fixture);
        fixture.inx();
        fixture.cpx_imm(MEASURED_FRAMES);
        fixture.bne_to(frame_loop);

        fixture.store_imm_abs(0x2143, SPC_STOP_TOKEN);
        let wait_published = fixture.pos();
        fixture.lda_abs(0x2142);
        fixture.cmp_imm(SPC_PUBLISHED);
        fixture.bne_to(wait_published);

        fixture.lda_abs(0x2140);
        fixture.sta_long(RESULT_ADDR);
        fixture.lda_abs(0x2141);
        fixture.sta_long(RESULT_ADDR + 1);
        fixture.pass_marker_and_idle();
        fixture.build()
    }

    fn measure_spc_counts(hardware: SnesHardware) -> u32 {
        let label = format!("spc-rate-{hardware:?}.sfc");
        let config = RunConfig::new(0, 60).with_hardware(hardware);
        let result = run_rom(&spc_frame_counter_rom(), &label, config);
        assert_passed(&result, &label);
        u32::from(result.result_bytes[0]) | (u32::from(result.result_bytes[1]) << 8)
    }

    /// The end-to-end refresh-rate and APU-clock check.
    ///
    /// Over the same number of frames, a PAL console gives the SPC700 more
    /// wall-clock time for two independent reasons, and this ratio is the
    /// product of both:
    ///
    /// * the frame is longer -- 312x1364 = 425,568 master clocks against
    ///   NTSC's 357,366 average (a factor of 1.190846); and
    /// * each master clock is longer -- 21,281,370 Hz against 21,477,270 Hz,
    ///   while the SPC700's own 24.576 MHz crystal is unchanged (a factor of
    ///   1.009205).
    ///
    /// Expected ratio 1.201808. Had the APU kept the NTSC clock denominator
    /// only the first term would apply and the ratio would be 1.190846, which
    /// the tolerance here deliberately excludes. Comparing the two runs rather
    /// than asserting absolute counts makes the check independent of how many
    /// SPC cycles the counting loop itself takes.
    #[test]
    fn pal_frames_give_the_spc700_proportionally_more_time() {
        let ntsc = measure_spc_counts(SnesHardware::Ntsc);
        let pal = measure_spc_counts(SnesHardware::Pal);

        assert!(ntsc > 1_000, "NTSC counter should have advanced: {ntsc}");
        let ratio = f64::from(pal) / f64::from(ntsc);
        assert!(
            (ratio - 1.201808).abs() < 0.004,
            "PAL/NTSC SPC counts should be in the 1.201808 frame-length x \
             clock-rate ratio (1.190846 would mean the APU still runs on the \
             NTSC master clock), got {ratio:.6} from ntsc={ntsc} pal={pal}"
        );
    }

    // ---- Group D: output dimensions and overscan --------------------------

    /// HDMA table driving INIDISP `$2100` master brightness in three
    /// horizontal bands (80 scanlines each): line-count byte then data byte
    /// per entry, terminated by a zero line count (fullsnes "HDMA Table
    /// Format"). Bands give the screen real vertical structure, so the
    /// cross-region comparison below would notice content shifting rows --
    /// which a flat backdrop could not.
    const BRIGHTNESS_BANDS: [u8; 7] = [0x50, 0x0F, 0x50, 0x0A, 0x50, 0x05, 0x00];
    /// Backdrop colour: BGR555 R=31 G=20 B=8, so the three channels scale
    /// differently under each band's brightness.
    const BACKDROP_LOW: u8 = 0x9F;
    const BACKDROP_HIGH: u8 = 0x22;

    /// Renders the banded screen, lets it settle for four frames, and reports
    /// PASS from inside VBlank so the runner's snapshot always sees a complete
    /// frame rather than a partially drawn one.
    fn banded_screen_rom(setini: u8) -> Vec<u8> {
        let mut fixture = FixtureRom::new(b"PAL SCREEN BANDS");
        fixture.force_blank_on();
        fixture.store_imm_abs(0x2121, 0x00); // CGADD = backdrop entry
        fixture.store_imm_abs(0x2122, BACKDROP_LOW);
        fixture.store_imm_abs(0x2122, BACKDROP_HIGH);
        fixture.store_imm_abs(0x2133, setini);

        let table = fixture.place_data(&BRIGHTNESS_BANDS);
        fixture.setup_hdma(0, 0x00, 0x00, table, 0x00);
        fixture.enable_hdma(0x01);
        fixture.force_blank_off();

        fixture.ldx_imm(0x00);
        let frame_loop = fixture.pos();
        emit_wait_vblank_edge(&mut fixture);
        fixture.inx();
        fixture.cpx_imm(4);
        fixture.bne_to(frame_loop);
        fixture.pass_marker_and_idle();
        fixture.build()
    }

    fn render_banded_screen(setini: u8, hardware: SnesHardware) -> RunResult {
        let label = format!("bands-{setini:02X}-{hardware:?}.sfc");
        let result = run_pal_fixture(&banded_screen_rom(setini), &label, Some(hardware));
        assert_passed(&result, &label);
        result
    }

    /// PAL changes *when* frames end, never *what* they contain. The active
    /// area is 224 lines (239 with SETINI overscan) on both consoles, and the
    /// output framebuffer is the same 256-pixel-wide image -- Mesen2 derives
    /// its frame height from the overscan flag alone, with no region term.
    ///
    /// The two runs are each other's oracle here: identical CRCs over the same
    /// banded screen prove PAL neither resizes the output nor shifts content
    /// down the extra 50 blanking lines, and no approved golden is needed.
    #[test]
    fn dimensions_and_pixels_are_region_independent() {
        let ntsc = render_banded_screen(0x00, SnesHardware::Ntsc);
        let pal = render_banded_screen(0x00, SnesHardware::Pal);

        assert_eq!(ntsc.screen_dimensions, (256, 224));
        assert_eq!(pal.screen_dimensions, (256, 224));
        assert_eq!(
            ntsc.screen_crc32, pal.screen_crc32,
            "the same banded screen must render identically on both consoles \
             (ntsc=0x{:08X} pal=0x{:08X})",
            ntsc.screen_crc32, pal.screen_crc32
        );
    }

    /// SETINI bit 2 selects the 239-line tall screen.  It is a PPU mode, not a
    /// region property: both consoles grow the active area to 239 lines.  The
    /// Mesen2-compatible snapshot clips the output to the standard 224-line
    /// window (#3001) so both regions report 256×224.
    #[test]
    fn overscan_output_is_224_lines_mesen2_compat_in_both_regions() {
        let ntsc = render_banded_screen(0x04, SnesHardware::Ntsc);
        let pal = render_banded_screen(0x04, SnesHardware::Pal);

        assert_eq!(ntsc.screen_dimensions, (256, 224));
        assert_eq!(pal.screen_dimensions, (256, 224));
        assert_eq!(
            ntsc.screen_crc32, pal.screen_crc32,
            "overscan output must match across regions (ntsc=0x{:08X} pal=0x{:08X})",
            ntsc.screen_crc32, pal.screen_crc32
        );
    }

    /// Guards the comparison above against passing vacuously: a blank or
    /// uniform screen would satisfy "the CRCs match" without proving anything.
    /// The banded fixture must differ from an all-black screen, from a
    /// force-blanked one, and between its 224- and 239-line variants.
    #[test]
    fn the_banded_screen_is_a_discriminating_oracle() {
        let bands = render_banded_screen(0x00, SnesHardware::Pal);
        let tall = render_banded_screen(0x04, SnesHardware::Pal);

        let mut blank = FixtureRom::new(b"PAL BLANK");
        blank.force_blank_on();
        blank.pass_marker_and_idle();
        let blank = run_pal_fixture(&blank.build(), "blank.sfc", Some(SnesHardware::Pal));
        assert_passed(&blank, "blank screen");

        assert_ne!(
            bands.screen_crc32, blank.screen_crc32,
            "banded screen must not be a blank screen"
        );
        assert_ne!(
            bands.screen_crc32, tall.screen_crc32,
            "224-line and 239-line renders must differ"
        );
    }

    // ---- Group E: input on the PAL VBlank boundary ------------------------

    /// Start button bit in JOY1H `$4219` (B Y Select Start Up Down Left Right,
    /// bit 7 down to bit 0).
    const START_BIT_JOY1H: u8 = 0x10;

    /// Automatic joypad reading starts a fixed distance into the *first*
    /// VBlank scanline (fullsnes: "between H=32.5 and H=95.5 of the first
    /// V-Blank scanline"), so it hangs off the same region-independent line
    /// 225 boundary that `vblank_starts_on_the_same_scanline_in_both_regions`
    /// pins. This guards the interaction end-to-end: a scripted press must
    /// still reach `$4219` on a PAL console, where the VBlank the latch lives
    /// in is 88 scanlines long instead of 38.
    #[test]
    fn auto_joypad_reads_still_land_on_a_pal_console() {
        let mut fixture = FixtureRom::new(b"PAL AUTO JOYPAD");
        fixture.country(0x02); // Europe: PAL by header, no override needed
        fixture.store_imm_abs(0x4200, 0x01); // NMITIMEN bit 0: auto-joypad enable
        let poll = fixture.pos();
        fixture.lda_abs(0x4219);
        fixture.cmp_imm(START_BIT_JOY1H);
        fixture.bne_to(poll);
        fixture.pass_marker_and_idle();

        let script = [InputEvent::button(5, SnesButton::Start, true)];
        let result = run_rom(
            &fixture.build(),
            "pal-auto-joypad.sfc",
            RunConfig::new(0, 120).with_input_script(&script),
        );

        assert_passed(&result, "PAL auto-joypad read");
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
