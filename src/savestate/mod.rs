//! Save-state support for the NES emulator.
//!
//! This module provides serialization and deserialization of full emulator state,
//! allowing for save/load functionality that enables resuming from any point.

use serde::{Deserialize, Serialize};

/// Current save-state format version.
/// Increment this when making breaking changes to the state format.
pub const SAVESTATE_VERSION: u32 = 1;

/// Complete emulator state snapshot.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SaveState {
    /// Version of the save-state format
    pub version: u32,
    /// CPU state
    pub cpu: CpuState,
    /// PPU state
    pub ppu: PpuState,
    /// APU state
    pub apu: ApuState,
    /// CPU RAM (2KB, mirrored to 8KB)
    pub ram: Vec<u8>,
    /// Mapper state (serialized as opaque bytes)
    pub mapper: MapperState,
}

/// CPU register and internal state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CpuState {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub p: u8,
    pub total_cycles: u64,
    pub halted: bool,
    pub nmi_pending: bool,
    pub irq_pending: bool,
    pub prev_need_nmi: bool,
    pub prev_run_irq: bool,
    pub run_irq: bool,
}

/// PPU timing state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PpuTimingState {
    pub scanline: u16,
    pub pixel: u16,
    pub total_cycles: u64,
    pub frame_count: u64,
}

/// PPU register state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PpuRegisterState {
    pub control: u8,
    pub mask: u8,
    pub oam_addr: u8,
    pub v: u16,
    pub t: u16,
    pub fine_x: u8,
    pub w: bool,
    pub io_bus: u8,
}

/// PPU complete state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PpuState {
    pub timing: PpuTimingState,
    pub registers: PpuRegisterState,
    pub vblank_flag: bool,
    pub sprite_zero_hit: bool,
    pub sprite_overflow: bool,
    pub nmi_occurred: bool,
    pub vram: Vec<u8>,
    pub palette: Vec<u8>,
    pub oam: Vec<u8>,
    pub secondary_oam: Vec<u8>,
    pub read_buffer: u8,
}

/// APU channel envelope state.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct EnvelopeState {
    pub start_flag: bool,
    pub divider: u8,
    pub decay_level: u8,
    pub constant_volume: bool,
    pub loop_flag: bool,
    pub period: u8,
}

/// APU pulse channel state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PulseState {
    pub timer: u16,
    pub timer_period: u16,
    pub length_counter: u8,
    pub length_counter_enabled: bool,
    pub duty: u8,
    pub duty_position: u8,
    pub envelope: EnvelopeState,
    pub sweep_enabled: bool,
    pub sweep_period: u8,
    pub sweep_negate: bool,
    pub sweep_shift: u8,
    pub sweep_reload: bool,
    pub sweep_divider: u8,
}

/// APU triangle channel state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TriangleState {
    pub timer: u16,
    pub timer_period: u16,
    pub length_counter: u8,
    pub length_counter_enabled: bool,
    pub linear_counter: u8,
    pub linear_counter_reload: u8,
    pub linear_counter_reload_flag: bool,
    pub control_flag: bool,
    pub sequence_position: u8,
}

/// APU noise channel state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NoiseState {
    pub timer: u16,
    pub timer_period: u16,
    pub length_counter: u8,
    pub length_counter_enabled: bool,
    pub envelope: EnvelopeState,
    pub mode_flag: bool,
    pub shift_register: u16,
}

/// APU DMC channel state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DmcState {
    pub timer: u16,
    pub timer_period: u16,
    pub output_level: u8,
    pub sample_address: u16,
    pub sample_length: u16,
    pub current_address: u16,
    pub bytes_remaining: u16,
    pub sample_buffer: Option<u8>,
    pub shift_register: u8,
    pub bits_remaining: u8,
    pub silence_flag: bool,
    pub irq_enabled: bool,
    pub irq_flag: bool,
    pub loop_flag: bool,
}

/// APU frame counter state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrameCounterState {
    pub cycle_counter: u32,
    pub mode: bool,
    pub irq_inhibit: bool,
    pub irq_flag: bool,
}

/// APU complete state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApuState {
    pub frame_counter: FrameCounterState,
    pub pulse1: PulseState,
    pub pulse2: PulseState,
    pub triangle: TriangleState,
    pub noise: NoiseState,
    pub dmc: DmcState,
    pub apu_cycle: u32,
    pub cpu_cycle: u64,
    pub last_4017_write: u8,
}

/// Mapper state (opaque serialization).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MapperState {
    pub mapper_number: u8,
    pub prg_ram: Vec<u8>,
    pub chr_ram: Vec<u8>,
    pub registers: Vec<u8>,
}

impl SaveState {
    /// Create a new save state with the current version.
    pub fn new(
        cpu: CpuState,
        ppu: PpuState,
        apu: ApuState,
        ram: Vec<u8>,
        mapper: MapperState,
    ) -> Self {
        Self {
            version: SAVESTATE_VERSION,
            cpu,
            ppu,
            apu,
            ram,
            mapper,
        }
    }

