//! GBA Picture Processing Unit (PPU).
//!
//! Implements the GBA LCD controller timing and a small subset of the
//! display modes. This module is the foundation for subsequent rendering
//! work — additional display modes (1, 5), tile background layers, and
//! sprite (OBJ) rendering will be added in follow-up sub-issues of
//! rmstdope/neser#2207.
//!
//! What is implemented here:
//!
//! * Scanline / dot timing per GBATek: 308 dots × 4 cycles = 1232 cycles
//!   per scanline, 228 scanlines per frame, totalling 280 896 cycles per
//!   frame at 59.7275 Hz.
//! * `DISPCNT`, `DISPSTAT`, `VCOUNT` register state and dispatch from
//!   the I/O unit.
//! * V-Blank / H-Blank flag transitions, V-Counter match flag, and the
//!   three associated IRQ sources (`VBLANK`, `HBLANK`, `VCOUNT`).
//! * Mode 0 tile background rendering (BG0–BG3 with 4bpp text backgrounds,
//!   priority compositing across all enabled layers).
//! * Mode 2 stub — renders backdrop only (affine tile rendering deferred).
//! * Mode 3 background rendering (240×160 15-bit BGR555 direct bitmap
//!   from VRAM) when `BG2` is enabled.
//! * Mode 4 background rendering (240×160 8-bit paletted bitmap from
//!   VRAM) with dual-frame support via DISPCNT bit 4.
//! * Backdrop fill from the first palette entry for unimplemented
//!   display modes (modes 1, 5, 6, 7) and Mode 3/4 when BG2 is disabled.
//!   Modes 6-7 are "prohibited" per GBATek but are handled gracefully.
//! * Forced-blank outputs solid white (per GBATek).
//!
//! Out of scope (deferred to follow-up sub-issues):
//!
//! * Mode 1 mixed affine mode, Mode 5 (160×128 15-bit) rendering.
//! * Mode 2 full affine tile background rendering for BG2/BG3.
//! * Sprite (OBJ) rendering and OAM attribute decoding.
//! * Window masks, alpha blending, mosaic, brightness effects.
//!
//! References:
//! * GBATek "LCD I/O Display Control": <https://problemkaputt.de/gbatek.htm#lcdiodisplaycontrol>
//! * GBATek "LCD I/O Interrupts and Status": <https://problemkaputt.de/gbatek.htm#lcdiointerruptsandstatus>
//! * Tonc "Video Introduction": <https://www.coranac.com/tonc/text/video.htm>

pub mod affine;
pub mod color;
pub mod obj;

use self::affine::BgAffine;
use super::bus::interrupt::{InterruptController, bits as irq_bits};

/// GBA visible screen width in pixels.
pub const SCREEN_WIDTH: u32 = 240;
/// GBA visible screen height in pixels.
pub const SCREEN_HEIGHT: u32 = 160;
/// Bytes per pixel in the RGB888 framebuffer exposed to the frontend.
pub const BYTES_PER_PIXEL: usize = 3;
/// Total framebuffer size in bytes (240 × 160 × 3).
pub const FRAMEBUFFER_BYTES: usize =
    (SCREEN_WIDTH as usize) * (SCREEN_HEIGHT as usize) * BYTES_PER_PIXEL;

/// CPU cycles per scanline (308 dots × 4 cycles/dot).
pub const CYCLES_PER_SCANLINE: u32 = 1232;
/// Cycle within a scanline at which the H-Blank flag becomes set.
///
/// Per GBATek, the H-Blank status bit is "0" during the first 1006
/// cycles and "1" during the last 226 cycles of each scanline.
pub const HBLANK_START_CYCLE: u32 = 1006;
/// Number of visible scanlines (lines 0..=159 are rendered).
pub const VISIBLE_SCANLINES: u32 = 160;
/// Total scanlines per frame including V-Blank period.
pub const SCANLINES_PER_FRAME: u32 = 228;
/// Last scanline on which the V-Blank flag is set.
///
/// The V-Blank status bit is set for scanlines 160..=226 and cleared
/// during scanline 227 (the final scanline of the frame).
pub const VBLANK_LAST_SCANLINE: u32 = 226;

/// `DISPCNT` bit masks used by the PPU.
pub mod dispcnt {
    /// BG mode (0..7), DISPCNT[2:0]. Modes 6, 7 are invalid on hardware.
    pub const MODE_MASK: u16 = 0x0007;
    /// Display Frame Select for modes 4, 5 (DISPCNT[4]).
    pub const FRAME_SELECT: u16 = 1 << 4;
    /// Forced Blank (DISPCNT[7]) — when set, the PPU outputs white.
    pub const FORCED_BLANK: u16 = 1 << 7;
    /// Display BG0 (DISPCNT[8]).
    pub const BG0_ENABLE: u16 = 1 << 8;
    /// Display BG1 (DISPCNT[9]).
    pub const BG1_ENABLE: u16 = 1 << 9;
    /// Display BG2 (DISPCNT[10]).
    pub const BG2_ENABLE: u16 = 1 << 10;
    /// Display BG3 (DISPCNT[11]).
    pub const BG3_ENABLE: u16 = 1 << 11;
    /// Display OBJ (DISPCNT[12]).
    pub const OBJ_ENABLE: u16 = 1 << 12;
    /// OBJ character VRAM mapping: 1 = 1D, 0 = 2D (DISPCNT[6]).
    pub const OBJ_MAPPING_1D: u16 = 1 << 6;
}

/// `DISPSTAT` bit masks used by the PPU.
pub mod dispstat {
    /// V-Blank flag (read-only status, DISPSTAT[0]).
    pub const VBLANK_FLAG: u16 = 1 << 0;
    /// H-Blank flag (read-only status, DISPSTAT[1]).
    pub const HBLANK_FLAG: u16 = 1 << 1;
    /// V-Counter match flag (read-only status, DISPSTAT[2]).
    pub const VCOUNT_FLAG: u16 = 1 << 2;
    /// Status bits — owned by the PPU, not by software writes.
    pub const STATUS_MASK: u16 = VBLANK_FLAG | HBLANK_FLAG | VCOUNT_FLAG;
    /// V-Blank IRQ enable (DISPSTAT[3]).
    pub const VBLANK_IRQ_ENABLE: u16 = 1 << 3;
    /// H-Blank IRQ enable (DISPSTAT[4]).
    pub const HBLANK_IRQ_ENABLE: u16 = 1 << 4;
    /// V-Counter match IRQ enable (DISPSTAT[5]).
    pub const VCOUNT_IRQ_ENABLE: u16 = 1 << 5;
    /// V-Count Setting (LYC) — high byte of DISPSTAT.
    pub const VCOUNT_SETTING_MASK: u16 = 0xFF00;
    /// Mask of bits writeable by software (everything except status and
    /// the always-zero bits 6..7).
    pub const WRITE_MASK: u16 =
        VBLANK_IRQ_ENABLE | HBLANK_IRQ_ENABLE | VCOUNT_IRQ_ENABLE | VCOUNT_SETTING_MASK;
}

/// I/O register addresses owned by the PPU.
pub const REG_DISPCNT: u32 = 0x0400_0000;
pub const REG_BG0CNT: u32 = 0x0400_0008;
pub const REG_BG1CNT: u32 = 0x0400_000A;
pub const REG_BG2CNT: u32 = 0x0400_000C;
pub const REG_BG3CNT: u32 = 0x0400_000E;
pub const REG_DISPSTAT: u32 = 0x0400_0004;
pub const REG_VCOUNT: u32 = 0x0400_0006;
pub const REG_BG0HOFS: u32 = 0x0400_0010;
pub const REG_BG0VOFS: u32 = 0x0400_0012;
pub const REG_BG1HOFS: u32 = 0x0400_0014;
pub const REG_BG1VOFS: u32 = 0x0400_0016;
pub const REG_BG2HOFS: u32 = 0x0400_0018;
pub const REG_BG2VOFS: u32 = 0x0400_001A;
pub const REG_BG3HOFS: u32 = 0x0400_001C;
pub const REG_BG3VOFS: u32 = 0x0400_001E;

// Affine background register file (BG2 and BG3). All eight registers
// per background are write-only; reads are handled by the bus's
// open-bus / I/O backing-store fallback.
pub const REG_BG2PA: u32 = 0x0400_0020;
pub const REG_BG2PB: u32 = 0x0400_0022;
pub const REG_BG2PC: u32 = 0x0400_0024;
pub const REG_BG2PD: u32 = 0x0400_0026;
pub const REG_BG2X_L: u32 = 0x0400_0028;
pub const REG_BG2X_H: u32 = 0x0400_002A;
pub const REG_BG2Y_L: u32 = 0x0400_002C;
pub const REG_BG2Y_H: u32 = 0x0400_002E;
pub const REG_BG3PA: u32 = 0x0400_0030;
pub const REG_BG3PB: u32 = 0x0400_0032;
pub const REG_BG3PC: u32 = 0x0400_0034;
pub const REG_BG3PD: u32 = 0x0400_0036;
pub const REG_BG3X_L: u32 = 0x0400_0038;
pub const REG_BG3X_H: u32 = 0x0400_003A;
pub const REG_BG3Y_L: u32 = 0x0400_003C;
pub const REG_BG3Y_H: u32 = 0x0400_003E;

/// Result of stepping the PPU — counts telling the bus how many DMA
/// hooks (V-Blank / H-Blank) to fire after a step.
///
/// The PPU does not own the bus, so it cannot directly invoke the
/// `notify_*` paths on the bus that wake DMA channels. Instead it
/// reports the edges that occurred during the most recent step and the
/// caller (the bus) routes them. Counts (rather than booleans) are
/// required because a single `step()` call may span many scanlines —
/// e.g. a full-frame step crosses 160 visible H-Blanks and one V-Blank,
/// and the bus must propagate every one to keep H-Blank-mode DMA
/// channels firing per scanline.
#[derive(Debug, Default, Clone, Copy)]
pub struct PpuStepEvents {
    /// Number of V-Blank periods that started during this step
    /// (transitions into scanline 160).
    pub vblank_starts: u32,
    /// Number of H-Blank periods that started on a *visible* scanline
    /// during this step. H-Blank also occurs during V-Blank scanlines
    /// but only visible-scanline H-Blanks trigger H-Blank-mode DMA per
    /// GBATek.
    pub hblank_starts: u32,
    /// Number of complete frames that finished during this step. The
    /// framebuffer is ready for the frontend to read whenever this is
    /// non-zero (and [`Ppu::frame_ready`] is set).
    pub frames_completed: u32,
}

