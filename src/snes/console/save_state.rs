//! SNES save-state serialization.

use serde::{Deserialize, Serialize};

use crate::platform::save_state::SaveStateError;
use crate::snes::apu::SnesApuState;
use crate::snes::cartridge::Mapping;
use crate::snes::input::InputPortsState;

pub const SNES_SAVESTATE_VERSION: u32 = 2;

fn default_dma_regs() -> Vec<u8> {
    vec![0; 0x80]
}

fn default_dma_bbus_ports() -> Vec<u8> {
    vec![0; 0x100]
}

fn default_dma_channel_bools() -> Vec<bool> {
    vec![false; 8]
}

fn default_dma_channel_lines_left() -> Vec<u16> {
    vec![0; 8]
}

fn default_last_hperiod() -> u16 {
    1364
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SnesBlockMoveDirection {
    #[default]
    Increment,
    Decrement,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SnesBlockMoveState {
    #[serde(default)]
    pub dst_bank: u8,
    #[serde(default)]
    pub src_bank: u8,
    #[serde(default)]
    pub direction: SnesBlockMoveDirection,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct SnesCpuState {
    #[serde(default)]
    pub a: u16,
    #[serde(default)]
    pub x: u16,
    #[serde(default)]
    pub y: u16,
    #[serde(default)]
    pub d: u16,
    #[serde(default)]
    pub dbr: u8,
    #[serde(default)]
    pub pbr: u8,
    #[serde(default)]
    pub s: u16,
    #[serde(default)]
    pub pc: u16,
    #[serde(default)]
    pub p: u8,
    #[serde(default)]
    pub e: bool,
    #[serde(default)]
    pub extra_cycles: u8,
    #[serde(default)]
    pub last_page_crossed: bool,
    #[serde(default)]
    pub nmi_pending: bool,
    #[serde(default)]
    pub irq_pending: bool,
    #[serde(default)]
    pub abort_pending: bool,
    #[serde(default)]
    pub waiting: bool,
    #[serde(default)]
    pub fast_rom: bool,
    #[serde(default)]
    pub memory_bus_cycles: u8,
    #[serde(default)]
    pub irq_lock_step: bool,
    #[serde(default)]
    pub block_move_state: Option<SnesBlockMoveState>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct SnesDmaState {
    #[serde(default = "default_dma_regs")]
    pub regs: Vec<u8>,
    #[serde(default = "default_dma_bbus_ports")]
    pub bbus_ports: Vec<u8>,
    #[serde(default)]
    pub hdma_active_mask: u8,
    #[serde(default = "default_dma_channel_bools")]
    pub hdma_do_transfer: Vec<bool>,
    #[serde(default = "default_dma_channel_bools")]
    pub hdma_repeat_mode: Vec<bool>,
    #[serde(default = "default_dma_channel_lines_left")]
    pub hdma_lines_left: Vec<u16>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SnesBusState {
    #[serde(default)]
    pub wram: Vec<u8>,
    #[serde(default)]
    pub wmadd: u32,
    #[serde(default)]
    pub wrmpya: u8,
    #[serde(default)]
    pub wrdiv: u16,
    #[serde(default)]
    pub rddiv: u16,
    #[serde(default)]
    pub rdmpy: u16,
    #[serde(default)]
    pub memsel: u8,
    #[serde(default)]
    pub hdmaen: u8,
    #[serde(default)]
    pub dma: SnesDmaState,
    #[serde(default)]
    pub mdr: u8,
    #[serde(default)]
    pub ticks: u64,
    #[serde(default)]
    pub sram: Vec<u8>,
    #[serde(default)]
    pub apu: SnesApuState,
    #[serde(default)]
    pub input: InputPortsState,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct SnesRomIdentity {
    #[serde(default)]
    pub mapping: Option<Mapping>,
    #[serde(default)]
    pub crc32: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct SnesPpuState {
    #[serde(default)]
    pub vram: Vec<u8>,
    #[serde(default)]
    pub cgram: Vec<u8>,
    #[serde(default)]
    pub oam: Vec<u8>,
    #[serde(default)]
    pub scanline: u16,
    #[serde(default)]
    pub dot: u16,
    #[serde(default)]
    pub master_cycle_accumulator: u32,
    #[serde(default)]
    pub line_clock: u16,
    #[serde(default = "default_last_hperiod")]
    pub last_hperiod: u16,
    #[serde(default)]
    pub line_timing_profile: u8,
    #[serde(default)]
    pub inidisp: u8,
    #[serde(default)]
    pub nmi_enable: bool,
    #[serde(default)]
    pub nmi_flag: bool,
    #[serde(default)]
    pub vblank_active: bool,
    #[serde(default)]
    pub nmi_line_prev: bool,
    #[serde(default)]
    pub nmi_edge: bool,
    #[serde(default)]
    pub frame_complete: bool,
    #[serde(default)]
    pub vram_increment_after_high: bool,
    #[serde(default)]
    pub vram_increment_step: u16,
    #[serde(default)]
    pub vram_address: u16,
    #[serde(default)]
    pub vram_prefetch: u16,
    #[serde(default)]
    pub cgram_address: u16,
    #[serde(default)]
    pub cgram_latch: u8,
    #[serde(default)]
    pub oam_address: u16,
    #[serde(default)]
    pub oam_latch: u8,
    #[serde(default)]
    pub ophct_latch: u16,
    #[serde(default)]
    pub opvct_latch: u16,
    #[serde(default)]
    pub counter_latch_flag: bool,
    #[serde(default)]
    pub ophct_read_high: bool,
    #[serde(default)]
    pub opvct_read_high: bool,
    #[serde(default)]
    pub wrio: u8,
    #[serde(default)]
    pub irq_mode: u8,
    #[serde(default)]
    pub htime: u16,
    #[serde(default)]
    pub vtime: u16,
    #[serde(default)]
    pub timeup_flag: bool,
    #[serde(default)]
    pub irq_line: bool,
    #[serde(default)]
    pub irq_edge_age: u32,
    #[serde(default)]
    pub interlace_field: bool,
    #[serde(default)]
    pub video_region: u8,
    #[serde(default)]
    pub bg_mode: u8,
    #[serde(default)]
    pub bg3_priority: bool,
    #[serde(default)]
    pub bg_tile_size_16: [bool; 4],
    #[serde(default)]
    pub bg_tilemap_base: [u16; 4],
    #[serde(default)]
    pub bg_screen_size: [u8; 4],
    #[serde(default)]
    pub bg_char_base: [u16; 4],
    #[serde(default)]
    pub bg_hofs: [u16; 4],
    #[serde(default)]
    pub bg_vofs: [u16; 4],
    #[serde(default)]
    pub bg_old: u8,
    #[serde(default)]
    pub tm: u8,
    #[serde(default)]
    pub ts: u8,
    #[serde(default)]
    pub tmw: u8,
    #[serde(default)]
    pub tsw: u8,
    #[serde(default)]
    pub cgwsel: u8,
    #[serde(default)]
    pub cgadsub: u8,
    #[serde(default)]
    pub coldata: u16,
    #[serde(default)]
    pub w12sel: u8,
    #[serde(default)]
    pub w34sel: u8,
    #[serde(default)]
    pub wobjsel: u8,
    #[serde(default)]
    pub wh: [u8; 4],
    #[serde(default)]
    pub wbglog: u8,
    #[serde(default)]
    pub wobjlog: u8,
    #[serde(default)]
    pub setini: u8,
    #[serde(default)]
    pub m7a: u16,
    #[serde(default)]
    pub m7b: u16,
    #[serde(default)]
    pub m7c: u16,
    #[serde(default)]
    pub m7d: u16,
    #[serde(default)]
    pub m7x: u16,
    #[serde(default)]
    pub m7y: u16,
    #[serde(default)]
    pub m7hofs: u16,
    #[serde(default)]
    pub m7vofs: u16,
    #[serde(default)]
    pub m7sel: u8,
    #[serde(default)]
    pub m7_old: u8,
    #[serde(default)]
    pub obsel: u8,
    #[serde(default)]
    pub oam_addr_reload: u16,
    #[serde(default)]
    pub oam_priority_rotation: bool,
    #[serde(default)]
    pub stat77_range_over: bool,
    #[serde(default)]
    pub stat77_time_over: bool,
    #[serde(default)]
    pub obj_range_over_dot: Option<u16>,
    #[serde(default)]
    pub obj_time_over_pending: bool,
    #[serde(default)]
    pub obj_eval_dirty: bool,
    #[serde(default)]
    pub mosaic: u8,
    #[serde(default)]
    pub mosaic_vblock_size: u8,
    #[serde(default)]
    pub mosaic_vcount: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SnesSaveState {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub rom_identity: SnesRomIdentity,
    #[serde(default)]
    pub cpu: SnesCpuState,
    #[serde(default)]
    pub bus: SnesBusState,
    #[serde(default)]
    pub ppu: SnesPpuState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnesSaveStateError {
    /// A shared save-state error (version, (de)serialization, or restore).
    Common(SaveStateError),
    /// The save-state was captured from a different ROM than the one loaded.
    RomMismatch {
        expected: SnesRomIdentity,
        found: SnesRomIdentity,
    },
}

impl std::fmt::Display for SnesSaveStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Common(err) => write!(f, "{err}"),
            Self::RomMismatch { expected, found } => write!(
                f,
                "save-state ROM mismatch (expected {:?}, found {:?})",
                expected, found
            ),
        }
    }
}

impl std::error::Error for SnesSaveStateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Common(err) => Some(err),
            Self::RomMismatch { .. } => None,
        }
    }
}

impl From<SaveStateError> for SnesSaveStateError {
    fn from(err: SaveStateError) -> Self {
        Self::Common(err)
    }
}

impl SnesSaveState {
    pub fn to_bytes(&self) -> Result<Vec<u8>, SaveStateError> {
        crate::platform::save_state::to_bytes(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SaveStateError> {
        let state: Self = crate::platform::save_state::from_bytes(bytes)?;
        crate::platform::save_state::check_version(state.version, &[SNES_SAVESTATE_VERSION])?;
        Ok(state)
    }
}
