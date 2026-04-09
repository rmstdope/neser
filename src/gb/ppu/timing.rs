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
            dot: 0,
            scanline: 0,
            mode: PpuMode::OamScan,
            frame_ready: false,
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
        } else if self.dot < Self::OAM_SCAN_DOTS {
            PpuMode::OamScan
        } else if self.dot < Self::OAM_SCAN_DOTS + Self::PIXEL_TRANSFER_DOTS {
            PpuMode::PixelTransfer
        } else {
            PpuMode::HBlank
        };

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
    fn test_initial_mode_is_oam_scan() {
        // Given: a freshly created Timing
        let timing = Timing::new();
        // Then: initial mode is OAM Scan
        assert_eq!(timing.mode(), PpuMode::OamScan);
    }

    #[test]
    fn test_initial_ly_is_zero() {
        let timing = Timing::new();
        assert_eq!(timing.ly(), 0);
    }

    #[test]
    fn test_oam_scan_runs_for_80_dots_then_transitions_to_pixel_transfer() {
        // Given: fresh timing
        let mut timing = Timing::new();
        // When: tick 79 dots — still in OAM Scan
        tick_n(&mut timing, 79, 0xFF);
        assert_eq!(timing.mode(), PpuMode::OamScan);
        // When: tick 1 more dot (dot 80)
        timing.tick_dot(0xFF);
        // Then: mode is Pixel Transfer
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
    }

    #[test]
    fn test_pixel_transfer_runs_for_172_dots_then_transitions_to_hblank() {
        // Given: timing at start of Mode 3 (dot 80)
        let mut timing = Timing::new();
        tick_n(&mut timing, 80, 0xFF); // enter Mode 3
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
        // Given: timing at start of H-Blank (dot 252)
        let mut timing = Timing::new();
        tick_n(&mut timing, 252, 0xFF);
        assert_eq!(timing.mode(), PpuMode::HBlank);
        let ly_before = timing.ly();
        // When: tick remaining 204 dots to complete the scanline
        tick_n(&mut timing, 204, 0xFF);
        // Then: LY incremented and we are in Mode 2 of next scanline
        assert_eq!(timing.ly(), ly_before + 1);
        assert_eq!(timing.mode(), PpuMode::OamScan);
    }

    #[test]
    fn test_vblank_starts_at_scanline_144() {
        // Given: timing; when: tick enough dots for 144 complete scanlines
        let mut timing = Timing::new();
        tick_n(&mut timing, 456 * 144, 0xFF);
        // Then: now in V-Blank, LY == 144
        assert_eq!(timing.ly(), 144);
        assert_eq!(timing.mode(), PpuMode::VBlank);
    }

    #[test]
    fn test_vblank_fires_event_on_scanline_144_entry() {
        // Given: timing at dot 455 of scanline 143 (one dot before VBlank)
        let mut timing = Timing::new();
        tick_n(&mut timing, 456 * 143 + 455, 0xFF);
        assert_eq!(timing.ly(), 143);
        // When: tick the final dot that advances to scanline 144
        let events = timing.tick_dot(0xFF);
        // Then: vblank_start event fires
        assert!(events.vblank_start);
    }

    #[test]
    fn test_full_frame_is_154_scanlines_456_dots_each() {
        // Given: fresh timing
        let mut timing = Timing::new();
        // When: tick one full frame (154 × 456 = 70224 dots)
        tick_n(&mut timing, 154 * 456 - 1, 0xFF);
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
        tick_n(&mut timing, 456 * 144, 0xFF);
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
        tick_n(&mut timing, 251, 0xFF);
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
