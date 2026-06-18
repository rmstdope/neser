//! SNES save-state serialization.

use serde::{Deserialize, Serialize};

use crate::snes::cartridge::Mapping;

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
    pub fast_rom: bool,
    #[serde(default)]
    pub memory_bus_cycles: u8,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
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
    pub interlace_field: bool,
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
    pub cgwsel: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
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
    IncompatibleVersion {
        expected: u32,
        found: u32,
    },
    RomMismatch {
        expected: SnesRomIdentity,
        found: SnesRomIdentity,
    },
    DeserializationFailed(String),
    SerializationFailed(String),
    RestoreFailed(String),
}

impl std::fmt::Display for SnesSaveStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompatibleVersion { expected, found } => {
                write!(
                    f,
                    "incompatible save-state version (expected {expected}, found {found})"
                )
            }
            Self::RomMismatch { expected, found } => write!(
                f,
                "save-state ROM mismatch (expected {:?}, found {:?})",
                expected, found
            ),
            Self::DeserializationFailed(msg) => write!(f, "deserialization failed: {msg}"),
            Self::SerializationFailed(msg) => write!(f, "serialization failed: {msg}"),
            Self::RestoreFailed(msg) => write!(f, "restore failed: {msg}"),
        }
    }
}

impl std::error::Error for SnesSaveStateError {}

impl SnesSaveState {
    pub fn to_bytes(&self) -> Result<Vec<u8>, SnesSaveStateError> {
        serde_json::to_vec(self).map_err(|e| SnesSaveStateError::SerializationFailed(e.to_string()))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SnesSaveStateError> {
        let state: Self = serde_json::from_slice(bytes)
            .map_err(|e| SnesSaveStateError::DeserializationFailed(e.to_string()))?;
        if state.version != SNES_SAVESTATE_VERSION {
            return Err(SnesSaveStateError::IncompatibleVersion {
                expected: SNES_SAVESTATE_VERSION,
                found: state.version,
            });
        }
        Ok(state)
    }
}
