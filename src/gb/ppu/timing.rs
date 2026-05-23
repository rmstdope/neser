use serde::{Deserialize, Serialize};

use crate::trace_ppu;

/// DMG PPU operating modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
/// DMG PPU mode schedule per scanline:
///
/// Scan 0 (first_scanline_after_enable, LY=0, 452 dots starting at dot=4):
///   dots 4–83:    Mode 0 (HBlank; no OAM Scan period)
///   dots 84–255:  Mode 3 (Pixel Transfer; OAM+VRAM blocked)
///   dots 256–455: Mode 0 (HBlank)
///   STAT mode bits: physical mode (no lag).
///   OAM/VRAM read blocked: [84, 256).  OAM/VRAM write blocked: [84, 256).
///
/// Scan 1 (second_scanline_after_enable, LY=1, 456 dots):
///   Physical mode: HBlank [0,4), OamScan [4,80), PixelTransfer [80,256+extra), HBlank
///   STAT mode bits: Mode2 (OAM Scan) is immediate; Mode3 lags 4T via stat_mode snapshot;
///                   Mode0 is immediate.
///   OAM read blocked:  [0, 256+extra)         — conservative latch from Mode3 end.
///   OAM write blocked: [4,80) ∪ [84,256+extra) — write gate has 4T delayed lock/unlock.
///   VRAM read blocked: [80, 256+extra).
///   VRAM write blocked:[84, 256+extra).
///
/// Scan 2+ (regular scans, LY=2+, 456 dots each):
///   Physical mode: OamScan [0,80), PixelTransfer [80,252+extra), HBlank [252+extra,456)
///   STAT mode bits: physical mode (no lag for the mode bits themselves).
///   OAM read blocked:  [0, 252+extra).
///   OAM write blocked: [4,80) ∪ [84,252+extra) — write gate has 4T delayed lock/unlock.
///   VRAM read blocked: [80, 252+extra).
///   VRAM write blocked:[84, 252+extra).
///
/// VBlank: scanlines 144–153 (all Mode 1; 4560 dots total).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timing {
    dot: u16,
    scanline: u8,
    mode: PpuMode,
    /// Snapshot of `mode` taken 4 T-cycles ago (one M-cycle).
    ///
    /// Used on scan 1 for Mode2/3 transitions. `save_stat_mode()` is called once per
    /// M-cycle (before each group of 4 dot ticks in `Ppu::tick_dots`), providing the
    /// 4T lag the STAT register shows for Mode2/3 transitions on scan 1.
    stat_mode: PpuMode,
    frame_ready: bool,
    /// True during the first scanline after LCD is enabled (scan 0).
    first_scanline_after_enable: bool,
    /// True during the second scanline after LCD is enabled (scan 1).
    ///
    /// Scan 1 is a "compensation" scanline: physical OamScan starts at dot=4 (not 0)
    /// and ends at dot=80 (same as scan 2+). Mode3 runs [80,256) — 4 extra dots.
    /// STAT Mode2/3 use the 4T stat_mode lag; Mode0 is shown immediately.
    second_scanline_after_enable: bool,
    /// True during the third scanline after LCD is enabled (scan 2, LY=2).
    ///
    /// Scan 2 is the first "normal" scanline (OamScan [0,80), Mode3 [80,252),
    /// HBlank [252,456)), but STAT still shows a 4T lag at Mode 2 and Mode 3 starts.
    /// From scan 3 onwards, STAT uses physical mode for all transitions.
    third_scanline_after_enable: bool,
    /// (-1 = suppress all mode IRQs,
    /// 0-3 = mode whose STAT IRQ source is currently active).
    ///
    /// This differs from the STAT mode bits:
    /// - Mode 2 source becomes active 4 T-cycles before mode bits show Mode 2.
    /// - Mode 0 source becomes active 4 T-cycles before mode bits show Mode 0.
    mode_for_irq: i8,
    /// Extra dots added to Mode 3 (OBJ/SCX/window penalties).
    mode3_extra_dots: u16,
    /// The LY register value exposed to the CPU.
    ///
    /// On DMG, LY increments 4 T-cycles before the physical scanline boundary
    /// (simultaneously with the Mode 2 STAT source firing at `MODE2_IRQ_DOT`).
    /// For scanlines 144+ (VBlank) and the frame wrap (153→0), LY increments at the
    /// physical dot-456 boundary instead (no early Mode 2 fire during VBlank).
    ly: u8,
    /// Total number of completed frames since emulation started.
    ///
    /// Increments when LY wraps from 153 to 0 (frame boundary).
    /// Used for CPU tracing and debugging to correlate events with frame numbers.
    frame_count: u64,
    #[serde(default = "default_line_153_ly_zero_dot")]
    line_153_ly_zero_dot: u16,
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

fn default_line_153_ly_zero_dot() -> u16 {
    8
}

