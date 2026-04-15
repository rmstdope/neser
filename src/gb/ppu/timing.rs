/// DMG PPU operating modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpuMode {
    /// Mode 0 — H-Blank: CPU/DMA has access to VRAM and OAM.
    HBlank = 0,
    /// Mode 1 — V-Blank (scanlines 144–153).
    VBlank = 1,
    /// Mode 2 — OAM Scan: OAM locked, VRAM accessible.
    OamScan = 2,
    /// Mode 3 — Pixel Transfer: OAM and VRAM locked.
    PixelTransfer = 3,
}

/// Dot-level scanline timing for the DMG PPU.
///
/// Fixed-width scanline model (MVP):
/// - Mode 2 (OAM Scan):     dots 0–79    (80 dots)
/// - Mode 3 (Pixel Xfer):   dots 80–(251+extra)  (172+extra dots)
/// - Mode 0 (H-Blank):      dots (252+extra)–455 (204-extra dots)
/// - Mode 1 (V-Blank):      scanlines 144–153 (4560 dots total)
pub struct Timing {
    dot: u16,
    scanline: u8,
    mode: PpuMode,
    frame_ready: bool,
    /// True during the first scanline after LCD is enabled.
    ///
    /// On real DMG hardware, when the LCD is turned on the first scanline
    /// does **not** begin with Mode 2 (OAM Scan). Instead, STAT reports
    /// Mode 0 (HBlank) for the first 80 dots, then the PPU transitions
    /// directly to Mode 3 (Pixel Transfer).
    first_scanline_after_enable: bool,
    /// Mirrors SameBoy's `mode_for_interrupt` (-1 = suppress all mode IRQs,
    /// 0–3 = mode whose STAT IRQ source is currently active).
    ///
    /// This differs from the STAT mode bits:
    /// - Mode 2 source becomes active 4 T-cycles before mode bits show Mode 2.
    /// - Mode 0 source and mode bits become active together.
    mode_for_irq: i8,
    /// Extra dots added to Mode 3 (OBJ/SCX/window penalties).
    /// Mode 0 starts at dot `OAM_SCAN_DOTS + PIXEL_TRANSFER_DOTS + mode3_extra_dots`.
    mode3_extra_dots: u16,
}

/// Events returned by a single dot tick.
#[derive(Debug, Default)]
pub struct DotEvents {
    /// Scanline pixel transfer just ended — render the current scanline.
    pub render_scanline: bool,
    /// V-Blank just started (LY just became 144).
    pub vblank_start: bool,
    /// PPU mode changed this dot.
    pub mode_changed: bool,
    /// A new frame just began (LY wrapped from 153 back to 0).
    pub new_frame: bool,
}

impl Timing {
    const DOTS_PER_SCANLINE: u16 = 456;
    const TOTAL_SCANLINES: u8 = 154;
    const VBLANK_START_LINE: u8 = 144;
    const OAM_SCAN_DOTS: u16 = 80;
    const PIXEL_TRANSFER_DOTS: u16 = 172;
    /// Dot at which the Mode 2 STAT IRQ source fires (4 dots before mode bits change to Mode 2).
    const MODE2_IRQ_DOT: u16 = 452;

    pub fn new() -> Self {
        Self {
            // The first scanline after LCD enable is shorter than normal:
            // the PPU effectively starts at dot 4 rather than dot 0.
            // This is documented in SameBoy ("+8 extra cycles_for_line"
            // compensation) and verified by Blargg's oam_bug/1-lcd_sync test:
            // after 110 M-cycles (452 dots) LY must have incremented to 1,
            // which requires the first scanline to be ≤ 452 dots.
            dot: 4,
            scanline: 0,
            mode: PpuMode::HBlank,
            frame_ready: false,
            first_scanline_after_enable: true,
            mode_for_irq: -1,
            mode3_extra_dots: 0,
        }
    }

