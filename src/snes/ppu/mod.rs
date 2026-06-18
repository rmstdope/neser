//! SNES PPU (Picture Processing Unit) emulation.
//!
//! The PPU is owned by [`crate::snes::bus::SnesSystemBus`], which advances it one master clock
//! per `tick()` and routes the `$2100-$213F` register file plus the PPU-owned CPU I/O ports
//! (`$4200` NMITIMEN, `$4201` WRIO, `$4210` RDNMI, `$4211` TIMEUP, `$4212` HVBJOY) to it.
//!
//! This module is organized into focused submodules:
//! - [`registers`] — register read/write dispatch and VRAM/CGRAM/OAM access.
//! - [`timing`] — dot/scanline counters, VBlank/NMI, and H/V counter latching.
//! - [`framebuffer`] — backdrop rendering and BGR555 -> RGB888 output.
//! - [`save_state`] — PPU save-state capture/restore.

mod background;
mod framebuffer;
mod registers;
mod save_state;
mod timing;

const VRAM_SIZE: usize = 0x10_000;
const CGRAM_SIZE: usize = 0x200;
const OAM_SIZE: usize = 0x220;

/// Visible framebuffer width (NTSC).
pub(super) const SCREEN_WIDTH: usize = 256;
/// Visible framebuffer height (NTSC, 224-line mode).
pub(super) const SCREEN_HEIGHT: usize = 224;
/// First visible dot within a scanline (active display is dots 22..=277).
pub(super) const VISIBLE_DOT_START: u16 = 22;
/// First visible scanline (active display is lines 1..=224).
pub(super) const VISIBLE_LINE_START: u16 = 1;

/// PPU1 (5C77) version number reported in STAT77 ($213E).
pub(super) const PPU1_VERSION: u8 = 1;
/// PPU2 (5C78) version number reported in STAT78 ($213F).
pub(super) const PPU2_VERSION: u8 = 1;
/// CPU (5A22) version number reported in RDNMI ($4210).
pub(super) const CPU_VERSION: u8 = 2;

/// First VBlank scanline (NTSC, 224-line mode): the visible region is lines 1-224.
pub(super) const VBLANK_START_LINE: u16 = 225;
/// Dot at which the HBlank flag (HVBJOY bit 6) goes high (approximate; leading edge is a TODO).
pub(super) const HBLANK_START_DOT: u16 = 274;

/// Master clocks per dot (normal-speed dots).
pub(super) const MASTER_CYCLES_PER_DOT: u32 = 4;
/// Dots per scanline.
pub(super) const DOTS_PER_SCANLINE: u16 = 341;
/// NTSC scanlines per frame.
pub(super) const NTSC_SCANLINES_PER_FRAME: u16 = 262;

/// Current scan position (scanline + dot within the scanline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScanPosition {
    pub scanline: u16,
    pub dot: u16,
}

/// SNES Picture Processing Unit.
#[derive(Debug, Clone)]
pub struct Ppu {
    vram: Vec<u8>,
    cgram: Vec<u8>,
    oam: Vec<u8>,
    position: ScanPosition,
    /// Accumulated master clocks not yet converted into dots.
    master_cycle_accumulator: u32,
    inidisp: u8,
    nmi_enable: bool,
    /// RDNMI ($4210) bit 7: VBlank NMI flag (set at VBlank start, cleared at VBlank end / read).
    nmi_flag: bool,
    /// True during the VBlank period (scanlines >= [`VBLANK_START_LINE`]).
    vblank_active: bool,
    /// Previous level of the NMI line (`nmi_enable && nmi_flag`) for rising-edge detection.
    nmi_line_prev: bool,
    /// Latched NMI rising edge awaiting delivery to the CPU (consumed by `poll_nmi`).
    nmi_edge: bool,
    vram_increment_after_high: bool,
    vram_increment_step: u16,
    vram_address: u16,
    vram_prefetch: u16,
    cgram_address: u16,
    cgram_latch: u8,
    oam_address: u16,
    oam_latch: u8,
    /// Latched horizontal counter (OPHCT, $213C).
    ophct_latch: u16,
    /// Latched vertical counter (OPVCT, $213D).
    opvct_latch: u16,
    /// STAT78 ($213F) bit 6: set when H/V counters are latched, cleared on STAT78 read.
    counter_latch_flag: bool,
    /// OPHCT read-twice flipflop (false = next read is the low byte).
    ophct_read_high: bool,
    /// OPVCT read-twice flipflop (false = next read is the low byte).
    opvct_read_high: bool,
    /// Current WRIO ($4201) value; bit 7 gates counter latching.
    wrio: u8,
    /// STAT78 ($213F) bit 7: interlace field flag.
    interlace_field: bool,
    /// Visible framebuffer in 15-bit BGR555 (converted to RGB888 at snapshot time).
    framebuffer: Vec<u16>,
    /// Set when the PPU enters VBlank (a full visible frame has been produced).
    frame_complete: bool,
    /// BGMODE ($2105) bits 0-2: BG screen mode (0-7).
    bg_mode: u8,
    /// BGMODE ($2105) bit 3: BG3 high-priority option (Mode 1 only).
    bg3_priority: bool,
    /// Per-BG tile size (BGMODE bits 4-7): true = 16x16, false = 8x8.
    bg_tile_size_16: [bool; 4],
    /// Per-BG tilemap base address in VRAM words (BGnSC bits 2-7, 1K-word steps).
    bg_tilemap_base: [u16; 4],
    /// Per-BG tilemap size (BGnSC bits 0-1): 0=32x32,1=64x32,2=32x64,3=64x64.
    bg_screen_size: [u8; 4],
    /// Per-BG character/tile base address in VRAM words (BGxxNBA, 4K-word steps).
    bg_char_base: [u16; 4],
    /// Per-BG horizontal scroll (10-bit), built via the shared BG_old write-twice latch.
    bg_hofs: [u16; 4],
    /// Per-BG vertical scroll (10-bit), built via the shared BG_old write-twice latch.
    bg_vofs: [u16; 4],
    /// Shared write-twice latch (BG_old) for the BGnHOFS/BGnVOFS registers.
    bg_old: u8,
    /// TM ($212C): main-screen layer enable (bits 0-3 = BG1-4, bit 4 = OBJ).
    tm: u8,
    /// CGWSEL ($2130): only bit 0 (direct-color enable) is used here; rest is #2764.
    cgwsel: u8,
}