/// GBA Picture Processing Unit state.
#[derive(Debug, Clone)]
pub struct Ppu {
    /// `DISPCNT` (0x0400_0000) — display control.
    dispcnt: u16,
    /// `DISPSTAT` (0x0400_0004) — display status / IRQ enables.
    /// The low 3 bits (V-Blank/H-Blank/V-Count flags) are status owned
    /// by the PPU; software writes to them are ignored.
    dispstat: u16,
    /// `BGnCNT` (0x0400_0008..0x0400_000E) — BG0–BG3 control registers.
    bg_cnt: [u16; 4],
    /// Current scanline (`VCOUNT`, 0x0400_0006). Wraps at
    /// [`SCANLINES_PER_FRAME`].
    vcount: u16,
    /// Cycle counter within the current scanline (`0..CYCLES_PER_SCANLINE`).
    line_cycle: u32,
    /// 240×160 RGB888 framebuffer. Updated incrementally as scanlines
    /// complete.
    framebuffer: Vec<u8>,
    /// True after the PPU finishes scanline 159's render and until the
    /// frontend acknowledges via [`Self::clear_frame_ready`].
    frame_ready: bool,
    /// Affine register file for BG2 and BG3 (`0x0400_0020..=0x0400_003E`).
    /// Index `0` is BG2, index `1` is BG3. Consumed by the (future)
    /// affine renderer; the registers are write-only on the bus side.
    bg_affine: [BgAffine; 2],
    /// BG0–BG3 horizontal and vertical scroll offsets (low 9 bits are valid).
    bg_scroll: [(u16, u16); 4],
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    /// Create a new PPU with all registers zero, scanline 0, and a
    /// blank black framebuffer. The V-Counter match flag is set
    /// immediately because the default LYC (`DISPSTAT[15:8]`) is 0 and
    /// the default `VCOUNT` is 0 — hardware sets the match flag whenever
    /// the current `VCOUNT == LYC`, including at reset.
    pub fn new() -> Self {
        let mut ppu = Self {
            dispcnt: 0,
            dispstat: 0,
            bg_cnt: [0; 4],
            vcount: 0,
            line_cycle: 0,
            framebuffer: vec![0; FRAMEBUFFER_BYTES],
            frame_ready: false,
            bg_affine: [BgAffine::default(); 2],
            bg_scroll: [(0, 0); 4],
        };
        // VCOUNT == LYC == 0 at reset; reflect that in the match flag.
        // No IRQ is raised here — the controller hasn't been wired up
        // yet at construction time.
        ppu.update_vcount_match_flag(None);
        ppu
    }

    /// Read `DISPCNT`.
    pub fn read_dispcnt(&self) -> u16 {
        self.dispcnt
    }

    /// Write `DISPCNT`.
    pub fn write_dispcnt(&mut self, value: u16) {
        self.dispcnt = value;
    }

    /// Read `DISPSTAT` — returns the live status bits OR'd with the
    /// software-writeable IRQ enables and V-Count setting.
    pub fn read_dispstat(&self) -> u16 {
        self.dispstat
    }

    /// Read `BG0CNT`.
    pub fn read_bg0cnt(&self) -> u16 {
        self.bg_cnt[0]
    }

    /// Write `DISPSTAT`. Only the IRQ enables (bits 3..5) and V-Count
    /// setting (bits 8..15) are settable; the read-only status bits
    /// retain their PPU-owned values. Writing `DISPSTAT` may change LYC,
    /// so the V-Counter match flag is re-evaluated immediately and the
    /// V-Count IRQ is raised if the new LYC matches the current `VCOUNT`
    /// and the V-Count IRQ enable bit is set.
    pub fn write_dispstat(&mut self, value: u16, ic: &mut InterruptController) {
        let status = self.dispstat & dispstat::STATUS_MASK;
        self.dispstat = status | (value & dispstat::WRITE_MASK);
        self.update_vcount_match_flag(Some(ic));
    }

    /// Write `BG0CNT`.
    pub fn write_bg0cnt(&mut self, value: u16) {
        self.bg_cnt[0] = value;
    }

    /// Read `BGnCNT` for background layer `n` (0–3).
    ///
    /// Per GBATek, bit 13 is not used for BG0/BG1 and reads as zero.
    pub fn read_bg_cnt(&self, n: usize) -> u16 {
        let mask = if n <= 1 { 0xDFFF } else { 0xFFFF };
        self.bg_cnt[n] & mask
    }

    /// Write `BGnCNT` for background layer `n` (0–3).
    pub fn write_bg_cnt(&mut self, n: usize, value: u16) {
        self.bg_cnt[n] = value;
    }

    /// Read `VCOUNT`.
    pub fn read_vcount(&self) -> u16 {
        self.vcount
    }

    /// The current display mode (DISPCNT[2:0]).
    pub fn mode(&self) -> u8 {
        (self.dispcnt & dispcnt::MODE_MASK) as u8
    }

    /// Whether the screen is currently in forced-blank mode.
    pub fn forced_blank(&self) -> bool {
        self.dispcnt & dispcnt::FORCED_BLANK != 0
    }

    /// Whether `BG2` is enabled in DISPCNT.
    pub fn bg2_enabled(&self) -> bool {
        self.dispcnt & dispcnt::BG2_ENABLE != 0
    }

    /// Whether `BG0` is enabled in DISPCNT.
    pub fn bg0_enabled(&self) -> bool {
        self.dispcnt & dispcnt::BG0_ENABLE != 0
    }

    /// Frame selection for Mode 4/5 (DISPCNT bit 4).
    /// Returns `true` for frame 1, `false` for frame 0.
    pub fn frame_select(&self) -> bool {
        self.dispcnt & dispcnt::FRAME_SELECT != 0
    }

    /// Borrow the affine register file for BG2 (`bg`=0) or BG3 (`bg`=1).
    /// Used by the affine renderer (and tests) to read the latched
    /// parameters and reference points.
    ///
    /// Returns `None` if `bg` is not a valid affine background index.
    pub fn bg_affine(&self, bg: usize) -> Option<&BgAffine> {
        self.bg_affine.get(bg)
    }

    /// Read the current value of an affine BG register as a halfword.
    /// Returns `None` if `addr` is not in the affine BG window
    /// (`0x0400_0020..=0x0400_003E`).
    ///
    /// These registers are write-only on real hardware (CPU reads
    /// fall through to open-bus / I/O backing store). This accessor
    /// exposes the *internal* latched value so that byte-granular
    /// writes can be implemented as read-modify-write of the live
    /// affine state without losing the previously-written byte.
    pub fn read_affine(&self, addr: u32) -> Option<u16> {
        let bg = match addr {
            0x0400_0020..=0x0400_002F => 0,
            0x0400_0030..=0x0400_003F => 1,
            _ => return None,
        };
        let a = &self.bg_affine[bg];
        Some(match addr & 0x000F {
            0x0 => a.pa as u16,
            0x2 => a.pb as u16,
            0x4 => a.pc as u16,
            0x6 => a.pd as u16,
            0x8 => (a.x as u32) as u16,
            0xA => ((a.x as u32) >> 16) as u16,
            0xC => (a.y as u32) as u16,
            0xE => ((a.y as u32) >> 16) as u16,
            _ => return None,
        })
    }

    /// Write a halfword to one of the 16 affine BG registers
    /// (`0x0400_0020..=0x0400_003E`). The address must be the exact
    /// register address; the bus dispatcher routes here directly.
    /// Returns `true` if the address matched an affine register and the
    /// write was consumed.
    pub fn write_affine(&mut self, addr: u32, value: u16) -> bool {
        // Each BG occupies a 16-byte block; index 0 = BG2 at 0x20,
        // index 1 = BG3 at 0x30.
        let bg = match addr {
            0x0400_0020..=0x0400_002F => 0,
            0x0400_0030..=0x0400_003F => 1,
            _ => return false,
        };
        let a = &mut self.bg_affine[bg];
        match addr & 0x000F {
            0x0 => a.pa = value as i16,
            0x2 => a.pb = value as i16,
            0x4 => a.pc = value as i16,
            0x6 => a.pd = value as i16,
            0x8 => a.write_x_low(value),
            0xA => a.write_x_high(value),
            0xC => a.write_y_low(value),
            0xE => a.write_y_high(value),
            _ => return false, // odd-aligned writes don't reach here via halfword bus
        }
        true
    }

    /// Write BG0HOFS (0x0400_0010). Only the low 9 bits are significant.
    pub fn write_bg0_hofs(&mut self, value: u16) {
        self.bg_scroll[0].0 = value & 0x01FF;
    }

    /// Read BG0HOFS (0x0400_0010).
    pub fn read_bg0_hofs(&self) -> u16 {
        self.bg_scroll[0].0
    }

    /// Write BG0VOFS (0x0400_0012). Only the low 9 bits are significant.
    pub fn write_bg0_vofs(&mut self, value: u16) {
        self.bg_scroll[0].1 = value & 0x01FF;
    }

    /// Read BG0VOFS (0x0400_0012).
    pub fn read_bg0_vofs(&self) -> u16 {
        self.bg_scroll[0].1
    }

    /// Write `BGnHOFS` for background layer `n` (0–3).
    pub fn write_bg_hofs(&mut self, n: usize, value: u16) {
        self.bg_scroll[n].0 = value & 0x01FF;
    }

    /// Read `BGnHOFS` for background layer `n` (0–3).
    pub fn read_bg_hofs(&self, n: usize) -> u16 {
        self.bg_scroll[n].0
    }

    /// Write `BGnVOFS` for background layer `n` (0–3).
    pub fn write_bg_vofs(&mut self, n: usize, value: u16) {
        self.bg_scroll[n].1 = value & 0x01FF;
    }

    /// Read `BGnVOFS` for background layer `n` (0–3).
    pub fn read_bg_vofs(&self, n: usize) -> u16 {
        self.bg_scroll[n].1
    }

    /// True after a completed frame, until [`Self::clear_frame_ready`].
    pub fn frame_ready(&self) -> bool {
        self.frame_ready
    }

    /// Acknowledge a completed frame.
    pub fn clear_frame_ready(&mut self) {
        self.frame_ready = false;
    }