    /// Advance timing by one dot and return any events that occurred.
    ///
    /// `lyc` — the current LYC register value (for coincidence detection).
    pub fn tick_dot(&mut self, _lyc: u8) -> DotEvents {
        let mut events = DotEvents::default();

        self.dot += 1;
        if self.dot >= Self::DOTS_PER_SCANLINE {
            self.dot = 0;
            self.scanline += 1;
            if self.scanline >= Self::TOTAL_SCANLINES {
                self.scanline = 0;
                self.frame_ready = true;
                events.new_frame = true;
                // Reset mode3_extra_dots at the start of each new frame.
                self.mode3_extra_dots = 0;
            }
        }

        let mode3_end = Self::OAM_SCAN_DOTS + Self::PIXEL_TRANSFER_DOTS + self.mode3_extra_dots;

        // Determine mode from current dot/scanline position.
        let new_mode = if self.scanline >= Self::VBLANK_START_LINE {
            PpuMode::VBlank
        } else if self.first_scanline_after_enable
            && self.scanline == 0
            && self.dot < Self::OAM_SCAN_DOTS
        {
            // First scanline after LCD enable: Mode 0 instead of Mode 2.
            PpuMode::HBlank
        } else if self.dot < Self::OAM_SCAN_DOTS {
            PpuMode::OamScan
        } else if self.dot < mode3_end {
            PpuMode::PixelTransfer
        } else {
            PpuMode::HBlank
        };

        // Clear the first-scanline flag once the first scanline ends.
        if self.first_scanline_after_enable && self.scanline > 0 {
            self.first_scanline_after_enable = false;
        }

        if new_mode != self.mode {
            events.mode_changed = true;
            if new_mode == PpuMode::HBlank {
                events.render_scanline = true;
                // mode_for_irq was already set to 0 four dots earlier (at mode0_irq_dot).
                // Set it again here for any scanline where mode0_irq_dot may have been skipped
                // (e.g. VBlank scanlines don't reach mode0_irq_dot on visible scanlines).
                self.mode_for_irq = 0;
                // Extra dots were consumed for this scanline; reset for the next.
                self.mode3_extra_dots = 0;
            }
            if new_mode == PpuMode::VBlank {
                events.vblank_start = true;
                self.mode_for_irq = 1;
            }
            if new_mode == PpuMode::OamScan {
                // Mode bits just changed to Mode 2. mode_for_irq was already set to 2
                // four dots earlier (at dot 452). Keep it at 2.
                // (mode_for_irq was set to 2 at dot 452 of the previous scanline)
            }
            if new_mode == PpuMode::PixelTransfer {
                // Mode 3 has no STAT IRQ source; set to -1 to suppress mode IRQs.
                // (The Mode 2 source has already fired at dot 452 / dot 0.)
                self.mode_for_irq = -1;
            }
            self.mode = new_mode;
        }

        // Mode 2 STAT IRQ source activates 4 dots before mode bits change to Mode 2.
        // Dot 452 of any scanline whose next scanline starts with Mode 2:
        //   - Scanlines 0–142 (next = 1–143, all visible with Mode 2)
        //   - Scanline 153 (next = 0, visible with Mode 2)
        // Scanlines 143 (next = 144, VBlank) and 144–152 (VBlank) are excluded.
        if self.dot == Self::MODE2_IRQ_DOT {
            let next_scanline_has_mode2 = self.scanline < Self::VBLANK_START_LINE - 1
                || self.scanline == Self::TOTAL_SCANLINES - 1;
            if next_scanline_has_mode2 {
                self.mode_for_irq = 2;
            }
        }

        // Mode 0 STAT IRQ source activates 4 dots before mode bits change to HBlank,
        // symmetric with Mode 2. Only on visible scanlines (0–143).
        let mode0_irq_dot = mode3_end - 4;
        if self.dot == mode0_irq_dot && self.scanline < Self::VBLANK_START_LINE {
            self.mode_for_irq = 0;
        }

        events
    }

    pub fn mode(&self) -> PpuMode {
        self.mode
    }

    /// Current scanline (LY register value).
    pub fn ly(&self) -> u8 {
        self.scanline
    }

    pub fn dot(&self) -> u16 {
        self.dot
    }

    /// Whether the PPU is on the first scanline after LCD enable.
    ///
    /// During this scanline, Mode 0 is reported instead of Mode 2,
    /// and STAT mode interrupts are suppressed.
    pub fn is_first_scanline_after_enable(&self) -> bool {
        self.first_scanline_after_enable
    }

    pub fn is_frame_ready(&self) -> bool {
        self.frame_ready
    }

    pub fn clear_frame_ready(&mut self) {
        self.frame_ready = false;
    }

    /// Returns the mode whose STAT IRQ source is currently active.
    ///
    /// -1 = all mode IRQ sources suppressed (first scanline after LCD enable).
    /// 0–3 = the mode whose source is active.
    ///
    /// This mirrors SameBoy's `mode_for_interrupt`. Mode 2 source activates
    /// 4 T-cycles before the STAT mode bits change to Mode 2.
    pub fn mode_for_irq(&self) -> i8 {
        self.mode_for_irq
    }

    /// Set the number of extra dots to add to Mode 3 (OBJ/SCX/window penalties).
    ///
    /// Call at the Mode 2→Mode 3 transition (dot 80 of each visible scanline).
    pub fn set_mode3_extra_dots(&mut self, extra: u16) {
        self.mode3_extra_dots = extra;
    }
}

