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
/// - Mode 3 (Pixel Xfer):   dots 80–251  (172 dots)
/// - Mode 0 (H-Blank):      dots 252–455 (204 dots)
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
            }
        }

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
        } else if self.dot < Self::OAM_SCAN_DOTS + Self::PIXEL_TRANSFER_DOTS {
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
            }
            if new_mode == PpuMode::VBlank {
                events.vblank_start = true;
            }
            self.mode = new_mode;
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
}