impl Timing {
    const DOTS_PER_SCANLINE: u16 = 456;
    const TOTAL_SCANLINES: u8 = 154;
    const VBLANK_START_LINE: u8 = 144;
    /// Dot at which Mode 2 (OAM Scan) STAT bits appear on scan 1 and scan 2+.
    /// On scan 1, this is also the physical OamScan start.
    /// On scan 2+, physical OamScan starts at dot 0 but STAT shows Mode 2 from dot 4.
    const OAM_SCAN_START: u16 = 4;
    const OAM_SCAN_DOTS: u16 = 80;
    const PIXEL_TRANSFER_DOTS: u16 = 172;
    /// Dot at which the Mode 2 STAT IRQ source fires (4 dots before mode bits change to Mode 2).
    const MODE2_IRQ_DOT: u16 = 452;

    pub fn new() -> Self {
        Self {
            // PPU starts at dot 4 when LCD is first enabled (DMG).
            // The first scanline is 452 dots long (dot 4 through dot 455).
            dot: 4,
            scanline: 0,
            mode: PpuMode::HBlank,
            stat_mode: PpuMode::HBlank,
            frame_ready: false,
            first_scanline_after_enable: true,
            second_scanline_after_enable: false,
            third_scanline_after_enable: false,
            mode_for_irq: -1,
            mode3_extra_dots: 0,
            ly: 0,
            frame_count: 0,
            line_153_ly_zero_dot: default_line_153_ly_zero_dot(),
        }
    }

pub(crate) fn set_line_153_ly_zero_dot(&mut self, dot: u16) {
    debug_assert!(dot < Self::DOTS_PER_SCANLINE);
    self.line_153_ly_zero_dot = dot;
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
                trace_ppu!(1; "frame wrap y={} dot={} frame={}",
                    self.scanline, self.dot, self.frame_count + 1);
                self.scanline = 0;
                self.frame_ready = true;
                events.new_frame = true;
                self.mode3_extra_dots = 0;
                self.frame_count += 1;
            }
            // For VBlank scans (144+) and the frame wrap (153→0), LY increments
            // at the physical dot boundary (no early MODE2_IRQ_DOT increment).
            self.ly = self.scanline;
        }

        // Advance scan-type state machine.
        // Scan 0→1→2→3+ transitions on scanline increment.
        if self.first_scanline_after_enable && self.scanline > 0 {
            self.first_scanline_after_enable = false;
            self.second_scanline_after_enable = true;
        } else if self.second_scanline_after_enable && self.scanline > 1 {
            self.second_scanline_after_enable = false;
            self.third_scanline_after_enable = true;
        } else if self.third_scanline_after_enable && self.scanline > 2 {
            self.third_scanline_after_enable = false;
        }

        // Compute mode boundaries based on scan type.
        //
        // Scan 0: no OamScan; Mode3 starts at dot=84, ends at dot=256+extra.
        // Scan 1: OamScan [4,80); Mode3 starts at dot=80, ends at dot=256+extra.
        //   (OamScan is 76 dots: 4T late start, same end as scan 2+.)
        // Scan 2+: OamScan [0,80); Mode3 starts at dot=80, ends at dot=252+extra.
        let mode3_start = if self.first_scanline_after_enable {
            // Scan 0: no OamScan; HBlank fills [dot_start, 84), Mode3 from 84.
            Self::OAM_SCAN_START + Self::OAM_SCAN_DOTS // 84
        } else {
            // Scan 1 and scan 2+: Mode3 starts at dot 80.
            Self::OAM_SCAN_DOTS // 80
        };
        let mode3_end = self.mode3_end();

        // Determine physical mode from current dot/scanline position.
        let new_mode = if self.scanline >= Self::VBLANK_START_LINE {
            PpuMode::VBlank
        } else if self.first_scanline_after_enable {
            // Scan 0: HBlank for [4,84), PixelTransfer for [84,256), HBlank for [256,456).
            // (dot starts at 4; dots 0-3 are not reached on scan 0.)
            if self.dot < mode3_start {
                PpuMode::HBlank
            } else if self.dot < mode3_end {
                PpuMode::PixelTransfer
            } else {
                PpuMode::HBlank
            }
        } else if self.second_scanline_after_enable {
            // Scan 1: HBlank [0,4), OamScan [4,80), PixelTransfer [80,256), HBlank [256,456).
            if self.dot < Self::OAM_SCAN_START {
                PpuMode::HBlank
            } else if self.dot < mode3_start {
                PpuMode::OamScan
            } else if self.dot < mode3_end {
                PpuMode::PixelTransfer
            } else {
                PpuMode::HBlank
            }
        } else {
            // Scan 2+: physical OamScan [0,80), PixelTransfer [80,252+extra), HBlank [252+extra,456).
            if self.dot < mode3_start {
                PpuMode::OamScan
            } else if self.dot < mode3_end {
                PpuMode::PixelTransfer
            } else {
                PpuMode::HBlank
            }
        };

        if new_mode != self.mode {
            events.mode_changed = true;
            if new_mode == PpuMode::HBlank {
                events.render_scanline = true;
                // mode_for_irq=0 is set 4 dots early (at mode3_end-4), not here.
                // Extra dots were consumed for this scanline; reset for the next.
                self.mode3_extra_dots = 0;
                trace_ppu!(1; "mode0 enter y={} dot={}", self.ly, self.dot);
            }
            if new_mode == PpuMode::VBlank {
                events.vblank_start = true;
                self.mode_for_irq = 1;
                trace_ppu!(1; "vblank enter y={} dot={}", self.ly, self.dot);
            }
            if new_mode == PpuMode::PixelTransfer {
                // Mode 3 has no STAT IRQ source; set to -1 to suppress mode IRQs.
                self.mode_for_irq = -1;
                trace_ppu!(1; "mode3 enter y={} dot={}", self.ly, self.dot);
            }
            if new_mode == PpuMode::OamScan {
                trace_ppu!(1; "mode2 enter y={} dot={}", self.ly, self.dot);
            }
            // Detect vblank exit: transition from VBlank to non-VBlank mode
            if self.mode == PpuMode::VBlank && new_mode != PpuMode::VBlank {
                trace_ppu!(1; "vblank exit y={} dot={}", self.ly, self.dot);
            }
            self.mode = new_mode;
        }

        // Mode 2 STAT IRQ source activates 4 dots before mode bits change to Mode 2.
        // Dot 452 of visible scanlines 0-142 (next scanline starts with Mode 2).
        // Scanlines 143 (next = 144, VBlank) and 144-152 (VBlank) are excluded.
        // Scanline 153 (frame wrap) is also excluded: it fires at dot=0 of scan 0 below.
        // LY also increments here — 4 T-cycles before the physical scan boundary — to
        // match DMG hardware where LY changes simultaneously with the Mode 2 STAT fire.
        // Exception: scan 0 and scan 1 (the first two scans after LCD enable) retain the
        // physical boundary increment to preserve lcdon_timing-GS pass behaviour.
        if self.dot == Self::MODE2_IRQ_DOT && self.scanline < Self::VBLANK_START_LINE - 1 {
            self.mode_for_irq = 2;
            if !self.first_scanline_after_enable && !self.second_scanline_after_enable {
                self.ly = self.scanline + 1;
            }
        }

        // At frame wrap (scan 153 → scan 0, dot=0), fire the Mode 2 IRQ source.
        // On real hardware, this fires 4 T-cycles later than the dot-452 path (i.e., at the
        // actual moment scan 0 begins), which is required for intr_1_2_timing-GS to pass.
        if self.dot == 0 && self.scanline == 0 {
            self.mode_for_irq = 2;
        }

        // During the last VBlank line, LY begins reading and comparing as 0
        // before the physical frame boundary. Raster code can use LYC=0 here
        // to start work for the next visible frame before scanline 0 begins.
        if self.scanline == Self::TOTAL_SCANLINES - 1 && self.dot == self.line_153_ly_zero_dot {
            self.ly = 0;
        }

        // Mode 0 STAT IRQ source fires 4 T-cycles before HBlank physically starts (DMG).
        // This matches intr_2_0_timing-GS (B counts loop iterations between Mode2 and Mode0
        // dispatches) and is consistent with hardware measurements showing the 4-dot lead.
        if self.dot == mode3_end - 4 && self.scanline < Self::VBLANK_START_LINE {
            self.mode_for_irq = 0;
        }

        events
    }

    pub fn mode(&self) -> PpuMode {
        self.mode
    }

    /// Snapshot the current mode into `stat_mode` for the 4T-late STAT register.
    ///
    /// Call once per M-cycle (before each group of 4 dot ticks) in `Ppu::tick_dots`.
    /// On scan 1 and scan 2+ the STAT mode bits lag the physical mode by one M-cycle
    /// (4 T-cycles); this snapshot provides that delay.
    /// On scan 0 (`first_scanline_after_enable`) the bits are immediate and
    /// `mode_for_stat()` ignores `stat_mode` entirely.
    pub fn save_stat_mode(&mut self) {
        self.stat_mode = self.mode;
    }

    /// Returns the mode to report in the STAT register mode bits.
    ///
    /// Rules derived from the lcdon_timing-GS ROM test on DMG hardware:
    ///
    /// - Scan 0 (`first_scanline_after_enable`): physical mode. Mode 3 starts at
    ///   dot=84 with no lag.
    /// - VBlank (scanlines 144–153): physical mode (VBlank is immediate).
    /// - HBlank (any scan): physical (Mode 0 always appears immediately).
    /// - Scan 1 (`second_scanline_after_enable`): HBlank and OamScan are physical
    ///   (immediate). Only PixelTransfer uses the 4T-lagged `stat_mode` snapshot.
    /// - Scan 2 (`third_scanline_after_enable`): HBlank is physical; OamScan and
    ///   PixelTransfer use the 4T-lagged `stat_mode` snapshot.
    /// - Scan 3+ (normal operation): physical mode for all transitions. The
    ///   LCD-enable initialization lag has worn off by this point.
    pub fn mode_for_stat(&self) -> PpuMode {
        // Use the 4T-lagged stat_mode snapshot only for:
        //   scan1 PixelTransfer, and scan2 OamScan/PixelTransfer.
        // Everything else (scan0, VBlank, HBlank, scan1 OamScan, scan3+) uses
        // the physical mode immediately.
        let use_lag = (self.second_scanline_after_enable && self.mode == PpuMode::PixelTransfer)
            || (self.third_scanline_after_enable && self.mode != PpuMode::HBlank);
        if use_lag { self.stat_mode } else { self.mode }
    }

    /// Returns whether VRAM is blocked for CPU **read** access at the current dot.
    ///
    /// Scan 0: blocked during [84, 256+extra) — no OamScan, Mode3 is [84, 256+extra).
    /// Scan 1: blocked during [80, 256+extra) — physical Mode3 starts at dot=80.
    /// Scan 2+: blocked during [80, 252+extra) — physical Mode3 [80,252+extra).
    ///
    /// Read access follows the STAT mode bits (no lag for the CPU read gate).
    pub fn is_vram_blocked(&self) -> bool {
        if self.scanline >= Self::VBLANK_START_LINE {
            return false;
        }
        let vram_start = if self.first_scanline_after_enable {
            84u16
        } else {
            80
        };
        self.dot >= vram_start && self.dot < self.mode3_end()
    }

    /// Returns whether VRAM is blocked for CPU **write** access at the current dot.
    ///
    /// Identical to `is_vram_blocked` for scan 0: blocked during [84, 256+extra).
    /// Scan 1 and scan 2+: the write gate lags the read gate by 4T — Mode3 physically
    /// locks writes 4 dots later than it locks reads.  This matches the lcdon_write_timing-GS
    /// test on DMG: at dot=80 VRAM reads return $FF but writes succeed, at dot=84 writes fail.
    pub fn is_vram_write_blocked(&self) -> bool {
        if self.scanline >= Self::VBLANK_START_LINE {
            return false;
        }
        self.dot >= 84 && self.dot < self.mode3_end()
    }

    /// Returns whether OAM is blocked for CPU **read** access at the current dot.
    ///
    /// Scan 0: blocked during [84, 256+extra) — no OamScan; only Mode3 blocks OAM.
    /// Scan 1: blocked during [0, 256+extra) — OAM is blocked from dot=0 even
    ///   though STAT shows Mode0 until dot=4. The brief HBlank [0,4) is "fake":
    ///   the OAM read bus remains blocked throughout [0, mode3_end).
    /// Scan 2+: blocked during [0, 252+extra) — physical OamScan from dot=0.
    ///
    /// Read access follows the STAT mode (or conservative latch) — no lag.
    pub fn is_oam_blocked(&self) -> bool {
        if self.scanline >= Self::VBLANK_START_LINE {
            return false;
        }
        if self.first_scanline_after_enable {
            self.dot >= 84 && self.dot < self.mode3_end()
        } else {
            self.dot < self.mode3_end()
        }
    }

    /// Returns whether OAM is blocked for CPU **write** access at the current dot.
    ///
    /// Scan 0: blocked during [84, 256+extra) — no OamScan; only Mode3 blocks OAM writes.
    /// Scan 1: blocked during [4, 80) ∪ [84, 256+extra).
    ///   - Physical OAM scan write-lock starts at dot=4, ends at dot=80.
    ///   - Mode3 write-lock starts 4T after STAT Mode3 (dot=84, not dot=80).
    ///   - Gaps [0,4) and [80,84) are accessible for writes on DMG.
    ///
    /// Scan 2+: blocked during [4, 80) ∪ [84, 252+extra).
    ///   - OAM write-lock starts at dot=4 (STAT shows Mode2 from dot=0, but write
    ///     gate lags 4T).  Mode3 write-lock starts at dot=84, not dot=80.
    pub fn is_oam_write_blocked(&self) -> bool {
        if self.scanline >= Self::VBLANK_START_LINE {
            return false;
        }
        let mode3_end = self.mode3_end();
        if self.first_scanline_after_enable {
            self.dot >= 84 && self.dot < mode3_end
        } else {
            (self.dot >= 4 && self.dot < 80) || (self.dot >= 84 && self.dot < mode3_end)
        }
    }

    /// Returns whether CGB palette RAM is blocked for CPU **read** access.
    ///
    /// Per Pan Docs: "data in palette memory cannot be read or written during
    /// the time when the PPU is reading from it, that is, Mode 3."
    ///
    /// Follows the same timing as `is_vram_blocked`:
    /// - Scan 0: blocked during [84, mode3_end)
    /// - Scan 1+: blocked during [80, mode3_end)
    pub fn is_palette_blocked(&self) -> bool {
        if self.scanline >= Self::VBLANK_START_LINE {
            return false;
        }
        let start = if self.first_scanline_after_enable {
            84u16
        } else {
            80
        };
        self.dot >= start && self.dot < self.mode3_end()
    }

    /// Returns whether CGB palette RAM is blocked for CPU **write** access.
    ///
    /// Per Pan Docs: writes to BCPD/OCPD are blocked during Mode 3, but the
    /// auto-increment of BCPS/OCPS still happens.
    ///
    /// Follows VRAM write timing with 4T lag: blocked from dot 84 (not 80).
    pub fn is_palette_write_blocked(&self) -> bool {
        if self.scanline >= Self::VBLANK_START_LINE {
            return false;
        }
        self.dot >= 84 && self.dot < self.mode3_end()
    }

    /// Current LY register value.
    ///
    /// On DMG, LY increments 4 T-cycles early (at `MODE2_IRQ_DOT`) for
    /// scanlines 0–142, simultaneously with the Mode 2 STAT source firing.
    pub fn ly(&self) -> u8 {
        self.ly
    }

    pub fn scanline(&self) -> u8 {
        self.scanline
    }

    pub fn dot(&self) -> u16 {
        self.dot
    }

    /// Whether the PPU is on the first scanline after LCD enable (scan 0).
    pub fn is_first_scanline_after_enable(&self) -> bool {
        self.first_scanline_after_enable
    }

    /// Whether the PPU is on the second scanline after LCD enable (scan 1).
    pub fn is_second_scanline_after_enable(&self) -> bool {
        self.second_scanline_after_enable
    }

    pub fn is_frame_ready(&self) -> bool {
        self.frame_ready
    }

    pub fn clear_frame_ready(&mut self) {
        self.frame_ready = false;
    }

    /// Force the frame-ready flag to `true`.
    ///
    /// Used to preserve a pending frame signal across LCD disable→enable
    /// transitions that reset the timing state.
    pub fn set_frame_ready(&mut self) {
        self.frame_ready = true;
    }

    /// Returns the total number of completed frames since emulation started.
    ///
    /// This counter increments when LY wraps from 153 to 0 (frame boundary).
    /// Used for CPU tracing and debugging to correlate events with frame numbers.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Returns the mode whose STAT IRQ source is currently active.
    ///
    /// -1 = all mode IRQ sources suppressed (first scanline after LCD enable).
    /// 0-3 = the mode whose source is active.
    ///
    /// Mode 2 source activates 4 T-cycles before the STAT mode bits change to Mode 2.
    /// Mode 0 source also activates 4 T-cycles before HBlank begins (dot 252 with no penalty).
    pub fn mode_for_irq(&self) -> i8 {
        self.mode_for_irq
    }

    /// Set the number of extra dots to add to Mode 3 (OBJ/SCX/window penalties).
    ///
    /// Call at the Mode 2→Mode 3 transition: dot 84 for scan 0 (first scanline after
    /// LCD enable), dot 80 for scan 1 and scan 2+.
    pub fn set_mode3_extra_dots(&mut self, extra: u16) {
        self.mode3_extra_dots = extra;
    }

    /// Dot at which Mode 3 (Pixel Transfer) ends for the current scan type.
    ///
    /// Scan 0 and scan 1: dot 256 + extra (OAM_SCAN_START + OAM_SCAN_DOTS + PIXEL_TRANSFER_DOTS).
    /// Scan 2+: dot 252 + extra (OAM_SCAN_DOTS + PIXEL_TRANSFER_DOTS).
    fn mode3_end(&self) -> u16 {
        if self.first_scanline_after_enable || self.second_scanline_after_enable {
            Self::OAM_SCAN_START
                + Self::OAM_SCAN_DOTS
                + Self::PIXEL_TRANSFER_DOTS
                + self.mode3_extra_dots
        } else {
            Self::OAM_SCAN_DOTS + Self::PIXEL_TRANSFER_DOTS + self.mode3_extra_dots
        }
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

    /// Tick `m_cycles` M-cycles (4 dots each), calling `save_stat_mode` before
    /// each group of 4 dots — exactly as `Ppu::tick_dots` does in production.
    fn tick_m(timing: &mut Timing, m_cycles: u32, lyc: u8) -> DotEvents {
        let mut last = DotEvents::default();
        for _ in 0..m_cycles {
            timing.save_stat_mode();
            for _ in 0..4 {
                last = timing.tick_dot(lyc);
            }
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
        // And the PPU starts at dot 4 (first scanline is 452 dots: 4–455)
        assert_eq!(timing.dot(), 4);
    }

    #[test]
    fn test_initial_ly_is_zero() {
        let timing = Timing::new();
        assert_eq!(timing.ly(), 0);
    }

    #[test]
    fn test_line_153_compares_as_ly_zero_before_frame_wrap() {
        let mut timing = Timing::new();

        tick_n(&mut timing, 452 + 152 * 456 + 7, 0xFF);
        assert_eq!(timing.scanline(), 153);
        assert_eq!(timing.ly(), 153);

        tick_n(&mut timing, 1, 0xFF);
        assert_eq!(timing.scanline(), 153);
        assert_eq!(timing.ly(), 0);
    }

    #[test]
    fn test_first_scanline_stays_hblank_for_84_dots_then_pixel_transfer() {
        // Given: fresh timing (first scanline after LCD enable, starting at dot 4)
        let mut timing = Timing::new();
        // When: tick 79 dots (to dot 83) — still in HBlank (no OamScan on first scanline)
        tick_n(&mut timing, 79, 0xFF);
        assert_eq!(timing.dot(), 83);
        assert_eq!(timing.mode(), PpuMode::HBlank);
        // When: tick 1 more dot (dot 84)
        timing.tick_dot(0xFF);
        // Then: mode is Pixel Transfer (Mode 3 starts at dot 84)
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
    }

    #[test]
    fn test_second_scanline_has_normal_oam_scan() {
        // Given: timing advanced past the first scanline (452 dots from dot 4 = wrap + 4 more)
        let mut timing = Timing::new();
        // 452 ticks from dot=4 wrap to dot=0 on scan1 (brief Mode 0 at dots 0-3).
        // 4 more ticks reach dot=4 where Mode 2 (OAM Scan) begins.
        tick_n(&mut timing, 456, 0xFF);
        // Then: scan1, dot=4, Mode 2 (OAM Scan)
        assert_eq!(timing.ly(), 1);
        assert_eq!(timing.dot(), 4);
        assert_eq!(timing.mode(), PpuMode::OamScan);
        assert!(!timing.is_first_scanline_after_enable());
    }

    #[test]
    fn test_second_scanline_brief_hblank_at_dot_0() {
        // On normal scans, dots 0-3 are a brief Mode 0 carryover before Mode 2.
        let mut timing = Timing::new();
        tick_n(&mut timing, 452, 0xFF); // dot=0 on scan1
        assert_eq!(timing.ly(), 1);
        assert_eq!(timing.dot(), 0);
        assert_eq!(
            timing.mode(),
            PpuMode::HBlank,
            "dots 0-3 on normal scans are Mode 0"
        );
    }

    #[test]
    fn test_pixel_transfer_runs_for_172_dots_then_transitions_to_hblank() {
        // Given: timing at start of Mode 3 (dot 84, reached by ticking 80 dots from dot 4)
        let mut timing = Timing::new();
        tick_n(&mut timing, 80, 0xFF); // enter Mode 3 (dot 4 + 80 = dot 84)
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
        // When: tick 171 more dots (still in Mode 3, dot 255)
        tick_n(&mut timing, 171, 0xFF);
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
        // When: tick 1 more dot (dot 256)
        timing.tick_dot(0xFF);
        // Then: mode is H-Blank (Mode 0 starts at dot 256)
        assert_eq!(timing.mode(), PpuMode::HBlank);
    }

    #[test]
    fn test_hblank_ends_at_dot_456_and_ly_increments() {
        // Given: timing at start of H-Blank (dot 256, first scanline)
        let mut timing = Timing::new();
        tick_n(&mut timing, 252, 0xFF); // dot 4 + 252 = 256 (Mode 0 starts at dot 256)
        assert_eq!(timing.mode(), PpuMode::HBlank);
        let ly_before = timing.ly();
        // When: tick remaining dots to complete the scanline (200) plus 4 to reach Mode 2
        tick_n(&mut timing, 204, 0xFF);
        // Then: LY incremented and we are in Mode 2 of next scanline (dot 4 of scan1)
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
        // Given: timing at dot 255 (last dot of Mode 3 on scanline 0)
        let mut timing = Timing::new();
        tick_n(&mut timing, 251, 0xFF); // dot 4 + 251 = 255
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
        // When: tick one more dot (dot 256 = Mode 0 start)
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
        // On scanline 1, HBlank starts at dot 256 (Mode 3 ends at dot 256).
        // Note: mode_for_irq is first set to 0 at dot 252 (4 dots early), so
        // at dot 256 it is already 0 (set a second time redundantly).
        // First scanline is 452 dots (starting at dot 4). Scanline 1 starts at dot 0.
        // Tick 452 (scanline 0 end) + 256 (HBlank start on scanline 1) = 708 dots.
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 256, 0xFF);
        assert_eq!(timing.scanline, 1);
        assert_eq!(timing.dot(), 256);
        assert_eq!(timing.mode(), PpuMode::HBlank);
        assert_eq!(
            timing.mode_for_irq(),
            0,
            "mode_for_irq must be 0 when mode bits show HBlank"
        );
    }

    #[test]
    fn test_mode_for_irq_becomes_0_at_dot_252_before_hblank() {
        // The Mode 0 STAT interrupt source must activate at dot 252 of scanline 1,
        // which is 4 dots before the STAT mode bits change to HBlank at dot 256.
        // This is symmetric with Mode 2 firing at dot 452 (4 dots before Mode 2 bits).
        // First scanline: 452 dots. Scanline 1 starts at dot 0.
        // Tick 452 (scanline 0 end) + 252 = 704 dots.
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 252, 0xFF);
        assert_eq!(timing.scanline, 1);
        assert_eq!(timing.dot(), 252);
        assert_eq!(
            timing.mode(),
            PpuMode::PixelTransfer,
            "Mode bits must still show PixelTransfer at dot 252 (HBlank starts at 256)"
        );
        assert_eq!(
            timing.mode_for_irq(),
            0,
            "mode_for_irq must be 0 at dot 252 (4 dots before Mode 0 mode bits)"
        );
    }

    #[test]
    fn test_mode_for_irq_becomes_0_at_dot_251_not_early() {
        // One dot before the early Mode 0 fire: mode_for_irq should NOT yet be 0.
        // At dot 251 of scanline 1, mode_for_irq is still -1 (PixelTransfer suppressed).
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 251, 0xFF);
        assert_eq!(timing.scanline, 1);
        assert_eq!(timing.dot(), 251);
        assert_ne!(
            timing.mode_for_irq(),
            0,
            "mode_for_irq must not be 0 before dot 252"
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
    fn test_mode_for_irq_is_1_at_dot_452_of_scanline_153() {
        // Scanline 153 is VBlank; the Mode 2 STAT source does NOT fire at dot 452 here.
        // (It fires at dot=0 of scanline 0 on the frame wrap instead — see test below.)
        // mode_for_irq remains 1 (VBlank) throughout scanline 153.
        //
        // First scanline: 452 dots. Scanlines 1-153: 456 * 153 = 69768 dots.
        // dot 452 of scanline 153 = tick(452 + 456*152 + 452)
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 456 * 152 + 452, 0xFF);
        assert_eq!(timing.scanline, 153);
        assert_eq!(timing.dot(), 452);
        assert_eq!(
            timing.mode_for_irq(),
            1,
            "mode_for_irq must be 1 (VBlank) at dot 452 of scanline 153"
        );
    }

    #[test]
    fn test_mode_for_irq_becomes_2_at_frame_wrap_dot_0_scanline_0() {
        // At the frame wrap (dot=0 of scanline 0 in frame 2+), the Mode 2 STAT source fires
        // on both DMG and CGB. Full first frame: 452 + 456 * 153 = 70220 dots.
        let mut timing = Timing::new();
        tick_n(&mut timing, 452 + 456 * 153, 0xFF);
        assert_eq!(timing.scanline, 0);
        assert_eq!(timing.dot(), 0);
        assert_eq!(
            timing.mode_for_irq(),
            2,
            "mode_for_irq must be 2 at dot=0 of scanline 0 (frame wrap)"
        );
    }

    #[test]
    fn test_mode3_extra_dots_shifts_hblank_start() {
        // When mode3_extra_dots = 10, HBlank should start at dot 266 (not 256).
        // Base mode3_end for scan 1 = OAM_SCAN_DOTS(80) + PIXEL_TRANSFER_DOTS(172) + OAM_SCAN_START(4) = 256.
        // With extra=10: mode3_end = 256 + 10 = 266.
        let mut timing = Timing::new();
        tick_n(&mut timing, 452, 0xFF); // complete first scanline (now at dot=0 scan1)
        assert_eq!(timing.scanline, 1);
        timing.set_mode3_extra_dots(10);
        tick_n(&mut timing, 256, 0xFF); // would normally be HBlank at dot 256, but +10 extra
        assert_eq!(timing.dot(), 256);
        assert_eq!(
            timing.mode(),
            PpuMode::PixelTransfer,
            "With mode3_extra_dots=10, mode 3 should still be active at dot 256"
        );
        tick_n(&mut timing, 10, 0xFF); // advance past extra dots
        assert_eq!(timing.dot(), 266);
        assert_eq!(
            timing.mode(),
            PpuMode::HBlank,
            "With mode3_extra_dots=10, HBlank should start at dot 266"
        );
    }

    // ── Third scanline (scan 2) and scan 3+ behavior ─────────────────────────

    #[test]
    fn test_third_scanline_flag_set_and_cleared() {
        let mut timing = Timing::new();
        assert!(timing.first_scanline_after_enable);
        // Complete scan0 (452 dots from dot=4)
        tick_n(&mut timing, 452, 0xFF);
        assert!(!timing.first_scanline_after_enable);
        assert!(timing.second_scanline_after_enable);
        // Complete scan1 (456 dots)
        tick_n(&mut timing, 456, 0xFF);
        assert!(!timing.second_scanline_after_enable);
        assert!(
            timing.third_scanline_after_enable,
            "scan2 should have third flag set"
        );
        assert_eq!(timing.ly(), 2);
        // Complete scan2 (456 dots)
        tick_n(&mut timing, 456, 0xFF);
        assert!(
            !timing.third_scanline_after_enable,
            "scan3 should clear third flag"
        );
        assert_eq!(timing.ly(), 3);
    }

    #[test]
    fn test_mode_for_stat_scan2_oam_shows_hblank_for_4t() {
        // On scan2, OamScan start [0,4) shows HBlank in STAT (stat_mode lag).
        // Use tick_m so that save_stat_mode is called every M-cycle (as in Ppu::tick_dots).
        let mut timing = Timing::new();
        // Complete scan0 (113 M-cycles = 452 dots) and scan1 (114 M-cycles = 456 dots).
        tick_m(&mut timing, 113 + 114, 0xFF); // at scan2 dot=0
        assert_eq!(timing.ly(), 2);
        assert_eq!(timing.dot(), 0);
        assert!(timing.third_scanline_after_enable);
        // Physical mode = OamScan; stat_mode was HBlank (from end of scan1).
        assert_eq!(timing.mode(), PpuMode::OamScan);
        assert_eq!(
            timing.mode_for_stat(),
            PpuMode::HBlank,
            "scan2 dot=0: STAT shows HBlank for 4T"
        );
        // After 1 M-cycle (4 dots): stat_mode updates to OamScan.
        tick_m(&mut timing, 1, 0xFF);
        assert_eq!(timing.dot(), 4);
        assert_eq!(
            timing.mode_for_stat(),
            PpuMode::OamScan,
            "scan2 dot=4: STAT now shows OamScan"
        );
    }

    #[test]
    fn test_mode_for_stat_scan2_pixel_transfer_shows_oam_for_4t() {
        // On scan2, PixelTransfer start shows OamScan in STAT (stat_mode lag).
        let mut timing = Timing::new();
        // scan0 (113M) + scan1 (114M) + 20M to reach scan2 dot=80.
        tick_m(&mut timing, 113 + 114 + 20, 0xFF); // at scan2 dot=80
        assert_eq!(timing.ly(), 2);
        assert_eq!(timing.dot(), 80);
        assert_eq!(timing.mode(), PpuMode::PixelTransfer);
        assert_eq!(
            timing.mode_for_stat(),
            PpuMode::OamScan,
            "scan2 dot=80: STAT still shows OamScan for 4T"
        );
        tick_m(&mut timing, 1, 0xFF);
        assert_eq!(timing.dot(), 84);
        assert_eq!(
            timing.mode_for_stat(),
            PpuMode::PixelTransfer,
            "scan2 dot=84: STAT shows PixelTransfer"
        );
    }

    #[test]
    fn test_mode_for_stat_scan3_uses_physical_mode() {
        // On scan3+, STAT uses physical mode immediately (no lag).
        let mut timing = Timing::new();
        // scan0 (113M) + scan1 (114M) + scan2 (114M) = reach scan3 dot=0.
        tick_m(&mut timing, 113 + 114 + 114, 0xFF);
        assert_eq!(timing.ly(), 3);
        assert!(!timing.third_scanline_after_enable);
        // save_stat_mode was called before the M-cycle that spans scan3 dot=0.
        // At scan3 dot=0: physical mode = OamScan; mode_for_stat = physical (no lag).
        assert_eq!(timing.mode(), PpuMode::OamScan);
        assert_eq!(
            timing.mode_for_stat(),
            PpuMode::OamScan,
            "scan3+ OamScan: physical mode in STAT"
        );
    }

    #[test]
    fn test_is_oam_blocked_scan1_dot0_is_blocked() {
        // On scan1, OAM is blocked from dot=0 (brief HBlank [0,4) still blocks).
        let mut timing = Timing::new();
        tick_n(&mut timing, 452, 0xFF); // reach scan1 dot=0
        assert_eq!(timing.ly(), 1);
        assert_eq!(timing.dot(), 0);
        assert_eq!(
            timing.mode(),
            PpuMode::HBlank,
            "scan1 dot=0 physical mode is HBlank"
        );
        assert!(timing.is_oam_blocked(), "scan1 dot=0: OAM must be blocked");
    }

    #[test]
    fn test_frame_count_starts_at_zero() {
        // Given: a freshly created Timing
        let timing = Timing::new();
        // Then: frame count should start at 0
        assert_eq!(timing.frame_count(), 0, "frame_count should start at 0");
    }

    #[test]
    fn test_frame_count_increments_on_frame_completion() {
        // Given: a freshly created Timing (frame_count = 0)
        let mut timing = Timing::new();
        assert_eq!(timing.frame_count(), 0);

        // When: complete one full frame (70224 dots total)
        // Frame structure: scan 0 (452 dots) + scans 1-153 (153 × 456 dots)
        // = 452 + 69768 = 70220 dots... but scan 0 starts at dot 4, so we tick 70224 - 4 = 70220 dots
        let frame_dots = 452 + (153 * 456);
        tick_n(&mut timing, frame_dots, 0xFF);

        // Then: frame_count increments to 1 when LY wraps from 153 to 0
        assert_eq!(timing.ly(), 0, "LY should wrap to 0 after completing frame");
        assert_eq!(
            timing.frame_count(),
            1,
            "frame_count should increment to 1 after completing one frame"
        );

        // When: complete another frame
        tick_n(&mut timing, 154 * 456, 0xFF); // Normal frame is 154 scanlines × 456 dots

        // Then: frame_count increments to 2
        assert_eq!(timing.ly(), 0, "LY should wrap to 0 again");
        assert_eq!(
            timing.frame_count(),
            2,
            "frame_count should increment to 2 after completing two frames"
        );
    }
}
