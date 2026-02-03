//! Save-state support for the NES emulator.
//!
//! This module provides serialization and deserialization of full emulator state,
//! allowing for save/load functionality that enables resuming from any point.

use serde::{Deserialize, Serialize};

/// Current save-state format version.
/// Increment this when making breaking changes to the state format.
pub const SAVESTATE_VERSION: u32 = 6;

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
    /// Bus state
    pub bus: BusState,
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
    pub delayed_i_flag: Option<bool>,
    pub forced_irq_pending: bool,
    pub skip_interrupt_latch_this_cycle: bool,
    pub master_clock: u64,
    pub master_clock_ppu: u64,
    pub oob_master_clock: u64,
    pub oob_master_clock_ppu: u64,
    pub dmc_dma_running: bool,
    pub dmc_dma_need_halt: bool,
    pub dmc_dma_need_dummy_read: bool,
    pub interrupt_stack: Vec<crate::cpu::InterruptKind>,
    pub current_tick_info: Option<(u8, u8)>,
}

/// PPU timing state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PpuTimingState {
    pub scanline: u16,
    pub pixel: u16,
    pub total_cycles: u64,
    pub frame_count: u64,
    pub rendering_enabled_d1: bool,
    pub rendering_enabled_d2: bool,
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
    pub io_bus_refresh_time: [u64; 8],
    pub cycle_count: u64,
}

/// PPU complete state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PpuState {
    pub timing: PpuTimingState,
    pub registers: PpuRegisterState,
    pub vblank_flag: bool,
    pub sprite_zero_hit: bool,
    pub pending_sprite_zero_hit: bool,
    pub sprite_overflow: bool,
    /// Internal edge-detection latch for NMI generation.
    /// This is polled and cleared by the CPU, not the NMI enable control bit.
    pub nmi_pending: bool,
    pub frame_complete: bool,
    pub vblank_suppressed_for_frame: bool,
    pub vblank_for_nmi: bool,
    pub prev_a12: bool,
    pub vram: Vec<u8>,
    pub palette: Vec<u8>,
    pub last_palette_index: Option<u8>,
    pub last_palette_value: u8,
    pub mirroring_mode: crate::cartridge::MirroringMode,
    pub oam: Vec<u8>,
    pub secondary_oam: Vec<u8>,
    pub sprites_found: u8,
    pub sprite_count: u8,
    pub next_sprite_count: u8,
    pub sprite_buffers_ready: bool,
    pub sprite_0_index: Option<usize>,
    pub next_sprite_0_index: Option<usize>,
    pub sprite_eval_n: u8,
    pub sprite_eval_m: u8,
    pub sprite_eval_cycle: u8,
    pub sprite_eval_in_range: bool,
    pub sprite_pattern_shift_lo: [u8; 8],
    pub sprite_pattern_shift_hi: [u8; 8],
    pub sprite_x_positions: [u8; 8],
    pub sprite_attributes: [u8; 8],
    pub next_sprite_pattern_shift_lo: [u8; 8],
    pub next_sprite_pattern_shift_hi: [u8; 8],
    pub next_sprite_x_positions: [u8; 8],
    pub next_sprite_attributes: [u8; 8],
    pub bg_pattern_shift_lo: u16,
    pub bg_pattern_shift_hi: u16,
    pub bg_attribute_shift_lo: u16,
    pub bg_attribute_shift_hi: u16,
    pub nametable_latch: u8,
    pub attribute_latch: u8,
    pub pattern_lo_latch: u8,
    pub pattern_hi_latch: u8,
    pub screen_buffer: Vec<u8>,
    pub read_buffer: u8,
}

