//! GBA Picture Processing Unit (PPU).
//!
//! Implements the GBA LCD controller timing and a small subset of the
//! display modes. This module is the foundation for subsequent rendering
//! work — additional display effects, affine OBJ features, and remaining
//! background details will be added in follow-up sub-issues of
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
//! * Mode 0 tile background rendering (BG0–BG3 with 4bpp/8bpp text
//!   backgrounds, priority compositing across all enabled layers).
//! * Mode 1 mixed background rendering (BG0/BG1 regular text plus BG2 affine).
//! * Mode 2 affine tile background rendering: BG2 and BG3 with wrapping,
//!   mosaic, and priority compositing. Both BG2 and BG3 support affine
//!   transforms (PA/PB/PC/PD + BG2X/BG2Y / BG3X/BG3Y reference points).
//! * Mode 3 background rendering (240×160 15-bit BGR555 direct bitmap
//!   from VRAM) when `BG2` is enabled.
//! * Mode 4 background rendering (240×160 8-bit paletted bitmap from
//!   VRAM) with dual-frame support via DISPCNT bit 4.
//! * Mode 5 background rendering (160×128 15-bit BGR555 direct bitmap
//!   from VRAM) with dual-frame support via DISPCNT bit 4 and BG2 affine
//!   positioning/scaling.
//! * Modes 6-7 are "prohibited" per GBATek: no BG layers are rendered but OBJ
//!   sprites and the backdrop behave normally (matching mGBA behavior). The
//!   backdrop color shows through wherever no OBJ pixel lands.
//! * OBJ rendering (regular and affine), OAM attribute decoding, and OBJ
//!   Window mask generation.
//! * Window masks, alpha blending / brightness effects, and MOSAIC for
//!   BG/OBJ render paths.
//! * Forced-blank outputs solid white (per GBATek).
//! * Forced-blank mid-frame restart: when DISPCNT bit 7 (Forced Blank)
//!   transitions from 1 to 0 mid-frame, the PPU continues to output white
//!   for ~2 additional scanlines, then resets VCOUNT to 0 and resumes
//!   normal rendering from line 0.  This models the GBATek behaviour
//!   "display restarts from line 0 after 2 vertical lines".
//! * `BG2X`/`BG2Y` mid-frame writes: per GBATek "LCD I/O BG
//!   Rotation/Scaling", a write to `BG2X`/`BG2Y` (or the BG3 equivalents)
//!   outside V-Blank immediately applies to the internal reference point for
//!   the current scanline.  The write_affine methods on `Ppu` implement this
//!   correctly — `internal_x`/`internal_y` are updated immediately alongside
//!   the register values.  The PB/PD accumulation then continues from the
//!   newly written base on every subsequent scanline, and the value is
//!   re-latched at the next V-Blank as usual.
//!
//! Out of scope (deferred to follow-up sub-issues):
//!
//! * Per-pixel (cycle-exact) mid-scanline register changes: effects such as
//!   raster-scroll tricks that require a register change to take effect at a
//!   specific dot within a single scanline are not yet modelled.  Rendering
//!   is performed atomically at the H-Blank edge, so all pixels in a scanline
//!   share the same register snapshot.  The `BG2X`/`BG2Y` mid-frame write
//!   described above is an exception and is already handled.
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
use serde::{Deserialize, Serialize};

/// GBA visible screen width in pixels.
pub const SCREEN_WIDTH: u32 = 240;
/// GBA visible screen height in pixels.
pub const SCREEN_HEIGHT: u32 = 160;
/// Bytes per pixel in the RGB888 framebuffer exposed to the frontend.
pub const BYTES_PER_PIXEL: usize = 3;
/// Total framebuffer size in bytes (240 × 160 × 3).
pub const FRAMEBUFFER_BYTES: usize =
    (SCREEN_WIDTH as usize) * (SCREEN_HEIGHT as usize) * BYTES_PER_PIXEL;

/// Marker value for a transparent pixel in per-layer color buffers.
/// Valid BGR555 colors use bits 0-14 only, so bit 15 set means "no pixel".
const TRANSPARENT: u16 = 0x8000;
/// Mode 3 bitmap frame base in VRAM.
const MODE3_FRAME_BASE: usize = 0x0000;
/// Mode 5 bitmap width in pixels.
const MODE5_WIDTH: usize = 160;
/// Mode 5 bitmap height in pixels.
const MODE5_HEIGHT: usize = 128;

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
    /// H-Blank Interval Free (DISPCNT[5]) — allows OAM access during H-Blank but
    /// reduces available OBJ rendering cycles from 1210 to 954 per scanline.
    pub const HBLANK_INTERVAL_FREE: u16 = 1 << 5;
    /// Display Window 0 (DISPCNT[13]).
    pub const WIN0_ENABLE: u16 = 1 << 13;
    /// Display Window 1 (DISPCNT[14]).
    pub const WIN1_ENABLE: u16 = 1 << 14;
    /// Display OBJ Window (DISPCNT[15]).
    pub const OBJ_WIN_ENABLE: u16 = 1 << 15;
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
/// Green Swap (0x0400_0002, R/W). Bit 0: swap green between adjacent pixel pairs.
pub const REG_GREEN_SWAP: u32 = 0x0400_0002;
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

// Window registers (write-only for H/V coords, read/write for IN/OUT).
pub const REG_WIN0H: u32 = 0x0400_0040;
pub const REG_WIN1H: u32 = 0x0400_0042;
pub const REG_WIN0V: u32 = 0x0400_0044;
pub const REG_WIN1V: u32 = 0x0400_0046;
pub const REG_WININ: u32 = 0x0400_0048;
pub const REG_WINOUT: u32 = 0x0400_004A;
/// `MOSAIC` (0x0400_004C): BG/OBJ mosaic sizes (write-only).
pub const REG_MOSAIC: u32 = 0x0400_004C;

// Color special effect registers.
/// `BLDCNT` (0x0400_0050): color effect control (R/W).
pub const REG_BLDCNT: u32 = 0x0400_0050;
/// `BLDALPHA` (0x0400_0052): alpha blend coefficients (R/W).
pub const REG_BLDALPHA: u32 = 0x0400_0052;
/// `BLDY` (0x0400_0054): brightness coefficient (W only).
pub const REG_BLDY: u32 = 0x0400_0054;

/// Result of stepping the PPU — counts telling the bus how many DMA
/// hooks (V-Blank / H-Blank) to fire after a step.
///
/// The PPU does not own the bus, so it cannot directly invoke the
/// `notify_*` paths on the bus that wake DMA channels. Instead it
/// reports the edges that occurred during the most recent step and the
/// caller (the bus) routes them. Counts (rather than booleans) are
/// required because a single `step()` call may span many scanlines —
/// e.g. a full-frame step crosses 228 H-Blanks and one V-Blank,
/// and the bus must propagate every one to keep H-Blank-mode DMA
/// channels firing per scanline.
#[derive(Debug, Default, Clone, Copy)]
pub struct PpuStepEvents {
    /// Number of V-Blank periods that started during this step
    /// (transitions into scanline 160).
    pub vblank_starts: u32,
    /// Number of H-Blank periods that started during this step (on any
    /// scanline, including non-visible scanlines 160–227). Per GBATek,
    /// H-Blank DMA fires on all 228 scanlines. Only the H-Blank IRQ is
    /// suppressed during V-Blank (scanlines 160–[`VBLANK_LAST_SCANLINE`]).
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
    /// Window horizontal boundaries: `[WIN0H, WIN1H]`.
    /// Each register: bits 8-15 = X1 (left), bits 0-7 = X2 (right).
    win_h: [u16; 2],
    /// Window vertical boundaries: `[WIN0V, WIN1V]`.
    /// Each register: bits 8-15 = Y1 (top), bits 0-7 = Y2 (bottom).
    win_v: [u16; 2],
    /// `WININ` (0x0400_0048): layer enable for inside Window 0 (bits 0-5)
    /// and Window 1 (bits 8-13).
    winin: u16,
    /// `WINOUT` (0x0400_004A): layer enable for outside all windows (bits 0-5)
    /// and inside OBJ Window (bits 8-13).
    winout: u16,
    /// Green Swap (0x0400_0002, bit 0). When set, swaps the green channel
    /// between each pair of adjacent pixels (even/odd x) in the final output.
    green_swap: bool,
    /// `BLDCNT` (0x0400_0050): color special effect control.
    /// Bits 0-5: 1st target selection (BG0-BG3, OBJ, Backdrop).
    /// Bits 6-7: effect mode (0=None, 1=Alpha, 2=Brighten, 3=Darken).
    /// Bits 8-13: 2nd target selection (BG0-BG3, OBJ, Backdrop).
    bldcnt: u16,
    /// `BLDALPHA` (0x0400_0052): alpha blend coefficients.
    /// Bits 0-4: EVA (1st target, 0-16).
    /// Bits 8-12: EVB (2nd target, 0-16).
    bldalpha: u16,
    /// `BLDY` (0x0400_0054, write-only): brightness coefficient.
    /// Bits 0-4: EVY (0-16).
    bldy: u8,
    /// `MOSAIC` (0x0400_004C, write-only): BG/OBJ mosaic block sizes.
    mosaic: u16,
    /// Countdown of remaining scanlines before the display restarts from
    /// line 0, triggered when Forced Blank (DISPCNT bit 7) transitions from
    /// 1 to 0 mid-frame.  Per GBATek, the display restarts from line 0
    /// approximately 2 vertical lines after the Forced Blank is de-asserted.
    /// Counts down from 2 to 0; while > 0, the PPU outputs white just like
    /// active Forced Blank.  Cleared immediately if Forced Blank is
    /// re-asserted before the restart completes.
    forced_blank_restart_lines: u32,
    /// When true, apply GBA LCD color correction (gamma ≈ 4 curve) when
    /// converting BGR555 palette entries to RGB888.
    ///
    /// Simulates the GBA TFT LCD's physical non-linear response where values
    /// 0–14 appear nearly black. Default: false (linear expansion).
    color_correction: bool,
}

/// Serializable GBA PPU snapshot for save states.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PpuState {
    dispcnt: u16,
    dispstat: u16,
    bg_cnt: [u16; 4],
    vcount: u16,
    line_cycle: u32,
    framebuffer: Vec<u8>,
    frame_ready: bool,
    bg_affine: [BgAffine; 2],
    bg_scroll: [(u16, u16); 4],
    win_h: [u16; 2],
    win_v: [u16; 2],
    winin: u16,
    winout: u16,
    green_swap: bool,
    bldcnt: u16,
    bldalpha: u16,
    bldy: u8,
    mosaic: u16,
    forced_blank_restart_lines: u32,
    color_correction: bool,
}

// ---- Color special effect helpers ----------------------------------------

/// Apply alpha blending to two BGR555 colors using integer fixed-point math.
///
/// Per GBATek: `I = MIN(31, (I1 * EVA + I2 * EVB) >> 4)` per channel.
/// `eva` and `evb` must already be clamped to `[0, 16]`.
#[inline]
fn alpha_blend_bgr555(c1: u16, c2: u16, eva: u8, evb: u8) -> u16 {
    let eva = eva as u32;
    let evb = evb as u32;
    let r = ((c1 & 0x1F) as u32 * eva + (c2 & 0x1F) as u32 * evb) >> 4;
    let g = (((c1 >> 5) & 0x1F) as u32 * eva + ((c2 >> 5) & 0x1F) as u32 * evb) >> 4;
    let b = (((c1 >> 10) & 0x1F) as u32 * eva + ((c2 >> 10) & 0x1F) as u32 * evb) >> 4;
    (r.min(31) | (g.min(31) << 5) | (b.min(31) << 10)) as u16
}

/// Apply brightness increase to a BGR555 color.
///
/// Per GBATek: `I = I1 + (31 - I1) * EVY / 16` per channel (EVY ∈ [0,16]).
/// `evy` must already be clamped to `[0, 16]`.
#[inline]
fn brighten_bgr555(c: u16, evy: u8) -> u16 {
    let evy = evy as u32;
    let r = (c & 0x1F) as u32;
    let g = ((c >> 5) & 0x1F) as u32;
    let b = ((c >> 10) & 0x1F) as u32;
    let r2 = r + (((31 - r) * evy) >> 4);
    let g2 = g + (((31 - g) * evy) >> 4);
    let b2 = b + (((31 - b) * evy) >> 4);
    (r2 | (g2 << 5) | (b2 << 10)) as u16
}

/// Apply brightness decrease to a BGR555 color.
///
/// Per GBATek: `I = I1 - I1 * EVY / 16` per channel (EVY ∈ [0,16]).
/// `evy` must already be clamped to `[0, 16]`.
#[inline]
fn darken_bgr555(c: u16, evy: u8) -> u16 {
    let evy = evy as u32;
    let r = (c & 0x1F) as u32;
    let g = ((c >> 5) & 0x1F) as u32;
    let b = ((c >> 10) & 0x1F) as u32;
    let r2 = r - ((r * evy) >> 4);
    let g2 = g - ((g * evy) >> 4);
    let b2 = b - ((b * evy) >> 4);
    (r2 | (g2 << 5) | (b2 << 10)) as u16
}

// --------------------------------------------------------------------------

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

// ---- Compositing priority sort-key constants --------------------------------

/// Spacing between priority levels in the compositing sort-key.
///
/// OBJ at hardware priority `p` gets sort_key `p * SORT_KEY_PRIORITY_SPACING`.
/// BG bg_idx at priority `p` gets sort_key `p * SORT_KEY_PRIORITY_SPACING +
/// SORT_KEY_BG_OFFSET + bg_idx`.
///
/// This ensures OBJ always beats all BGs at the same hardware priority level
/// (since SORT_KEY_BG_OFFSET > 0), lower BG index beats higher BG index at
/// equal priority, and lower priority number means higher draw order.
const SORT_KEY_PRIORITY_SPACING: u16 = 10;

/// Base offset added to BG sort-keys so that BG always loses to OBJ at the
/// same hardware priority level.
const SORT_KEY_BG_OFFSET: u16 = 5;

// --------------------------------------------------------------------------

/// Extract a 4-bit MOSAIC size field and return its effective pixel size.
///
/// `shift` selects BG H (0), BG V (4), OBJ H (8), or OBJ V (12). Hardware
/// stores each size as `effective_size - 1`, so this returns `field + 1`.
#[inline]
fn mosaic_size(mosaic: u16, shift: u32) -> u32 {
    (((mosaic >> shift) & 0x000F) + 1) as u32
}