    /// Borrow the 240×160 RGB888 framebuffer.
    pub fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    /// Step the PPU forward by `cycles` CPU cycles. Drives scanline
    /// timing, DISPSTAT flags, and IRQs via `ic`. Returns counts of
    /// V-Blank / H-Blank / frame edges the bus must propagate to the
    /// DMA hooks.
    ///
    /// Rendering for the just-completed scanline happens at the moment
    /// the scanline transitions from "active" to "H-Blank" — i.e. when
    /// the line cycle crosses [`HBLANK_START_CYCLE`].
    pub fn step(
        &mut self,
        cycles: u32,
        ic: &mut InterruptController,
        vram: &[u8],
        pram: &[u8],
        oam: &[u8],
    ) -> PpuStepEvents {
        let mut events = PpuStepEvents::default();
        let mut remaining = cycles;
        while remaining > 0 {
            let take = remaining.min(CYCLES_PER_SCANLINE - self.line_cycle);
            let next_cycle = self.line_cycle + take;

            // H-Blank flag rising edge.
            if self.line_cycle < HBLANK_START_CYCLE && next_cycle >= HBLANK_START_CYCLE {
                self.dispstat |= dispstat::HBLANK_FLAG;
                if (self.vcount as u32) < VISIBLE_SCANLINES {
                    // Render the just-completed visible scanline before
                    // signalling H-Blank so DMA HBlank transfers see the
                    // updated framebuffer (sprite/affine state will use
                    // this hook in later increments).
                    self.render_scanline(self.vcount as u32, vram, pram, oam);
                    // Increment affine internal reference points after each
                    // visible scanline (ref_x += PB, ref_y += PD).
                    for aff in &mut self.bg_affine {
                        aff.increment_reference_points();
                    }
                    events.hblank_starts = events.hblank_starts.saturating_add(1);
                }
                if self.dispstat & dispstat::HBLANK_IRQ_ENABLE != 0 {
                    ic.raise(irq_bits::HBLANK);
                }
            }

            self.line_cycle = next_cycle;
            remaining -= take;

            if self.line_cycle >= CYCLES_PER_SCANLINE {
                self.line_cycle -= CYCLES_PER_SCANLINE;
                self.advance_scanline(ic, &mut events);
            }
        }
        events
    }

    /// Advance to the next scanline, updating V-Count, V-Blank flag,
    /// V-Counter match, and raising the corresponding interrupts.
    fn advance_scanline(&mut self, ic: &mut InterruptController, events: &mut PpuStepEvents) {
        // Leaving this scanline — clear H-Blank flag.
        self.dispstat &= !dispstat::HBLANK_FLAG;

        let next = (self.vcount as u32 + 1) % SCANLINES_PER_FRAME;
        self.vcount = next as u16;

        // V-Blank flag tracks scanlines 160..=226. (Cleared on 227.)
        if next == VISIBLE_SCANLINES {
            self.dispstat |= dispstat::VBLANK_FLAG;
            events.vblank_starts = events.vblank_starts.saturating_add(1);
            events.frames_completed = events.frames_completed.saturating_add(1);
            self.frame_ready = true;
            // Latch affine internal reference points at VBlank start.
            for aff in &mut self.bg_affine {
                aff.latch_reference_points();
            }
            if self.dispstat & dispstat::VBLANK_IRQ_ENABLE != 0 {
                ic.raise(irq_bits::VBLANK);
            }
        } else if next > VBLANK_LAST_SCANLINE {
            // Final scanline of the frame: clear V-Blank flag.
            self.dispstat &= !dispstat::VBLANK_FLAG;
        }

        self.update_vcount_match_flag(Some(ic));
    }

    /// Update the V-Counter match flag based on the current `VCOUNT`
    /// and the LYC value latched into the high byte of `DISPSTAT`. If
    /// `ic` is provided and the match flag rises (i.e. transitions from
    /// 0 → 1), and the V-Count IRQ is enabled, raise the IRQ.
    fn update_vcount_match_flag(&mut self, ic: Option<&mut InterruptController>) {
        let lyc = (self.dispstat >> 8) as u32;
        let prev = self.dispstat & dispstat::VCOUNT_FLAG != 0;
        let now = (self.vcount as u32) == lyc;
        if now {
            self.dispstat |= dispstat::VCOUNT_FLAG;
        } else {
            self.dispstat &= !dispstat::VCOUNT_FLAG;
        }
        // Raise IRQ on rising edge of the match condition only.
        if !prev
            && now
            && self.dispstat & dispstat::VCOUNT_IRQ_ENABLE != 0
            && let Some(ic) = ic
        {
            ic.raise(irq_bits::VCOUNT);
        }
    }

    /// Render scanline `y` (0..160) into the framebuffer.
    fn render_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        if self.forced_blank() {
            // Forced blank → output white per GBATek.
            let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
            let row_end = row_start + (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
            for byte in &mut self.framebuffer[row_start..row_end] {
                *byte = 0xFF;
            }
            return;
        }
        match self.mode() {
            0 => self.render_mode0_scanline(y, vram, pram, oam),
            1 => self.render_mode1_scanline(y, vram, pram, oam),
            2 => self.render_mode2_scanline(y, vram, pram, oam),
            3 => self.render_mode3_scanline(y, vram, pram, oam),
            4 => self.render_mode4_scanline(y, vram, pram, oam),
            _ => self.render_backdrop_scanline(y, pram),
        }
    }

    /// Mode 0: render enabled text-mode BG layers (BG0–BG3) with priority
    /// compositing. Lower BGCNT priority value = on top; at equal priority,
    /// lower BG number wins.
    fn render_mode0_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        // Collect enabled text-mode BGs.
        let bg_enables = [
            self.dispcnt & dispcnt::BG0_ENABLE != 0,
            self.dispcnt & dispcnt::BG1_ENABLE != 0,
            self.dispcnt & dispcnt::BG2_ENABLE != 0,
            self.dispcnt & dispcnt::BG3_ENABLE != 0,
        ];

        let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        let backdrop = self.backdrop_bgr555(pram);

        // Fill with backdrop first.
        let (br, bg, bb) = color::bgr555_to_rgb888(backdrop);
        for x in 0..(SCREEN_WIDTH as usize) {
            let dst = row_start + x * BYTES_PER_PIXEL;
            self.framebuffer[dst] = br;
            self.framebuffer[dst + 1] = bg;
            self.framebuffer[dst + 2] = bb;
        }