// TODO We are duplicating a lot of sprite state info in PpuState
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpritesState {
    pub oam_data: [u8; 256],
    pub secondary_oam: [u8; 32],
    pub sprites_found: u8,
    pub sprite_count: u8,
    pub next_sprite_count: u8,
    pub sprite_buffers_ready: bool,
    pub sprite_0_index: Option<usize>,
    pub next_sprite_0_index: Option<usize>,
    pub sprite_eval_n: u8,
    pub sprite_eval_m: u8,
    pub sprite_eval_cycle: u8,
    pub sprite_eval_in_range: bool,
    pub sprite_pattern_shift_lo: [u8; 8],
    pub sprite_pattern_shift_hi: [u8; 8],
    pub sprite_x_positions: [u8; 8],
    pub sprite_attributes: [u8; 8],
    pub next_sprite_pattern_shift_lo: [u8; 8],
    pub next_sprite_pattern_shift_hi: [u8; 8],
    pub next_sprite_x_positions: [u8; 8],
    pub next_sprite_attributes: [u8; 8],
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
    pub length_counter_halt: bool,
    pub length_counter_pending_halt: Option<bool>,
    pub length_counter_reload_value: u8,
    pub length_counter_previous_value: u8,
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
    pub length_counter_halt: bool,
    pub length_counter_pending_halt: Option<bool>,
    pub length_counter_reload_value: u8,
    pub length_counter_previous_value: u8,
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
    pub length_counter_halt: bool,
    pub length_counter_pending_halt: Option<bool>,
    pub length_counter_reload_value: u8,
    pub length_counter_previous_value: u8,
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
    pub dma_pending: bool,
    pub transfer_start_delay: u8,
}

/// APU frame counter state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FrameCounterState {
    pub cycle_counter: u32,
    pub mode: bool,
    pub irq_inhibit: bool,
    pub irq_flag: bool,
    pub irq_assert_cycles_remaining: u8,
    pub block_frame_counter: bool,
    pub five_step_extra_cycle: bool,
    pub pending_write: Option<u8>,
    pub write_delay: u8,
    pub pending_write_on_odd_cpu_cycle: bool,
    pub pending_immediate_quarter: bool,
    pub pending_immediate_half: bool,
}

/// Bus joypad state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JoypadState {
    pub strobe: bool,
    pub button_index: u8,
    pub button_states: u8,
}

/// Bus paddle state.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PaddleState {
    pub strobe: bool,
    pub shift_index: u8,
    pub position: u8,
    pub latched_position: u8,
    pub trigger: bool,
    pub enabled: bool,
}

/// Bus state for save-state support.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BusState {
    pub open_bus: u8,
    pub oam_dma_page: Option<u8>,
    pub port1_controller: ControllerStateWrapper,
    pub port2_controller: ControllerStateWrapper,
}

