//! GBA Picture Processing Unit (PPU).
//!
//! Implements the GBA LCD controller timing and a small subset of the
//! display modes. This module is the foundation for subsequent rendering
//! work — additional display modes (0/4/5), tile background layers, and
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
//! * Mode 3 background rendering (240×160 15-bit BGR555 direct bitmap
//!   from VRAM) when `BG2` is enabled.
//! * Backdrop fill from the first palette entry for unimplemented
//!   display modes (and Mode 3 when BG2 is disabled).
//! * Forced-blank outputs solid white (per GBATek).
//!
//! Out of scope (deferred to follow-up sub-issues):
//!
//! * Mode 0 tile background layers (BG0–BG3 with tile maps + char data).
//! * Mode 1, Mode 2 affine modes, Mode 4 (8-bit paletted) and Mode 5
//!   (160×128 15-bit) rendering.
//! * Sprite (OBJ) rendering and OAM attribute decoding.
//! * Window masks, alpha blending, mosaic, brightness effects.
//!
//! References:
//! * GBATek "LCD I/O Display Control": <https://problemkaputt.de/gbatek.htm#lcdiodisplaycontrol>
//! * GBATek "LCD I/O Interrupts and Status": <https://problemkaputt.de/gbatek.htm#lcdiointerruptsandstatus>
//! * Tonc "Video Introduction": <https://www.coranac.com/tonc/text/video.htm>

