//! SNES save-state serialization.

use serde::{Deserialize, Serialize};

use crate::platform::save_state::SaveStateError;
use crate::snes::apu::SnesApuState;
use crate::snes::cartridge::Mapping;
use crate::snes::input::InputPortsState;

pub const SNES_SAVESTATE_VERSION: u32 = 2;

fn default_irq_i_shadow() -> bool {
    true
}

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

fn default_dram_refresh_position() -> u16 {
    538
}

/// `$2228` BWPA's hardware reset value (fullsnes "Reset" table), used as the `#[serde(default)]`
/// fallback so a `SnesSa1State` predating this field still deserializes to the fully-protected
/// power-on state rather than `bwpa=$00` (protected-area size `256` bytes only).
fn default_sa1_bwpa() -> u8 {
    0xFF
}

/// `$2221`/`$2222`/`$2223` DXB/EXB/FXB's hardware reset values (fullsnes "Reset" table: ROM
/// slots 1/2/3 in order). Used as `#[serde(default)]` fallbacks so a `SnesSa1State` saved before
/// these fields existed (i.e. before #2959) deserializes to the same ROM mapping as power-on --
/// plain `0` would incorrectly show ROM slot 0 in all three quarters instead.
fn default_sa1_dxb() -> u8 {
    0x01
}

fn default_sa1_exb() -> u8 {
    0x02
}