/// Wrapper for controller state to support serialization.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ControllerStateWrapper {
    Joypad(JoypadState),
    Paddle(PaddleState),
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
    pub sample_accumulator: f32,
    pub cycles_per_sample: f32,
    pub pending_samples: Vec<f32>,
    pub pulse1_enabled: bool,
    pub pulse2_enabled: bool,
    pub triangle_enabled: bool,
    pub noise_enabled: bool,
    pub dmc_enabled: bool,
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
        bus: BusState,
        ram: Vec<u8>,
        mapper: MapperState,
    ) -> Self {
        Self {
            version: SAVESTATE_VERSION,
            cpu,
            ppu,
            apu,
            bus,
            ram,
            mapper,
        }
    }

    /// Serialize the save state to JSON.
    #[cfg(test)]
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize a save state from JSON.
    #[cfg(test)]
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
            delayed_i_flag: None,
            forced_irq_pending: false,
            skip_interrupt_latch_this_cycle: false,
            master_clock: 0,
            master_clock_ppu: 0,
            oob_master_clock: 0,
            oob_master_clock_ppu: 0,
            dmc_dma_running: false,
            dmc_dma_need_halt: false,
            dmc_dma_need_dummy_read: false,
            interrupt_stack: Vec::new(),
            current_tick_info: None,
        }
    }

    fn create_test_ppu_state() -> PpuState {
        PpuState {
            timing: PpuTimingState {
                scanline: 100,
                pixel: 200,
                total_cycles: 50000,
                frame_count: 0,
                rendering_enabled_d1: false,
                rendering_enabled_d2: false,
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
                io_bus_refresh_time: [0; 8],
                cycle_count: 0,
            },
            vblank_flag: false,
            sprite_zero_hit: false,
            pending_sprite_zero_hit: false,
            sprite_overflow: false,
            nmi_pending: false,
            frame_complete: false,
            vblank_suppressed_for_frame: false,
            vblank_for_nmi: false,
            prev_a12: false,
            vram: vec![0; 0x800],
            palette: vec![0; 32],
            last_palette_index: None,
            last_palette_value: 0,
            mirroring_mode: crate::cartridge::MirroringMode::Horizontal,
            oam: vec![0; 256],
            secondary_oam: vec![0; 32],
            sprites_found: 0,
            sprite_count: 0,
            next_sprite_count: 0,
            sprite_buffers_ready: false,
            sprite_0_index: None,
            next_sprite_0_index: None,
            sprite_eval_n: 0,
            sprite_eval_m: 0,
            sprite_eval_cycle: 0,
            sprite_eval_in_range: false,
            sprite_pattern_shift_lo: [0; 8],
            sprite_pattern_shift_hi: [0; 8],
            sprite_x_positions: [0; 8],
            sprite_attributes: [0; 8],
            next_sprite_pattern_shift_lo: [0; 8],
            next_sprite_pattern_shift_hi: [0; 8],
            next_sprite_x_positions: [0; 8],
            next_sprite_attributes: [0; 8],
            bg_pattern_shift_lo: 0,
            bg_pattern_shift_hi: 0,
            bg_attribute_shift_lo: 0,
            bg_attribute_shift_hi: 0,
            nametable_latch: 0,
            attribute_latch: 0,
            pattern_lo_latch: 0,
            pattern_hi_latch: 0,
            screen_buffer: vec![0; 256 * 240 * 3],
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
                irq_assert_cycles_remaining: 0,
                block_frame_counter: false,
                five_step_extra_cycle: false,
                pending_write: None,
                write_delay: 0,
                pending_write_on_odd_cpu_cycle: false,
                pending_immediate_quarter: false,
                pending_immediate_half: false,
            },
            pulse1: PulseState {
                timer: 0,
                timer_period: 0,
                length_counter: 0,
                length_counter_enabled: false,
                length_counter_halt: false,
                length_counter_pending_halt: None,
                length_counter_reload_value: 0,
                length_counter_previous_value: 0,
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
                length_counter_halt: false,
                length_counter_pending_halt: None,
                length_counter_reload_value: 0,
                length_counter_previous_value: 0,
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
                length_counter_halt: false,
                length_counter_pending_halt: None,
                length_counter_reload_value: 0,
                length_counter_previous_value: 0,
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
                length_counter_halt: false,
                length_counter_pending_halt: None,
                length_counter_reload_value: 0,
                length_counter_previous_value: 0,
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
                dma_pending: false,
                transfer_start_delay: 0,
            },
            sample_accumulator: 0.0,
            cycles_per_sample: 0.0,
            pending_samples: Vec::new(),
            pulse1_enabled: true,
            pulse2_enabled: true,
            triangle_enabled: true,
            noise_enabled: true,
            dmc_enabled: true,
            apu_cycle: 0,
            cpu_cycle: 0,
            last_4017_write: 0,
        }
    }

    fn create_test_bus_state() -> BusState {
        BusState {
            open_bus: 0xFF,
            oam_dma_page: None,
            port1_controller: ControllerStateWrapper::Joypad(JoypadState {
                strobe: false,
                button_index: 0,
                button_states: 0,
            }),
            port2_controller: ControllerStateWrapper::Joypad(JoypadState {
                strobe: false,
                button_index: 0,
                button_states: 0,
            }),
        }
    }

    #[test]
    fn test_savestate_version() {
        assert_eq!(SAVESTATE_VERSION, 6);
    }

    #[test]
    fn test_savestate_json_roundtrip() {
        let state = SaveState::new(
            create_test_cpu_state(),
            create_test_ppu_state(),
            create_test_apu_state(),
            create_test_bus_state(),
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
            create_test_bus_state(),
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