        if !bg_enables.iter().any(|&e| e) && self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            return;
        }

        // Track per-pixel priority for OBJ compositing (4 = backdrop/no BG).
        let mut pixel_priority = [4u8; SCREEN_WIDTH as usize];

        if bg_enables.iter().any(|&e| e) {
            // Build render order: paint from lowest visual priority (behind) to
            // highest (on top). Higher BGCNT priority value = behind; at equal
            // priority, higher BG number = behind.
            let mut layers: [(u16, usize); 4] = [(0, 0); 4];
            let mut count = 0;
            for (i, &enabled) in bg_enables.iter().enumerate() {
                if enabled {
                    layers[count] = (self.bg_cnt[i] & 3, i);
                    count += 1;
                }
            }
            layers[..count].sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

            for &(prio, bg_idx) in &layers[..count] {
                self.render_text_bg_layer_with_priority(
                    bg_idx,
                    y,
                    vram,
                    pram,
                    row_start,
                    backdrop,
                    prio as u8,
                    &mut pixel_priority,
                );
            }
        }

        self.overlay_obj_pixels(y, vram, pram, oam, row_start, &pixel_priority);
    }

    /// Render a single 4bpp text-mode BG layer onto the framebuffer row
    /// at `row_start`. Transparent pixels (palette index 0) are skipped.
    /// Updates `pixel_priority` for each opaque pixel written.
    #[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
    fn render_text_bg_layer_with_priority(
        &mut self,
        bg_idx: usize,
        y: u32,
        vram: &[u8],
        pram: &[u8],
        row_start: usize,
        backdrop: u16,
        bg_prio: u8,
        pixel_priority: &mut [u8; SCREEN_WIDTH as usize],
    ) {
        let bgcnt = self.bg_cnt[bg_idx];

        assert!(
            bgcnt & (1 << 7) == 0,
            "unimplemented GBA Mode 0 BG{bg_idx} 8bpp rendering requested via BGCNT bit 7"
        );

        let bg_size = (bgcnt >> 14) & 0x0003;
        let (width_tiles, height_tiles) = match bg_size {
            0 => (32usize, 32usize),
            1 => (64usize, 32usize),
            2 => (32usize, 64usize),
            _ => (64usize, 64usize),
        };
        let width_mask = width_tiles * 8 - 1;
        let height_mask = height_tiles * 8 - 1;
        let screenblock_base = (((bgcnt >> 8) & 0x001F) as usize) * 0x800;
        let charblock_base = (((bgcnt >> 2) & 0x0003) as usize) * 16 * 1024;
        let (hofs, vofs) = self.bg_scroll[bg_idx];
        let screen_y = ((y as usize) + vofs as usize) & height_mask;

        for x in 0..(SCREEN_WIDTH as usize) {
            let screen_x = (x + hofs as usize) & width_mask;
            let tile_x = screen_x >> 3;
            let tile_y = screen_y >> 3;
            let screenblock_x = tile_x >> 5;
            let screenblock_y = tile_y >> 5;
            let screenblock = screenblock_y * (width_tiles >> 5) + screenblock_x;
            let local_tile_x = tile_x & 31;
            let local_tile_y = tile_y & 31;
            let map_off =
                screenblock_base + screenblock * 0x800 + (local_tile_y * 32 + local_tile_x) * 2;

            let entry = if map_off + 1 < vram.len() {
                u16::from_le_bytes([vram[map_off], vram[map_off + 1]])
            } else {
                0
            };

            let tile_id = (entry & 0x03FF) as usize;
            let hflip = (entry & (1 << 10)) != 0;
            let vflip = (entry & (1 << 11)) != 0;
            let palette_bank = ((entry >> 12) & 0x000F) as usize;
            let pixel_x = if hflip {
                7 - (screen_x & 7)
            } else {
                screen_x & 7
            };
            let pixel_y = if vflip {
                7 - (screen_y & 7)
            } else {
                screen_y & 7
            };
            let tile_addr = charblock_base + tile_id * 32 + pixel_y * 4 + (pixel_x >> 1);

            let palette_index = vram
                .get(tile_addr)
                .map(|byte| {
                    if pixel_x & 1 == 0 {
                        byte & 0x0F
                    } else {
                        byte >> 4
                    }
                })
                .unwrap_or(0) as usize;

            // Palette index 0 is transparent — skip (keep whatever is below).
            if palette_index == 0 {
                continue;
            }

            let bgr555 = {
                let pram_index = (palette_bank * 16 + palette_index) * 2;
                if pram_index + 1 < pram.len() {
                    u16::from_le_bytes([pram[pram_index], pram[pram_index + 1]])
                } else {
                    backdrop
                }
            };

            let dst = row_start + x * BYTES_PER_PIXEL;
            color::write_pixel(&mut self.framebuffer, dst, bgr555);
            pixel_priority[x] = bg_prio;
        }
    }

    /// Render a single 8bpp affine tile background layer onto the framebuffer
    /// row at `row_start`. Uses the internal reference points and affine
    /// parameters for the per-pixel texture coordinate calculation.
    ///
    /// `affine_idx` is 0 for BG2, 1 for BG3.
    #[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
    fn render_affine_bg_layer(
        &mut self,
        bg_idx: usize,
        affine_idx: usize,
        vram: &[u8],
        pram: &[u8],
        row_start: usize,
        bg_prio: u8,
        pixel_priority: &mut [u8; SCREEN_WIDTH as usize],
    ) {
        let bgcnt = self.bg_cnt[bg_idx];
        let aff = self.bg_affine[affine_idx];

        // Affine map sizes: 16x16, 32x32, 64x64, 128x128 tiles.
        let size_shift = ((bgcnt >> 14) & 3) as u32;
        let tiles_wide = 16u32 << size_shift;
        let map_pixels = tiles_wide * 8;

        let wrapping = (bgcnt & (1 << 13)) != 0;
        let screenblock_base = (((bgcnt >> 8) & 0x001F) as usize) * 0x800;
        let charblock_base = (((bgcnt >> 2) & 0x0003) as usize) * 16 * 1024;

        let pa = aff.pa as i32;
        let pc = aff.pc as i32;

        // Start from internal reference points (already positioned for this
        // scanline via VBlank latch + per-scanline PB/PD increments).
        let mut tex_x = aff.internal_x;
        let mut tex_y = aff.internal_y;

        for x in 0..(SCREEN_WIDTH as usize) {
            // Convert from 8.8 fixed-point to integer pixel coordinates.
            let px = tex_x >> 8;
            let py = tex_y >> 8;

            tex_x = tex_x.wrapping_add(pa);
            tex_y = tex_y.wrapping_add(pc);

            // Bounds check.
            let (fx, fy) = if wrapping {
                (
                    (px as u32) & (map_pixels - 1),
                    (py as u32) & (map_pixels - 1),
                )
            } else {
                if px < 0 || py < 0 || px >= map_pixels as i32 || py >= map_pixels as i32 {
                    continue; // transparent — keep whatever is below
                }
                (px as u32, py as u32)
            };

            let tile_x = (fx >> 3) as usize;
            let tile_y = (fy >> 3) as usize;
            let pixel_x = (fx & 7) as usize;
            let pixel_y = (fy & 7) as usize;

            // Affine map: 1-byte entries, linear layout.
            let map_off = screenblock_base + tile_y * (tiles_wide as usize) + tile_x;
            let tile_id = *vram.get(map_off).unwrap_or(&0) as usize;

            // 8bpp tile: 64 bytes per tile, 1 byte per pixel.
            let tile_addr = charblock_base + tile_id * 64 + pixel_y * 8 + pixel_x;
            let palette_index = *vram.get(tile_addr).unwrap_or(&0) as usize;

            // Palette index 0 is transparent.
            if palette_index == 0 {
                continue;
            }

            // 256-color palette: single palette, 2 bytes per entry.
            let pram_index = palette_index * 2;
            let bgr555 = if pram_index + 1 < pram.len() {
                u16::from_le_bytes([pram[pram_index], pram[pram_index + 1]])
            } else {
                self.backdrop_bgr555(pram)
            };

            let dst = row_start + x * BYTES_PER_PIXEL;
            color::write_pixel(&mut self.framebuffer, dst, bgr555);
            pixel_priority[x] = bg_prio;
        }
    }

    /// Mode 2: affine tile backgrounds (BG2 and BG3 only).
    fn render_mode2_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        let bg_enables = [
            self.dispcnt & dispcnt::BG2_ENABLE != 0,
            self.dispcnt & dispcnt::BG3_ENABLE != 0,
        ];

        let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        let backdrop = self.backdrop_bgr555(pram);
        let (br, bg, bb) = color::bgr555_to_rgb888(backdrop);
        for sx in 0..(SCREEN_WIDTH as usize) {
            let dst = row_start + sx * BYTES_PER_PIXEL;
            self.framebuffer[dst] = br;
            self.framebuffer[dst + 1] = bg;
            self.framebuffer[dst + 2] = bb;
        }

        if !bg_enables.iter().any(|&e| e) && self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            return;
        }

        let mut pixel_priority = [4u8; SCREEN_WIDTH as usize];

        if bg_enables.iter().any(|&e| e) {
            let mut layers: [(u16, usize, usize); 2] = [(0, 0, 0); 2];
            let mut count = 0;
            let bg_indices = [2usize, 3usize];
            for (i, &bg_idx) in bg_indices.iter().enumerate() {
                if bg_enables[i] {
                    layers[count] = (self.bg_cnt[bg_idx] & 3, bg_idx, i);
                    count += 1;
                }
            }
            layers[..count].sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

            for &(prio, bg_idx, affine_idx) in &layers[..count] {
                self.render_affine_bg_layer(
                    bg_idx,
                    affine_idx,
                    vram,
                    pram,
                    row_start,
                    prio as u8,
                    &mut pixel_priority,
                );
            }
        }

        self.overlay_obj_pixels(y, vram, pram, oam, row_start, &pixel_priority);
    }

    /// Mode 1: BG0/BG1 regular text + BG2 affine.
    fn render_mode1_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        let bg_enables = [
            self.dispcnt & dispcnt::BG0_ENABLE != 0,
            self.dispcnt & dispcnt::BG1_ENABLE != 0,
            self.dispcnt & dispcnt::BG2_ENABLE != 0,
        ];

        let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        let backdrop = self.backdrop_bgr555(pram);
        let (br, bg, bb) = color::bgr555_to_rgb888(backdrop);
        for sx in 0..(SCREEN_WIDTH as usize) {
            let dst = row_start + sx * BYTES_PER_PIXEL;
            self.framebuffer[dst] = br;
            self.framebuffer[dst + 1] = bg;
            self.framebuffer[dst + 2] = bb;
        }

        if !bg_enables.iter().any(|&e| e) && self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            return;
        }

        let mut pixel_priority = [4u8; SCREEN_WIDTH as usize];

        if bg_enables.iter().any(|&e| e) {
            let mut layers: [(u16, usize, bool); 3] = [(0, 0, false); 3];
            let mut count = 0;
            for (i, &bg_idx) in [0usize, 1, 2].iter().enumerate() {
                if bg_enables[i] {
                    layers[count] = (self.bg_cnt[bg_idx] & 3, bg_idx, bg_idx == 2);
                    count += 1;
                }
            }
            layers[..count].sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));

            for &(prio, bg_idx, is_affine) in &layers[..count] {
                if is_affine {
                    self.render_affine_bg_layer(
                        bg_idx,
                        0,
                        vram,
                        pram,
                        row_start,
                        prio as u8,
                        &mut pixel_priority,
                    );
                } else {
                    self.render_text_bg_layer_with_priority(
                        bg_idx,
                        y,
                        vram,
                        pram,
                        row_start,
                        backdrop,
                        prio as u8,
                        &mut pixel_priority,
                    );
                }
            }
        }

        self.overlay_obj_pixels(y, vram, pram, oam, row_start, &pixel_priority);
    }

    /// Mode 3: 240×160 direct 15-bit bitmap starting at the base of
    /// VRAM. Each pixel is a 16-bit BGR555 value. When BG2 is disabled,
    /// the scanline is filled with the backdrop color (palette entry 0
    /// in PRAM) per GBATek — every pixel is "no BG/OBJ pixel drawn", so
    /// the backdrop shows through.
    #[allow(clippy::needless_range_loop)]
    fn render_mode3_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        let bg2_prio = (self.bg_cnt[2] & 3) as u8;
        let mut pixel_priority = [4u8; SCREEN_WIDTH as usize];

        if self.bg2_enabled() {
            let line_byte_offset = (y as usize) * (SCREEN_WIDTH as usize) * 2;
            for x in 0..(SCREEN_WIDTH as usize) {
                let src = line_byte_offset + x * 2;
                let bgr555 = u16::from_le_bytes([vram[src], vram[src + 1]]);
                let dst = row_start + x * BYTES_PER_PIXEL;
                color::write_pixel(&mut self.framebuffer, dst, bgr555);
                pixel_priority[x] = bg2_prio;
            }
        } else {
            self.render_backdrop_scanline(y, pram);
        }

        self.overlay_obj_pixels(y, vram, pram, oam, row_start, &pixel_priority);
    }

    /// Mode 4: 240×160 8-bit paletted bitmap. Each byte in VRAM is a
    /// palette index (0-255) that selects a BGR555 color from PRAM.
    /// Palette index 0 displays palette entry 0 (which is also the
    /// backdrop color).
    ///
    /// Two frames are available:
    /// - Frame 0: 0x06000000 - 0x060095FF (38,400 bytes)
    /// - Frame 1: 0x0600A000 - 0x060135FF (38,400 bytes)
    ///
    /// DISPCNT bit 4 selects the displayed frame.
    #[allow(clippy::needless_range_loop)]
    fn render_mode4_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        let bg2_prio = (self.bg_cnt[2] & 3) as u8;
        let mut pixel_priority = [4u8; SCREEN_WIDTH as usize];

        if self.bg2_enabled() {
            let backdrop = self.backdrop_bgr555(pram);
            let frame_base = if self.frame_select() {
                0xA000usize
            } else {
                0x0000usize
            };

            let line_byte_offset = frame_base + (y as usize) * (SCREEN_WIDTH as usize);

            for x in 0..(SCREEN_WIDTH as usize) {
                let src = line_byte_offset + x;
                let pal_index = if src < vram.len() { vram[src] } else { 0 };

                let bgr555 = if pal_index == 0 {
                    backdrop
                } else {
                    let pal_offset = (pal_index as usize) * 2;
                    if pal_offset + 1 < pram.len() {
                        u16::from_le_bytes([pram[pal_offset], pram[pal_offset + 1]])
                    } else {
                        backdrop
                    }
                };

                let dst = row_start + x * BYTES_PER_PIXEL;
                color::write_pixel(&mut self.framebuffer, dst, bgr555);
                // All bitmap pixels have BG2's priority.
                pixel_priority[x] = bg2_prio;
            }
        } else {
            self.render_backdrop_scanline(y, pram);
        }

        self.overlay_obj_pixels(y, vram, pram, oam, row_start, &pixel_priority);
    }

    /// Overlay OBJ pixels on top of the BG-composited framebuffer row.
    /// An OBJ pixel with priority P draws on top of any BG pixel with
    /// priority >= P (lower number = higher priority; OBJ wins at same level).
    #[allow(clippy::needless_range_loop)]
    fn overlay_obj_pixels(
        &mut self,
        y: u32,
        vram: &[u8],
        pram: &[u8],
        oam: &[u8],
        row_start: usize,
        pixel_priority: &[u8; SCREEN_WIDTH as usize],
    ) {
        if self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            return;
        }

        let mapping_1d = self.dispcnt & dispcnt::OBJ_MAPPING_1D != 0;
        let obj_scanline = obj::render_obj_scanline(y, oam, vram, pram, mapping_1d);

        for x in 0..(SCREEN_WIDTH as usize) {
            let px = &obj_scanline.pixels[x];
            if px.opaque && px.priority <= pixel_priority[x] {
                let dst = row_start + x * BYTES_PER_PIXEL;
                color::write_pixel(&mut self.framebuffer, dst, px.color);
            }
        }
    }

    /// Backdrop fill — uses palette entry 0 from PRAM. Used for modes
    /// where rendering is not yet implemented in this increment.
    fn render_backdrop_scanline(&mut self, y: u32, pram: &[u8]) {
        let backdrop = self.backdrop_bgr555(pram);
        let (r, g, b) = color::bgr555_to_rgb888(backdrop);
        let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        for x in 0..(SCREEN_WIDTH as usize) {
            let dst = row_start + x * BYTES_PER_PIXEL;
            self.framebuffer[dst] = r;
            self.framebuffer[dst + 1] = g;
            self.framebuffer[dst + 2] = b;
        }
    }

    fn backdrop_bgr555(&self, pram: &[u8]) -> u16 {
        if pram.len() >= 2 {
            u16::from_le_bytes([pram[0], pram[1]])
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ic() -> InterruptController {
        let mut ic = InterruptController::new();
        ic.write_ie(0xFFFF);
        ic.write_ime(1);
        ic
    }

    fn make_vram() -> Vec<u8> {
        vec![0; 96 * 1024]
    }

    fn make_pram() -> Vec<u8> {
        vec![0; 1024]
    }

    fn make_oam() -> Vec<u8> {
        // All OBJs hidden (obj_mode=2 in attr0 bits 8-9) to avoid ghost sprites.
        let mut oam = vec![0u8; 1024];
        for i in 0..128 {
            let offset = i * 8;
            // attr0: set bits 8-9 to 0b10 (hidden mode)
            let attr0 = 0x0200u16;
            oam[offset] = attr0 as u8;
            oam[offset + 1] = (attr0 >> 8) as u8;
        }
        oam
    }

    #[test]
    fn new_ppu_has_zeroed_state_and_blank_framebuffer() {
        let ppu = Ppu::new();
        assert_eq!(ppu.read_dispcnt(), 0);
        // DISPSTAT bit 2 (V-Counter match) is set at reset because
        // VCOUNT == LYC == 0; everything else is clear.
        assert_eq!(ppu.read_dispstat(), dispstat::VCOUNT_FLAG);
        assert_eq!(ppu.read_vcount(), 0);
        assert!(!ppu.frame_ready());
        assert_eq!(ppu.framebuffer().len(), FRAMEBUFFER_BYTES);
        assert!(ppu.framebuffer().iter().all(|&b| b == 0));
    }

    #[test]
    fn dispstat_status_bits_are_read_only() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        // Software write attempts to set V-Blank/H-Blank/V-Count flags
        // must be ignored — the PPU owns those bits. (Setting LYC=0xAB
        // also can't change the V-Count match flag for VCOUNT=0.)
        ppu.write_dispstat(0x0007 | 0x0038 | 0xAB00, &mut ic);
        assert_eq!(ppu.read_dispstat() & dispstat::HBLANK_FLAG, 0);
        assert_eq!(ppu.read_dispstat() & dispstat::VBLANK_FLAG, 0);
        // IRQ enables and V-Count setting must round-trip.
        assert_eq!(
            ppu.read_dispstat() & !dispstat::STATUS_MASK,
            0x0038 | 0xAB00
        );
    }

    #[test]
    fn step_advances_line_cycle_within_scanline() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        ppu.step(500, &mut ic, &make_vram(), &make_pram(), &make_oam());
        assert_eq!(ppu.read_vcount(), 0);
        // No H-Blank yet at cycle 500.
        assert_eq!(ppu.read_dispstat() & dispstat::HBLANK_FLAG, 0);
    }

    #[test]
    fn hblank_flag_sets_at_cycle_1006() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        ppu.step(
            HBLANK_START_CYCLE - 1,
            &mut ic,
            &make_vram(),
            &make_pram(),
            &make_oam(),
        );
        assert_eq!(ppu.read_dispstat() & dispstat::HBLANK_FLAG, 0);
        ppu.step(1, &mut ic, &make_vram(), &make_pram(), &make_oam());
        assert_ne!(ppu.read_dispstat() & dispstat::HBLANK_FLAG, 0);
    }

    #[test]
    fn hblank_flag_clears_when_scanline_advances() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        ppu.step(
            CYCLES_PER_SCANLINE,
            &mut ic,
            &make_vram(),
            &make_pram(),
            &make_oam(),
        );
        assert_eq!(ppu.read_vcount(), 1);
        // After crossing the scanline boundary the H-Blank flag is gone.
        assert_eq!(ppu.read_dispstat() & dispstat::HBLANK_FLAG, 0);
    }

    #[test]
    fn hblank_irq_fires_only_when_enabled() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        // Without H-Blank IRQ enable bit, no IRQ should be raised.
        ppu.step(
            HBLANK_START_CYCLE,
            &mut ic,
            &make_vram(),
            &make_pram(),
            &make_oam(),
        );
        assert_eq!(ic.if_flags & irq_bits::HBLANK, 0);

        // Enable + advance to next H-Blank → IRQ flagged.
        ppu.write_dispstat(dispstat::HBLANK_IRQ_ENABLE, &mut ic);
        ppu.step(
            CYCLES_PER_SCANLINE,
            &mut ic,
            &make_vram(),
            &make_pram(),
            &make_oam(),
        );
        assert_ne!(ic.if_flags & irq_bits::HBLANK, 0);
    }

    #[test]
    fn hblank_event_only_on_visible_scanlines() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        // Step up to the *start* of scanline 160 (V-Blank).
        let cycles = CYCLES_PER_SCANLINE * VISIBLE_SCANLINES;
        ppu.step(cycles, &mut ic, &vram, &pram, &make_oam());
        assert_eq!(ppu.read_vcount(), 160);
        // Step through H-Blank of scanline 160 — no visible H-Blank.
        let events = ppu.step(HBLANK_START_CYCLE, &mut ic, &vram, &pram, &make_oam());
        assert_eq!(events.hblank_starts, 0);
    }

    #[test]
    fn step_full_frame_counts_every_visible_hblank() {
        // A full-frame step must report all 160 visible-scanline H-Blanks
        // and exactly one V-Blank / completed frame so the bus can
        // forward each edge to the DMA hooks.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        let events = ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );
        assert_eq!(events.hblank_starts, VISIBLE_SCANLINES);
        assert_eq!(events.vblank_starts, 1);
        assert_eq!(events.frames_completed, 1);
    }

    #[test]
    fn step_two_frames_counts_two_vblanks() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        let events = ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME * 2,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );
        assert_eq!(events.vblank_starts, 2);
        assert_eq!(events.frames_completed, 2);
        assert_eq!(events.hblank_starts, VISIBLE_SCANLINES * 2);
    }

    #[test]
    fn vblank_starts_at_scanline_160() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        let cycles = CYCLES_PER_SCANLINE * VISIBLE_SCANLINES;
        let events = ppu.step(cycles, &mut ic, &vram, &pram, &make_oam());
        assert_eq!(ppu.read_vcount(), 160);
        assert_ne!(ppu.read_dispstat() & dispstat::VBLANK_FLAG, 0);
        assert_eq!(events.vblank_starts, 1);
        assert_eq!(events.frames_completed, 1);
        assert!(ppu.frame_ready());
    }

    #[test]
    fn vblank_irq_fires_when_enabled() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        ppu.write_dispstat(dispstat::VBLANK_IRQ_ENABLE, &mut ic);
        let cycles = CYCLES_PER_SCANLINE * VISIBLE_SCANLINES;
        ppu.step(cycles, &mut ic, &make_vram(), &make_pram(), &make_oam());
        assert_ne!(ic.if_flags & irq_bits::VBLANK, 0);
    }

    #[test]
    fn vblank_flag_clears_on_scanline_227() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        // Advance to the start of the final scanline (227).
        let cycles = CYCLES_PER_SCANLINE * (VBLANK_LAST_SCANLINE + 1);
        ppu.step(cycles, &mut ic, &vram, &pram, &make_oam());
        assert_eq!(ppu.read_vcount() as u32, VBLANK_LAST_SCANLINE + 1);
        assert_eq!(ppu.read_dispstat() & dispstat::VBLANK_FLAG, 0);
    }

    #[test]
    fn frame_completes_after_a_full_frame() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        let total = CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME;
        ppu.step(total, &mut ic, &vram, &pram, &make_oam());
        assert_eq!(ppu.read_vcount(), 0);
        assert!(ppu.frame_ready());
        ppu.clear_frame_ready();
        assert!(!ppu.frame_ready());
    }

    #[test]
    fn vcount_match_flag_set_at_reset_when_lyc_zero() {
        // VCOUNT == LYC == 0 at reset: the match flag must be high so
        // software that polls DISPSTAT.bit2 sees the correct state on
        // the very first scanline (hardware is level-sensitive).
        let ppu = Ppu::new();
        assert_ne!(ppu.read_dispstat() & dispstat::VCOUNT_FLAG, 0);
    }

    #[test]
    fn vcount_match_flag_updates_on_dispstat_write() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        // Start: VCOUNT=0, LYC=0 → match flag set.
        assert_ne!(ppu.read_dispstat() & dispstat::VCOUNT_FLAG, 0);
        // Set LYC=5 — match no longer holds, flag must clear.
        ppu.write_dispstat(5 << 8, &mut ic);
        assert_eq!(ppu.read_dispstat() & dispstat::VCOUNT_FLAG, 0);
        // Set LYC=0 again — match flag must reassert immediately
        // without waiting for the next scanline boundary.
        ppu.write_dispstat(0, &mut ic);
        assert_ne!(ppu.read_dispstat() & dispstat::VCOUNT_FLAG, 0);
    }

    #[test]
    fn vcount_match_irq_fires_when_lyc_written_to_match_current_vcount() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        // Step to scanline 7.
        ppu.step(
            CYCLES_PER_SCANLINE * 7,
            &mut ic,
            &make_vram(),
            &make_pram(),
            &make_oam(),
        );
        assert_eq!(ppu.read_vcount(), 7);
        // Initial match flag is low (VCOUNT=7, LYC=0). Now enable VCount
        // IRQ and write LYC=7 — IRQ must fire on the rising edge of
        // the match condition, even though no scanline boundary occurred.
        ic.if_flags = 0;
        ppu.write_dispstat(dispstat::VCOUNT_IRQ_ENABLE | (7 << 8), &mut ic);
        assert_ne!(ppu.read_dispstat() & dispstat::VCOUNT_FLAG, 0);
        assert_ne!(ic.if_flags & irq_bits::VCOUNT, 0);
    }

    #[test]
    fn vcount_match_flag_and_irq() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        // LYC = 5, enable V-Count IRQ.
        ppu.write_dispstat(dispstat::VCOUNT_IRQ_ENABLE | (5 << 8), &mut ic);
        // Step to scanline 5.
        ppu.step(
            CYCLES_PER_SCANLINE * 5,
            &mut ic,
            &make_vram(),
            &make_pram(),
            &make_oam(),
        );
        assert_eq!(ppu.read_vcount(), 5);
        assert_ne!(ppu.read_dispstat() & dispstat::VCOUNT_FLAG, 0);
        assert_ne!(ic.if_flags & irq_bits::VCOUNT, 0);
        // Advance one more line — flag clears.
        ppu.step(
            CYCLES_PER_SCANLINE,
            &mut ic,
            &make_vram(),
            &make_pram(),
            &make_oam(),
        );
        assert_eq!(ppu.read_vcount(), 6);
        assert_eq!(ppu.read_dispstat() & dispstat::VCOUNT_FLAG, 0);
    }

    #[test]
    fn mode3_renders_bitmap_from_vram() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();
        // Set Mode 3 + BG2 enabled.
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        // Paint the first row of the bitmap red (BGR555 0x001F).
        for x in 0..(SCREEN_WIDTH as usize) {
            let off = x * 2;
            vram[off] = 0x1F;
            vram[off + 1] = 0x00;
        }
        // Run a full frame so scanline 0 gets rendered into the buffer.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );
        // Sample first pixel of scanline 0 — should be pure red.
        let fb = ppu.framebuffer();
        assert_eq!(fb[0], 0xFF, "R");
        assert_eq!(fb[1], 0x00, "G");
        assert_eq!(fb[2], 0x00, "B");
        // And the last pixel of scanline 0 too.
        let last = ((SCREEN_WIDTH as usize) - 1) * BYTES_PER_PIXEL;
        assert_eq!(&fb[last..last + 3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode3_with_bg2_disabled_renders_backdrop() {
        // On hardware the backdrop color (PRAM[0]) is shown for any
        // pixel where no BG/OBJ pixel is drawn — including all of
        // Mode 3 when BG2 is disabled.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        // Mode 3 but BG2 disabled.
        ppu.write_dispcnt(3);
        // Backdrop = pure red (BGR555 0x001F) in PRAM[0].
        pram[0] = 0x1F;
        pram[1] = 0x00;
        // Paint VRAM blue — should NOT appear because BG2 is off.
        vram[0] = 0x00;
        vram[1] = 0x7C;
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );
        assert_eq!(&ppu.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn forced_blank_outputs_white() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        // Forced blank set; mode/bg2 irrelevant.
        ppu.write_dispcnt(dispcnt::FORCED_BLANK);
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );
        // Every pixel should be white.
        assert!(ppu.framebuffer().iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn backdrop_fill_uses_pram_entry_0() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let mut pram = make_pram();
        // Mode 0 (not yet rendered) → backdrop fill from PRAM[0].
        // BGR555 pure blue (0x7C00).
        pram[0] = 0x00;
        pram[1] = 0x7C;
        ppu.write_dispcnt(0);
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0, 0xFF]);
    }

    #[test]
    fn mode0_bg0_4bpp_renders_first_tile_pixel() {
        // Arrange: Mode 0 with BG0 enabled. Tile 0 pixel (0,0) uses color
        // index 1 from BG palette bank 0, which we set to pure red.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);

        // BG palette entry 1 = BGR555 red (0x001F).
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Charblock 0, tile 1, row 0, first byte: pixel0=1, pixel1=1.
        vram[32] = 0x11;

        // Screenblock 0, map entry (0,0): tile index 1, palbank 0, no flip.
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x00;

        // Act: render one full frame.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Assert: top-left output pixel should come from tile data, not backdrop.
        assert_eq!(&ppu.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode0_bg0_4bpp_hflip_mirrors_tile_pixels() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);

        // BG palette entry 1 = pure red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Tile 1 row 0: pixel 7 uses color index 1, others are 0.
        // Byte 3 contains pixels 6 (low nibble) and 7 (high nibble).
        vram[32 + 3] = 0x10;

        // Screenblock entry (0,0): tile 1 + horizontal flip (bit 10).
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x04;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // With H-flip set, source pixel 7 appears at output x=0.
        assert_eq!(&ppu.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode0_bg0_4bpp_vflip_mirrors_tile_rows() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);

        // BG palette entry 1 = pure red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Tile 1 row 7 (byte offset +28): first pixel uses color index 1.
        vram[32 + 28] = 0x01;

        // Screenblock entry (0,0): tile 1 + vertical flip (bit 11).
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x08;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // With V-flip set, source row 7 appears at output y=0.
        assert_eq!(&ppu.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    #[should_panic(expected = "unimplemented GBA Mode 0 BG0 8bpp rendering")]
    fn mode0_bg0_8bpp_panics_until_implemented() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();

        // Mode 0 + BG0 enabled, and BG0CNT bit 7 (8bpp) set.
        ppu.write_dispcnt(dispcnt::BG0_ENABLE);
        ppu.write_bg0cnt(1 << 7);

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );
    }

    #[test]
    fn write_affine_routes_pa_pb_pc_pd_for_bg2_and_bg3() {
        let mut ppu = Ppu::new();
        // Identity-ish parameters for BG2.
        assert!(ppu.write_affine(REG_BG2PA, 0x0100));
        assert!(ppu.write_affine(REG_BG2PB, 0xFF00)); // -1.0
        assert!(ppu.write_affine(REG_BG2PC, 0x0080)); // 0.5
        assert!(ppu.write_affine(REG_BG2PD, 0x0100));
        // Independent values for BG3.
        assert!(ppu.write_affine(REG_BG3PA, 0x0040));
        assert!(ppu.write_affine(REG_BG3PD, 0x0040));

        let bg2 = ppu.bg_affine(0).expect("BG2 affine state must exist");
        assert_eq!(bg2.pa, 0x0100);
        assert_eq!(bg2.pb, -256);
        assert_eq!(bg2.pc, 0x0080);
        assert_eq!(bg2.pd, 0x0100);

        let bg3 = ppu.bg_affine(1).expect("BG3 affine state must exist");
        assert_eq!(bg3.pa, 0x0040);
        assert_eq!(bg3.pd, 0x0040);
        // BG3 must not have inherited BG2's parameters.
        assert_eq!(bg3.pb, 0);
        assert_eq!(bg3.pc, 0);
    }

    #[test]
    fn bg_affine_returns_none_for_out_of_range_index() {
        let ppu = Ppu::new();
        assert!(ppu.bg_affine(2).is_none());
        assert!(ppu.bg_affine(usize::MAX).is_none());
    }

    #[test]
    fn write_affine_x_y_assembles_28_bit_signed_reference() {
        let mut ppu = Ppu::new();
        // Compose BG2X = 0x0005_1234 via two halfword writes.
        assert!(ppu.write_affine(REG_BG2X_L, 0x1234));
        assert!(ppu.write_affine(REG_BG2X_H, 0x0005));
        // BG2Y high halfword has bit 27 set ⇒ negative.
        assert!(ppu.write_affine(REG_BG2Y_L, 0xFFFF));
        assert!(ppu.write_affine(REG_BG2Y_H, 0x0FFF));

        let bg2 = ppu.bg_affine(0).expect("BG2 affine state must exist");
        assert_eq!(bg2.x, 0x0005_1234);
        assert_eq!(bg2.y, -1);
    }

    #[test]
    fn write_affine_returns_false_for_unrelated_address() {
        let mut ppu = Ppu::new();
        // 0x0400_0000 (DISPCNT) is not an affine register.
        assert!(!ppu.write_affine(REG_DISPCNT, 0xFFFF));
        // Affine state must remain at default.
        let bg2 = ppu.bg_affine(0).expect("BG2 affine state must exist");
        assert_eq!(bg2.pa, 0);
        assert_eq!(bg2.x, 0);
    }

    #[test]
    fn mode2_renders_backdrop_color() {
        // Mode 2 is an affine tile mode for BG2/BG3. Currently we render
        // only the backdrop; full affine tile rendering is deferred.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let mut pram = make_pram();

        // Mode 2 (bits 0-2 = 2).
        ppu.write_dispcnt(2);

        // Backdrop = pure green (BGR555 0x03E0) in PRAM[0].
        pram[0] = 0xE0;
        pram[1] = 0x03;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // First pixel should be green (RGB888: 0, 255, 0).
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0xFF, 0]);
    }

    #[test]
    fn mode4_renders_paletted_bitmap_from_vram() {
        // Mode 4: 8-bit paletted bitmap. Each VRAM byte is a palette index.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 4 (bits 0-2 = 4), BG2 enabled.
        ppu.write_dispcnt(4 | dispcnt::BG2_ENABLE);

        // PRAM entry 5 = pure red (BGR555: 0x001F).
        pram[10] = 0x1F;
        pram[11] = 0x00;

        // VRAM[0] = palette index 5 for pixel (0, 0).
        vram[0] = 5;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // First pixel should be red (RGB888: 255, 0, 0).
        assert_eq!(&ppu.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode4_palette_index_0_shows_backdrop() {
        // Palette index 0 displays palette entry 0 (the backdrop color).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 4, BG2 enabled.
        ppu.write_dispcnt(4 | dispcnt::BG2_ENABLE);

        // Backdrop = pure blue (BGR555 0x7C00) in PRAM[0].
        pram[0] = 0x00;
        pram[1] = 0x7C;

        // VRAM[0] = palette index 0 (displays backdrop color).
        vram[0] = 0;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // First pixel should be backdrop blue (RGB888: 0, 0, 255).
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0, 0xFF]);
    }

    #[test]
    fn mode4_with_bg2_disabled_renders_backdrop() {
        // When BG2 is disabled, mode 4 should just render the backdrop.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let mut pram = make_pram();

        // Mode 4, BG2 NOT enabled.
        ppu.write_dispcnt(4);

        // Backdrop = pure green (BGR555 0x03E0) in PRAM[0].
        pram[0] = 0xE0;
        pram[1] = 0x03;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // First pixel should be green (RGB888: 0, 255, 0).
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0xFF, 0]);
    }

    #[test]
    fn mode4_frame_select_uses_correct_frame_base() {
        // Frame 1 is at VRAM offset 0xA000.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 4, BG2 enabled, frame 1 selected (bit 4).
        ppu.write_dispcnt(4 | dispcnt::BG2_ENABLE | dispcnt::FRAME_SELECT);

        // PRAM entry 7 = pure green (BGR555 0x03E0).
        pram[14] = 0xE0;
        pram[15] = 0x03;

        // Frame 0 pixel (0,0) = palette index 1.
        vram[0] = 1;
        // Frame 1 pixel (0,0) = palette index 7.
        vram[0xA000] = 7;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // First pixel should be from frame 1, which is green (RGB888: 0, 255, 0).
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0xFF, 0]);
    }

    #[test]
    fn mode0_bg1_renders_independently_of_bg0() {
        // BG1 enabled (not BG0). BG1 uses charblock 1, screenblock 8.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 0, BG1 enabled only.
        ppu.write_dispcnt(dispcnt::BG1_ENABLE);
        // BG1CNT: priority 0, charblock 1, screenblock 8.
        ppu.write_bg_cnt(1, (1 << 2) | (8 << 8));

        // BG palette entry 2 = pure green (BGR555 0x03E0).
        pram[4] = 0xE0;
        pram[5] = 0x03;

        // Charblock 1 (offset 0x4000), tile 1, row 0: pixel0=2.
        vram[0x4000 + 32] = 0x02;

        // Screenblock 8 (offset 0x4000), map entry (0,0): tile 1.
        vram[0x4000] = 0x01;
        vram[0x4001] = 0x00;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // First pixel = green from BG1.
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0xFF, 0]);
    }

    #[test]
    fn mode0_bg_priority_higher_priority_layer_on_top() {
        // BG0 (priority 1) and BG1 (priority 0). BG1 should be on top.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 0, BG0 + BG1 enabled.
        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::BG1_ENABLE);
        // BG0: priority 1, charblock 0, screenblock 0.
        ppu.write_bg_cnt(0, 1);
        // BG1: priority 0, charblock 1, screenblock 8.
        ppu.write_bg_cnt(1, (1 << 2) | (8 << 8));

        // BG palette entry 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;
        // BG palette entry 2 = green.
        pram[4] = 0xE0;
        pram[5] = 0x03;

        // BG0 tile 1 at charblock 0: pixel0 = palette index 1 (red).
        vram[32] = 0x01;
        // BG0 screenblock 0, entry (0,0): tile 1.
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x00;

        // BG1 tile 1 at charblock 1: pixel0 = palette index 2 (green).
        vram[0x4000 + 32] = 0x02;
        // BG1 screenblock 8, entry (0,0): tile 1.
        vram[0x4000] = 0x01;
        vram[0x4001] = 0x00;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // BG1 has lower priority number = on top, so pixel should be green.
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0xFF, 0]);
    }

    #[test]
    fn mode0_bg_transparent_pixel_shows_layer_below() {
        // BG0 (priority 0, on top) has transparent pixel, BG1 (priority 1) has red.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 0, BG0 + BG1 enabled.
        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::BG1_ENABLE);
        // BG0: priority 0 (on top), charblock 0, screenblock 0.
        ppu.write_bg_cnt(0, 0);
        // BG1: priority 1, charblock 1, screenblock 8.
        ppu.write_bg_cnt(1, 1 | (1 << 2) | (8 << 8));

        // BG palette entry 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // BG0: tile 1 pixel0 = 0 (transparent).
        vram[32] = 0x00;
        // BG0 screenblock 0: tile 1.
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x00;

        // BG1: tile 1 pixel0 = palette index 1 (red).
        vram[0x4000 + 32] = 0x01;
        // BG1 screenblock 8: tile 1.
        vram[0x4000] = 0x01;
        vram[0x4001] = 0x00;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // BG0 is transparent → BG1's red shows through.
        assert_eq!(&ppu.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    #[test]
    fn mode0_equal_priority_lower_bg_number_wins() {
        // BG0 and BG1 both at priority 0. BG0 should be on top (lower number).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::BG1_ENABLE);
        // Both at priority 0.
        ppu.write_bg_cnt(0, 0);
        ppu.write_bg_cnt(1, (1 << 2) | (8 << 8));

        // BG palette entry 1 = red, entry 2 = blue.
        pram[2] = 0x1F;
        pram[3] = 0x00;
        pram[4] = 0x00;
        pram[5] = 0x7C;

        // BG0: tile 1, pixel0 = 1 (red).
        vram[32] = 0x01;
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x00;

        // BG1: tile 1, pixel0 = 2 (blue).
        vram[0x4000 + 32] = 0x02;
        vram[0x4000] = 0x01;
        vram[0x4001] = 0x00;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Equal priority: BG0 (lower number) wins → red.
        assert_eq!(&ppu.framebuffer()[0..3], &[0xFF, 0, 0]);
    }

    /// Per GBATek, BGnCNT bit 13 is not used (zero) for BG0 and BG1.
    /// Reads of BG0CNT/BG1CNT must mask bit 13 out; BG2/BG3 are unmasked.
    #[test]
    fn bg0_bg1_cnt_mask_bit_13_on_read() {
        let mut ppu = Ppu::new();
        // Write all bits set.
        ppu.write_bg_cnt(0, 0xFFFF);
        ppu.write_bg_cnt(1, 0xFFFF);
        ppu.write_bg_cnt(2, 0xFFFF);
        ppu.write_bg_cnt(3, 0xFFFF);
        // BG0 and BG1: bit 13 must be masked out → 0xDFFF.
        assert_eq!(ppu.read_bg_cnt(0), 0xDFFF, "BG0CNT should mask bit 13");
        assert_eq!(ppu.read_bg_cnt(1), 0xDFFF, "BG1CNT should mask bit 13");
        // BG2 and BG3: all bits readable.
        assert_eq!(ppu.read_bg_cnt(2), 0xFFFF, "BG2CNT should be unmasked");
        assert_eq!(ppu.read_bg_cnt(3), 0xFFFF, "BG3CNT should be unmasked");
    }

    // ---- Affine internal reference point tests ----

    #[test]
    fn affine_internal_ref_latches_at_vblank() {
        // Internal reference points should be copied from register values
        // at VBlank (scanline entering 160).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();

        // Set BG2 reference point X=0x1000 (in 8.8 fixed), Y=0x2000.
        ppu.write_affine(REG_BG2X_L, 0x1000);
        ppu.write_affine(REG_BG2Y_L, 0x2000);

        // Run one full frame to trigger VBlank latch.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Internal refs should have been latched from the register values.
        let aff = ppu.bg_affine(0).unwrap();
        assert_eq!(aff.internal_x, 0x1000, "internal_x should latch from x");
        assert_eq!(aff.internal_y, 0x2000, "internal_y should latch from y");
    }

    #[test]
    fn affine_internal_ref_increments_per_scanline() {
        // After each visible scanline, internal refs are incremented by PB/PD.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();

        // BG2: PA=0x0100 (1.0), PB=0x0010 (1/16), PC=0, PD=0x0020 (1/8).
        // X=0, Y=0.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PB, 0x0010);
        ppu.write_affine(REG_BG2PC, 0x0000);
        ppu.write_affine(REG_BG2PD, 0x0020);

        // Run one full frame so VBlank latches internal refs (to 0,0),
        // then run 10 visible scanlines of the next frame.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );
        // Run 10 scanlines (each increments internal refs by PB/PD).
        ppu.step(CYCLES_PER_SCANLINE * 10, &mut ic, &vram, &pram, &make_oam());

        let aff = ppu.bg_affine(0).unwrap();
        // After 10 scanlines: internal_x += PB*10 = 0x0010*10 = 0x00A0
        assert_eq!(
            aff.internal_x, 0x00A0,
            "internal_x should increment by PB per scanline"
        );
        // internal_y += PD*10 = 0x0020*10 = 0x0140
        assert_eq!(
            aff.internal_y, 0x0140,
            "internal_y should increment by PD per scanline"
        );
    }

    // ---- Mode 2 affine tile rendering tests ----

    /// Helper: set up a minimal affine tile background in VRAM.
    /// Places a single non-zero tile (tile 1) at the given map position
    /// in a 256x256 (32x32 tiles) affine map.
    ///
    /// - `screenblock`: screen base block number (0-31)
    /// - `charblock`: char base block number (0-3)
    /// - `map_x`, `map_y`: tile position in the map (0-31)
    /// - `pram`: palette RAM — sets color 1 to the given BGR555 value
    fn setup_affine_tile(
        vram: &mut [u8],
        pram: &mut [u8],
        screenblock: usize,
        charblock: usize,
        map_x: usize,
        map_y: usize,
        color_bgr555: u16,
    ) {
        // Affine map: 1-byte entries, linear layout, 32x32 for size 1.
        let map_base = screenblock * 0x800;
        let map_entry = map_base + map_y * 32 + map_x;
        vram[map_entry] = 1; // tile index 1

        // Tile 1 in charblock: 8x8 8bpp = 64 bytes per tile.
        let tile_base = charblock * 16 * 1024 + 64;
        // Fill all 64 pixels with palette index 1.
        for i in 0..64 {
            vram[tile_base + i] = 1;
        }

        // Set palette color 1.
        let bytes = color_bgr555.to_le_bytes();
        pram[2] = bytes[0]; // palette[1] low byte
        pram[3] = bytes[1]; // palette[1] high byte
    }

    #[test]
    fn mode2_affine_identity_renders_tile() {
        // Mode 2, BG2 enabled, identity affine transform (PA=1.0, PD=1.0),
        // reference points at (0,0). Should render tile at map position (0,0).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 2 + BG2 enable.
        ppu.write_dispcnt(2 | dispcnt::BG2_ENABLE);

        // BG2CNT: charblock 0, screenblock 8, size 1 (256x256), no wrap.
        ppu.write_bg_cnt(2, (8 << 8) | (1 << 14));

        // Identity affine: PA=0x0100 (1.0), PD=0x0100 (1.0), PB=PC=0.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Place tile 1 at map (0,0) with green color.
        let green = 0x03E0u16; // BGR555 pure green
        setup_affine_tile(&mut vram, &mut pram, 8, 0, 0, 0, green);

        // Backdrop = black.
        pram[0] = 0;
        pram[1] = 0;

        // Run one full frame + first scanline of next frame.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME + CYCLES_PER_SCANLINE,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Pixel (0,0) should be green (from tile 1 at map 0,0).
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0xFF, 0],
            "pixel (0,0) should be green from affine tile"
        );
    }

    #[test]
    fn mode2_affine_palette_index_0_is_transparent() {
        // A tile with palette index 0 should be transparent (show backdrop).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 2 + BG2 enable.
        ppu.write_dispcnt(2 | dispcnt::BG2_ENABLE);

        // BG2CNT: charblock 0, screenblock 8, size 1 (256x256).
        ppu.write_bg_cnt(2, (8 << 8) | (1 << 14));

        // Identity affine.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Map entry at (0,0) points to tile 1, but tile 1 is all zeros
        // (palette index 0 = transparent).
        let map_base = 8 * 0x800;
        vram[map_base] = 1;
        // tile 1 data is already all zeros.

        // Backdrop = red.
        let red_bgr = 0x001Fu16; // BGR555 red
        pram[0] = red_bgr as u8;
        pram[1] = (red_bgr >> 8) as u8;

        // Run full frame + scanline.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME + CYCLES_PER_SCANLINE,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Should show backdrop (red), not tile color.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0xFF, 0, 0],
            "transparent tile should show backdrop"
        );
    }

    #[test]
    fn mode2_affine_out_of_bounds_is_transparent_when_no_wrap() {
        // When wrapping is disabled (BGCNT bit 13 = 0), pixels outside
        // the map bounds should be transparent (show backdrop).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 2 + BG2 enable.
        ppu.write_dispcnt(2 | dispcnt::BG2_ENABLE);

        // BG2CNT: charblock 0, screenblock 8, size 0 (128x128 = 16x16 tiles),
        // NO wrapping.
        ppu.write_bg_cnt(2, 8 << 8);

        // Identity affine, but offset to render outside the 128x128 map.
        // dx = 200 pixels = 200 << 8 in fixed 8.8 = 0xC800.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        ppu.write_affine(REG_BG2X_L, 0xC800);

        // Place tile 1 at map position (0,0) with green color.
        let green = 0x03E0u16;
        setup_affine_tile(&mut vram, &mut pram, 8, 0, 0, 0, green);

        // Backdrop = blue.
        let blue_bgr = 0x7C00u16;
        pram[0] = blue_bgr as u8;
        pram[1] = (blue_bgr >> 8) as u8;

        // Run full frame + scanline.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME + CYCLES_PER_SCANLINE,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Pixel (0,0) maps to texture (200, 0) which is outside 128x128.
        // Should show backdrop (blue).
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0, 0xFF],
            "out-of-bounds should show backdrop when no wrap"
        );
    }

    #[test]
    fn mode2_affine_wrapping_wraps_coordinates() {
        // When wrapping is enabled (BGCNT bit 13 = 1), out-of-bounds
        // coordinates should wrap around the map.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 2 + BG2 enable.
        ppu.write_dispcnt(2 | dispcnt::BG2_ENABLE);

        // BG2CNT: charblock 0, screenblock 8, size 0 (128x128),
        // WITH wrapping (bit 13 = 1).
        ppu.write_bg_cnt(2, (8 << 8) | (1 << 13));

        // Identity affine, offset by 128 pixels (= full map width).
        // Should wrap back to position 0.
        // 128 pixels in 8.8 fixed = 128 << 8 = 0x8000.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        ppu.write_affine(REG_BG2X_L, 0x8000); // X = 128 in 8.8 fixed

        // Place tile at map (0,0) with green.
        let green = 0x03E0u16;
        setup_affine_tile(&mut vram, &mut pram, 8, 0, 0, 0, green);

        // Backdrop = black.
        pram[0] = 0;
        pram[1] = 0;

        // Run full frame + scanline.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME + CYCLES_PER_SCANLINE,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Texture x = 128 wraps to 0 in a 128-pixel map → should see tile.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0xFF, 0],
            "wrapping should show tile at wrapped position"
        );
    }

    #[test]
    fn mode2_affine_bg2_bg3_priority_compositing() {
        // Two affine BGs with different priorities: higher priority (lower
        // BGCNT value) should appear on top.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 2 + BG2 + BG3 enable.
        ppu.write_dispcnt(2 | dispcnt::BG2_ENABLE | dispcnt::BG3_ENABLE);

        // BG2: priority 1, charblock 0, screenblock 8, size 1 (256x256).
        ppu.write_bg_cnt(2, 1 | (8 << 8) | (1 << 14));
        // BG3: priority 0 (higher visual priority), charblock 0, screenblock 16, size 1.
        ppu.write_bg_cnt(3, (16 << 8) | (1 << 14));

        // Identity affine for both.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        ppu.write_affine(REG_BG3PA, 0x0100);
        ppu.write_affine(REG_BG3PD, 0x0100);

        // BG2 tile at (0,0): red.
        let red = 0x001Fu16;
        let map2_base = 8 * 0x800;
        vram[map2_base] = 1; // tile 1
        let tile1_base = 64; // charblock 0 + tile 1 (64 bytes per 8bpp tile)
        for i in 0..64 {
            vram[tile1_base + i] = 1;
        }
        pram[2] = red as u8;
        pram[3] = (red >> 8) as u8;

        // BG3 tile at (0,0): green (using tile 2 and palette index 2).
        let green = 0x03E0u16;
        let map3_base = 16 * 0x800;
        vram[map3_base] = 2; // tile 2
        let tile2_base = 128; // charblock 0 + tile 2 (64 bytes per 8bpp tile)
        for i in 0..64 {
            vram[tile2_base + i] = 2;
        }
        pram[4] = green as u8;
        pram[5] = (green >> 8) as u8;

        // Backdrop = black.
        pram[0] = 0;
        pram[1] = 0;

        // Run full frame + scanline.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME + CYCLES_PER_SCANLINE,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // BG3 has priority 0 (on top), BG2 has priority 1 (behind).
        // Pixel should be green (BG3 on top).
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0xFF, 0],
            "BG3 (priority 0) should be on top of BG2 (priority 1)"
        );
    }

    #[test]
    fn mode1_mixes_regular_and_affine_bgs() {
        // Mode 1: BG0/BG1 regular text + BG2 affine.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 1 + BG0 + BG2 enable.
        ppu.write_dispcnt(1 | dispcnt::BG0_ENABLE | dispcnt::BG2_ENABLE);

        // BG0: priority 1, 4bpp text mode. Charblock 0, screenblock 4.
        ppu.write_bg_cnt(0, 1 | (4 << 8));
        // BG2: priority 0 (on top), affine. Charblock 2, screenblock 8, size 1.
        ppu.write_bg_cnt(2, (1 << 7) | (2 << 2) | (8 << 8) | (1 << 14));

        // Identity affine for BG2.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Set up BG2 affine tile at (0,0): green.
        let green = 0x03E0u16;
        let map_base = 8 * 0x800;
        vram[map_base] = 1; // tile 1
        let charblock2_base = 2 * 16 * 1024;
        let tile1_base = charblock2_base + 64;
        for i in 0..64 {
            vram[tile1_base + i] = 1;
        }
        pram[2] = green as u8;
        pram[3] = (green >> 8) as u8;

        // Set up BG0 text tile at (0,0): red (4bpp, palette bank 0).
        // Screenblock 4, tile 1.
        let red = 0x001Fu16;
        let sb4_base = 4 * 0x800;
        // Map entry: tile 1, no flip, palette bank 0.
        vram[sb4_base] = 1;
        vram[sb4_base + 1] = 0;
        // Tile 1 in charblock 0 (4bpp: 32 bytes per tile).
        let text_tile_base = 32; // charblock 0 + tile 1 (32 bytes per 4bpp tile)
        // Fill with palette index 1 (4bpp: two pixels per byte, both index 1).
        for i in 0..32 {
            vram[text_tile_base + i] = 0x11;
        }
        // Palette bank 0, color 1 = red.
        pram[2] = red as u8;
        pram[3] = (red >> 8) as u8;

        // Wait — BG0 and BG2 share palette. BG2 uses 256-color mode.
        // Palette[1] = red (set above). BG2 tile uses palette index 1 → red.
        // Let's use palette index 2 for BG2 instead.
        for i in 0..64 {
            vram[tile1_base + i] = 2; // palette index 2
        }
        pram[4] = green as u8;
        pram[5] = (green >> 8) as u8;

        // Backdrop = blue.
        let blue = 0x7C00u16;
        pram[0] = blue as u8;
        pram[1] = (blue >> 8) as u8;

        // Run full frame + scanline.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME + CYCLES_PER_SCANLINE,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // BG2 (priority 0, affine) should be on top of BG0 (priority 1, text).
        // Pixel should be green.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0xFF, 0],
            "Mode 1: BG2 affine (priority 0) on top of BG0 text (priority 1)"
        );
    }
}