pub mod affine;
pub mod color;

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
pub const REG_DISPSTAT: u32 = 0x0400_0004;
pub const REG_VCOUNT: u32 = 0x0400_0006;

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
            vcount: 0,
            line_cycle: 0,
            framebuffer: vec![0; FRAMEBUFFER_BYTES],
            frame_ready: false,
            bg_affine: [BgAffine::default(); 2],
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

    /// Borrow the affine register file for BG2 (`bg`=0) or BG3 (`bg`=1).
    /// Used by the affine renderer (and tests) to read the latched
    /// parameters and reference points.
    pub fn bg_affine(&self, bg: usize) -> &BgAffine {
        &self.bg_affine[bg]
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
                    self.render_scanline(self.vcount as u32, vram, pram);
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
    fn render_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8]) {
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
            3 => self.render_mode3_scanline(y, vram, pram),
            _ => self.render_backdrop_scanline(y, pram),
        }
    }

    /// Mode 3: 240×160 direct 15-bit bitmap starting at the base of
    /// VRAM. Each pixel is a 16-bit BGR555 value. When BG2 is disabled,
    /// the scanline is filled with the backdrop color (palette entry 0
    /// in PRAM) per GBATek — every pixel is "no BG/OBJ pixel drawn", so
    /// the backdrop shows through.
    fn render_mode3_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8]) {
        if !self.bg2_enabled() {
            self.render_backdrop_scanline(y, pram);
            return;
        }
        let line_byte_offset = (y as usize) * (SCREEN_WIDTH as usize) * 2;
        for x in 0..(SCREEN_WIDTH as usize) {
            let src = line_byte_offset + x * 2;
            // VRAM is at least 96 KB; mode 3 uses the first 240*160*2 =
            // 76 800 bytes which always fits.
            let bgr555 = u16::from_le_bytes([vram[src], vram[src + 1]]);
            let dst = ((y as usize) * (SCREEN_WIDTH as usize) + x) * BYTES_PER_PIXEL;
            color::write_pixel(&mut self.framebuffer, dst, bgr555);
        }
    }

    /// Backdrop fill — uses palette entry 0 from PRAM. Used for modes
    /// where rendering is not yet implemented in this increment.
    fn render_backdrop_scanline(&mut self, y: u32, pram: &[u8]) {
        let backdrop = if pram.len() >= 2 {
            u16::from_le_bytes([pram[0], pram[1]])
        } else {
            0
        };
        let (r, g, b) = color::bgr555_to_rgb888(backdrop);
        let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        for x in 0..(SCREEN_WIDTH as usize) {
            let dst = row_start + x * BYTES_PER_PIXEL;
            self.framebuffer[dst] = r;
            self.framebuffer[dst + 1] = g;
            self.framebuffer[dst + 2] = b;
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
        ppu.step(500, &mut ic, &make_vram(), &make_pram());
        assert_eq!(ppu.read_vcount(), 0);
        // No H-Blank yet at cycle 500.
        assert_eq!(ppu.read_dispstat() & dispstat::HBLANK_FLAG, 0);
    }

    #[test]
    fn hblank_flag_sets_at_cycle_1006() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        ppu.step(HBLANK_START_CYCLE - 1, &mut ic, &make_vram(), &make_pram());
        assert_eq!(ppu.read_dispstat() & dispstat::HBLANK_FLAG, 0);
        ppu.step(1, &mut ic, &make_vram(), &make_pram());
        assert_ne!(ppu.read_dispstat() & dispstat::HBLANK_FLAG, 0);
    }

    #[test]
    fn hblank_flag_clears_when_scanline_advances() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        ppu.step(CYCLES_PER_SCANLINE, &mut ic, &make_vram(), &make_pram());
        assert_eq!(ppu.read_vcount(), 1);
        // After crossing the scanline boundary the H-Blank flag is gone.
        assert_eq!(ppu.read_dispstat() & dispstat::HBLANK_FLAG, 0);
    }

    #[test]
    fn hblank_irq_fires_only_when_enabled() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        // Without H-Blank IRQ enable bit, no IRQ should be raised.
        ppu.step(HBLANK_START_CYCLE, &mut ic, &make_vram(), &make_pram());
        assert_eq!(ic.if_flags & irq_bits::HBLANK, 0);

        // Enable + advance to next H-Blank → IRQ flagged.
        ppu.write_dispstat(dispstat::HBLANK_IRQ_ENABLE, &mut ic);
        ppu.step(CYCLES_PER_SCANLINE, &mut ic, &make_vram(), &make_pram());
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
        ppu.step(cycles, &mut ic, &vram, &pram);
        assert_eq!(ppu.read_vcount(), 160);
        // Step through H-Blank of scanline 160 — no visible H-Blank.
        let events = ppu.step(HBLANK_START_CYCLE, &mut ic, &vram, &pram);
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
        let events = ppu.step(cycles, &mut ic, &vram, &pram);
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
        ppu.step(cycles, &mut ic, &make_vram(), &make_pram());
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
        ppu.step(cycles, &mut ic, &vram, &pram);
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
        ppu.step(total, &mut ic, &vram, &pram);
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
        ppu.step(CYCLES_PER_SCANLINE * 7, &mut ic, &make_vram(), &make_pram());
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
        ppu.step(CYCLES_PER_SCANLINE * 5, &mut ic, &make_vram(), &make_pram());
        assert_eq!(ppu.read_vcount(), 5);
        assert_ne!(ppu.read_dispstat() & dispstat::VCOUNT_FLAG, 0);
        assert_ne!(ic.if_flags & irq_bits::VCOUNT, 0);
        // Advance one more line — flag clears.
        ppu.step(CYCLES_PER_SCANLINE, &mut ic, &make_vram(), &make_pram());
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
        );
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0, 0xFF]);
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

        let bg2 = ppu.bg_affine(0);
        assert_eq!(bg2.pa, 0x0100);
        assert_eq!(bg2.pb, -256);
        assert_eq!(bg2.pc, 0x0080);
        assert_eq!(bg2.pd, 0x0100);

        let bg3 = ppu.bg_affine(1);
        assert_eq!(bg3.pa, 0x0040);
        assert_eq!(bg3.pd, 0x0040);
        // BG3 must not have inherited BG2's parameters.
        assert_eq!(bg3.pb, 0);
        assert_eq!(bg3.pc, 0);
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

        let bg2 = ppu.bg_affine(0);
        assert_eq!(bg2.x, 0x0005_1234);
        assert_eq!(bg2.y, -1);
    }

    #[test]
    fn write_affine_returns_false_for_unrelated_address() {
        let mut ppu = Ppu::new();
        // 0x0400_0000 (DISPCNT) is not an affine register.
        assert!(!ppu.write_affine(REG_DISPCNT, 0xFFFF));
        // Affine state must remain at default.
        let bg2 = ppu.bg_affine(0);
        assert_eq!(bg2.pa, 0);
        assert_eq!(bg2.x, 0);
    }
}