impl Default for Ppu {
    fn default() -> Self {
        Self::new()
    }
}

impl Ppu {
    /// Create a new PPU in its power-on state.
    pub fn new() -> Self {
        Self {
            vram: vec![0; VRAM_SIZE],
            cgram: vec![0; CGRAM_SIZE],
            oam: vec![0; OAM_SIZE],
            position: ScanPosition::default(),
            master_cycle_accumulator: 0,
            inidisp: 0,
            nmi_enable: false,
            nmi_flag: false,
            vblank_active: false,
            nmi_line_prev: false,
            nmi_edge: false,
            vram_increment_after_high: false,
            vram_increment_step: 1,
            vram_address: 0,
            vram_prefetch: 0,
            cgram_address: 0,
            cgram_latch: 0,
            oam_address: 0,
            oam_latch: 0,
            ophct_latch: 0,
            opvct_latch: 0,
            counter_latch_flag: false,
            ophct_read_high: false,
            opvct_read_high: false,
            wrio: 0xFF,
            interlace_field: false,
            framebuffer: vec![0; SCREEN_WIDTH * SCREEN_HEIGHT],
            frame_complete: false,
            bg_mode: 0,
            bg3_priority: false,
            bg_tile_size_16: [false; 4],
            bg_tilemap_base: [0; 4],
            bg_screen_size: [0; 4],
            bg_char_base: [0; 4],
            bg_hofs: [0; 4],
            bg_vofs: [0; 4],
            bg_old: 0,
            tm: 0,
            cgwsel: 0,
        }
    }

    /// Current scan position (scanline + dot).
    pub fn position(&self) -> ScanPosition {
        self.position
    }

    /// Whether VBlank NMI generation is enabled (NMITIMEN bit 7).
    pub fn nmi_enabled(&self) -> bool {
        self.nmi_enable
    }

    /// Poll for and consume a pending NMI rising edge (for delivery to the CPU).
    pub fn poll_nmi(&mut self) -> bool {
        let edge = self.nmi_edge;
        self.nmi_edge = false;
        edge
    }

    /// Whether the PPU is currently in the VBlank period.
    pub fn in_vblank(&self) -> bool {
        self.vblank_active
    }

    /// Returns and clears the frame-complete flag (set when the PPU enters VBlank).
    pub fn take_frame_complete(&mut self) -> bool {
        let done = self.frame_complete;
        self.frame_complete = false;
        done
    }

    /// Read a raw VRAM byte (test/inspection helper).
    pub fn vram_byte(&self, index: usize) -> u8 {
        self.vram[index]
    }

    /// Read a raw CGRAM byte (test/inspection helper).
    pub fn cgram_byte(&self, index: usize) -> u8 {
        self.cgram[index]
    }

    /// Read a raw OAM byte (test/inspection helper).
    pub fn oam_byte(&self, index: usize) -> u8 {
        self.oam[index]
    }
}