impl Default for Timing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tick_n(timing: &mut Timing, n: u32, lyc: u8) -> DotEvents {
        let mut last = DotEvents::default();
        for _ in 0..n {
            last = timing.tick_dot(lyc);
        }
        last
    }

    #[test]
    fn test_initial_mode_is_hblank_after_lcd_enable() {
        // Given: a freshly created Timing (simulates LCD just enabled)
        let timing = Timing::new();
        // Then: initial mode is HBlank (first scanline after LCD enable
        // does not have a Mode 2 OAM Scan period)
        assert_eq!(timing.mode(), PpuMode::HBlank);
        assert!(timing.is_first_scanline_after_enable());
        // And the PPU starts at dot 4 (first scanline is shorter)
        assert_eq!(timing.dot(), 4);
    }

    #[test]
    fn test_initial_ly_is_zero() {
        let timing = Timing::new();
        assert_eq!(timing.ly(), 0);
    }

    #[test]
    fn test_first_scanline_stays_hblank_for_80_dots_then_pixel_transfer() {
        // Given: fresh timing (first scanline after LCD enable, starting at dot 4)
        let mut timing = Timing::new();
        // When: tick 75 dots (to dot 79) — still in HBlank (not Mode 2 on first scanline)
        tick_n(&mut timing, 75, 0xFF);
        assert_eq!(timing.dot(), 79);
        assert_eq!(timing.mode(), PpuMode::HBlank);
        // When: tick 1 more dot (dot 80)
        timing.tick_dot(0xFF);
        // Then: mode is Pixel Transfer (Mode 3)
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
    }

    #[test]
    fn test_second_scanline_has_normal_oam_scan() {
        // Given: timing advanced past the first scanline (452 dots = 456 - 4 initial offset)
        let mut timing = Timing::new();
        tick_n(&mut timing, 452, 0xFF); // complete first scanline
        // Then: second scanline starts with normal Mode 2 (OAM Scan)
        assert_eq!(timing.ly(), 1);
        assert_eq!(timing.mode(), PpuMode::OamScan);
        assert!(!timing.is_first_scanline_after_enable());
    }

    #[test]
    fn test_pixel_transfer_runs_for_172_dots_then_transitions_to_hblank() {
        // Given: timing at start of Mode 3 (dot 80, reached by ticking 76 dots from dot 4)
        let mut timing = Timing::new();
        tick_n(&mut timing, 76, 0xFF); // enter Mode 3 (dot 4 + 76 = dot 80)
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
        // When: tick 171 more dots (still in Mode 3)
        tick_n(&mut timing, 171, 0xFF);
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
        // When: tick 1 more dot (dot 252)
        timing.tick_dot(0xFF);
        // Then: mode is H-Blank
        assert_eq!(timing.mode(), PpuMode::HBlank);
    }

    #[test]
    fn test_hblank_ends_at_dot_456_and_ly_increments() {
        // Given: timing at start of H-Blank (dot 252, first scanline)
        let mut timing = Timing::new();
        tick_n(&mut timing, 248, 0xFF); // dot 4 + 248 = 252
        assert_eq!(timing.mode(), PpuMode::HBlank);
        let ly_before = timing.ly();
        // When: tick remaining dots to complete the scanline (456 - 252 = 204)
        tick_n(&mut timing, 204, 0xFF);
        // Then: LY incremented and we are in Mode 2 of next scanline
        assert_eq!(timing.ly(), ly_before + 1);
        assert_eq!(timing.mode(), PpuMode::OamScan);
        // And the first-scanline flag is cleared
        assert!(!timing.is_first_scanline_after_enable());
    }

    #[test]
    fn test_vblank_starts_at_scanline_144() {
        // Given: timing; when: tick enough dots for 144 complete scanlines
        // First scanline is 452 dots (starts at dot 4), remaining 143 are 456 each
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 456 * 143, 0xFF);
        // Then: now in V-Blank, LY == 144
        assert_eq!(timing.ly(), 144);
        assert_eq!(timing.mode(), PpuMode::VBlank);
    }

    #[test]
    fn test_vblank_fires_event_on_scanline_144_entry() {
        // Given: timing at the last dot of scanline 143 (one dot before VBlank)
        // First scanline: 452 dots, scanlines 1-143: 456 * 143 dots, minus 1
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 456 * 143 - 1, 0xFF);
        assert_eq!(timing.ly(), 143);
        // When: tick the final dot that advances to scanline 144
        let events = timing.tick_dot(0xFF);
        // Then: vblank_start event fires
        assert!(events.vblank_start);
    }

    #[test]
    fn test_full_frame_is_154_scanlines() {
        // Given: fresh timing
        // First scanline: 452 dots, remaining 153: 456 * 153 dots = total 70,220
        let mut timing = Timing::new();
        let total_dots = 452 + 456 * 153;
        // When: tick one full frame
        tick_n(&mut timing, total_dots - 1, 0xFF);
        assert!(!timing.is_frame_ready());
        timing.tick_dot(0xFF);
        // Then: frame is ready and LY wraps to 0
        assert!(timing.is_frame_ready());
        assert_eq!(timing.ly(), 0);
    }

    #[test]
    fn test_vblank_mode_persists_through_scanlines_144_to_153() {
        // Given: timing at scanline 144
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 456 * 143, 0xFF);
        // When: tick through scanlines 144–153
        for expected_ly in 144..=153u8 {
            assert_eq!(timing.ly(), expected_ly);
            assert_eq!(timing.mode(), PpuMode::VBlank);
            tick_n(&mut timing, 456, 0xFF);
        }
    }

    #[test]
    fn test_render_scanline_event_fires_on_hblank_entry() {
        // Given: timing at dot 251 (last dot of Mode 3 on scanline 0)
        let mut timing = Timing::new();
        tick_n(&mut timing, 247, 0xFF); // dot 4 + 247 = 251
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
        // When: tick one more dot
        let events = timing.tick_dot(0xFF);
        // Then: render_scanline fires
        assert!(events.render_scanline);
        assert_eq!(timing.mode(), PpuMode::HBlank);
    }

    #[test]
    fn test_lyc_ly_match_detected_correctly() {
        // Given: timing running with lyc = 5
        let mut timing = Timing::new();
        // When: tick to scanline 5
        tick_n(&mut timing, 456 * 5, 5);
        // Then: ly == 5 == lyc
        assert_eq!(timing.ly(), 5);
        // The coincidence is checked in the PPU; timing exposes ly() for that
        // This test just ensures ly() returns the correct scanline
        assert_eq!(timing.ly(), 5);
    }

    // ── mode_for_irq timing ───────────────────────────────────────────────────

    #[test]
    fn test_mode_for_irq_is_suppressed_initially() {
        // Given: fresh Timing (first scanline after LCD enable)
        let timing = Timing::new();
        // Then: mode_for_irq = -1 (all mode STAT IRQs suppressed on first scanline)
        assert_eq!(timing.mode_for_irq(), -1);
    }

    #[test]
    fn test_mode_for_irq_becomes_2_at_dot_452_of_scanline_0() {
        // The Mode 2 STAT interrupt source must activate at dot 452 of scanline 0,
        // which is 4 dots before the STAT mode bits change to Mode 2 on scanline 1.
        //
        // Scanline 0 starts at dot 4, so 452 - 4 = 448 additional dots to reach dot 452.
        let mut timing = Timing::new(); // dot=4, scanline=0
        tick_n(&mut timing, 448, 0xFF); // advance to dot 452
        assert_eq!(timing.dot(), 452);
        assert_eq!(timing.scanline, 0);
        assert_eq!(
            timing.mode_for_irq(),
            2,
            "mode_for_irq must be 2 at dot 452 (4 dots before Mode 2 mode bits on scanline 1)"
        );
    }

    #[test]
    fn test_mode_for_irq_not_2_at_dot_451_of_scanline_0() {
        // One dot before dot 452: mode_for_irq should NOT yet be 2.
        let mut timing = Timing::new(); // dot=4
        tick_n(&mut timing, 447, 0xFF); // advance to dot 451
        assert_eq!(timing.dot(), 451);
        assert_ne!(
            timing.mode_for_irq(),
            2,
            "mode_for_irq must not be 2 before dot 452"
        );
    }

    #[test]
    fn test_mode_for_irq_becomes_0_when_mode_0_starts() {
        // mode_for_irq must equal 0 when the STAT mode bits change to HBlank.
        // On scanline 1, HBlank starts at dot 252.
        // Note: mode_for_irq is first set to 0 at dot 248 (4 dots early), so
        // at dot 252 it is already 0 (set a second time redundantly).
        // First scanline is 452 dots (starting at dot 4). Scanline 1 starts at dot 0.
        // Tick 452 (scanline 0 end) + 252 (HBlank start on scanline 1) = 704 dots.
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 252, 0xFF);
        assert_eq!(timing.scanline, 1);
        assert_eq!(timing.dot(), 252);
        assert_eq!(timing.mode(), PpuMode::HBlank);
        assert_eq!(
            timing.mode_for_irq(),
            0,
            "mode_for_irq must be 0 when mode bits show HBlank"
        );
    }

    #[test]
    fn test_mode_for_irq_becomes_0_at_dot_248_before_hblank() {
        // The Mode 0 STAT interrupt source must activate at dot 248 of scanline 1,
        // which is 4 dots before the STAT mode bits change to HBlank at dot 252.
        // This is symmetric with Mode 2 firing at dot 452 (4 dots before Mode 2 bits).
        // First scanline: 452 dots. Scanline 1 starts at dot 0.
        // Tick 452 (scanline 0 end) + 248 = 700 dots.
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 248, 0xFF);
        assert_eq!(timing.scanline, 1);
        assert_eq!(timing.dot(), 248);
        assert_eq!(
            timing.mode(),
            PpuMode::PixelTransfer,
            "Mode bits must still show PixelTransfer at dot 248 (HBlank starts at 252)"
        );
        assert_eq!(
            timing.mode_for_irq(),
            0,
            "mode_for_irq must be 0 at dot 248 (4 dots before Mode 0 mode bits)"
        );
    }

    #[test]
    fn test_mode_for_irq_becomes_0_at_dot_247_not_early() {
        // One dot before the early Mode 0 fire: mode_for_irq should NOT yet be 0.
        // At dot 247 of scanline 1, mode_for_irq is still -1 (PixelTransfer suppressed).
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 247, 0xFF);
        assert_eq!(timing.scanline, 1);
        assert_eq!(timing.dot(), 247);
        assert_ne!(
            timing.mode_for_irq(),
            0,
            "mode_for_irq must not be 0 before dot 248"
        );
    }

    #[test]
    fn test_mode_for_irq_is_not_2_at_end_of_scanline_143() {
        // Scanline 143 transitions to VBlank, not Mode 2.
        // mode_for_irq must NOT fire Mode 2 at dot 452 of scanline 143.
        //
        // First scanline: 452 dots. Scanlines 1-143: 456 * 143 = 65208 dots.
        // Total to end of scanline 143: 452 + 65208 = 65660 dots.
        // Dot 452 of scanline 143 = 452 + 65208 - 456 + 452 = ...
        // Scanline 143 starts at dot 0 after tick 452 + 456 * 142 dots.
        // dot 452 of scanline 143 = tick(452 + 456 * 142 + 452).
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 456 * 142 + 452, 0xFF);
        assert_eq!(timing.scanline, 143);
        assert_eq!(timing.dot(), 452);
        assert_ne!(
            timing.mode_for_irq(),
            2,
            "mode_for_irq must not be 2 at dot 452 of scanline 143 (next is VBlank)"
        );
    }

    #[test]
    fn test_mode_for_irq_becomes_2_at_dot_452_of_scanline_153() {
        // Scanline 153 transitions to scanline 0 (visible), so mode_for_irq
        // must be 2 at dot 452 of scanline 153.
        //
        // First scanline: 452 dots. Scanlines 1-153: 456 * 153 = 69768 dots.
        // dot 452 of scanline 153 = tick(452 + 456 * 153 - 456 + 452).
        // = tick(452 + 456*152 + 452)
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 456 * 152 + 452, 0xFF);
        assert_eq!(timing.scanline, 153);
        assert_eq!(timing.dot(), 452);
        assert_eq!(
            timing.mode_for_irq(),
            2,
            "mode_for_irq must be 2 at dot 452 of scanline 153 (next is scanline 0 with Mode 2)"
        );
    }

    #[test]
    fn test_mode3_extra_dots_shifts_hblank_start() {
        // When mode3_extra_dots = 10, HBlank should start at dot 262 (not 252).
        let mut timing = Timing::new();
        tick_n(&mut timing, 452, 0xFF); // complete first scanline
        assert_eq!(timing.scanline, 1);
        timing.set_mode3_extra_dots(10);
        tick_n(&mut timing, 252, 0xFF); // would normally be HBlank, but +10 extra dots
        assert_eq!(timing.dot(), 252);
        assert_eq!(
            timing.mode(),
            PpuMode::PixelTransfer,
            "With mode3_extra_dots=10, mode 3 should still be active at dot 252"
        );
        tick_n(&mut timing, 10, 0xFF); // advance past extra dots
        assert_eq!(timing.dot(), 262);
        assert_eq!(
            timing.mode(),
            PpuMode::HBlank,
            "With mode3_extra_dots=10, HBlank should start at dot 262"
        );
    }
}