    /// Serialize the save state to JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize a save state from JSON.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// Serialize the save state to JSON-encoded UTF-8 bytes.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    /// Deserialize a save state from JSON-encoded UTF-8 bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_cpu_state() -> CpuState {
        CpuState {
            a: 0x42,
            x: 0x10,
            y: 0x20,
            sp: 0xFD,
            pc: 0x8000,
            p: 0x24,
            total_cycles: 1000,
            halted: false,
            nmi_pending: false,
            irq_pending: false,
            prev_need_nmi: false,
            prev_run_irq: false,
            run_irq: false,
        }
    }

    fn create_test_ppu_state() -> PpuState {
        PpuState {
            timing: PpuTimingState {
                scanline: 100,
                pixel: 200,
                total_cycles: 50000,
                frame_count: 0,
            },
            registers: PpuRegisterState {
                control: 0x80,
                mask: 0x1E,
                oam_addr: 0,
                v: 0x2000,
                t: 0x2000,
                fine_x: 0,
                w: false,
                io_bus: 0,
            },
            vblank_flag: false,
            sprite_zero_hit: false,
            sprite_overflow: false,
            nmi_occurred: false,
            vram: vec![0; 0x800],
            palette: vec![0; 32],
            oam: vec![0; 256],
            secondary_oam: vec![0; 32],
            read_buffer: 0,
        }
    }

    fn create_test_apu_state() -> ApuState {
        ApuState {
            frame_counter: FrameCounterState {
                cycle_counter: 0,
                mode: false,
                irq_inhibit: false,
                irq_flag: false,
            },
            pulse1: PulseState {
                timer: 0,
                timer_period: 0,
                length_counter: 0,
                length_counter_enabled: false,
                duty: 0,
                duty_position: 0,
                envelope: EnvelopeState::default(),
                sweep_enabled: false,
                sweep_period: 0,
                sweep_negate: false,
                sweep_shift: 0,
                sweep_reload: false,
                sweep_divider: 0,
            },
            pulse2: PulseState {
                timer: 0,
                timer_period: 0,
                length_counter: 0,
                length_counter_enabled: false,
                duty: 0,
                duty_position: 0,
                envelope: EnvelopeState::default(),
                sweep_enabled: false,
                sweep_period: 0,
                sweep_negate: false,
                sweep_shift: 0,
                sweep_reload: false,
                sweep_divider: 0,
            },
            triangle: TriangleState {
                timer: 0,
                timer_period: 0,
                length_counter: 0,
                length_counter_enabled: false,
                linear_counter: 0,
                linear_counter_reload: 0,
                linear_counter_reload_flag: false,
                control_flag: false,
                sequence_position: 0,
            },
            noise: NoiseState {
                timer: 0,
                timer_period: 0,
                length_counter: 0,
                length_counter_enabled: false,
                envelope: EnvelopeState::default(),
                mode_flag: false,
                shift_register: 1,
            },
            dmc: DmcState {
                timer: 0,
                timer_period: 428,
                output_level: 0,
                sample_address: 0xC000,
                sample_length: 0,
                current_address: 0xC000,
                bytes_remaining: 0,
                sample_buffer: None,
                shift_register: 0,
                bits_remaining: 0,
                silence_flag: true,
                irq_enabled: false,
                irq_flag: false,
                loop_flag: false,
            },
            apu_cycle: 0,
            cpu_cycle: 0,
            last_4017_write: 0,
        }
    }

    #[test]
    fn test_savestate_version() {
        assert_eq!(SAVESTATE_VERSION, 1);
    }

    #[test]
    fn test_savestate_json_roundtrip() {
        let state = SaveState::new(
            create_test_cpu_state(),
            create_test_ppu_state(),
            create_test_apu_state(),
            vec![0; 0x800],
            MapperState {
                mapper_number: 0,
                prg_ram: vec![],
                chr_ram: vec![],
                registers: vec![],
            },
        );

        let json = state.to_json().expect("serialization should succeed");
        let restored = SaveState::from_json(&json).expect("deserialization should succeed");

        assert_eq!(restored.version, state.version);
        assert_eq!(restored.cpu.a, state.cpu.a);
        assert_eq!(restored.cpu.pc, state.cpu.pc);
        assert_eq!(restored.ppu.timing.scanline, state.ppu.timing.scanline);
    }

    #[test]
    fn test_savestate_bytes_roundtrip() {
        let state = SaveState::new(
            create_test_cpu_state(),
            create_test_ppu_state(),
            create_test_apu_state(),
            vec![0; 0x800],
            MapperState {
                mapper_number: 0,
                prg_ram: vec![],
                chr_ram: vec![],
                registers: vec![],
            },
        );

        let bytes = state.to_bytes().expect("serialization should succeed");
        let restored = SaveState::from_bytes(&bytes).expect("deserialization should succeed");

        assert_eq!(restored.version, state.version);
        assert_eq!(restored.cpu.x, state.cpu.x);
        assert_eq!(restored.cpu.y, state.cpu.y);
    }
}
