//! SNES PPU (Picture Processing Unit) emulation.
//!
//! The PPU is owned by [`crate::snes::bus::SnesSystemBus`], which advances it one master clock
//! per `tick()` and routes the `$2100-$213F` register file plus `$4200`/`$4210`/`$4212` to it.
//!
//! This module is organized into focused submodules:
//! - [`registers`] — register read/write dispatch and VRAM/CGRAM/OAM access.
//! - [`timing`] — dot/scanline counters and frame progression.

mod registers;
mod timing;

const VRAM_SIZE: usize = 0x10_000;
const CGRAM_SIZE: usize = 0x200;
const OAM_SIZE: usize = 0x220;

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
    nmi_pending: bool,
    vram_increment_after_high: bool,
    vram_increment_step: u16,
    vram_address: u16,
    vram_prefetch: u16,
    cgram_address: u16,
    cgram_latch: u8,
    oam_address: u16,
    oam_latch: u8,
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
            nmi_pending: false,
            vram_increment_after_high: false,
            vram_increment_step: 1,
            vram_address: 0,
            vram_prefetch: 0,
            cgram_address: 0,
            cgram_latch: 0,
            oam_address: 0,
            oam_latch: 0,
        }
    }

    /// Current scan position (scanline + dot).
    pub fn position(&self) -> ScanPosition {
        self.position
    }

    /// Whether a VBlank NMI is latched and pending delivery to the CPU.
    pub fn nmi_pending(&self) -> bool {
        self.nmi_pending
    }

    /// Whether VBlank NMI generation is enabled (NMITIMEN bit 7).
    pub fn nmi_enabled(&self) -> bool {
        self.nmi_enable
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