fn default_sa1_fxb() -> u8 {
    0x03
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
    /// See Cpu::irq_i_shadow; defaults to true (I-set at reset) for older saves.
    #[serde(default = "default_irq_i_shadow")]
    pub irq_i_shadow: bool,
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

/// SA-1 enhancement chip state: control/vector registers (`$2200-$220F`), I-RAM plus its two
/// independent write-protection registers (`$2229`/`$222A`), Super MMC ROM banking and BW-RAM
/// mapping/write-protection registers (`$2220-$2228`), and the second 65816 CPU core's own
/// architectural state. `None`/absent on `SnesBusState` for non-SA-1 cartridges and for save
/// states captured before SA-1 support existed. BW-RAM's own bytes are `SnesBusState::sram` --
/// shared with the SNES CPU's cartridge RAM, not duplicated here.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct SnesSa1State {
    #[serde(default)]
    pub ccnt: u8,
    #[serde(default)]
    pub sie: u8,
    #[serde(default)]
    pub reset_vector: u16,
    #[serde(default)]
    pub nmi_vector: u16,
    #[serde(default)]
    pub irq_vector: u16,
    #[serde(default)]
    pub scnt: u8,
    #[serde(default)]
    pub cie: u8,
    #[serde(default)]
    pub snes_nmi_vector: u16,
    #[serde(default)]
    pub snes_irq_vector: u16,
    #[serde(default)]
    pub iram: Vec<u8>,
    #[serde(default)]
    pub iram_snes_write_protect: u8,
    #[serde(default)]
    pub iram_sa1_write_protect: u8,
    #[serde(default)]
    pub cxb: u8,
    #[serde(default = "default_sa1_dxb")]
    pub dxb: u8,
    #[serde(default = "default_sa1_exb")]
    pub exb: u8,
    #[serde(default = "default_sa1_fxb")]
    pub fxb: u8,
    #[serde(default)]
    pub bmaps: u8,
    #[serde(default)]
    pub bmap: u8,
    #[serde(default)]
    pub sbwe: u8,
    #[serde(default)]
    pub cbwe: u8,
    #[serde(default = "default_sa1_bwpa")]
    pub bwpa: u8,
    #[serde(default)]
    pub cpu: SnesCpuState,
    #[serde(default)]
    pub booted: bool,
    #[serde(default)]
    pub master_clock_debt: i64,
    /// CFR bit 7 (SA-1-side IRQ-from-SNES pending). Not re-derivable from `ccnt` alone, since
    /// its message nibble may have been overwritten since the flag latched.
    #[serde(default)]
    pub sa1_irq_pending: bool,
    /// CFR bit 4 (SA-1-side NMI-from-SNES pending). See `sa1_irq_pending`.
    #[serde(default)]
    pub sa1_nmi_pending: bool,
    /// SFR bit 7 (SNES-side IRQ-from-SA-1 pending). See `sa1_irq_pending`.
    #[serde(default)]
    pub snes_irq_pending: bool,
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
    /// `None` for non-SA-1 cartridges, and for save states captured before SA-1 support existed
    /// (`#[serde(default)]` keeps those loadable).
    #[serde(default)]
    pub sa1: Option<SnesSa1State>,
    /// Armed-but-not-started GPDMA as `(cpu_cycle_countdown, mdmaen, fallback_clock)`
    /// (see `SnesSystemBus::pending_gpdma`); `None` when no transfer is pending.
    #[serde(default)]
    pub pending_gpdma: Option<(u8, u8, u64)>,
    /// Armed-but-not-run HDMA line/init work as `(cpu_cycle_countdown, kind, fallback_clock)`
    /// (see `SnesSystemBus::pending_hdma`); `None` when nothing is pending.
    #[serde(default)]
    pub pending_hdma: Option<(u8, u8, u64)>,
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
    pub total_master_clocks: u64,
    #[serde(default = "default_dram_refresh_position")]
    pub dram_refresh_position: u16,
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
    /// Undrained vblank-entry count (transient; drained every CPU step, so it
    /// is 0 in practice except for a state captured mid-instruction). Replaces
    /// the pre-#2990 `frame_complete: bool`, whose value old saves simply lose.
    #[serde(default)]
    pub pending_completed_frames: u32,
    #[serde(default)]
    pub vram_increment_after_high: bool,
    #[serde(default)]
    pub vram_increment_step: u16,
    /// VMAIN $2115 bits 3-2 (0..=3); old saves default to 0 = no translation.
    #[serde(default)]
    pub vram_address_translation: u8,
    #[serde(default)]
    pub vram_address: u16,
    #[serde(default)]
    pub vram_prefetch: u16,
    #[serde(default)]
    pub cgram_address: u16,
    #[serde(default)]
    pub cgram_latch: u8,
    /// Renderer's current CGRAM fetch cursor (mid-render CGRAM writes land here).
    /// Defaults to 0 for states saved before it existed.
    #[serde(default)]
    pub cgram_render_index: u8,
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
    pub ppu2_open_bus: u8,
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
    pub frame_has_extra_scanline: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A `SnesSa1State` predating #2959 (i.e. before CXB/DXB/EXB/FXB/BMAPS/BMAP/SBWE/CBWE/BWPA
    /// existed) deserializes as an empty JSON object for those fields.
    #[test]
    fn sa1_state_missing_memory_control_fields_deserializes_to_hardware_reset_values() {
        let state: SnesSa1State = serde_json::from_str("{}").expect("deserialize");
        assert_eq!(state.cxb, 0x00);
        assert_eq!(state.dxb, 0x01);
        assert_eq!(state.exb, 0x02);
        assert_eq!(state.fxb, 0x03);
        assert_eq!(state.bmaps, 0x00);
        assert_eq!(state.bmap, 0x00);
        assert_eq!(state.sbwe, 0x00);
        assert_eq!(state.cbwe, 0x00);
        assert_eq!(state.bwpa, 0xFF);
    }

    /// A `SnesSa1State` predating #2960 (i.e. before the cross-CPU IRQ pending flags existed)
    /// deserializes with all three flags clear -- the correct "nothing pending" hardware
    /// power-on state, and also what a pre-#2960 save state's SA-1 would actually have been in
    /// (since the interrupt lines didn't exist to have anything pending).
    #[test]
    fn sa1_state_missing_irq_pending_fields_deserializes_to_nothing_pending() {
        let state: SnesSa1State = serde_json::from_str("{}").expect("deserialize");
        assert!(!state.sa1_irq_pending);
        assert!(!state.sa1_nmi_pending);
        assert!(!state.snes_irq_pending);
    }
}