/// Return the anchor coordinate of the mosaic block containing `value`.
///
/// For a block size of `N`, coordinate `C` maps to `C - (C % N)`, the
/// upper-left coordinate of the block. `block_size` must be positive; MOSAIC
/// fields satisfy this because [`mosaic_size`] returns values in 1..=16.
fn mosaic_anchor(value: u32, block_size: u32) -> u32 {
    debug_assert!(block_size > 0);
    value - (value % block_size)
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
            win_h: [0; 2],
            win_v: [0; 2],
            winin: 0,
            winout: 0,
            green_swap: false,
            bldcnt: 0,
            bldalpha: 0,
            bldy: 0,
            mosaic: 0,
            forced_blank_restart_lines: 0,
            color_correction: false,
        };
        // VCOUNT == LYC == 0 at reset; reflect that in the match flag.
        // No IRQ is raised here — the controller hasn't been wired up
        // yet at construction time.
        ppu.update_vcount_match_flag(None);
        ppu
    }

    /// Capture PPU state for save-state serialization.
    pub fn capture_state(&self) -> PpuState {
        PpuState {
            dispcnt: self.dispcnt,
            dispstat: self.dispstat,
            bg_cnt: self.bg_cnt,
            vcount: self.vcount,
            line_cycle: self.line_cycle,
            framebuffer: self.framebuffer.clone(),
            frame_ready: self.frame_ready,
            bg_affine: self.bg_affine,
            bg_scroll: self.bg_scroll,
            win_h: self.win_h,
            win_v: self.win_v,
            winin: self.winin,
            winout: self.winout,
            green_swap: self.green_swap,
            bldcnt: self.bldcnt,
            bldalpha: self.bldalpha,
            bldy: self.bldy,
            mosaic: self.mosaic,
            forced_blank_restart_lines: self.forced_blank_restart_lines,
            color_correction: self.color_correction,
        }
    }

    /// Restore PPU state from a save-state snapshot.
    pub fn restore_state(&mut self, state: &PpuState) {
        self.dispcnt = state.dispcnt;
        self.dispstat = state.dispstat;
        self.bg_cnt = state.bg_cnt;
        self.vcount = state.vcount;
        self.line_cycle = state.line_cycle;
        self.framebuffer.clone_from(&state.framebuffer);
        self.frame_ready = state.frame_ready;
        self.bg_affine = state.bg_affine;
        self.bg_scroll = state.bg_scroll;
        self.win_h = state.win_h;
        self.win_v = state.win_v;
        self.winin = state.winin;
        self.winout = state.winout;
        self.green_swap = state.green_swap;
        self.bldcnt = state.bldcnt;
        self.bldalpha = state.bldalpha;
        self.bldy = state.bldy;
        self.mosaic = state.mosaic;
        self.forced_blank_restart_lines = state.forced_blank_restart_lines;
        self.color_correction = state.color_correction;
    }

    /// Read `DISPCNT`.
    pub fn read_dispcnt(&self) -> u16 {
        self.dispcnt
    }

    /// Write `DISPCNT`.
    ///
    /// Per GBATek "LCD I/O Display Control": when Forced Blank (bit 7)
    /// transitions from 1 to 0, the display restarts from line 0 after
    /// approximately 2 vertical lines.  We model this with a countdown
    /// field that keeps forced-blank white output active for 2 more
    /// scanlines, then resets VCOUNT to 0.  If Forced Blank is
    /// re-asserted before the restart completes, the countdown is cleared.
    pub fn write_dispcnt(&mut self, value: u16) {
        let was_forced_blank = self.dispcnt & dispcnt::FORCED_BLANK != 0;
        let is_forced_blank = value & dispcnt::FORCED_BLANK != 0;
        self.dispcnt = value;
        if was_forced_blank && !is_forced_blank {
            // Forced Blank de-asserted: begin 2-scanline restart countdown.
            self.forced_blank_restart_lines = 2;
        } else if is_forced_blank {
            // Forced Blank asserted (or kept asserted): cancel any pending restart.
            self.forced_blank_restart_lines = 0;
        }
    }

    /// Read Green Swap register (0x0400_0002). Returns bit 0 only.
    pub fn read_green_swap(&self) -> u16 {
        self.green_swap as u16
    }

    /// Write Green Swap register (0x0400_0002). Only bit 0 is used;
    /// bits 1–15 are ignored per GBATek.
    pub fn write_green_swap(&mut self, value: u16) {
        self.green_swap = value & 1 != 0;
    }

    /// Enable or disable GBA LCD color correction.
    ///
    /// When enabled, BGR555 → RGB888 conversion uses a gamma ≈ 4 lookup
    /// table that simulates the GBA TFT LCD's physical non-linear response.
    /// When disabled (the default), the standard linear bit-replication
    /// formula is used.
    pub fn set_color_correction(&mut self, enabled: bool) {
        self.color_correction = enabled;
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
    ///
    /// Returns `true` when the FORCED_BLANK bit (DISPCNT bit 7) is set, or
    /// when the post-de-assert restart countdown is still active (during those
    /// ~2 scanlines the display still outputs white).
    pub fn forced_blank(&self) -> bool {
        self.dispcnt & dispcnt::FORCED_BLANK != 0 || self.forced_blank_restart_lines > 0
    }

    /// Returns `true` when VRAM and Palette RAM accesses require a 1-cycle wait
    /// due to the PPU actively reading those regions.
    ///
    /// Per GBATek "LCD VRAM Overview": the extra wait applies during active
    /// display (visible scanlines 0–159, not in H-Blank) while the PPU is
    /// actively rendering (`forced_blank()` is `false`).
    pub fn vram_pram_active_wait(&self) -> bool {
        if self.forced_blank() {
            return false;
        }
        if (self.vcount as u32) >= VISIBLE_SCANLINES {
            return false;
        }
        self.dispstat & dispstat::HBLANK_FLAG == 0
    }

    /// Returns `true` when OAM accesses require a 1-cycle wait.
    ///
    /// Per GBATek: OAM is restricted during active display and during H-Blank
    /// on visible scanlines unless DISPCNT bit 5 (H-Blank Interval Free) is
    /// set. Forced Blank and V-Blank allow full-speed access.
    pub fn oam_active_wait(&self) -> bool {
        if self.forced_blank() {
            return false;
        }
        if (self.vcount as u32) >= VISIBLE_SCANLINES {
            return false;
        }
        // During H-Blank with HBLANK_INTERVAL_FREE set: OAM is freely accessible.
        if self.dispstat & dispstat::HBLANK_FLAG != 0
            && self.dispcnt & dispcnt::HBLANK_INTERVAL_FREE != 0
        {
            return false;
        }
        true
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

    /// Write `BGnHOFS` for background layer `n` (0–3).
    ///
    /// BG scroll registers are write-only per GBATek. Only the low 9 bits
    /// are significant; the remaining bits are discarded.
    pub fn write_bg_hofs(&mut self, n: usize, value: u16) {
        self.bg_scroll[n].0 = value & 0x01FF;
    }

    /// Return the internal HOFS scroll value for layer `n`.
    ///
    /// **Not for CPU reads.** BG scroll registers are write-only on hardware;
    /// CPU reads must return open-bus (handled by the I/O layer returning
    /// `None` from `try_read16`). This accessor exists solely for byte-write
    /// merging in `IoRegisters::write8`.
    pub fn read_bg_hofs(&self, n: usize) -> u16 {
        self.bg_scroll[n].0
    }

    /// Write `BGnVOFS` for background layer `n` (0–3).
    ///
    /// BG scroll registers are write-only per GBATek. Only the low 9 bits
    /// are significant; the remaining bits are discarded.
    pub fn write_bg_vofs(&mut self, n: usize, value: u16) {
        self.bg_scroll[n].1 = value & 0x01FF;
    }

    /// Return the internal VOFS scroll value for layer `n`.
    ///
    /// **Not for CPU reads.** BG scroll registers are write-only on hardware;
    /// CPU reads must return open-bus (handled by the I/O layer returning
    /// `None` from `try_read16`). This accessor exists solely for byte-write
    /// merging in `IoRegisters::write8`.
    pub fn read_bg_vofs(&self, n: usize) -> u16 {
        self.bg_scroll[n].1
    }

    /// Write `WINnH` (n=0 or 1). Sets horizontal window boundaries.
    pub fn write_win_h(&mut self, n: usize, value: u16) {
        self.win_h[n] = value;
    }

    /// Read `WINnH` (n=0 or 1). Used for byte-write merging.
    pub fn read_win_h(&self, n: usize) -> u16 {
        self.win_h[n]
    }

    /// Write `WINnV` (n=0 or 1). Sets vertical window boundaries.
    pub fn write_win_v(&mut self, n: usize, value: u16) {
        self.win_v[n] = value;
    }

    /// Read `WINnV` (n=0 or 1). Used for byte-write merging.
    pub fn read_win_v(&self, n: usize) -> u16 {
        self.win_v[n]
    }

    /// Write `WININ` (0x0400_0048). Only bits 0-5 and 8-13 are valid.
    pub fn write_winin(&mut self, value: u16) {
        self.winin = value & 0x3F3F;
    }

    /// Read `WININ`.
    pub fn read_winin(&self) -> u16 {
        self.winin
    }

    /// Write `WINOUT` (0x0400_004A). Only bits 0-5 and 8-13 are valid.
    pub fn write_winout(&mut self, value: u16) {
        self.winout = value & 0x3F3F;
    }

    /// Read `WINOUT`.
    pub fn read_winout(&self) -> u16 {
        self.winout
    }

    /// Write `BLDCNT` (0x0400_0050). Bits 14-15 are unused and discarded.
    pub fn write_bldcnt(&mut self, value: u16) {
        self.bldcnt = value & 0x3FFF;
    }

    /// Read `BLDCNT`.
    pub fn read_bldcnt(&self) -> u16 {
        self.bldcnt
    }

    /// Write `BLDALPHA` (0x0400_0052). Only bits 0-4 (EVA) and 8-12 (EVB)
    /// are valid; other bits are discarded.
    pub fn write_bldalpha(&mut self, value: u16) {
        self.bldalpha = value & 0x1F1F;
    }

    /// Read `BLDALPHA`.
    pub fn read_bldalpha(&self) -> u16 {
        self.bldalpha
    }

    /// Write `BLDY` (0x0400_0054, write-only). Only bits 0-4 (EVY) are
    /// significant; values 17-31 are treated as 16 at effect time.
    pub fn write_bldy(&mut self, value: u16) {
        self.bldy = (value & 0x1F) as u8;
    }

    /// Read the raw BLDY backing value as a halfword.
    ///
    /// **Not for CPU reads.** BLDY is write-only on hardware; CPU reads return
    /// open-bus (handled by the I/O layer returning `None` from `try_read16`).
    /// This accessor exists solely for byte-write merging in
    /// `IoRegisters::write8`.
    pub fn read_bldy(&self) -> u16 {
        self.bldy as u16
    }

    /// Write `MOSAIC` (0x0400_004C). All 16 bits are significant.
    pub fn write_mosaic(&mut self, value: u16) {
        self.mosaic = value;
    }

    /// Read the raw MOSAIC backing value as a halfword.
    ///
    /// **Not for CPU reads.** MOSAIC is write-only on hardware; CPU reads return
    /// open-bus (handled by the I/O layer returning `None` from `try_read16`).
    /// This accessor exists for byte-write merging and rendering.
    pub fn read_mosaic(&self) -> u16 {
        self.mosaic
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
                }
                // Per GBATek, H-Blank DMA fires on ALL 228 scanlines
                // (visible and V-Blank alike). Only the H-Blank IRQ is
                // suppressed during V-Blank (see comment below).
                events.hblank_starts = events.hblank_starts.saturating_add(1);
                // Per GBATek: "no H-Blank interrupts are generated within
                // V-Blank period." Only raise on visible scanlines (0-159).
                if self.dispstat & dispstat::HBLANK_IRQ_ENABLE != 0
                    && (self.vcount as u32) < VISIBLE_SCANLINES
                {
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

        // Handle the forced-blank mid-frame restart countdown.
        // When Forced Blank (DISPCNT bit 7) is de-asserted mid-frame, the PPU
        // keeps outputting white for ~2 more scanlines, then restarts from
        // line 0.  The countdown is decremented here at every scanline
        // advance; when it reaches 0, VCOUNT is reset to 0 and normal
        // operation resumes.
        if self.forced_blank_restart_lines > 0 {
            self.forced_blank_restart_lines -= 1;
            if self.forced_blank_restart_lines == 0 {
                // Display restarts from line 0: reset VCOUNT, clear any
                // V-Blank flag that may have been set during the countdown.
                self.vcount = 0;
                self.dispstat &= !dispstat::VBLANK_FLAG;
                // Latch affine reference points as if starting a new frame.
                for aff in &mut self.bg_affine {
                    aff.latch_reference_points();
                }
                self.update_vcount_match_flag(Some(ic));
                return;
            }
        }

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
            5 => self.render_mode5_scanline(y, vram, pram, oam),
            // Modes 6-7 are "prohibited" per GBATek. No BG layers are rendered,
            // but OBJ sprites and the backdrop behave normally.
            6 | 7 => self.render_prohibited_mode_scanline(y, vram, pram, oam),
            _ => self.render_backdrop_scanline(y, pram),
        }
        // Green Swap (0x0400_0002): exchange the green channel between
        // each pair of adjacent pixels in the composited scanline output.
        if self.green_swap {
            let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
            for x in (0..SCREEN_WIDTH as usize).step_by(2) {
                let i0 = row_start + x * BYTES_PER_PIXEL + 1;
                let i1 = row_start + (x + 1) * BYTES_PER_PIXEL + 1;
                self.framebuffer.swap(i0, i1);
            }
        }
    }

    /// Mode 0: render enabled text-mode BG layers (BG0–BG3) with priority
    /// compositing and window support.
    fn render_mode0_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        let bg_enables = [
            self.dispcnt & dispcnt::BG0_ENABLE != 0,
            self.dispcnt & dispcnt::BG1_ENABLE != 0,
            self.dispcnt & dispcnt::BG2_ENABLE != 0,
            self.dispcnt & dispcnt::BG3_ENABLE != 0,
        ];

        if !bg_enables.iter().any(|&e| e) && self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            self.render_no_layers_scanline(y, vram, pram, oam);
            return;
        }

        // Collect enabled BG layers sorted front-to-back (lowest priority first,
        // lowest BG number breaks ties).
        let mut layers: Vec<(usize, u8, [u16; SCREEN_WIDTH as usize])> = Vec::new();
        for (i, &enabled) in bg_enables.iter().enumerate() {
            if enabled {
                let mut buf = [TRANSPARENT; SCREEN_WIDTH as usize];
                self.render_text_bg_layer(i, y, vram, pram, &mut buf);
                let prio = (self.bg_cnt[i] & 3) as u8;
                layers.push((i, prio, buf));
            }
        }

        self.composite_scanline(y, pram, vram, oam, &layers);
    }

    /// Render a single text-mode BG layer into a color buffer.
    /// Transparent pixels are marked with [`TRANSPARENT`]. The buffer
    /// is filled for the full 240 pixels of the scanline.
    #[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
    fn render_text_bg_layer(
        &self,
        bg_idx: usize,
        y: u32,
        vram: &[u8],
        pram: &[u8],
        buf: &mut [u16; SCREEN_WIDTH as usize],
    ) {
        let bgcnt = self.bg_cnt[bg_idx];
        let is_8bpp = bgcnt & (1 << 7) != 0;
        let mosaic_enabled = bgcnt & (1 << 6) != 0;
        let (mosaic_h, mosaic_v) = self.bg_mosaic_size();

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
        let sample_y = if mosaic_enabled {
            mosaic_anchor(y, mosaic_v as u32) as usize
        } else {
            y as usize
        };
        let screen_y = (sample_y + vofs as usize) & height_mask;

        for x in 0..(SCREEN_WIDTH as usize) {
            let sample_x = if mosaic_enabled {
                mosaic_anchor(x as u32, mosaic_h as u32) as usize
            } else {
                x
            };
            let screen_x = (sample_x + hofs as usize) & width_mask;
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
            let palette_index = if is_8bpp {
                let tile_addr = charblock_base + tile_id * 64 + pixel_y * 8 + pixel_x;
                vram.get(tile_addr).copied().unwrap_or(0) as usize
            } else {
                let tile_addr = charblock_base + tile_id * 32 + pixel_y * 4 + (pixel_x >> 1);
                vram.get(tile_addr)
                    .map(|byte| {
                        if pixel_x & 1 == 0 {
                            byte & 0x0F
                        } else {
                            byte >> 4
                        }
                    })
                    .unwrap_or(0) as usize
            };

            if palette_index == 0 {
                buf[x] = TRANSPARENT;
                continue;
            }

            let pram_index = if is_8bpp {
                palette_index * 2
            } else {
                let palette_bank = ((entry >> 12) & 0x000F) as usize;
                (palette_bank * 16 + palette_index) * 2
            };
            buf[x] = if pram_index + 1 < pram.len() {
                u16::from_le_bytes([pram[pram_index], pram[pram_index + 1]]) & 0x7FFF
            } else {
                TRANSPARENT
            };
        }
    }

    /// Render a single 8bpp affine tile background layer into a color buffer.
    /// Transparent pixels are marked with [`TRANSPARENT`].
    ///
    /// `affine_idx` is 0 for BG2, 1 for BG3.
    #[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
    fn render_affine_bg_layer(
        &self,
        bg_idx: usize,
        affine_idx: usize,
        vram: &[u8],
        pram: &[u8],
        buf: &mut [u16; SCREEN_WIDTH as usize],
    ) {
        let bgcnt = self.bg_cnt[bg_idx];
        let aff = self.bg_affine[affine_idx];
        let mosaic_enabled = bgcnt & (1 << 6) != 0;
        let (mosaic_h, mosaic_v) = self.bg_mosaic_size();

        // Affine map sizes: 16x16, 32x32, 64x64, 128x128 tiles.
        let size_shift = ((bgcnt >> 14) & 3) as u32;
        let tiles_wide = 16u32 << size_shift;
        let map_pixels = tiles_wide * 8;

        let wrapping = (bgcnt & (1 << 13)) != 0;
        let screenblock_base = (((bgcnt >> 8) & 0x001F) as usize) * 0x800;
        let charblock_base = (((bgcnt >> 2) & 0x0003) as usize) * 16 * 1024;

        let pa = aff.pa as i32;
        let pb = aff.pb as i32;
        let pc = aff.pc as i32;
        let pd = aff.pd as i32;

        let mosaic_y_offset = if mosaic_enabled {
            let y = self.vcount as u32;
            (y - mosaic_anchor(y, mosaic_v as u32)) as usize
        } else {
            0
        };
        // `internal_x/y` point at the current scanline. For vertical mosaic,
        // sample from the block's top scanline instead, so rewind by PB/PD
        // (source x/y deltas per screen scanline) multiplied by the offset
        // within the current mosaic block.
        let line_anchor_x = aff
            .internal_x
            .wrapping_sub(pb.wrapping_mul(mosaic_y_offset as i32));
        let line_anchor_y = aff
            .internal_y
            .wrapping_sub(pd.wrapping_mul(mosaic_y_offset as i32));

        for x in 0..(SCREEN_WIDTH as usize) {
            let sample_x = if mosaic_enabled {
                mosaic_anchor(x as u32, mosaic_h as u32) as usize
            } else {
                x
            };
            let px = line_anchor_x.wrapping_add(pa.wrapping_mul(sample_x as i32)) >> 8;
            let py = line_anchor_y.wrapping_add(pc.wrapping_mul(sample_x as i32)) >> 8;

            let (fx, fy) = if wrapping {
                (
                    (px as u32) & (map_pixels - 1),
                    (py as u32) & (map_pixels - 1),
                )
            } else {
                if px < 0 || py < 0 || px >= map_pixels as i32 || py >= map_pixels as i32 {
                    buf[x] = TRANSPARENT;
                    continue;
                }
                (px as u32, py as u32)
            };

            let tile_x = (fx >> 3) as usize;
            let tile_y = (fy >> 3) as usize;
            let pixel_x = (fx & 7) as usize;
            let pixel_y = (fy & 7) as usize;

            let map_off = screenblock_base + tile_y * (tiles_wide as usize) + tile_x;
            let tile_id = *vram.get(map_off).unwrap_or(&0) as usize;

            let tile_addr = charblock_base + tile_id * 64 + pixel_y * 8 + pixel_x;
            let palette_index = *vram.get(tile_addr).unwrap_or(&0) as usize;

            if palette_index == 0 {
                buf[x] = TRANSPARENT;
                continue;
            }

            let pram_index = palette_index * 2;
            buf[x] = if pram_index + 1 < pram.len() {
                u16::from_le_bytes([pram[pram_index], pram[pram_index + 1]]) & 0x7FFF
            } else {
                TRANSPARENT
            };
        }
    }

    /// Mode 2: affine tile backgrounds (BG2 and BG3 only).
    fn render_mode2_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        let bg_enables = [
            self.dispcnt & dispcnt::BG2_ENABLE != 0,
            self.dispcnt & dispcnt::BG3_ENABLE != 0,
        ];

        if !bg_enables.iter().any(|&e| e) && self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            self.render_no_layers_scanline(y, vram, pram, oam);
            return;
        }

        let mut layers: Vec<(usize, u8, [u16; SCREEN_WIDTH as usize])> = Vec::new();
        let bg_indices = [2usize, 3usize];
        let affine_indices = [0usize, 1usize];
        for (i, &bg_idx) in bg_indices.iter().enumerate() {
            if bg_enables[i] {
                let mut buf = [TRANSPARENT; SCREEN_WIDTH as usize];
                self.render_affine_bg_layer(bg_idx, affine_indices[i], vram, pram, &mut buf);
                let prio = (self.bg_cnt[bg_idx] & 3) as u8;
                layers.push((bg_idx, prio, buf));
            }
        }

        self.composite_scanline(y, pram, vram, oam, &layers);
    }

    /// Mode 1: BG0/BG1 regular text + BG2 affine.
    fn render_mode1_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        let bg_enables = [
            self.dispcnt & dispcnt::BG0_ENABLE != 0,
            self.dispcnt & dispcnt::BG1_ENABLE != 0,
            self.dispcnt & dispcnt::BG2_ENABLE != 0,
        ];

        if !bg_enables.iter().any(|&e| e) && self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            self.render_no_layers_scanline(y, vram, pram, oam);
            return;
        }

        let mut layers: Vec<(usize, u8, [u16; SCREEN_WIDTH as usize])> = Vec::new();
        for (i, &bg_idx) in [0usize, 1, 2].iter().enumerate() {
            if bg_enables[i] {
                let mut buf = [TRANSPARENT; SCREEN_WIDTH as usize];
                if bg_idx == 2 {
                    self.render_affine_bg_layer(bg_idx, 0, vram, pram, &mut buf);
                } else {
                    self.render_text_bg_layer(bg_idx, y, vram, pram, &mut buf);
                }
                let prio = (self.bg_cnt[bg_idx] & 3) as u8;
                layers.push((bg_idx, prio, buf));
            }
        }

        self.composite_scanline(y, pram, vram, oam, &layers);
    }

    /// Render one BG2 affine bitmap layer scanline for bitmap modes.
    ///
    /// Applies BG2 affine transform and BG mosaic rules to map each visible
    /// screen pixel on scanline `y` into a source `(x, y)` within a bitmap of
    /// size `width`×`height`, starting at VRAM `frame_base`. Source samples
    /// outside bitmap bounds are left transparent.
    ///
    /// Per GBATek, the BG2CNT area overflow/wrap bit (bit 13) is intentionally
    /// ignored for bitmap modes (3, 4, 5). Out-of-bounds pixels are always
    /// transparent in bitmap modes regardless of that bit's value. Wrapping
    /// only applies to affine tile modes (1 & 2).
    fn render_affine_bitmap_layer(
        &self,
        y: u32,
        vram: &[u8],
        width: usize,
        height: usize,
        frame_base: usize,
    ) -> [u16; SCREEN_WIDTH as usize] {
        let mut buf = [TRANSPARENT; SCREEN_WIDTH as usize];
        let aff = self.bg_affine[0];
        let pa = aff.pa as i32;
        let pb = aff.pb as i32;
        let pc = aff.pc as i32;
        let pd = aff.pd as i32;
        let mosaic_enabled = self.bg_cnt[2] & (1 << 6) != 0;
        let (mosaic_h, mosaic_v) = self.bg_mosaic_size();
        let mosaic_y_offset = if mosaic_enabled {
            (y - mosaic_anchor(y, mosaic_v as u32)) as usize
        } else {
            0
        };
        // `internal_x/y` point at the current scanline. For vertical mosaic,
        // sample from the block's top scanline instead, so rewind by PB/PD
        // (source x/y deltas per screen scanline) multiplied by the offset
        // within the current mosaic block.
        let line_anchor_x = aff
            .internal_x
            .wrapping_sub(pb.wrapping_mul(mosaic_y_offset as i32));
        let line_anchor_y = aff
            .internal_y
            .wrapping_sub(pd.wrapping_mul(mosaic_y_offset as i32));

        for (x, out_pixel) in buf.iter_mut().enumerate() {
            let sample_screen_x = if mosaic_enabled {
                mosaic_anchor(x as u32, mosaic_h as u32) as usize
            } else {
                x
            };
            let sample_x = line_anchor_x.wrapping_add(pa.wrapping_mul(sample_screen_x as i32));
            let sample_y = line_anchor_y.wrapping_add(pc.wrapping_mul(sample_screen_x as i32));

            if sample_x < 0 || sample_y < 0 {
                continue;
            }

            let px = (sample_x >> 8) as usize;
            let py = (sample_y >> 8) as usize;
            if px >= width || py >= height {
                continue;
            }

            let src = frame_base + (py * width + px) * 2;
            // Keep bit 15 clear so valid BGR555 pixels never collide with
            // TRANSPARENT (0x8000) sentinel values in layer buffers.
            *out_pixel = u16::from_le_bytes([vram[src], vram[src + 1]]) & 0x7FFF;
        }

        buf
    }

    /// Render one BG2 affine 8-bit paletted bitmap layer scanline for Mode 4.
    ///
    /// Applies BG2 affine transform and BG mosaic rules to map each visible
    /// screen pixel on scanline `y` into a source `(x, y)` within an 8-bit
    /// paletted bitmap of size `width`×`height`, starting at VRAM `frame_base`.
    /// Palette index 0 is transparent. Source samples outside bitmap bounds
    /// are left transparent.
    ///
    /// Per GBATek, the BG2CNT area overflow/wrap bit (bit 13) is intentionally
    /// ignored for bitmap modes (3, 4, 5). Out-of-bounds pixels are always
    /// transparent in bitmap modes regardless of that bit's value. Wrapping
    /// only applies to affine tile modes (1 & 2).
    fn render_affine_paletted_bitmap_layer(
        &self,
        y: u32,
        vram: &[u8],
        pram: &[u8],
        width: usize,
        height: usize,
        frame_base: usize,
    ) -> [u16; SCREEN_WIDTH as usize] {
        let mut buf = [TRANSPARENT; SCREEN_WIDTH as usize];
        let aff = self.bg_affine[0];
        let pa = aff.pa as i32;
        let pb = aff.pb as i32;
        let pc = aff.pc as i32;
        let pd = aff.pd as i32;
        let mosaic_enabled = self.bg_cnt[2] & (1 << 6) != 0;
        let (mosaic_h, mosaic_v) = self.bg_mosaic_size();
        let mosaic_y_offset = if mosaic_enabled {
            (y - mosaic_anchor(y, mosaic_v as u32)) as usize
        } else {
            0
        };
        // `internal_x/y` point at the current scanline. For vertical mosaic,
        // sample from the block's top scanline instead, so rewind by PB/PD
        // (source x/y deltas per screen scanline) multiplied by the offset
        // within the current mosaic block.
        let line_anchor_x = aff
            .internal_x
            .wrapping_sub(pb.wrapping_mul(mosaic_y_offset as i32));
        let line_anchor_y = aff
            .internal_y
            .wrapping_sub(pd.wrapping_mul(mosaic_y_offset as i32));

        for (x, out_pixel) in buf.iter_mut().enumerate() {
            let sample_screen_x = if mosaic_enabled {
                mosaic_anchor(x as u32, mosaic_h as u32) as usize
            } else {
                x
            };
            let sample_x = line_anchor_x.wrapping_add(pa.wrapping_mul(sample_screen_x as i32));
            let sample_y = line_anchor_y.wrapping_add(pc.wrapping_mul(sample_screen_x as i32));

            if sample_x < 0 || sample_y < 0 {
                continue;
            }

            let px = (sample_x >> 8) as usize;
            let py = (sample_y >> 8) as usize;
            if px >= width || py >= height {
                continue;
            }

            let src = frame_base + py * width + px;
            if src >= vram.len() {
                continue;
            }

            let pal_index = vram[src];
            *out_pixel = if pal_index == 0 {
                TRANSPARENT
            } else {
                let pal_offset = (pal_index as usize) * 2;
                if pal_offset + 1 < pram.len() {
                    u16::from_le_bytes([pram[pal_offset], pram[pal_offset + 1]]) & 0x7FFF
                } else {
                    TRANSPARENT
                }
            };
        }

        buf
    }

    /// Mode 3: 240×160 direct 15-bit bitmap. Each pixel is a 16-bit BGR555 value.
    ///
    /// BG2 affine parameters select source coordinates. Pixels outside the
    /// 240×160 source bitmap remain transparent so the backdrop or lower
    /// priority layers show through.
    fn render_mode3_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        if !self.bg2_enabled() && self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            self.render_no_layers_scanline(y, vram, pram, oam);
            return;
        }

        let mut layers: Vec<(usize, u8, [u16; SCREEN_WIDTH as usize])> = Vec::new();

        if self.bg2_enabled() {
            let buf = self.render_affine_bitmap_layer(
                y,
                vram,
                SCREEN_WIDTH as usize,
                SCREEN_HEIGHT as usize,
                MODE3_FRAME_BASE,
            );
            let prio = (self.bg_cnt[2] & 3) as u8;
            layers.push((2, prio, buf));
        }

        self.composite_scanline(y, pram, vram, oam, &layers);
    }

    /// Mode 4: 240×160 8-bit paletted bitmap. Two frames available.
    ///
    /// BG2 affine parameters select source coordinates. Pixels outside the
    /// 240×160 source bitmap remain transparent so the backdrop or lower
    /// priority layers show through.
    fn render_mode4_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        if !self.bg2_enabled() && self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            self.render_no_layers_scanline(y, vram, pram, oam);
            return;
        }

        let mut layers: Vec<(usize, u8, [u16; SCREEN_WIDTH as usize])> = Vec::new();

        if self.bg2_enabled() {
            let frame_base = if self.frame_select() {
                0xA000usize
            } else {
                0x0000usize
            };
            let buf = self.render_affine_paletted_bitmap_layer(
                y,
                vram,
                pram,
                SCREEN_WIDTH as usize,
                SCREEN_HEIGHT as usize,
                frame_base,
            );
            let prio = (self.bg_cnt[2] & 3) as u8;
            layers.push((2, prio, buf));
        }

        self.composite_scanline(y, pram, vram, oam, &layers);
    }

    /// Mode 5: 160×128 direct 15-bit bitmap. Two frames available.
    ///
    /// BG2 affine parameters select source coordinates. Pixels outside the
    /// 160×128 source bitmap remain transparent so the backdrop or lower
    /// priority layers show through.
    fn render_mode5_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        if !self.bg2_enabled() && self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            self.render_no_layers_scanline(y, vram, pram, oam);
            return;
        }

        let mut layers: Vec<(usize, u8, [u16; SCREEN_WIDTH as usize])> = Vec::new();

        if self.bg2_enabled() {
            let frame_base = if self.frame_select() {
                0xA000usize
            } else {
                0x0000usize
            };
            let buf =
                self.render_affine_bitmap_layer(y, vram, MODE5_WIDTH, MODE5_HEIGHT, frame_base);
            let prio = (self.bg_cnt[2] & 3) as u8;
            layers.push((2, prio, buf));
        }

        self.composite_scanline(y, pram, vram, oam, &layers);
    }

    /// Composite BG layers and OBJ into the framebuffer for one scanline.
    ///
    /// `bg_layers` contains `(bg_index, priority, color_buffer)` for each
    /// enabled BG (order does not matter — the function finds the highest-priority
    /// opaque layer per pixel by scanning all entries). The function evaluates
    /// window regions per-pixel and picks the highest-priority enabled+opaque layer.
    ///
    /// Color special effects (BLDCNT/BLDALPHA/BLDY) are applied after
    /// compositing when the 1st/2nd target conditions are met.
    #[allow(clippy::too_many_arguments, clippy::needless_range_loop)]
    fn composite_scanline(
        &mut self,
        y: u32,
        pram: &[u8],
        vram: &[u8],
        oam: &[u8],
        bg_layers: &[(usize, u8, [u16; SCREEN_WIDTH as usize])],
    ) {
        let row_start = (y as usize) * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        let backdrop = self.backdrop_bgr555(pram);

        // Render OBJ scanline (if enabled).
        let obj_enabled = self.dispcnt & dispcnt::OBJ_ENABLE != 0;
        let obj_scanline = if obj_enabled {
            let mapping_1d = self.dispcnt & dispcnt::OBJ_MAPPING_1D != 0;
            let bitmap_mode = self.mode() >= 3;
            let cycle_budget = if self.dispcnt & dispcnt::HBLANK_INTERVAL_FREE != 0 {
                obj::OBJ_CYCLE_BUDGET_HBLANK_FREE
            } else {
                obj::OBJ_CYCLE_BUDGET_NORMAL
            };
            Some(obj::render_obj_scanline(
                y,
                oam,
                vram,
                pram,
                mapping_1d,
                bitmap_mode,
                self.mosaic,
                cycle_budget,
            ))
        } else {
            None
        };

        // Determine if any window is active.
        let any_window_active = self.dispcnt
            & (dispcnt::WIN0_ENABLE | dispcnt::WIN1_ENABLE | dispcnt::OBJ_WIN_ENABLE)
            != 0;

        // Pre-decode color effect parameters.
        let bld_mode = (self.bldcnt >> 6) & 3;
        let first_target_mask = (self.bldcnt & 0x3F) as u8;
        let second_target_mask = ((self.bldcnt >> 8) & 0x3F) as u8;
        let eva = ((self.bldalpha & 0x1F) as u8).min(16);
        let evb = (((self.bldalpha >> 8) & 0x1F) as u8).min(16);
        let evy = self.bldy.min(16);

        // Select the pixel-write function once per scanline to avoid a
        // per-pixel branch inside the hot 240-pixel loop.
        let write_pixel: fn(&mut [u8], usize, u16) = if self.color_correction {
            color::write_pixel_corrected
        } else {
            color::write_pixel
        };

        for x in 0..(SCREEN_WIDTH as usize) {
            // Determine layer enable mask for this pixel.
            let layer_mask = if any_window_active {
                self.window_layer_mask(x as u32, y, obj_scanline.as_ref())
            } else {
                // No windows active → all layers visible.
                0x3F
            };

            // Bit 5 of layer_mask controls whether SFX apply to this pixel.
            let sfx_enabled = (layer_mask >> 5) & 1 != 0;

            let obj_px = obj_scanline.as_ref().map(|s| &s.pixels[x]);
            let obj_opaque = obj_px.is_some_and(|px| px.opaque);
            let obj_visible = obj_opaque && ((layer_mask & (1 << 4)) != 0);
            // Per GBATek "Semi-Transparent OBJ" section: "If a semi-transparent
            // OBJ pixel overlaps a 2nd Target pixel, semi-transparency wins;
            // brightness effect is NOT applied (neither to 1st nor 2nd target)."
            // This suppression applies regardless of the OBJ's priority relative
            // to other layers, matching mGBA's FLAG_REBLEND propagation behaviour.
            // Suppression requires a 2nd target to be configured (same condition
            // that sets FLAG_REBLEND in mGBA — when no 2nd target exists, the
            // OBJ's TARGET_1 flag is cleared instead, and brightness on layers
            // above the OBJ must still apply).
            let obj_is_semi_transparent =
                obj_visible && matches!(obj_px, Some(px) if px.semi_transparent);
            let suppress_brightness = obj_is_semi_transparent && second_target_mask != 0;

            // Find the top-2 visible pixels using sort_key (lower = higher
            // priority). Encoding: OBJ at prio p → key = p*10;
            // BG bg_idx at prio p → key = p*10 + 5 + bg_idx (so OBJ beats
            // BG at same priority, and lower BG index beats higher at same
            // priority). Backdrop always sits at sort_key = 0xFFFF.
            //
            // top: highest-priority visible pixel.
            // sec: second-highest-priority visible pixel (fallback = backdrop).
            let mut top_sort: u16 = 0xFFFF; // start at backdrop priority
            let mut top_layer: u8 = 5; // backdrop layer index
            let mut top_color: u16 = backdrop;
            let mut top_semi_transparent = false;

            let mut sec_sort: u16 = 0xFFFF; // backdrop as default second
            let mut sec_layer: u8 = 5;
            let mut sec_color: u16 = backdrop;

            // Insert OBJ candidate.
            if obj_visible {
                let px = obj_px.unwrap();
                let sort_key = (px.priority as u16) * SORT_KEY_PRIORITY_SPACING;
                // OBJ sort_key is always < 0xFFFF, so it always beats backdrop.
                sec_sort = top_sort;
                sec_layer = top_layer;
                sec_color = top_color;
                top_sort = sort_key;
                top_layer = 4;
                top_color = px.color;
                top_semi_transparent = px.semi_transparent;
            }

            // Insert BG candidates.
            for &(bg_idx, bg_prio, ref buf) in bg_layers {
                if layer_mask & (1 << bg_idx) == 0 {
                    continue; // disabled by window
                }
                if buf[x] == TRANSPARENT {
                    continue;
                }
                let sort_key = (bg_prio as u16) * SORT_KEY_PRIORITY_SPACING
                    + SORT_KEY_BG_OFFSET
                    + (bg_idx as u16);
                if sort_key < top_sort {
                    sec_sort = top_sort;
                    sec_layer = top_layer;
                    sec_color = top_color;
                    top_sort = sort_key;
                    top_layer = bg_idx as u8;
                    top_color = buf[x];
                    top_semi_transparent = false;
                } else if sort_key < sec_sort {
                    sec_sort = sort_key;
                    sec_layer = bg_idx as u8;
                    sec_color = buf[x];
                }
            }

            // Apply color special effects.
            //
            // Layer indices: 0-3 = BG0-BG3, 4 = OBJ, 5 = Backdrop.
            // `first_target_mask` bit k = layer k is a 1st target.
            // `second_target_mask` bit k = layer k is a 2nd target.
            let final_color = if top_semi_transparent && sfx_enabled {
                // Semi-transparent OBJ always alpha-blends regardless of
                // BLDCNT mode, treated as 1st target.  2nd target selection
                // still comes from BLDCNT bits 8-13.
                if (second_target_mask >> sec_layer) & 1 != 0 {
                    alpha_blend_bgr555(top_color, sec_color, eva, evb)
                } else {
                    // No valid 2nd target below — display at normal intensity.
                    top_color
                }
            } else if sfx_enabled {
                match bld_mode {
                    1 => {
                        // Alpha blend: both 1st and 2nd targets must be selected.
                        if (first_target_mask >> top_layer) & 1 != 0
                            && (second_target_mask >> sec_layer) & 1 != 0
                        {
                            alpha_blend_bgr555(top_color, sec_color, eva, evb)
                        } else {
                            top_color
                        }
                    }
                    2 => {
                        // Brightness increase — suppressed when a semi-transparent
                        // OBJ is present at this pixel and any 2nd target is set.
                        if !suppress_brightness && (first_target_mask >> top_layer) & 1 != 0 {
                            brighten_bgr555(top_color, evy)
                        } else {
                            top_color
                        }
                    }
                    3 => {
                        // Brightness decrease — suppressed when a semi-transparent
                        // OBJ is present at this pixel and any 2nd target is set.
                        if !suppress_brightness && (first_target_mask >> top_layer) & 1 != 0 {
                            darken_bgr555(top_color, evy)
                        } else {
                            top_color
                        }
                    }
                    _ => top_color, // mode 0 = no effect
                }
            } else {
                top_color
            };

            let dst = row_start + x * BYTES_PER_PIXEL;
            write_pixel(&mut self.framebuffer, dst, final_color);
        }
    }

    /// Determine the window layer-enable mask for pixel (x, y).
    /// Returns a 6-bit mask: bits 0-3 = BG0-BG3, bit 4 = OBJ, bit 5 = SFX.
    fn window_layer_mask(&self, x: u32, y: u32, obj_scanline: Option<&obj::ObjScanline>) -> u8 {
        // Window 0 (highest priority).
        if self.dispcnt & dispcnt::WIN0_ENABLE != 0 && self.pixel_in_window(0, x, y) {
            return (self.winin & 0x3F) as u8;
        }
        // Window 1.
        if self.dispcnt & dispcnt::WIN1_ENABLE != 0 && self.pixel_in_window(1, x, y) {
            return ((self.winin >> 8) & 0x3F) as u8;
        }
        // OBJ Window.
        if self.dispcnt & dispcnt::OBJ_WIN_ENABLE != 0
            && obj_scanline.is_some_and(|s| s.obj_window[x as usize])
        {
            return ((self.winout >> 8) & 0x3F) as u8;
        }
        // Outside all windows.
        (self.winout & 0x3F) as u8
    }

    /// Check if pixel (x, y) is inside window `n` (0 or 1).
    ///
    /// Per GBATek 'LCD I/O Window Feature' and hardware tests:
    /// - If X2 > 240: clamp X2 to 240 (window is X1..240).
    /// - If X1 > X2: window wraps around, covering 0..X2 AND X1..240.
    /// - If Y2 > 160: clamp Y2 to 160 (window is Y1..160).
    /// - If Y1 > Y2: window wraps around, covering 0..Y2 AND Y1..160.
    fn pixel_in_window(&self, n: usize, x: u32, y: u32) -> bool {
        let h = self.win_h[n];
        let v = self.win_v[n];
        let x1 = (h >> 8) as u32;
        let x2 = (h & 0xFF) as u32;
        let y1 = (v >> 8) as u32;
        let y2 = (v & 0xFF) as u32;

        let in_x = if x2 > SCREEN_WIDTH {
            // X2 out of range: clamp to SCREEN_WIDTH.
            x >= x1 && x < SCREEN_WIDTH
        } else if x1 > x2 {
            // Wraparound: covers 0..X2 AND X1..SCREEN_WIDTH.
            x >= x1 || x < x2
        } else {
            x >= x1 && x < x2
        };

        let in_y = if y2 > SCREEN_HEIGHT {
            // Y2 out of range: clamp to SCREEN_HEIGHT.
            y >= y1 && y < SCREEN_HEIGHT
        } else if y1 > y2 {
            // Wraparound: covers 0..Y2 AND Y1..SCREEN_HEIGHT.
            y >= y1 || y < y2
        } else {
            y >= y1 && y < y2
        };

        in_x && in_y
    }

    /// Prohibited modes 6 and 7: per GBATek these are "prohibited" but hardware
    /// still composites OBJ sprites over the backdrop — no BG layers are rendered.
    fn render_prohibited_mode_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        if self.dispcnt & dispcnt::OBJ_ENABLE == 0 {
            self.render_no_layers_scanline(y, vram, pram, oam);
            return;
        }
        self.composite_scanline(y, pram, vram, oam, &[]);
    }

    /// Render a scanline where no BG layers or OBJ are active (backdrop-only).
    ///
    /// When any window is active, delegates to `composite_scanline` so that
    /// per-pixel window-gated SFX are correctly applied to the backdrop (GBATek:
    /// WININ/WINOUT bit 5 controls 'Color Special Effect' per region).  When no
    /// window is active, uses the fast `render_backdrop_scanline` path.
    fn render_no_layers_scanline(&mut self, y: u32, vram: &[u8], pram: &[u8], oam: &[u8]) {
        let any_window_active = self.dispcnt
            & (dispcnt::WIN0_ENABLE | dispcnt::WIN1_ENABLE | dispcnt::OBJ_WIN_ENABLE)
            != 0;
        if any_window_active {
            self.composite_scanline(y, pram, vram, oam, &[]);
        } else {
            self.render_backdrop_scanline(y, pram);
        }
    }

    /// Backdrop fill — uses palette entry 0 from PRAM.
    ///
    /// Applies brightness effects uniformly when no window is active, backdrop
    /// is selected as a 1st target, and the SFX mode is brightness
    /// increase/decrease.  This function is only called when windows are
    /// inactive; when windows are active the caller routes to `composite_scanline`
    /// via `render_no_layers_scanline` so that per-pixel window-gated SFX apply.
    fn render_backdrop_scanline(&mut self, y: u32, pram: &[u8]) {
        let backdrop = self.backdrop_bgr555(pram);

        // Apply brightness effects when backdrop (layer 5) is selected as a
        // 1st target and SFX are not suppressed by a window.  When any window
        // is active the SFX gate varies per-pixel, so we cannot apply a
        // uniform effect; in that case fall back to the raw color.  (Window-
        // gated backdrop effects are already handled in composite_scanline.)
        let any_window_active = self.dispcnt
            & (dispcnt::WIN0_ENABLE | dispcnt::WIN1_ENABLE | dispcnt::OBJ_WIN_ENABLE)
            != 0;
        let bld_mode = (self.bldcnt >> 6) & 3;
        // Backdrop is layer index 5, corresponding to bit 5 in 1st-target mask.
        let backdrop_is_first_target = (self.bldcnt & (1 << 5)) != 0;
        let evy = self.bldy.min(16);

        let final_backdrop = if !any_window_active && backdrop_is_first_target {
            match bld_mode {
                2 => brighten_bgr555(backdrop, evy),
                3 => darken_bgr555(backdrop, evy),
                _ => backdrop,
            }
        } else {
            backdrop
        };

        let (r, g, b) = if self.color_correction {
            color::bgr555_to_rgb888_corrected(final_backdrop)
        } else {
            color::bgr555_to_rgb888(final_backdrop)
        };
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

    /// Return the BG mosaic `(horizontal_size, vertical_size)` from MOSAIC.
    ///
    /// BG horizontal size comes from bits 0-3 and BG vertical size comes from
    /// bits 4-7; both fields store `effective_size - 1`.
    fn bg_mosaic_size(&self) -> (usize, usize) {
        (
            mosaic_size(self.mosaic, 0) as usize,
            mosaic_size(self.mosaic, 4) as usize,
        )
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

    /// Step through one full frame (to latch state) then render scanline 0.
    /// Mutates `ppu` in place so scanline 0 is freshly rendered in the framebuffer.
    fn step_and_render_scanline0(
        ppu: &mut Ppu,
        ic: &mut InterruptController,
        vram: &[u8],
        pram: &[u8],
        oam: &[u8],
    ) {
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            ic,
            vram,
            pram,
            oam,
        );
        ppu.step(CYCLES_PER_SCANLINE, ic, vram, pram, oam);
    }

    #[test]
    fn save_state_restores_registers_and_affine_state() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        ppu.write_green_swap(1);
        ppu.write_dispstat(dispstat::VBLANK_IRQ_ENABLE | (42 << 8), &mut ic);
        ppu.write_bg_cnt(0, 0xDFFF);
        ppu.write_bg_cnt(2, 0xE0C3);
        ppu.write_bg_hofs(0, 0x0123);
        ppu.write_bg_vofs(0, 0x01F0);
        ppu.write_win_h(0, 0x1234);
        ppu.write_win_v(0, 0x5678);
        ppu.write_winin(0x3F3F);
        ppu.write_winout(0x2A2A);
        ppu.write_mosaic(0x4321);
        ppu.write_bldcnt(0x3F1F);
        ppu.write_bldalpha(0x100F);
        ppu.write_bldy(0x0011);
        ppu.write_affine(REG_BG2PA, 0x0200);
        ppu.write_affine(REG_BG2PB, 0x0010);
        ppu.write_affine(REG_BG2PC, 0xFFF0);
        ppu.write_affine(REG_BG2PD, 0x0180);
        ppu.write_affine(REG_BG2X_L, 0x3456);
        ppu.write_affine(REG_BG2X_H, 0x0001);
        ppu.write_affine(REG_BG2Y_L, 0x789A);
        ppu.write_affine(REG_BG2Y_H, 0x0800);
        ppu.set_color_correction(true);

        let saved = ppu.capture_state();

        ppu.write_dispcnt(0);
        ppu.write_green_swap(0);
        ppu.write_dispstat(0, &mut ic);
        ppu.write_bg_cnt(0, 0);
        ppu.write_bg_cnt(2, 0);
        ppu.write_bg_hofs(0, 0);
        ppu.write_bg_vofs(0, 0);
        ppu.write_win_h(0, 0);
        ppu.write_win_v(0, 0);
        ppu.write_winin(0);
        ppu.write_winout(0);
        ppu.write_mosaic(0);
        ppu.write_bldcnt(0);
        ppu.write_bldalpha(0);
        ppu.write_bldy(0);
        ppu.write_affine(REG_BG2PA, 0);
        ppu.write_affine(REG_BG2PB, 0);
        ppu.write_affine(REG_BG2PC, 0);
        ppu.write_affine(REG_BG2PD, 0);
        ppu.write_affine(REG_BG2X_L, 0);
        ppu.write_affine(REG_BG2X_H, 0);
        ppu.write_affine(REG_BG2Y_L, 0);
        ppu.write_affine(REG_BG2Y_H, 0);
        ppu.set_color_correction(false);

        ppu.restore_state(&saved);

        assert_eq!(ppu.read_dispcnt(), 3 | dispcnt::BG2_ENABLE);
        assert_eq!(ppu.read_green_swap(), 1);
        assert_eq!(
            ppu.read_dispstat() & dispstat::WRITE_MASK,
            dispstat::VBLANK_IRQ_ENABLE | (42 << 8)
        );
        assert_eq!(ppu.read_bg_cnt(0), 0xDFFF & !0x2000);
        assert_eq!(ppu.read_bg_cnt(2), 0xE0C3);
        assert_eq!(ppu.read_bg_hofs(0), 0x0123);
        assert_eq!(ppu.read_bg_vofs(0), 0x01F0);
        assert_eq!(ppu.read_win_h(0), 0x1234);
        assert_eq!(ppu.read_win_v(0), 0x5678);
        assert_eq!(ppu.read_winin(), 0x3F3F);
        assert_eq!(ppu.read_winout(), 0x2A2A);
        assert_eq!(ppu.read_mosaic(), 0x4321);
        assert_eq!(ppu.read_bldcnt(), 0x3F1F);
        assert_eq!(ppu.read_bldalpha(), 0x100F);
        assert_eq!(ppu.read_bldy(), 0x0011);
        assert_eq!(ppu.read_affine(REG_BG2PA), Some(0x0200));
        assert_eq!(ppu.read_affine(REG_BG2PB), Some(0x0010));
        assert_eq!(ppu.read_affine(REG_BG2PC), Some(0xFFF0));
        assert_eq!(ppu.read_affine(REG_BG2PD), Some(0x0180));
        assert_eq!(ppu.read_affine(REG_BG2X_L), Some(0x3456));
        assert_eq!(ppu.read_affine(REG_BG2X_H), Some(0x0001));
        assert_eq!(ppu.read_affine(REG_BG2Y_L), Some(0x789A));
        assert_eq!(ppu.read_affine(REG_BG2Y_H), Some(0xF800));
        assert!(ppu.color_correction);
    }

    #[test]
    fn save_state_restores_framebuffer_ready_and_timing_state() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();
        let oam = make_oam();
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        for x in 0..(SCREEN_WIDTH as usize) {
            let off = x * 2;
            vram[off..off + 2].copy_from_slice(&0x001Fu16.to_le_bytes());
        }

        ppu.step(
            CYCLES_PER_SCANLINE * VISIBLE_SCANLINES + 17,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );
        assert!(ppu.frame_ready());
        assert_eq!(ppu.read_vcount(), VISIBLE_SCANLINES as u16);
        assert_eq!(ppu.line_cycle, 17);
        let saved = ppu.capture_state();
        let saved_pixel = ppu.framebuffer()[0..3].to_vec();

        ppu.clear_frame_ready();
        ppu.step(CYCLES_PER_SCANLINE * 3, &mut ic, &vram, &pram, &oam);
        ppu.framebuffer[0..3].copy_from_slice(&[0, 0, 0]);

        ppu.restore_state(&saved);

        assert!(ppu.frame_ready());
        assert_eq!(ppu.read_vcount(), VISIBLE_SCANLINES as u16);
        assert_eq!(ppu.line_cycle, 17);
        assert_eq!(&ppu.framebuffer()[0..3], saved_pixel.as_slice());
    }

    #[test]
    fn save_state_roundtrips_through_json() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        ppu.write_dispcnt(5 | dispcnt::BG2_ENABLE | dispcnt::FRAME_SELECT);
        ppu.write_dispstat(dispstat::HBLANK_IRQ_ENABLE | (7 << 8), &mut ic);
        ppu.step(123, &mut ic, &make_vram(), &make_pram(), &make_oam());

        let saved = ppu.capture_state();
        let bytes = serde_json::to_vec(&saved).expect("serialize PPU state");
        let decoded: PpuState = serde_json::from_slice(&bytes).expect("deserialize PPU state");
        let mut restored = Ppu::new();
        restored.restore_state(&decoded);

        assert_eq!(restored.read_dispcnt(), ppu.read_dispcnt());
        assert_eq!(restored.read_dispstat(), ppu.read_dispstat());
        assert_eq!(restored.line_cycle, ppu.line_cycle);
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
    fn hblank_irq_not_fired_during_vblank() {
        // Per GBATek: "no H-Blank interrupts are generated within V-Blank period."
        // The H-Blank IRQ should NOT fire on scanlines 160-227.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        let oam = make_oam();
        // Enable H-Blank IRQ.
        ppu.write_dispstat(dispstat::HBLANK_IRQ_ENABLE, &mut ic);
        // Advance to the start of scanline 160 (first V-Blank scanline).
        ppu.step(
            CYCLES_PER_SCANLINE * VISIBLE_SCANLINES,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );
        assert_eq!(ppu.read_vcount(), 160);
        // Clear any IRQs accumulated during the visible scanlines.
        ic.if_flags = 0;
        // Advance through H-Blank of scanline 160 (V-Blank period).
        ppu.step(HBLANK_START_CYCLE, &mut ic, &vram, &pram, &oam);
        // H-Blank IRQ must NOT fire during V-Blank.
        assert_eq!(
            ic.if_flags & irq_bits::HBLANK,
            0,
            "H-Blank IRQ must not fire during V-Blank (scanline 160)"
        );
    }

    #[test]
    fn hblank_dma_fires_during_vblank_scanlines() {
        // Per GBATek, H-Blank DMA should trigger on ALL 228 scanlines,
        // including the 67 V-Blank scanlines (160–226).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        // Step up to the *start* of scanline 160 (V-Blank begins).
        let cycles = CYCLES_PER_SCANLINE * VISIBLE_SCANLINES;
        ppu.step(cycles, &mut ic, &vram, &pram, &make_oam());
        assert_eq!(ppu.read_vcount(), 160);
        // Step through H-Blank of scanline 160 — DMA must still fire.
        let events = ppu.step(HBLANK_START_CYCLE, &mut ic, &vram, &pram, &make_oam());
        assert_eq!(
            events.hblank_starts, 1,
            "H-Blank DMA must fire on VBlank scanlines too (GBATek)"
        );
    }

    #[test]
    fn step_full_frame_counts_every_hblank() {
        // A full-frame step must report all 228 H-Blanks (visible + VBlank)
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
        assert_eq!(events.hblank_starts, SCANLINES_PER_FRAME);
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
        assert_eq!(events.hblank_starts, SCANLINES_PER_FRAME * 2);
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
    fn mode3_bg_mosaic_repeats_upper_left_anchor_pixel() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();

        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        ppu.write_bg_cnt(2, 1 << 6);
        ppu.write_mosaic(0x0011);
        // Mode 3 uses affine — set identity matrix so rendering is linear.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        vram[0..2].copy_from_slice(&0x001Fu16.to_le_bytes());
        vram[2..4].copy_from_slice(&0x03E0u16.to_le_bytes());
        let row1 = SCREEN_WIDTH as usize * 2;
        vram[row1..row1 + 2].copy_from_slice(&0x7C00u16.to_le_bytes());
        vram[row1 + 2..row1 + 4].copy_from_slice(&0x7FFFu16.to_le_bytes());
        let row2 = SCREEN_WIDTH as usize * 2 * 2;
        vram[row2..row2 + 2].copy_from_slice(&0x03E0u16.to_le_bytes());
        vram[row2 + 2..row2 + 4].copy_from_slice(&0x7C00u16.to_le_bytes());

        ppu.step(CYCLES_PER_SCANLINE * 3, &mut ic, &vram, &pram, &make_oam());

        let fb_row1 = SCREEN_WIDTH as usize * BYTES_PER_PIXEL;
        assert_eq!(&ppu.framebuffer()[fb_row1..fb_row1 + 3], &[0xFF, 0, 0]);
        assert_eq!(
            &ppu.framebuffer()[fb_row1 + BYTES_PER_PIXEL..fb_row1 + BYTES_PER_PIXEL + 3],
            &[0xFF, 0, 0]
        );
        let fb_row2 = fb_row1 * 2;
        assert_eq!(&ppu.framebuffer()[fb_row2..fb_row2 + 3], &[0, 0xFF, 0]);
        assert_eq!(
            &ppu.framebuffer()[fb_row2 + BYTES_PER_PIXEL..fb_row2 + BYTES_PER_PIXEL + 3],
            &[0, 0xFF, 0]
        );
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
    fn mode3_uses_bg2_affine_reference_point() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();

        // Mode 3, BG2 enabled, identity affine transform starting at source (1, 0).
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        // BG2X = 1.0 pixel (0x0100 = 256 in 8.8 fixed-point = 1.0)
        ppu.write_affine(REG_BG2X_L, 0x0100);

        // Source pixel (0,0) = red (BGR555 0x001F); source pixel (1,0) = green (0x03E0).
        vram[0] = 0x1F;
        vram[1] = 0x00;
        vram[2] = 0xE0;
        vram[3] = 0x03;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // With BG2X = 1.0, screen pixel 0 should map to source pixel 1 (green).
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0xFF, 0]);
    }

    #[test]
    fn mode3_outside_240x160_bitmap_is_transparent() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 3, BG2 enabled, identity transform with BG2X=240 so screen
        // pixel 0 maps to source x=240 which is out-of-bounds for 240-wide bitmap.
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        // BG2X = 240.0 in 8.8 fixed-point: 240 * 256 = 61440 = 0xF000 (low 16 bits).
        ppu.write_affine(REG_BG2X_L, 0xF000);

        // Backdrop = blue.
        pram[0] = 0x00;
        pram[1] = 0x7C;
        // Fill the entire 240×160 source bitmap with red.
        for y in 0..160usize {
            for x in 0..240usize {
                let off = (y * 240 + x) * 2;
                vram[off] = 0x1F;
                vram[off + 1] = 0x00;
            }
        }

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Screen pixel 0 maps to source x=240 (OOB) → transparent → backdrop = blue.
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0, 0xFF]);
    }

    #[test]
    fn mode3_bgcnt_wrap_bit_set_oob_is_still_transparent() {
        // Per GBATek: in bitmap modes, out-of-bounds source samples are always
        // transparent regardless of BGxCNT bit 13. Wrapping via this bit only
        // works in affine tile modes (1 & 2), NOT in bitmap modes (3, 4, 5).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 3, BG2 enabled. Set BG2CNT bit 13 (wrap/overflow) = 1.
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        ppu.write_bg_cnt(2, 1 << 13);

        // Identity transform with BG2X=240 so screen pixel 0 maps to source
        // x=240 which is out-of-bounds for the 240-wide bitmap.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        // BG2X = 240.0 in 8.8 fixed-point: 240 * 256 = 61440 = 0xF000.
        ppu.write_affine(REG_BG2X_L, 0xF000);

        // Backdrop = blue.
        pram[0] = 0x00;
        pram[1] = 0x7C;
        // Fill the entire 240×160 source bitmap with red.
        for y in 0..160usize {
            for x in 0..240usize {
                let off = (y * 240 + x) * 2;
                vram[off] = 0x1F;
                vram[off + 1] = 0x00;
            }
        }

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // OOB pixel must be transparent → backdrop (blue), NOT wrapped to red.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0, 0xFF],
            "Mode 3: BG2CNT wrap bit must be ignored; OOB pixels must show backdrop"
        );
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

    // ---- Forced blank mid-frame restart (GBATek) ----------------------------

    #[test]
    fn forced_blank_midframe_deassert_resets_vcount_after_two_scanlines() {
        // Per GBATek: when forced blank is de-asserted mid-frame, the display
        // restarts from line 0 after approximately 2 vertical lines.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        let oam = make_oam();

        // Enable forced blank and advance to scanline 80.
        ppu.write_dispcnt(dispcnt::FORCED_BLANK);
        ppu.step(CYCLES_PER_SCANLINE * 80, &mut ic, &vram, &pram, &oam);
        assert_eq!(ppu.read_vcount(), 80);

        // De-assert forced blank → countdown of 2 scanlines begins.
        ppu.write_dispcnt(0);

        // After 2 full scanlines, vcount must have been reset to 0.
        ppu.step(CYCLES_PER_SCANLINE * 2, &mut ic, &vram, &pram, &oam);
        assert_eq!(
            ppu.read_vcount(),
            0,
            "After forced blank de-assert, vcount must restart from 0 after 2 scanlines"
        );
    }

    #[test]
    fn forced_blank_transition_scanlines_still_output_white() {
        // During the 2 transition scanlines after forced blank is de-asserted,
        // the display must still output white (forced blank behaviour persists).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let mut pram = make_pram();
        let oam = make_oam();

        // Set a non-white backdrop so we can distinguish from forced-blank white.
        // BGR555 pure blue = 0x7C00 → RGB888 = (0, 0, 0xFF).
        pram[0] = 0x00;
        pram[1] = 0x7C;

        // Enable forced blank and advance to scanline 80.
        ppu.write_dispcnt(dispcnt::FORCED_BLANK);
        ppu.step(CYCLES_PER_SCANLINE * 80, &mut ic, &vram, &pram, &oam);

        // De-assert forced blank; render the first transition scanline (80).
        ppu.write_dispcnt(0);
        // Step to H-Blank to trigger rendering of scanline 80.
        ppu.step(HBLANK_START_CYCLE, &mut ic, &vram, &pram, &oam);

        // Scanline 80 row should be white even though FORCED_BLANK bit is cleared.
        let row_start = 80 * (SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        assert!(
            ppu.framebuffer()[row_start..row_start + SCREEN_WIDTH as usize * BYTES_PER_PIXEL]
                .iter()
                .all(|&b| b == 0xFF),
            "First transition scanline must output white while countdown is active"
        );
    }

    #[test]
    fn forced_blank_reassert_before_restart_cancels_countdown() {
        // If forced blank is re-asserted before the 2-line restart completes,
        // the countdown is cancelled and vcount continues advancing normally.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        let oam = make_oam();

        // Enable forced blank, advance to scanline 80.
        ppu.write_dispcnt(dispcnt::FORCED_BLANK);
        ppu.step(CYCLES_PER_SCANLINE * 80, &mut ic, &vram, &pram, &oam);
        assert_eq!(ppu.read_vcount(), 80);

        // De-assert forced blank → countdown starts.
        ppu.write_dispcnt(0);
        // Immediately re-assert forced blank → countdown must be cancelled.
        ppu.write_dispcnt(dispcnt::FORCED_BLANK);

        // Advance 2 scanlines — vcount must advance normally to 82, NOT restart.
        ppu.step(CYCLES_PER_SCANLINE * 2, &mut ic, &vram, &pram, &oam);
        assert_eq!(
            ppu.read_vcount(),
            82,
            "Re-asserting forced blank must cancel the restart countdown"
        );
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
    fn mode0_bg0_8bpp_renders_direct_palette_index_and_ignores_palette_bank() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 0 + BG0 enabled, and BG0CNT bit 7 (8bpp) set.
        ppu.write_dispcnt(dispcnt::BG0_ENABLE);
        ppu.write_bg0cnt(1 << 7);

        // BG palette entry 33 = pure red.
        pram[33 * 2] = 0x1F;
        pram[33 * 2 + 1] = 0x00;

        // Charblock 0, 8bpp tile 1, row 0, pixel 0 uses direct palette index 33.
        vram[64] = 33;

        // Map entry: tile 1 and palette bank 15. Palette bank bits are unused in 8bpp.
        vram[0x0000] = 0x01;
        vram[0x0001] = 0xF0;

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
    fn mode0_bg0_8bpp_palette_index_zero_is_transparent() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);
        ppu.write_bg0cnt(1 << 7);

        // Backdrop/palette entry 0 = pure blue. A transparent BG pixel should reveal it.
        pram[0] = 0x00;
        pram[1] = 0x7C;

        // Map entry tile 1; tile data remains 0 at pixel (0,0).
        vram[0x0000] = 0x01;

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
    fn mode0_bg0_8bpp_hflip_and_vflip_mirror_tile_pixels() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);
        ppu.write_bg0cnt(1 << 7);

        // BG palette entry 5 = pure red.
        pram[5 * 2] = 0x1F;
        pram[5 * 2 + 1] = 0x00;

        // 8bpp tile 1, source pixel (7,7) uses palette index 5.
        vram[64 + 7 * 8 + 7] = 5;

        // Map entry: tile 1 + horizontal and vertical flip.
        vram[0x0000] = 0x01;
        vram[0x0001] = 0x0C;

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
    fn mode1_bg1_8bpp_renders_text_layer() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(1 | dispcnt::BG1_ENABLE);
        ppu.write_bg_cnt(1, 1 << 7);

        // BG palette entry 7 = pure red.
        pram[7 * 2] = 0x1F;
        pram[7 * 2 + 1] = 0x00;

        // 8bpp tile 1, row 0, pixel 0 uses direct palette index 7.
        vram[64] = 7;
        vram[0x0000] = 0x01;

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
        // Affine state must remain at power-on default (PA = PD = 0x0100).
        let bg2 = ppu.bg_affine(0).expect("BG2 affine state must exist");
        assert_eq!(bg2.pa, 0x0100);
        assert_eq!(bg2.x, 0);
    }

    #[test]
    fn mode2_renders_backdrop_color() {
        // Mode 2 is an affine tile mode for BG2/BG3. With no BG enables set,
        // the backdrop color is rendered for all pixels.
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
        // Palette index 0 is transparent in Mode 4; the compositing logic
        // falls back to the backdrop color, so the result is still the backdrop.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 4, BG2 enabled.
        ppu.write_dispcnt(4 | dispcnt::BG2_ENABLE);

        // Backdrop = pure blue (BGR555 0x7C00) in PRAM[0].
        pram[0] = 0x00;
        pram[1] = 0x7C;

        // VRAM[0] = palette index 0 (transparent → backdrop shows).
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
    fn mode4_palette_index_0_transparent_allows_obj_through() {
        // In Mode 4, palette index 0 must be TRANSPARENT (per GBATek), so OBJs
        // can show through the bitmap at those positions.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        // Mode 4 + BG2 enabled (priority 0) + OBJ enabled + 1D OBJ mapping.
        ppu.write_dispcnt(4 | dispcnt::BG2_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);

        // Backdrop = blue (BGR555 0x7C00); used if neither BG2 nor OBJ covers pixel.
        pram[0] = 0x00;
        pram[1] = 0x7C;

        // VRAM[0] = palette index 0 → should be transparent in Mode 4.
        vram[0] = 0;

        // Place OBJ 0 at (0,0), tile 512, priority 1 (lower priority than BG2's 0).
        // In bitmap modes (3-5), tile IDs 0-511 are not displayed.
        setup_obj_in_oam(&mut oam, 0, 0, 0, 512, 1);

        // OBJ base is 0x10000; tile 512 (4bpp, 1D) begins at 0x10000 + 512 * 32 = 0x14000.
        let obj_tile_base = 0x1_0000 + 512 * 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11; // palette index 1 in both nibbles
        }

        // OBJ palette color 1 = red (BGR555: 0x001F).
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        // BG2 pixel at (0,0) is palette index 0 → transparent.
        // OBJ at (0,0) with priority 1 is lower than BG2's priority 0, but BG2
        // is transparent here, so OBJ should show through → red.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0xFF, 0, 0],
            "OBJ should show through Mode 4 palette index 0 (transparent) pixel"
        );
    }

    #[test]
    fn mode4_bg_mosaic_repeats_anchor_and_keeps_index_0_transparent() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(4 | dispcnt::BG2_ENABLE);
        ppu.write_bg_cnt(2, 1 << 6);
        ppu.write_mosaic(0x0011);
        // Identity affine transform so screen coordinates map 1:1 to source.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Backdrop = blue.
        pram[0] = 0x00;
        pram[1] = 0x7C;
        // Palette index 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;

        // Row 0: [1, 0, 0, 1]
        vram[0] = 1;
        vram[1] = 0;
        vram[2] = 0;
        vram[3] = 1;
        // Row 1 intentionally differs to verify vertical anchoring from row 0.
        vram[SCREEN_WIDTH as usize] = 0;
        vram[SCREEN_WIDTH as usize + 1] = 1;
        vram[SCREEN_WIDTH as usize + 2] = 1;
        vram[SCREEN_WIDTH as usize + 3] = 1;

        ppu.step(CYCLES_PER_SCANLINE * 3, &mut ic, &vram, &pram, &make_oam());

        let row1 = SCREEN_WIDTH as usize * BYTES_PER_PIXEL;
        assert_eq!(&ppu.framebuffer()[row1..row1 + 3], &[0xFF, 0, 0]);
        assert_eq!(
            &ppu.framebuffer()[row1 + BYTES_PER_PIXEL..row1 + BYTES_PER_PIXEL + 3],
            &[0xFF, 0, 0]
        );
        assert_eq!(
            &ppu.framebuffer()[row1 + BYTES_PER_PIXEL * 2..row1 + BYTES_PER_PIXEL * 2 + 3],
            &[0, 0, 0xFF]
        );
        assert_eq!(
            &ppu.framebuffer()[row1 + BYTES_PER_PIXEL * 3..row1 + BYTES_PER_PIXEL * 3 + 3],
            &[0, 0, 0xFF]
        );
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
    fn mode4_uses_bg2_affine_reference_point() {
        // Mode 4 must apply the BG2 affine reference point (BG2X/BG2Y) just as
        // Mode 3 and Mode 5 do. With BG2X = 1.0, screen pixel 0 should map to
        // source pixel 1, not source pixel 0.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 4, BG2 enabled, identity affine transform starting at source x=1.
        ppu.write_dispcnt(4 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        // BG2X = 1.0 pixel (0x0100 = 256 in 8.8 fixed-point = 1.0)
        ppu.write_affine(REG_BG2X_L, 0x0100);

        // Palette index 1 = red; palette index 2 = green.
        pram[2] = 0x1F;
        pram[3] = 0x00;
        pram[4] = 0xE0;
        pram[5] = 0x03;

        // Source (0,0) = palette index 1 (red); source (1,0) = palette index 2 (green).
        vram[0] = 1;
        vram[1] = 2;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // With BG2X = 1.0, screen pixel 0 should map to source pixel 1 (green).
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0xFF, 0]);
    }

    #[test]
    fn mode4_outside_240x160_bitmap_is_transparent() {
        // Pixels whose source coordinates land outside the 240×160 source bitmap
        // must be transparent (backdrop shows through), matching Mode 3/5 behavior.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 4, BG2 enabled, identity transform with BG2X=240 so screen
        // pixel 0 maps to source x=240 which is out-of-bounds for 240-wide bitmap.
        ppu.write_dispcnt(4 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        // BG2X = 240.0 in 8.8 fixed-point: 240 * 256 = 61440 = 0xF000 (low 16 bits).
        ppu.write_affine(REG_BG2X_L, 0xF000);

        // Backdrop = blue.
        pram[0] = 0x00;
        pram[1] = 0x7C;
        // Palette index 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;
        // Fill the entire 240×160 source bitmap with palette index 1 (red).
        for vram_byte in vram
            .iter_mut()
            .take(SCREEN_WIDTH as usize * SCREEN_HEIGHT as usize)
        {
            *vram_byte = 1;
        }

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Screen pixel 0 maps to source x=240 (OOB) → transparent → backdrop = blue.
        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0, 0xFF]);
    }

    #[test]
    fn mode4_bgcnt_wrap_bit_set_oob_is_still_transparent() {
        // Per GBATek: in bitmap modes, out-of-bounds source samples are always
        // transparent regardless of BGxCNT bit 13. Wrapping via this bit only
        // works in affine tile modes (1 & 2), NOT in bitmap modes (3, 4, 5).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 4, BG2 enabled. Set BG2CNT bit 13 (wrap/overflow) = 1.
        ppu.write_dispcnt(4 | dispcnt::BG2_ENABLE);
        ppu.write_bg_cnt(2, 1 << 13);

        // Identity transform with BG2X=240 so screen pixel 0 maps to source
        // x=240 which is out-of-bounds for the 240-wide bitmap.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        // BG2X = 240.0 in 8.8 fixed-point: 240 * 256 = 61440 = 0xF000.
        ppu.write_affine(REG_BG2X_L, 0xF000);

        // Backdrop = blue.
        pram[0] = 0x00;
        pram[1] = 0x7C;
        // Palette index 1 = red.
        pram[2] = 0x1F;
        pram[3] = 0x00;
        // Fill the entire 240×160 source bitmap with palette index 1 (red).
        for item in vram
            .iter_mut()
            .take(SCREEN_WIDTH as usize * SCREEN_HEIGHT as usize)
        {
            *item = 1;
        }

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // OOB pixel must be transparent → backdrop (blue), NOT wrapped to red.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0, 0xFF],
            "Mode 4: BG2CNT wrap bit must be ignored; OOB pixels must show backdrop"
        );
    }

    #[test]
    fn mode5_renders_160x128_bgr555_bitmap_from_vram() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 5, BG2 enabled, identity affine transform.
        ppu.write_dispcnt(5 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Backdrop = blue. Pixel (0,0) = red with bit 15 set (ignored).
        pram[0] = 0x00;
        pram[1] = 0x7C;
        vram[0] = 0x1F;
        vram[1] = 0x80;

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
    fn mode5_frame_select_uses_frame_1_at_0xa000() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();

        // Mode 5, BG2 enabled, frame 1 selected, identity affine transform.
        ppu.write_dispcnt(5 | dispcnt::BG2_ENABLE | dispcnt::FRAME_SELECT);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Frame 0 pixel (0,0) = red; frame 1 pixel (0,0) = green.
        vram[0] = 0x1F;
        vram[1] = 0x00;
        vram[0xA000] = 0xE0;
        vram[0xA001] = 0x03;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0xFF, 0]);
    }

    #[test]
    fn mode5_outside_160x128_bitmap_is_transparent() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 5, BG2 enabled, identity affine transform.
        ppu.write_dispcnt(5 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Backdrop = blue. Fill the valid source area with red.
        pram[0] = 0x00;
        pram[1] = 0x7C;
        for y in 0..MODE5_HEIGHT {
            for x in 0..MODE5_WIDTH {
                let off = (y * MODE5_WIDTH + x) * 2;
                vram[off] = 0x1F;
                vram[off + 1] = 0x00;
            }
        }

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        let x_outside = 160usize * BYTES_PER_PIXEL;
        let y_outside = (128usize * SCREEN_WIDTH as usize) * BYTES_PER_PIXEL;
        assert_eq!(&ppu.framebuffer()[0..3], &[0xFF, 0, 0]);
        assert_eq!(&ppu.framebuffer()[x_outside..x_outside + 3], &[0, 0, 0xFF]);
        assert_eq!(&ppu.framebuffer()[y_outside..y_outside + 3], &[0, 0, 0xFF]);
    }

    #[test]
    fn mode5_bgcnt_wrap_bit_set_oob_is_still_transparent() {
        // Per GBATek: in bitmap modes, out-of-bounds source samples are always
        // transparent regardless of BGxCNT bit 13. Wrapping via this bit only
        // works in affine tile modes (1 & 2), NOT in bitmap modes (3, 4, 5).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        // Mode 5 (160×128 bitmap), BG2 enabled. Set BG2CNT bit 13 (wrap/overflow) = 1.
        ppu.write_dispcnt(5 | dispcnt::BG2_ENABLE);
        ppu.write_bg_cnt(2, 1 << 13);

        // Identity transform with BG2X=160 so screen pixel 0 maps to source
        // x=160 which is out-of-bounds for the 160-wide Mode 5 bitmap.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        // BG2X = 160.0 in 8.8 fixed-point: 160 * 256 = 40960 = 0xA000.
        ppu.write_affine(REG_BG2X_L, 0xA000);

        // Backdrop = blue.
        pram[0] = 0x00;
        pram[1] = 0x7C;
        // Fill the 160×128 source bitmap with red.
        for y in 0..MODE5_HEIGHT {
            for x in 0..MODE5_WIDTH {
                let off = (y * MODE5_WIDTH + x) * 2;
                vram[off] = 0x1F;
                vram[off + 1] = 0x00;
            }
        }

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // OOB pixel must be transparent → backdrop (blue), NOT wrapped to red.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0, 0xFF],
            "Mode 5: BG2CNT wrap bit must be ignored; OOB pixels must show backdrop"
        );
    }

    #[test]
    fn mode5_uses_bg2_affine_reference_point() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();

        // Mode 5, BG2 enabled, identity affine transform starting at source (1, 0).
        ppu.write_dispcnt(5 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);
        ppu.write_affine(REG_BG2X_L, 0x0100);

        // Source pixel (0,0) = red; source pixel (1,0) = green.
        vram[0] = 0x1F;
        vram[1] = 0x00;
        vram[2] = 0xE0;
        vram[3] = 0x03;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        assert_eq!(&ppu.framebuffer()[0..3], &[0, 0xFF, 0]);
    }

    #[test]
    fn mode5_bg_mosaic_anchors_horizontally_and_vertically_with_affine() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();

        ppu.write_dispcnt(5 | dispcnt::BG2_ENABLE);
        ppu.write_bg_cnt(2, 1 << 6);
        ppu.write_mosaic(0x0011);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Row 0 source pixels: x=2 red, x=3 green, x=4 blue.
        let row0_col2 = 2 * 2;
        vram[row0_col2..row0_col2 + 2].copy_from_slice(&0x001Fu16.to_le_bytes());
        let row0_col3 = 3 * 2;
        vram[row0_col3..row0_col3 + 2].copy_from_slice(&0x03E0u16.to_le_bytes());
        let row0_col4 = 4 * 2;
        vram[row0_col4..row0_col4 + 2].copy_from_slice(&0x7C00u16.to_le_bytes());

        // Row 1 differs, so vertical mosaic anchoring is observable.
        let row1_base = MODE5_WIDTH * 2;
        let row1_col2 = row1_base + 2 * 2;
        vram[row1_col2..row1_col2 + 2].copy_from_slice(&0x7FFFu16.to_le_bytes());

        ppu.step(CYCLES_PER_SCANLINE * 3, &mut ic, &vram, &pram, &make_oam());

        let fb_row1 = SCREEN_WIDTH as usize * BYTES_PER_PIXEL;
        assert_eq!(
            &ppu.framebuffer()[fb_row1 + 2 * BYTES_PER_PIXEL..fb_row1 + 2 * BYTES_PER_PIXEL + 3],
            &[0xFF, 0, 0]
        );
        assert_eq!(
            &ppu.framebuffer()[fb_row1 + 3 * BYTES_PER_PIXEL..fb_row1 + 3 * BYTES_PER_PIXEL + 3],
            &[0xFF, 0, 0]
        );
        assert_eq!(
            &ppu.framebuffer()[fb_row1 + 4 * BYTES_PER_PIXEL..fb_row1 + 4 * BYTES_PER_PIXEL + 3],
            &[0, 0, 0xFF]
        );
        assert_eq!(
            &ppu.framebuffer()[fb_row1 + 5 * BYTES_PER_PIXEL..fb_row1 + 5 * BYTES_PER_PIXEL + 3],
            &[0, 0, 0xFF]
        );
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

    #[test]
    fn text_bg_mosaic_repeats_upper_left_anchor_pixel() {
        let mut ppu = Ppu::new();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut buf = [TRANSPARENT; SCREEN_WIDTH as usize];

        // BG0CNT: mosaic enabled, charblock 0, screenblock 0.
        ppu.write_bg_cnt(0, 1 << 6);
        // BG mosaic H/V sizes are both 2 pixels.
        ppu.write_mosaic(0x0011);

        // Palette entries 1..4 are distinct colors.
        pram[2] = 0x1F;
        pram[3] = 0x00;
        pram[4] = 0xE0;
        pram[5] = 0x03;
        pram[6] = 0x00;
        pram[7] = 0x7C;
        pram[8] = 0xFF;
        pram[9] = 0x7F;

        // Screen entry (0,0) -> tile 1.
        vram[0] = 1;
        // Tile 1: row 0 pixels 0/1 = palette 1/2, row 1 pixels 0/1 = 3/4.
        vram[32] = 0x21;
        vram[32 + 4] = 0x43;

        ppu.render_text_bg_layer(0, 1, &vram, &pram, &mut buf);

        assert_eq!(buf[0], 0x001F);
        assert_eq!(buf[1], 0x001F);
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

    // ---- Mid-frame BG2X/BG2Y write tests ----
    //
    // GBATek "LCD I/O BG Rotation/Scaling":
    // "If software writes to BG2X/BG2Y outside V-Blank, the value is
    //  immediately applied to the internal register for the current scanline."
    //
    // These tests verify the framebuffer-level effect of mid-frame writes and
    // that the PB/PD accumulation is reset to the new value.

    #[test]
    fn bg2x_write_outside_vblank_applies_to_current_scanline() {
        // Writing BG2X during the active period of a visible scanline (before
        // its H-Blank edge) must immediately update the internal reference so
        // that the scanline renders using the new value.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();

        // Mode 3 + BG2 enabled, identity affine (PA=PD=1.0, PB=PC=0).
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Source pixel (0,0) = red (BGR555 0x001F).
        // Source pixel (1,0) = green (BGR555 0x03E0).
        vram[0] = 0x1F;
        vram[1] = 0x00;
        vram[2] = 0xE0;
        vram[3] = 0x03;

        // Run a full frame so VBlank latch initialises internal_x = x = 0.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Mid-frame write (outside V-Blank): BG2X = 1.0 pixel (0x0000_0100 in
        // 19.8 fixed-point). Per GBATek, this must immediately update the
        // internal register.
        ppu.write_affine(REG_BG2X_L, 0x0100);

        // Run one scanline to trigger its render.
        ppu.step(CYCLES_PER_SCANLINE, &mut ic, &vram, &pram, &make_oam());

        // Screen pixel (0,0) → source pixel (1,0) = green.
        // Without the immediate internal update it would show source (0,0) = red.
        let (r, g, b) = color::bgr555_to_rgb888(0x03E0);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "mid-frame BG2X write must take effect on the current scanline"
        );
    }

    #[test]
    fn bg2y_write_outside_vblank_applies_to_current_scanline() {
        // Same as above but for the Y reference point: writing BG2Y outside
        // V-Blank must immediately update the source row used for the scanline.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();

        // Mode 3 + BG2 enabled, identity affine.
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Source row 0, col 0 = red; source row 1, col 0 = green.
        // Mode 3: 240 pixels per row, 2 bytes each → row 1 starts at byte 480.
        vram[0] = 0x1F;
        vram[1] = 0x00; // source (0,0) = red
        vram[480] = 0xE0;
        vram[481] = 0x03; // source (0,1) = green

        // Run a full frame so VBlank latch initialises internal_y = y = 0.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Mid-frame write: BG2Y = 1.0 row (0x0000_0100 in 19.8 fixed-point).
        ppu.write_affine(REG_BG2Y_L, 0x0100);

        // Render scanline 0.
        ppu.step(CYCLES_PER_SCANLINE, &mut ic, &vram, &pram, &make_oam());

        // Screen pixel (0,0) → source row 1, col 0 = green.
        let (r, g, b) = color::bgr555_to_rgb888(0x03E0);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "mid-frame BG2Y write must take effect on the current scanline"
        );
    }

    #[test]
    fn bg2x_midframe_write_resets_pb_accumulation() {
        // After a mid-frame write to BG2X, the PB-per-scanline accumulation
        // must restart from the new value rather than continuing from the old
        // accumulated position.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();

        // BG2: PB = 0x0010 (1/16 per scanline).
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PB, 0x0010);
        ppu.write_affine(REG_BG2PC, 0x0000);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Run one full frame so VBlank latches internal refs (to 0,0),
        // then run 10 visible scanlines: internal_x = 10 * 0x0010 = 0x00A0.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );
        ppu.step(CYCLES_PER_SCANLINE * 10, &mut ic, &vram, &pram, &make_oam());

        // Mid-frame write: reset BG2X to 0x0200.  This must immediately update
        // internal_x and restart PB accumulation from 0x0200.
        ppu.write_affine(REG_BG2X_L, 0x0200);

        // internal_x must be 0x0200 immediately (not the old accumulated 0x00A0).
        let aff = ppu.bg_affine(0).unwrap();
        assert_eq!(
            aff.internal_x, 0x0200,
            "mid-frame BG2X write must immediately set internal_x"
        );

        // Run 5 more scanlines.  internal_x should now be 0x0200 + 5*0x0010 = 0x0250.
        ppu.step(CYCLES_PER_SCANLINE * 5, &mut ic, &vram, &pram, &make_oam());
        let aff = ppu.bg_affine(0).unwrap();
        assert_eq!(
            aff.internal_x, 0x0250,
            "after mid-frame write, PB increments must count from the new value"
        );
    }

    #[test]
    fn bg2x_midframe_write_preserved_through_next_vblank() {
        // A mid-frame write to BG2X must be visible from the NEXT frame's
        // first scanline, because VBlank re-latches internal_x from the
        // register value (which was updated by the mid-frame write).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();

        // Mode 3 + BG2, identity affine.
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Source (0,0) = red; source (1,0) = green.
        vram[0] = 0x1F;
        vram[1] = 0x00;
        vram[2] = 0xE0;
        vram[3] = 0x03;

        // Run one full frame (latch: internal_x = x = 0).
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Mid-frame write at scanline 0 of frame 2.
        ppu.write_affine(REG_BG2X_L, 0x0100);

        // Complete frame 2 (228 scanlines) so VBlank latches x=0x0100 again.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &make_oam(),
        );

        // Frame 3, scanline 0.
        ppu.step(CYCLES_PER_SCANLINE, &mut ic, &vram, &pram, &make_oam());

        // internal_x was re-latched from x = 0x0100 at frame-2 VBlank entry
        // (i.e., before frame 3 scanline 0), so screen (0,0) → source (1,0) = green.
        let (r, g, b) = color::bgr555_to_rgb888(0x03E0);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "mid-frame write must persist into the next frame via VBlank latch"
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
    fn affine_bg_mosaic_repeats_upper_left_anchor_pixel() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(2 | dispcnt::BG2_ENABLE);
        ppu.write_bg_cnt(2, (1 << 6) | (8 << 8) | (1 << 14));
        ppu.write_mosaic(0x0011);
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Affine map entry (0,0) -> tile 1.
        vram[8 * 0x800] = 1;

        // Palette entries 1..4 are distinct colors.
        pram[2] = 0x1F;
        pram[3] = 0x00;
        pram[4] = 0xE0;
        pram[5] = 0x03;
        pram[6] = 0x00;
        pram[7] = 0x7C;
        pram[8] = 0xFF;
        pram[9] = 0x7F;

        // Tile 1 rows have distinct first pixels so row 2 verifies the next
        // vertical mosaic block uses a new anchor.
        let tile_base = 64;
        vram[tile_base] = 1;
        vram[tile_base + 1] = 2;
        vram[tile_base + 8] = 3;
        vram[tile_base + 9] = 4;
        vram[tile_base + 16] = 2;
        vram[tile_base + 17] = 3;

        ppu.step(CYCLES_PER_SCANLINE * 3, &mut ic, &vram, &pram, &make_oam());

        let row1 = SCREEN_WIDTH as usize * BYTES_PER_PIXEL;
        assert_eq!(&ppu.framebuffer()[row1..row1 + 3], &[0xFF, 0, 0]);
        assert_eq!(
            &ppu.framebuffer()[row1 + BYTES_PER_PIXEL..row1 + BYTES_PER_PIXEL + 3],
            &[0xFF, 0, 0]
        );
        let row2 = row1 * 2;
        assert_eq!(&ppu.framebuffer()[row2..row2 + 3], &[0, 0xFF, 0]);
        assert_eq!(
            &ppu.framebuffer()[row2 + BYTES_PER_PIXEL..row2 + BYTES_PER_PIXEL + 3],
            &[0, 0xFF, 0]
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

    /// Helper to set up a visible OBJ in OAM at position (x, y) with given tile
    /// and priority. Uses 4bpp, shape=0 size=0 (8x8), 1D mapping.
    fn setup_obj_in_oam(oam: &mut [u8], obj_idx: usize, x: u16, y: u8, tile: u16, priority: u8) {
        let base = obj_idx * 8;
        let attr0 = y as u16; // normal mode, 4bpp, shape=0
        let attr1 = x & 0x1FF; // size=0 (8x8)
        let attr2 = (tile & 0x3FF) | ((priority as u16 & 3) << 10);
        oam[base] = attr0 as u8;
        oam[base + 1] = (attr0 >> 8) as u8;
        oam[base + 2] = attr1 as u8;
        oam[base + 3] = (attr1 >> 8) as u8;
        oam[base + 4] = attr2 as u8;
        oam[base + 5] = (attr2 >> 8) as u8;
    }

    #[test]
    fn obj_renders_when_enabled_in_mode0() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let pram = make_pram();
        let mut oam = make_oam();

        // Enable mode 0 + OBJ + 1D mapping.
        ppu.write_dispcnt(dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);

        // Place OBJ 0 at (100, 0), tile 1, priority 0.
        setup_obj_in_oam(&mut oam, 0, 100, 0, 1, 0);

        // Fill OBJ tile 1 in VRAM (4bpp at OBJ base 0x10000).
        let tile_base = 0x1_0000 + 32;
        let mut vram = vram;
        for byte in &mut vram[tile_base..tile_base + 32] {
            *byte = 0x11; // palette index 1 in both nibbles
        }

        // Set OBJ palette color 1 (at PRAM offset 0x200 + 1*2).
        let mut pram = pram;
        pram[0x202] = 0x1F; // red (BGR555: 0x001F)
        pram[0x203] = 0x00;

        // Step one full frame.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        // Check pixel at (100, 0) — should be red.
        let dst = 100 * BYTES_PER_PIXEL;
        assert_eq!(
            &ppu.framebuffer()[dst..dst + 3],
            &[0xFF, 0, 0],
            "OBJ should render red pixel at x=100"
        );
    }

    #[test]
    fn obj_disabled_does_not_render() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        // Enable mode 0 with 1D mapping but NO OBJ_ENABLE.
        ppu.write_dispcnt(dispcnt::OBJ_MAPPING_1D);

        setup_obj_in_oam(&mut oam, 0, 100, 0, 1, 0);
        let tile_base = 0x1_0000 + 32;
        for byte in &mut vram[tile_base..tile_base + 32] {
            *byte = 0x11;
        }
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        // Pixel at (100, 0) should be backdrop (black = 0,0,0).
        let dst = 100 * BYTES_PER_PIXEL;
        assert_eq!(
            &ppu.framebuffer()[dst..dst + 3],
            &[0, 0, 0],
            "OBJ should not render when OBJ_ENABLE is off"
        );
    }

    #[test]
    fn hblank_interval_free_uses_reduced_obj_cycle_budget() {
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        // OBJ 0..13 are 64x64 and ON scanline 0: each consumes 64 cycles (14*64=896).
        for obj_idx in 0..14 {
            setup_obj_in_oam(&mut oam, obj_idx, 0, 0, 0, 0);
            let base = obj_idx * 8;
            let attr1 = (oam[base + 2] as u16) | ((oam[base + 3] as u16) << 8) | (3 << 14);
            oam[base + 2] = attr1 as u8;
            oam[base + 3] = (attr1 >> 8) as u8;
        }

        // OBJ 14 is the first visible 64x64 OBJ on scanline 0 (cost 64 cycles).
        // Total would be 960 cycles: fits normal budget (1210), exceeds hblank-free (954).
        setup_obj_in_oam(&mut oam, 14, 100, 0, 1, 0);
        let base = 14 * 8;
        let attr1 = (oam[base + 2] as u16) | ((oam[base + 3] as u16) << 8) | (3 << 14);
        oam[base + 2] = attr1 as u8;
        oam[base + 3] = (attr1 >> 8) as u8;

        // Fill OBJ tile 1 with palette index 1 and set OBJ palette color 1 to red.
        let tile_base = 0x1_0000 + 32;
        for byte in &mut vram[tile_base..tile_base + 32] {
            *byte = 0x11;
        }
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00;

        let mut ppu_normal = Ppu::new();
        let mut ppu_hblank_free = Ppu::new();
        let mut ic_normal = make_ic();
        let mut ic_hblank_free = make_ic();

        ppu_normal.write_dispcnt(dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);
        ppu_hblank_free.write_dispcnt(
            dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D | dispcnt::HBLANK_INTERVAL_FREE,
        );

        ppu_normal.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic_normal,
            &vram,
            &pram,
            &oam,
        );
        ppu_hblank_free.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic_hblank_free,
            &vram,
            &pram,
            &oam,
        );

        let dst = 100 * BYTES_PER_PIXEL;
        assert_eq!(
            &ppu_normal.framebuffer()[dst..dst + 3],
            &[0xFF, 0, 0],
            "OBJ should render with normal cycle budget"
        );
        assert_eq!(
            &ppu_hblank_free.framebuffer()[dst..dst + 3],
            &[0, 0, 0],
            "OBJ should be dropped with hblank-free reduced cycle budget"
        );
    }

    #[test]
    fn obj_priority_compositing_with_bg() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        // Mode 0, enable BG0 + OBJ + 1D mapping.
        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);

        // BG0: charblock 0, screenblock 8, priority 1.
        // BGCNT: priority=1, charblock=0, screenblock=8
        let bgcnt = 1 | (8 << 8); // prio=1, screenblock=8
        ppu.write_bg_cnt(0, bgcnt);

        // Fill BG0 tile 1 (charblock 0) with palette index 2 (green).
        let tile_base = 32; // tile 1
        for byte in &mut vram[tile_base..tile_base + 32] {
            *byte = 0x22;
        }
        // Screenblock 8 (offset 0x4000): set first tile map entry to tile 1.
        let sb_base = 8 * 0x800;
        vram[sb_base] = 1; // tile_id = 1
        vram[sb_base + 1] = 0; // no flip, palette bank 0

        // Set BG palette color 2 at PRAM offset 2*2=4.
        pram[4] = 0xE0; // green (BGR555: 0x03E0)
        pram[5] = 0x03;

        // OBJ at (0, 0), tile 1, priority 0 (higher than BG0's priority 1).
        setup_obj_in_oam(&mut oam, 0, 0, 0, 1, 0);
        let obj_tile_base = 0x1_0000 + 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11; // palette index 1
        }
        pram[0x202] = 0x1F; // red (BGR555: 0x001F)
        pram[0x203] = 0x00;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        // OBJ prio 0 should be on top of BG0 prio 1 → red pixel.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0xFF, 0, 0],
            "OBJ priority 0 should draw on top of BG priority 1"
        );

        // Now test OBJ with lower priority than BG.
        let mut ppu2 = Ppu::new();
        ppu2.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);
        // BG0 at priority 0, same charblock/screenblock.
        let bgcnt2 = 8 << 8; // prio=0, screenblock=8
        ppu2.write_bg_cnt(0, bgcnt2);

        // OBJ at priority 2 (lower than BG0 priority 0).
        setup_obj_in_oam(&mut oam, 0, 0, 0, 1, 2);

        ppu2.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        // BG0 prio 0 should be on top of OBJ prio 2 → green pixel.
        assert_eq!(
            &ppu2.framebuffer()[0..3],
            &[0, 0xFF, 0],
            "BG priority 0 should be on top of OBJ priority 2"
        );
    }

    // ─── Window register tests ───────────────────────────────────────────

    #[test]
    fn window_registers_default_to_zero() {
        let ppu = Ppu::new();
        assert_eq!(ppu.read_winin(), 0);
        assert_eq!(ppu.read_winout(), 0);
    }

    #[test]
    fn write_winin_masks_invalid_bits() {
        let mut ppu = Ppu::new();
        ppu.write_winin(0xFFFF);
        assert_eq!(ppu.read_winin(), 0x3F3F);
    }

    #[test]
    fn write_winout_masks_invalid_bits() {
        let mut ppu = Ppu::new();
        ppu.write_winout(0xFFFF);
        assert_eq!(ppu.read_winout(), 0x3F3F);
    }

    #[test]
    fn write_win_h_stores_value() {
        let mut ppu = Ppu::new();
        ppu.write_win_h(0, 0x1080); // X1=16, X2=128
        ppu.write_win_h(1, 0x20F0); // X1=32, X2=240
        assert_eq!(ppu.read_win_h(0), 0x1080);
        assert_eq!(ppu.read_win_h(1), 0x20F0);
    }

    #[test]
    fn write_win_v_stores_value() {
        let mut ppu = Ppu::new();
        ppu.write_win_v(0, 0x1060); // Y1=16, Y2=96
        ppu.write_win_v(1, 0x00A0); // Y1=0, Y2=160
        assert_eq!(ppu.read_win_v(0), 0x1060);
        assert_eq!(ppu.read_win_v(1), 0x00A0);
    }

    #[test]
    fn dispcnt_window_enable_bits() {
        let mut ppu = Ppu::new();
        ppu.write_dispcnt(dispcnt::WIN0_ENABLE | dispcnt::WIN1_ENABLE | dispcnt::OBJ_WIN_ENABLE);
        assert!(ppu.dispcnt & dispcnt::WIN0_ENABLE != 0);
        assert!(ppu.dispcnt & dispcnt::WIN1_ENABLE != 0);
        assert!(ppu.dispcnt & dispcnt::OBJ_WIN_ENABLE != 0);
    }

    // ─── Window classification tests ─────────────────────────────────────

    #[test]
    fn window_classify_outside_when_no_windows_active() {
        let mut ppu = Ppu::new();
        // DISPCNT with no window bits → all layers visible (mask 0x3F).
        ppu.write_dispcnt(dispcnt::BG0_ENABLE);
        // When no windows are enabled, composite_scanline uses 0x3F directly.
        // Test pixel_in_window helper won't matter but let's check it doesn't panic.
        assert!(!ppu.pixel_in_window(0, 100, 80));
    }

    #[test]
    fn pixel_in_window_basic_rect() {
        let mut ppu = Ppu::new();
        // Win0: X1=10, X2=50, Y1=20, Y2=100.
        ppu.write_win_h(0, (10 << 8) | 50);
        ppu.write_win_v(0, (20 << 8) | 100);

        assert!(ppu.pixel_in_window(0, 10, 20)); // top-left inside
        assert!(ppu.pixel_in_window(0, 49, 99)); // bottom-right edge inside
        assert!(!ppu.pixel_in_window(0, 50, 50)); // X2 is exclusive
        assert!(!ppu.pixel_in_window(0, 25, 100)); // Y2 is exclusive
        assert!(!ppu.pixel_in_window(0, 5, 50)); // left of window
    }

    #[test]
    fn pixel_in_window_wraparound_x() {
        let mut ppu = Ppu::new();
        // X1=200, X2=50 → X1 > X2: window wraps around.
        // Covers 0..50 (low range) AND 200..240 (high range).
        ppu.write_win_h(0, (200 << 8) | 50);
        ppu.write_win_v(0, 160); // full height (Y1=0, Y2=160)

        assert!(ppu.pixel_in_window(0, 210, 80)); // 200..240 high range
        assert!(ppu.pixel_in_window(0, 30, 80)); // 0..50 low range (wrap)
        assert!(!ppu.pixel_in_window(0, 100, 80)); // 50..200 gap
    }

    #[test]
    fn pixel_in_window_clamp_x2_out_of_range() {
        let mut ppu = Ppu::new();
        // X1=10, X2=250 → X2 > 240, per spec clamp X2 to 240.
        ppu.write_win_h(0, (10 << 8) | 250);
        ppu.write_win_v(0, 160); // full height
        assert!(ppu.pixel_in_window(0, 10, 80)); // left edge
        assert!(ppu.pixel_in_window(0, 239, 80)); // last pixel before clamped X2=240
        assert!(!ppu.pixel_in_window(0, 5, 80)); // left of window
    }

    #[test]
    fn pixel_in_window_wraparound_y() {
        let mut ppu = Ppu::new();
        // Y1=120, Y2=50 → Y1 > Y2: window wraps around.
        // Covers 0..50 (low range) AND 120..160 (high range).
        ppu.write_win_h(0, 240); // full width (X1=0, X2=240)
        ppu.write_win_v(0, (120 << 8) | 50);

        assert!(ppu.pixel_in_window(0, 100, 130)); // 120..160 high range
        assert!(ppu.pixel_in_window(0, 100, 30)); // 0..50 low range (wrap)
        assert!(!ppu.pixel_in_window(0, 100, 60)); // 50..120 gap
    }

    #[test]
    fn pixel_in_window_clamp_y2_out_of_range() {
        let mut ppu = Ppu::new();
        // Y1=10, Y2=200 → Y2 > 160, per spec clamp Y2 to 160.
        ppu.write_win_h(0, 240); // full width
        ppu.write_win_v(0, (10 << 8) | 200);
        assert!(ppu.pixel_in_window(0, 100, 10)); // top edge
        assert!(ppu.pixel_in_window(0, 100, 159)); // last pixel before clamped Y2=160
        assert!(!ppu.pixel_in_window(0, 100, 5)); // above window
    }

    #[test]
    fn pixel_in_window_wraparound_x_low_range_is_inside() {
        let mut ppu = Ppu::new();
        // X1=200, X2=50: X1 > X2, so window wraps around.
        // Per hardware: covers 0..50 AND 200..240.
        ppu.write_win_h(0, (200 << 8) | 50);
        ppu.write_win_v(0, 160); // full height (Y1=0, Y2=160)

        // Low range (0..50): should be INSIDE window.
        assert!(ppu.pixel_in_window(0, 0, 80)); // x=0 is in 0..50
        assert!(ppu.pixel_in_window(0, 30, 80)); // x=30 is in 0..50
        assert!(ppu.pixel_in_window(0, 49, 80)); // x=49 is last pixel of low range

        // Gap (50..200): should be OUTSIDE window.
        assert!(!ppu.pixel_in_window(0, 50, 80)); // x=50 is the start of the gap
        assert!(!ppu.pixel_in_window(0, 100, 80)); // x=100 is in gap
        assert!(!ppu.pixel_in_window(0, 199, 80)); // x=199 is last pixel of gap

        // High range (200..240): should be INSIDE window.
        assert!(ppu.pixel_in_window(0, 200, 80)); // x=200 is start of high range
        assert!(ppu.pixel_in_window(0, 239, 80)); // x=239 is last pixel
    }

    #[test]
    fn pixel_in_window_wraparound_y_low_range_is_inside() {
        let mut ppu = Ppu::new();
        // Y1=120, Y2=50: Y1 > Y2, so window wraps around.
        // Per hardware: covers 0..50 AND 120..160.
        ppu.write_win_h(0, 240); // full width (X1=0, X2=240)
        ppu.write_win_v(0, (120 << 8) | 50);

        // Low range (0..50): should be INSIDE window.
        assert!(ppu.pixel_in_window(0, 100, 0)); // y=0 is in 0..50
        assert!(ppu.pixel_in_window(0, 100, 30)); // y=30 is in 0..50
        assert!(ppu.pixel_in_window(0, 100, 49)); // y=49 is last pixel of low range

        // Gap (50..120): should be OUTSIDE window.
        assert!(!ppu.pixel_in_window(0, 100, 50)); // y=50 is start of gap
        assert!(!ppu.pixel_in_window(0, 100, 60)); // y=60 is in gap
        assert!(!ppu.pixel_in_window(0, 100, 119)); // y=119 is last pixel of gap

        // High range (120..160): should be INSIDE window.
        assert!(ppu.pixel_in_window(0, 100, 120)); // y=120 is start of high range
        assert!(ppu.pixel_in_window(0, 100, 159)); // y=159 is last pixel
    }

    #[test]
    fn window_layer_mask_win0_takes_priority() {
        let mut ppu = Ppu::new();
        ppu.write_dispcnt(dispcnt::WIN0_ENABLE | dispcnt::WIN1_ENABLE | dispcnt::BG0_ENABLE);
        // Win0: full screen. Win1: full screen.
        ppu.write_win_h(0, 240); // X1=0, X2=240
        ppu.write_win_v(0, 160); // Y1=0, Y2=160
        ppu.write_win_h(1, 240); // X1=0, X2=240
        ppu.write_win_v(1, 160); // Y1=0, Y2=160
        // Win0 enables BG0 only (0x01). Win1 enables BG1 only (0x02).
        ppu.write_winin(0x0201); // low byte = Win0, high byte = Win1
        // Pixel should use Win0's mask (0x01) since it has priority.
        let mask = ppu.window_layer_mask(100, 80, None);
        assert_eq!(mask, 0x01);
    }

    #[test]
    fn window_layer_mask_outside_when_not_in_any_window() {
        let mut ppu = Ppu::new();
        ppu.write_dispcnt(dispcnt::WIN0_ENABLE | dispcnt::BG0_ENABLE);
        // Win0 is a small rect: X=10..50, Y=10..50.
        ppu.write_win_h(0, (10 << 8) | 50);
        ppu.write_win_v(0, (10 << 8) | 50);
        ppu.write_winin(0x003F); // inside Win0: all enabled
        ppu.write_winout(0x0001); // outside: only BG0
        // Pixel at (100, 80) is outside Win0.
        let mask = ppu.window_layer_mask(100, 80, None);
        assert_eq!(mask, 0x01);
    }

    #[test]
    fn window_obj_window_uses_winout_high_byte() {
        let mut ppu = Ppu::new();
        ppu.write_dispcnt(dispcnt::OBJ_WIN_ENABLE | dispcnt::OBJ_ENABLE);
        ppu.write_winout(0x3F00); // Outside=0x00, OBJ Window=0x3F
        // Create a fake OBJ scanline with obj_window set at pixel 50.
        let mut obj_scanline = obj::ObjScanline::default();
        obj_scanline.obj_window[50] = true;

        // Pixel 50 is inside OBJ window.
        let mask = ppu.window_layer_mask(50, 0, Some(&obj_scanline));
        assert_eq!(mask, 0x3F);

        // Pixel 51 is outside all windows.
        let mask = ppu.window_layer_mask(51, 0, Some(&obj_scanline));
        assert_eq!(mask, 0x00);
    }

    #[test]
    fn composite_bios_like_obj_window_reveals_bg3() {
        // Simulate the BIOS boot: DISPCNT=0x9842 (OBJ_WIN + OBJ + BG3 + Mode 2 + 1D mapping)
        // WINOUT=0x3F27: Outside = 0x27 (BG0+BG1+BG2+OBJ, no BG3)
        //                OBJ Win = 0x3F (all enabled including BG3)
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        // DISPCNT: Mode 2 + BG3 + OBJ + OBJ_WIN_ENABLE + 1D OBJ mapping.
        ppu.write_dispcnt(
            0x0002
                | dispcnt::BG3_ENABLE
                | dispcnt::OBJ_ENABLE
                | dispcnt::OBJ_WIN_ENABLE
                | dispcnt::OBJ_MAPPING_1D,
        );
        ppu.write_winout(0x3F27);

        // BG3 palette: index 1 = green (BGR555 = 0x03E0).
        pram[2] = 0xE0;
        pram[3] = 0x03;

        // BG3: screenblock 16, charblock 0, size 0 (16x16 tiles).
        ppu.write_bg_cnt(3, 16 << 8);
        let sb_base = 16 * 0x800;
        vram[sb_base] = 1; // tile index 1 at map (0,0)
        // Tile 1 at charblock 0: 8bpp affine, 64 bytes/tile.
        for byte in &mut vram[64..128] {
            *byte = 1; // palette index 1
        }

        // BG3 affine: identity transform.
        ppu.write_affine(0x0400_0030, 0x0100); // PA = 1.0
        ppu.write_affine(0x0400_0036, 0x0100); // PD = 1.0

        // OBJ at position (0,0) in OBJ Window mode (mode 2), 8x8.
        let attr0: u16 = 2 << 10; // y=0, mode=2, shape=square
        let attr1: u16 = 0; // x=0, size=0 → 8x8
        let attr2: u16 = 1; // tile 1
        oam[0] = attr0 as u8;
        oam[1] = (attr0 >> 8) as u8;
        oam[2] = attr1 as u8;
        oam[3] = (attr1 >> 8) as u8;
        oam[4] = attr2 as u8;
        oam[5] = (attr2 >> 8) as u8;

        // OBJ tile 1 data (4bpp, 1D): at VRAM 0x10000 + 1*32 = 0x10020.
        for byte in &mut vram[0x10020..0x10040] {
            *byte = 0x11; // palette index 1 for both nibbles
        }
        // OBJ palette at pram[0x200+].
        pram[0x200 + 2] = 0x1F;
        pram[0x200 + 3] = 0x00;

        // Run one full frame to latch affine reference points at VBlank.
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        // Now at scanline 0, step one scanline to render it fresh.
        ppu.step(CYCLES_PER_SCANLINE, &mut ic, &vram, &pram, &oam);

        let fb = ppu.framebuffer();
        // Pixel at x=4 (inside OBJ window, inside BG3 tile) should show green.
        let px4 = &fb[4 * 3..4 * 3 + 3];
        let green_rgb = color::bgr555_to_rgb888(0x03E0);
        assert_eq!(
            px4,
            &[green_rgb.0, green_rgb.1, green_rgb.2],
            "Pixel 4 inside OBJ window should show BG3 (green)"
        );

        // Pixel at x=60 (outside OBJ window) should show backdrop (black)
        // because outside mask 0x27 has BG3 disabled (bit 3 = 0).
        let px60 = &fb[60 * 3..60 * 3 + 3];
        assert_eq!(
            px60,
            &[0, 0, 0],
            "Pixel 60 outside OBJ window should show backdrop (BG3 disabled)"
        );
    }

    // ── Green Swap (0x0400_0002) ──────────────────────────────────────────────

    #[test]
    fn green_swap_defaults_to_off() {
        let ppu = Ppu::new();
        assert_eq!(ppu.read_green_swap(), 0);
    }

    #[test]
    fn green_swap_register_round_trips() {
        let mut ppu = Ppu::new();
        ppu.write_green_swap(1);
        assert_eq!(ppu.read_green_swap(), 1);
        ppu.write_green_swap(0);
        assert_eq!(ppu.read_green_swap(), 0);
    }

    #[test]
    fn green_swap_only_bit0_is_stored() {
        let mut ppu = Ppu::new();
        // Writing any value with bit 0 set should return 1.
        ppu.write_green_swap(0xFFFF);
        assert_eq!(ppu.read_green_swap(), 1);
        // Writing a value with bit 0 clear should return 0.
        ppu.write_green_swap(0xFFFE);
        assert_eq!(ppu.read_green_swap(), 0);
    }

    #[test]
    fn green_swap_swaps_green_channel_between_adjacent_pixels() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram(); // backdrop = black
        let oam = make_oam();

        // Mode 3 (direct color bitmap), BG2 enabled.
        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        // Enable green swap.
        ppu.write_green_swap(1);
        // Mode 3 uses affine — set identity matrix so rendering is linear.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        // Pixel 0 (even): R=1, G=4, B=0 → BGR555 = (4<<5)|1 = 0x0081
        let color_a: u16 = (4u16 << 5) | 1;
        // Pixel 1 (odd):  R=2, G=8, B=0 → BGR555 = (8<<5)|2 = 0x0102
        let color_b: u16 = (8u16 << 5) | 2;
        vram[0] = color_a as u8;
        vram[1] = (color_a >> 8) as u8;
        vram[2] = color_b as u8;
        vram[3] = (color_b >> 8) as u8;

        step_and_render_scanline0(&mut ppu, &mut ic, &vram, &pram, &oam);

        let fb = ppu.framebuffer();
        let (r_a, g_a, b_a) = color::bgr555_to_rgb888(color_a);
        let (r_b, g_b, b_b) = color::bgr555_to_rgb888(color_b);

        // After green swap pixel 0 keeps R/B from color_a but gets G from pixel 1.
        assert_eq!(fb[0], r_a, "pixel 0 R unchanged");
        assert_eq!(fb[1], g_b, "pixel 0 G swapped from pixel 1");
        assert_eq!(fb[2], b_a, "pixel 0 B unchanged");

        // After green swap pixel 1 keeps R/B from color_b but gets G from pixel 0.
        assert_eq!(fb[3], r_b, "pixel 1 R unchanged");
        assert_eq!(fb[4], g_a, "pixel 1 G swapped from pixel 0");
        assert_eq!(fb[5], b_b, "pixel 1 B unchanged");
    }

    #[test]
    fn green_swap_disabled_leaves_pixels_unchanged() {
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let pram = make_pram();
        let oam = make_oam();

        ppu.write_dispcnt(3 | dispcnt::BG2_ENABLE);
        // Green swap disabled (default).
        // Mode 3 uses affine — set identity matrix so rendering is linear.
        ppu.write_affine(REG_BG2PA, 0x0100);
        ppu.write_affine(REG_BG2PD, 0x0100);

        let color_a: u16 = (4u16 << 5) | 1;
        let color_b: u16 = (8u16 << 5) | 2;
        vram[0] = color_a as u8;
        vram[1] = (color_a >> 8) as u8;
        vram[2] = color_b as u8;
        vram[3] = (color_b >> 8) as u8;

        step_and_render_scanline0(&mut ppu, &mut ic, &vram, &pram, &oam);

        let fb = ppu.framebuffer();
        let (r_a, g_a, b_a) = color::bgr555_to_rgb888(color_a);
        let (r_b, g_b, b_b) = color::bgr555_to_rgb888(color_b);

        // No swap: pixels retain their own channels.
        assert_eq!(
            &fb[0..3],
            &[r_a, g_a, b_a],
            "pixel 0 unchanged without swap"
        );
        assert_eq!(
            &fb[3..6],
            &[r_b, g_b, b_b],
            "pixel 1 unchanged without swap"
        );
    }
    // ---- BLDCNT / BLDALPHA / BLDY register tests ----------------------------

    #[test]
    fn bldcnt_write_masks_high_bits() {
        let mut ppu = Ppu::new();
        ppu.write_bldcnt(0xFFFF);
        assert_eq!(
            ppu.read_bldcnt(),
            0x3FFF,
            "BLDCNT: bits 14-15 must be discarded"
        );
    }

    #[test]
    fn bldcnt_initial_value_is_zero() {
        let ppu = Ppu::new();
        assert_eq!(ppu.read_bldcnt(), 0);
    }

    #[test]
    fn bldalpha_write_masks_unused_bits() {
        let mut ppu = Ppu::new();
        ppu.write_bldalpha(0xFFFF);
        assert_eq!(
            ppu.read_bldalpha(),
            0x1F1F,
            "BLDALPHA: only bits 0-4 and 8-12 are valid"
        );
    }

    #[test]
    fn bldalpha_initial_value_is_zero() {
        let ppu = Ppu::new();
        assert_eq!(ppu.read_bldalpha(), 0);
    }

    #[test]
    fn bldy_write_does_not_panic() {
        // BLDY is write-only; we just verify writes are accepted without panic.
        let mut ppu = Ppu::new();
        ppu.write_bldy(0x10);
        ppu.write_bldy(0xFF); // 0xFF masked to 0x1F internally
    }

    // ---- Color-effect rendering helpers -------------------------------------

    fn run_one_frame(
        ppu: &mut Ppu,
        ic: &mut InterruptController,
        vram: &[u8],
        pram: &[u8],
        oam: &[u8],
    ) {
        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            ic,
            vram,
            pram,
            oam,
        );
    }

    // ---- Brightness increase (mode 2) tests ---------------------------------

    #[test]
    fn brightness_increase_mode2_full_evy_makes_pixel_white() {
        // BG0 pixel at (0,0) is blue.  With mode 2 (brighten), 1st target = BG0,
        // and EVY = 16 (full), each channel is driven to max → white output.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);

        // BG palette entry 1 = pure blue (BGR555 0x7C00).
        pram[2] = 0x00;
        pram[3] = 0x7C;
        vram[32] = 0x11; // tile 1, pixel 0 = palette index 1
        vram[0x0000] = 0x01; // screenblock 0 entry (0,0) = tile 1

        // BLDCNT: effect mode = 2 (brighten, bits 6-7 = 10b), 1st target = BG0 (bit 0).
        ppu.write_bldcnt((2 << 6) | 0x01);
        // BLDY: EVY = 16 (full brightening).
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        // After full brightening all channels reach max (31) → white.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0xFF, 0xFF, 0xFF],
            "Full brightness increase should output white"
        );
    }

    #[test]
    fn brightness_increase_no_effect_when_first_target_not_selected() {
        // Same setup but 1st target does NOT include BG0 → no effect, blue remains.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);
        pram[2] = 0x00;
        pram[3] = 0x7C; // palette 1 = blue
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        // BLDCNT: mode=2 but 1st target bits 0-5 = 0 (nothing selected).
        ppu.write_bldcnt(2 << 6);
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        let (r, g, b) = color::bgr555_to_rgb888(0x7C00);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "No effect when 1st target does not include the top pixel layer"
        );
    }

    // ---- Brightness decrease (mode 3) tests ---------------------------------

    #[test]
    fn brightness_decrease_mode3_full_evy_makes_pixel_black() {
        // BG0 pixel = white.  mode 3 (darken) with EVY=16 → each channel drops to 0.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);

        // BG palette entry 1 = white (BGR555 0x7FFF).
        pram[2] = 0xFF;
        pram[3] = 0x7F;
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        // BLDCNT: mode=3 (bits 6-7 = 11b → 0xC0), 1st target = BG0 (bit 0).
        ppu.write_bldcnt((3 << 6) | 0x01);
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0, 0],
            "Full brightness decrease should output black"
        );
    }

    // ---- Alpha blend (mode 1) tests -----------------------------------------

    #[test]
    fn alpha_blend_mode1_bg_over_backdrop() {
        // BG0 (red) on top of backdrop (blue) with EVA=EVB=8 → 50/50 mix.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);

        // Backdrop = pure blue (BGR555 0x7C00).
        pram[0] = 0x00;
        pram[1] = 0x7C;
        // BG palette entry 1 = pure red (BGR555 0x001F).
        pram[2] = 0x1F;
        pram[3] = 0x00;
        vram[32] = 0x11; // tile 1, pixel 0 = index 1 (red)
        vram[0x0000] = 0x01;

        // BLDCNT: mode=1, 1st target = BG0 (bit 0), 2nd target = backdrop (bit 13).
        ppu.write_bldcnt((1 << 6) | 0x01 | (1 << 13));
        // BLDALPHA: EVA=8, EVB=8 (50% / 50%).
        ppu.write_bldalpha(8 | (8 << 8));

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        // Independent calculation for 50% red (0x001F) + 50% blue (0x7C00):
        //   R: (31 * 8 + 0 * 8) >> 4 = 248 >> 4 = 15
        //   G: (0  * 8 + 0 * 8) >> 4 = 0
        //   B: (0  * 8 + 31 * 8) >> 4 = 248 >> 4 = 15
        // BGR555 = 15 | (0 << 5) | (15 << 10) = 0x3C0F
        // RGB888: expand5(15) = (15<<3)|(15>>2) = 120|3 = 123 = 0x7B
        let (r, g, b) = color::bgr555_to_rgb888(0x3C0F);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "50/50 alpha blend of red BG0 over blue backdrop"
        );
    }

    #[test]
    fn alpha_blend_no_effect_when_second_target_not_selected() {
        // BG0 (red) over backdrop (blue), mode=1, but 2nd target bits = 0.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);
        pram[0] = 0x00;
        pram[1] = 0x7C; // backdrop = blue
        pram[2] = 0x1F;
        pram[3] = 0x00; // palette 1 = red
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        // BLDCNT: mode=1, 1st target = BG0, 2nd target = none.
        ppu.write_bldcnt((1 << 6) | 0x01);
        ppu.write_bldalpha(8 | (8 << 8));

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        // No blend → BG0 red shows as-is.
        let (r, g, b) = color::bgr555_to_rgb888(0x001F);
        assert_eq!(&ppu.framebuffer()[0..3], &[r, g, b]);
    }

    #[test]
    fn no_effect_when_bldcnt_mode_is_zero() {
        // Mode 0 = no effect even if 1st target includes all layers.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE);
        pram[2] = 0x00;
        pram[3] = 0x7C; // palette 1 = blue
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        // BLDCNT: mode=0, 1st target = all.
        ppu.write_bldcnt(0x003F);
        ppu.write_bldy(16); // would darken if mode were 3

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        // Mode 0 → no effect, pixel stays blue.
        let (r, g, b) = color::bgr555_to_rgb888(0x7C00);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "Mode 0 must not apply any color effect"
        );
    }

    // ---- Semi-transparent OBJ tests -----------------------------------------

    /// Set up a semi-transparent OBJ (gfx_mode=1) in OAM.
    fn setup_semi_transparent_obj_in_oam(
        oam: &mut [u8],
        obj_idx: usize,
        x: u16,
        y: u8,
        tile: u16,
        priority: u8,
    ) {
        let base = obj_idx * 8;
        // attr0: obj_mode=0 (normal), gfx_mode=1 (semi-transparent, bits 10-11 = 01 → 0x0400).
        let attr0 = y as u16 | 0x0400;
        let attr1 = x & 0x1FF;
        let attr2 = (tile & 0x3FF) | ((priority as u16 & 3) << 10);
        oam[base] = attr0 as u8;
        oam[base + 1] = (attr0 >> 8) as u8;
        oam[base + 2] = attr1 as u8;
        oam[base + 3] = (attr1 >> 8) as u8;
        oam[base + 4] = attr2 as u8;
        oam[base + 5] = (attr2 >> 8) as u8;
    }

    #[test]
    fn semi_transparent_obj_alpha_blends_with_bg_below() {
        // Semi-transparent OBJ (red) on top of BG0 (blue) with EVA=EVB=8.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);

        // BG0: priority 1 (below OBJ priority 0).
        ppu.write_bg_cnt(0, 1);

        // BG palette entry 1 = blue (BGR555 0x7C00).
        pram[2] = 0x00;
        pram[3] = 0x7C;
        vram[32] = 0x11; // tile 1, pixel 0 = index 1
        vram[0x0000] = 0x01;

        // OBJ tile 1 at OBJ VRAM base.
        let obj_tile_base = 0x1_0000 + 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11;
        }
        // OBJ palette entry 1 = red (BGR555 0x001F).
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00;

        // Semi-transparent OBJ at (0,0), priority 0.
        setup_semi_transparent_obj_in_oam(&mut oam, 0, 0, 0, 1, 0);

        // BLDCNT: 2nd target = BG0 (bit 8).  Mode bits irrelevant for semi-t OBJ.
        ppu.write_bldcnt(1 << 8);
        // BLDALPHA: EVA=8 (OBJ), EVB=8 (BG0), 50%/50%.
        ppu.write_bldalpha(8 | (8 << 8));

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &oam);

        // Independent calculation for 50% red OBJ (0x001F) + 50% blue BG (0x7C00):
        //   R: (31*8 + 0*8) >> 4 = 15,  G: 0,  B: (0*8 + 31*8) >> 4 = 15
        //   BGR555 = 0x3C0F → RGB888: expand5(15) = 0x7B
        let (r, g, b) = color::bgr555_to_rgb888(0x3C0F);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "Semi-transparent OBJ should alpha blend with BG below"
        );
    }

    #[test]
    fn semi_transparent_obj_shows_normal_when_no_second_target() {
        // Semi-transparent OBJ on top of BG0, but BG0 is not selected as 2nd target.
        // OBJ should display at normal (full) intensity.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);
        ppu.write_bg_cnt(0, 1); // BG0 priority 1

        pram[2] = 0x00;
        pram[3] = 0x7C; // palette 1 = blue
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        let obj_tile_base = 0x1_0000 + 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11;
        }
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00; // OBJ palette 1 = red

        setup_semi_transparent_obj_in_oam(&mut oam, 0, 0, 0, 1, 0);

        // BLDCNT: 2nd target = 0 (nothing selected).
        ppu.write_bldcnt(0);
        ppu.write_bldalpha(8 | (8 << 8));

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &oam);

        // No valid 2nd target → OBJ at normal intensity (red).
        let (r, g, b) = color::bgr555_to_rgb888(0x001F);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "Semi-transparent OBJ with no 2nd target should display at normal intensity"
        );
    }

    // Per GBATek: when a semi-transparent OBJ pixel is present at a position
    // AND any layer is set as a 2nd target, brightness effects are suppressed
    // for that pixel position, regardless of whether the OBJ is the topmost
    // layer.

    #[test]
    fn semi_transparent_obj_on_top_in_brightness_mode_alpha_blends_when_second_target_exists() {
        // Semi-transparent OBJ (red, priority 0) on top of BG0 (blue, priority 1).
        // BLDCNT mode=2 (brightness increase), 1st target=OBJ, 2nd target=BG0.
        // Expected: alpha blend (semi-transparent OBJ forces alpha blend regardless of mode).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);
        ppu.write_bg_cnt(0, 1); // BG0 priority 1 (below OBJ priority 0)

        pram[2] = 0x00;
        pram[3] = 0x7C; // palette 1 = blue
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        let obj_tile_base = 0x1_0000 + 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11;
        }
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00; // OBJ palette 1 = red

        setup_semi_transparent_obj_in_oam(&mut oam, 0, 0, 0, 1, 0); // obj_idx=0, x=0, y=0, tile=1, priority=0

        // BLDCNT: mode=2 (brightness), 1st target=OBJ (bit 4), 2nd target=BG0 (bit 8).
        ppu.write_bldcnt((2 << 6) | 0x10 | (1 << 8));
        ppu.write_bldalpha(8 | (8 << 8)); // EVA=8, EVB=8
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &oam);

        // Semi-transparent OBJ forces alpha blend even in brightness mode.
        // 50% red + 50% blue = 0x3C0F.
        let (r, g, b) = color::bgr555_to_rgb888(0x3C0F);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "Semi-transparent OBJ on top in brightness mode must alpha-blend with 2nd target"
        );
    }

    #[test]
    fn semi_transparent_obj_on_top_in_brightness_mode_shows_normal_when_no_second_target() {
        // Semi-transparent OBJ (red, priority 0) on top of BG0 (blue, priority 1).
        // BLDCNT mode=2 (brightness), 1st target=OBJ, 2nd target=none.
        // Expected: OBJ at normal red (no brightness applied to semi-transparent OBJ).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);
        ppu.write_bg_cnt(0, 1); // BG0 priority 1

        pram[2] = 0x00;
        pram[3] = 0x7C;
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        let obj_tile_base = 0x1_0000 + 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11;
        }
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00;

        setup_semi_transparent_obj_in_oam(&mut oam, 0, 0, 0, 1, 0); // obj_idx=0, x=0, y=0, tile=1, priority=0

        // BLDCNT: mode=2 (brightness), 1st target=OBJ (bit 4), no 2nd target.
        ppu.write_bldcnt((2 << 6) | 0x10);
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &oam);

        // No 2nd target → OBJ at normal intensity (not brightened).
        let (r, g, b) = color::bgr555_to_rgb888(0x001F);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "Semi-transparent OBJ on top in brightness mode with no 2nd target must show normally"
        );
    }

    #[test]
    fn semi_transparent_obj_below_top_layer_suppresses_brightness_increase() {
        // BG0 (blue, priority 0) is topmost, semi-transparent OBJ (red, priority 1)
        // is below. BLDCNT mode=2 (brightness increase), 1st target=BG0, 2nd
        // target=backdrop. Per GBATek, the semi-transparent OBJ suppresses
        // brightness because a 2nd target is configured.
        // Expected: BG0 shows at normal blue (NOT brightened to white).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);
        ppu.write_bg_cnt(0, 0); // BG0 priority 0 (top)

        pram[2] = 0x00;
        pram[3] = 0x7C; // palette 1 = blue (BGR555 0x7C00)
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        let obj_tile_base = 0x1_0000 + 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11;
        }
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00; // OBJ palette 1 = red

        // OBJ priority 1 → below BG0 priority 0.
        setup_semi_transparent_obj_in_oam(&mut oam, 0, 0, 0, 1, 1); // obj_idx=0, x=0, y=0, tile=1, priority=1

        // BLDCNT: mode=2, 1st target=BG0 (bit 0), 2nd target=backdrop (bit 13).
        ppu.write_bldcnt((2 << 6) | 0x01 | (1 << 13));
        ppu.write_bldy(16); // max brightness (would give white if applied)

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &oam);

        // Semi-transparent OBJ below + 2nd target present → brightness suppressed.
        // BG0 must appear at normal blue, not white.
        let (r, g, b) = color::bgr555_to_rgb888(0x7C00);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "Brightness increase must be suppressed when semi-transparent OBJ is below and 2nd target exists"
        );
    }

    #[test]
    fn semi_transparent_obj_below_top_layer_suppresses_brightness_decrease() {
        // Same setup as above but BLDCNT mode=3 (brightness decrease).
        // Expected: BG0 shows at normal blue (NOT darkened to black).
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);
        ppu.write_bg_cnt(0, 0); // BG0 priority 0 (top)

        pram[2] = 0x00;
        pram[3] = 0x7C; // blue
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        let obj_tile_base = 0x1_0000 + 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11;
        }
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00;

        setup_semi_transparent_obj_in_oam(&mut oam, 0, 0, 0, 1, 1); // obj_idx=0, x=0, y=0, tile=1, priority=1

        // BLDCNT: mode=3, 1st target=BG0 (bit 0), 2nd target=backdrop (bit 13).
        ppu.write_bldcnt((3 << 6) | 0x01 | (1 << 13));
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &oam);

        let (r, g, b) = color::bgr555_to_rgb888(0x7C00);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "Brightness decrease must be suppressed when semi-transparent OBJ is below and 2nd target exists"
        );
    }

    #[test]
    fn semi_transparent_obj_below_does_not_suppress_brightness_when_no_second_target() {
        // BG0 (blue, priority 0) on top, semi-transparent OBJ (red, priority 1) below.
        // BLDCNT mode=2 (brightness), 1st target=BG0, but NO 2nd target configured.
        // Without any 2nd target, brightness suppression does NOT apply (per GBATek).
        // Expected: BG0 brightened to white.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);
        ppu.write_bg_cnt(0, 0);

        pram[2] = 0x00;
        pram[3] = 0x7C;
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        let obj_tile_base = 0x1_0000 + 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11;
        }
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00;

        setup_semi_transparent_obj_in_oam(&mut oam, 0, 0, 0, 1, 1); // obj_idx=0, x=0, y=0, tile=1, priority=1

        // BLDCNT: mode=2, 1st target=BG0 (bit 0), NO 2nd target.
        ppu.write_bldcnt((2 << 6) | 0x01);
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &oam);

        // No 2nd target → brightness applies normally → white.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0xFF, 0xFF, 0xFF],
            "Brightness must apply normally when no 2nd target is configured"
        );
    }

    #[test]
    fn window_masked_obj_does_not_suppress_brightness() {
        // BG0 (blue, priority 0) is topmost; semi-transparent OBJ (red, priority 1)
        // overlaps but is hidden by WIN0 mask (OBJ bit disabled while BG0+SFX enabled).
        // BLDCNT mode=2, 1st target=BG0, 2nd target=backdrop.
        // Expected: brightness applies to BG0 (white), because OBJ is not visible.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        ppu.write_dispcnt(
            dispcnt::BG0_ENABLE
                | dispcnt::OBJ_ENABLE
                | dispcnt::OBJ_MAPPING_1D
                | dispcnt::WIN0_ENABLE,
        );
        ppu.write_bg_cnt(0, 0); // BG0 priority 0 (top)

        // BG0 tile/palette setup: blue pixel at (0,0).
        pram[2] = 0x00;
        pram[3] = 0x7C; // palette 1 = blue (BGR555 0x7C00)
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        // Semi-transparent OBJ pixel at (0,0), priority 1 (below BG0).
        let obj_tile_base = 0x1_0000 + 32;
        for byte in &mut vram[obj_tile_base..obj_tile_base + 32] {
            *byte = 0x11;
        }
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00; // OBJ palette 1 = red
        setup_semi_transparent_obj_in_oam(&mut oam, 0, 0, 0, 1, 1);

        // WIN0 covers full visible area: enable BG0 (bit 0) + SFX (bit 5),
        // but disable OBJ (bit 4 = 0).
        ppu.write_win_h(0, 240);
        ppu.write_win_v(0, 160);
        ppu.write_winin(0x0021);

        // BLDCNT: mode=2, 1st target=BG0 (bit 0), 2nd target=backdrop (bit 13).
        ppu.write_bldcnt((2 << 6) | 0x01 | (1 << 13));
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &oam);

        // OBJ is masked out by WIN0, so it must not suppress brightness.
        // BG0 should brighten from blue to white.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0xFF, 0xFF, 0xFF],
            "Window-masked OBJ must not suppress brightness"
        );
    }

    // ---- Window SFX bit tests -----------------------------------------------

    #[test]
    fn window_sfx_bit_zero_disables_color_effects() {
        // Window 0 covers the whole screen with SFX bit = 0.
        // Even though BLDCNT mode=2 and EVY=16, no effect should apply.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::WIN0_ENABLE);

        // BG palette entry 1 = blue.
        pram[2] = 0x00;
        pram[3] = 0x7C;
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        // WIN0 covers the entire visible area.
        ppu.write_win_h(0, 240);
        ppu.write_win_v(0, 160);
        // WININ: BG0 enabled (bit 0 = 1), SFX disabled (bit 5 = 0) → 0x0001.
        ppu.write_winin(0x0001);

        // BLDCNT: mode=2, 1st target = BG0 (full brightening would give white).
        ppu.write_bldcnt((2 << 6) | 0x01);
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        // SFX disabled by window → pixel stays blue.
        let (r, g, b) = color::bgr555_to_rgb888(0x7C00);
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[r, g, b],
            "Window SFX bit=0 should disable color effects"
        );
    }

    #[test]
    fn window_sfx_bit_one_enables_color_effects() {
        // Window 0 with SFX bit = 1 → brightness increase should apply.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(dispcnt::BG0_ENABLE | dispcnt::WIN0_ENABLE);

        pram[2] = 0x00;
        pram[3] = 0x7C; // palette 1 = blue
        vram[32] = 0x11;
        vram[0x0000] = 0x01;

        ppu.write_win_h(0, 240);
        ppu.write_win_v(0, 160);
        // WININ: BG0 enabled (bit 0 = 1) AND SFX enabled (bit 5 = 1) → 0x0021.
        ppu.write_winin(0x0021);

        ppu.write_bldcnt((2 << 6) | 0x01);
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        // SFX enabled → blue becomes white.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0xFF, 0xFF, 0xFF],
            "Window SFX bit=1 should enable color effects"
        );
    }

    // ---- Backdrop brightness effect tests -----------------------------------

    #[test]
    fn backdrop_brightness_increase_applies_when_all_layers_disabled() {
        // When no BG/OBJ layers are enabled, render_backdrop_scanline handles
        // the scanline.  BLDCNT mode=2 with backdrop as 1st target (bit 5)
        // and EVY=16 should brighten the backdrop to white.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let mut pram = make_pram();

        // No BGs or OBJ enabled → pure backdrop render path.
        ppu.write_dispcnt(0);

        // Backdrop = pure blue.
        pram[0] = 0x00;
        pram[1] = 0x7C;

        // BLDCNT: mode=2 (brighten), 1st target = backdrop (bit 5 = 0x20).
        ppu.write_bldcnt((2 << 6) | (1 << 5));
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0xFF, 0xFF, 0xFF],
            "Backdrop brightness increase with EVY=16 should output white"
        );
    }

    #[test]
    fn backdrop_brightness_decrease_applies_when_all_layers_disabled() {
        // BLDCNT mode=3 with backdrop as 1st target and EVY=16 → black.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let mut pram = make_pram();

        ppu.write_dispcnt(0);

        // Backdrop = white.
        pram[0] = 0xFF;
        pram[1] = 0x7F;

        ppu.write_bldcnt((3 << 6) | (1 << 5));
        ppu.write_bldy(16);

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0, 0, 0],
            "Backdrop brightness decrease with EVY=16 should output black"
        );
    }

    #[test]
    fn backdrop_sfx_applies_per_pixel_when_window_active_all_layers_disabled() {
        // Per GBATek 'LCD I/O Window Feature': WININ/WINOUT bit 5 gates SFX
        // per pixel, including for the backdrop.  When all BG layers and OBJ are
        // disabled (pure backdrop scanline), windows must still control SFX
        // per-pixel — pixels inside WIN0 with SFX=1 get the brightness effect;
        // pixels outside WIN0 (SFX=0) receive the raw backdrop color.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let mut pram = make_pram();

        // Mode 0, no BG/OBJ layers, WIN0 enabled.
        ppu.write_dispcnt(dispcnt::WIN0_ENABLE);

        // Backdrop = pure blue (BGR555 0x7C00).
        pram[0] = 0x00;
        pram[1] = 0x7C;

        // WIN0 covers x=0..120, full vertical.
        // WIN_H: X1=0 (bits 15-8), X2=120 (bits 7-0) → value = (0 << 8) | 120
        ppu.write_win_h(0, 120);
        // WIN_V: Y1=0 (bits 15-8), Y2=160 (bits 7-0) → value = (0 << 8) | 160
        ppu.write_win_v(0, 160);

        // WININ: WIN0 SFX bit enabled (bit 5 = 1), no BG/OBJ bits needed.
        ppu.write_winin(1 << 5); // 0x0020
        // WINOUT: SFX disabled outside any window (bit 5 = 0).
        ppu.write_winout(0);

        // BLDCNT: mode=2 (brighten), 1st target = backdrop (bit 5).
        ppu.write_bldcnt((2 << 6) | (1 << 5));
        ppu.write_bldy(16); // EVY=16 → maximum brightening → white

        run_one_frame(&mut ppu, &mut ic, &vram, &pram, &make_oam());

        // Pixel at x=0 is inside WIN0 (SFX=1): backdrop should be brightened to white.
        assert_eq!(
            &ppu.framebuffer()[0..3],
            &[0xFF, 0xFF, 0xFF],
            "Backdrop inside WIN0 with SFX=1 and EVY=16 should be white"
        );

        // Pixel at x=120 is outside WIN0 (SFX=0): raw backdrop (blue).
        let (r, g, b) = color::bgr555_to_rgb888(0x7C00);
        let pixel_start = 120 * BYTES_PER_PIXEL;
        assert_eq!(
            &ppu.framebuffer()[pixel_start..pixel_start + 3],
            &[r, g, b],
            "Backdrop outside WIN0 with SFX=0 should remain raw blue"
        );
    }

    // ---- BLDY accessor correctness ------------------------------------------

    #[test]
    fn bldy_read_bldy_returns_live_evy() {
        // read_bldy() returns the current EVY value stored in the PPU, which
        // is used by IoRegisters::write8 to merge byte writes to 0x0400_0055
        // against live state rather than open-bus zero.
        let mut ppu = Ppu::new();
        ppu.write_bldy(12);
        assert_eq!(ppu.read_bldy(), 12, "read_bldy should return the live EVY");
        // write_bldy clamps to 5 bits; read_bldy must reflect the clamped value.
        ppu.write_bldy(0x1F); // max
        assert_eq!(ppu.read_bldy(), 0x1F);
        ppu.write_bldy(0xFF); // clamped to 0x1F
        assert_eq!(ppu.read_bldy(), 0x1F);
    }

    /// Helper: fill 32 bytes of OBJ VRAM for tile `tile_id` (4bpp) with palette index `pal_idx`.
    fn fill_obj_tile_4bpp(vram: &mut [u8], tile_id: usize, pal_idx: u8) {
        let base = 0x1_0000 + tile_id * 32;
        let nibble_pair = pal_idx | (pal_idx << 4);
        for b in &mut vram[base..base + 32] {
            *b = nibble_pair;
        }
    }

    #[test]
    fn prohibited_mode_6_obj_renders_when_obj_enabled() {
        // GBA hardware (and mGBA) run OBJ rendering in prohibited modes 6-7.
        // No BG layers are displayed, but OBJs composite over the backdrop.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        // Mode 6, OBJ enabled, 1D mapping.
        ppu.write_dispcnt(6 | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);

        // In bitmap_mode (mode >= 3) tile IDs 0-511 are skipped; use tile 512.
        // Tile 512 lives at OBJ_VRAM_BASE + 512*32 = 0x1_4000.
        setup_obj_in_oam(&mut oam, 0, 100, 0, 512, 0);
        fill_obj_tile_4bpp(&mut vram, 512, 1);

        // OBJ palette color 1 = red (BGR555 0x001F → RGB888 255,0,0).
        pram[0x202] = 0x1F;
        pram[0x203] = 0x00;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        let dst = 100 * BYTES_PER_PIXEL;
        assert_eq!(
            &ppu.framebuffer()[dst..dst + 3],
            &[0xFF, 0, 0],
            "mode 6: OBJ should render red pixel at x=100"
        );
    }

    #[test]
    fn prohibited_mode_7_obj_renders_when_obj_enabled() {
        // Same as mode 6: OBJ must still composite over the backdrop in mode 7.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let mut vram = make_vram();
        let mut pram = make_pram();
        let mut oam = make_oam();

        ppu.write_dispcnt(7 | dispcnt::OBJ_ENABLE | dispcnt::OBJ_MAPPING_1D);

        setup_obj_in_oam(&mut oam, 0, 100, 0, 512, 0);
        fill_obj_tile_4bpp(&mut vram, 512, 1);

        pram[0x202] = 0x1F;
        pram[0x203] = 0x00;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        let dst = 100 * BYTES_PER_PIXEL;
        assert_eq!(
            &ppu.framebuffer()[dst..dst + 3],
            &[0xFF, 0, 0],
            "mode 7: OBJ should render red pixel at x=100"
        );
    }

    #[test]
    fn prohibited_mode_6_backdrop_when_obj_disabled() {
        // With OBJ disabled in mode 6, the whole scanline shows the backdrop.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let mut pram = make_pram();
        let oam = make_oam();

        ppu.write_dispcnt(6); // mode 6, OBJ disabled

        // Backdrop = pure green (BGR555 0x03E0 → RGB888 0,255,0).
        pram[0] = 0xE0;
        pram[1] = 0x03;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        let dst = 0;
        assert_eq!(
            &ppu.framebuffer()[dst..dst + 3],
            &[0, 0xFF, 0],
            "mode 6: backdrop (green) should fill scanline when OBJ disabled"
        );
    }

    #[test]
    fn prohibited_mode_7_backdrop_when_obj_disabled() {
        // With OBJ disabled in mode 7, the whole scanline shows the backdrop.
        let mut ppu = Ppu::new();
        let mut ic = make_ic();
        let vram = make_vram();
        let mut pram = make_pram();
        let oam = make_oam();

        ppu.write_dispcnt(7); // mode 7, OBJ disabled

        // Backdrop = pure green (BGR555 0x03E0 → RGB888 0,255,0).
        pram[0] = 0xE0;
        pram[1] = 0x03;

        ppu.step(
            CYCLES_PER_SCANLINE * SCANLINES_PER_FRAME,
            &mut ic,
            &vram,
            &pram,
            &oam,
        );

        let dst = 0;
        assert_eq!(
            &ppu.framebuffer()[dst..dst + 3],
            &[0, 0xFF, 0],
            "mode 7: backdrop (green) should fill scanline when OBJ disabled"
        );
    }

    // ---- Tests for PPU color correction -----------------------------------

    #[test]
    fn ppu_color_correction_defaults_to_false() {
        let ppu = Ppu::new();
        assert!(
            !ppu.color_correction,
            "color_correction should default to false"
        );
    }

    #[test]
    fn ppu_set_color_correction_enables_it() {
        let mut ppu = Ppu::new();
        ppu.set_color_correction(true);
        assert!(
            ppu.color_correction,
            "set_color_correction(true) should enable it"
        );
    }

    #[test]
    fn ppu_set_color_correction_disables_it() {
        let mut ppu = Ppu::new();
        ppu.set_color_correction(true);
        ppu.set_color_correction(false);
        assert!(
            !ppu.color_correction,
            "set_color_correction(false) should disable it"
        );
    }

    /// When color correction is enabled, mid-range palette colors are rendered
    /// darker than the linear expansion. This tests that the corrected path is
    /// actually taken in render_backdrop_scanline.
    #[test]
    fn ppu_color_correction_darkens_midrange_colors() {
        // Set up a mode-0 backdrop with r5=16, g5=0, b5=0 (medium red).
        // BGR555: r5=16 at bits 0..4 → 0x0010.
        let bgr555_mid_red: u16 = 16; // r5=16
        let vram = make_vram();
        let mut pram = make_pram();
        let oam = make_oam();
        let mut ic = make_ic();
        // Write the backdrop color (palette entry 0) into PRAM.
        pram[0] = bgr555_mid_red as u8;
        pram[1] = (bgr555_mid_red >> 8) as u8;

        // Render without color correction.
        let mut ppu_linear = Ppu::new();
        // Set mode 0 (BG/OBJ disabled → backdrop only).
        ppu_linear.write_dispcnt(0x0000);
        step_and_render_scanline0(&mut ppu_linear, &mut ic, &vram, &pram, &oam);
        let r_linear = ppu_linear.framebuffer()[0];

        // Render with color correction enabled.
        let mut ppu_corrected = Ppu::new();
        ppu_corrected.write_dispcnt(0x0000);
        ppu_corrected.set_color_correction(true);
        let mut ic2 = make_ic();
        step_and_render_scanline0(&mut ppu_corrected, &mut ic2, &vram, &pram, &oam);
        let r_corrected = ppu_corrected.framebuffer()[0];

        assert!(
            r_corrected < r_linear,
            "corrected red ({r_corrected}) should be darker than linear ({r_linear}) for r5=16"
        );
    }
}
