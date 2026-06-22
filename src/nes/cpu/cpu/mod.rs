use super::master_clock::MasterClock;
use super::opcode::{AddrMode, Mnemonic, OpCode};
use crate::nes::apu::Apu;
use crate::nes::bus::Bus;
use crate::nes::console::TimingMode;
use crate::nes::ppu::Ppu;
use crate::trace_cpu;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;

/// CPU register and internal state for save-state support.
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
    pub dmc_dma_phase: DmcDmaPhase,
    pub interrupt_stack: Vec<crate::nes::cpu::InterruptKind>,
    pub current_tick_info: Option<(u8, u8)>,
}

/// NES 6502 CPU
pub struct Cpu {
    /// Accumulator
    a: u8,
    /// X register
    x: u8,
    /// Y register
    y: u8,
    /// Stack pointer
    sp: u8,
    /// Program counter
    pc: u16,
    /// Status register (processor flags)
    /// Bit 7: N (Negative)
    /// Bit 6: V (Overflow)
    /// Bit 5: - (unused, always 1)
    /// Bit 4: B (Break)
    /// Bit 3: D (Decimal mode, not used on NES)
    /// Bit 2: I (Interrupt disable)
    /// Bit 1: Z (Zero)
    /// Bit 0: C (Carry)
    p: u8,
    /// Memory
    bus: Rc<RefCell<Bus>>,
    /// PPU
    ppu: Rc<RefCell<Ppu>>,
    /// APU
    #[allow(dead_code)]
    apu: Rc<RefCell<Apu>>,
    /// Halted state (set by KIL instruction)
    halted: bool,
    /// Total cycles executed since last reset
    total_cycles: u64,
    /// Delayed I flag value for IRQ polling
    /// When Some(value), use this value instead of the actual I flag for IRQ polling
    /// This implements the 1-instruction delay for CLI/PLP
    /// Set to Some(old_i_value) when CLI/PLP modifies I flag, cleared after next instruction
    delayed_i_flag: Option<bool>,
    /// NMI pending flag - set by external hardware (NES loop)
    /// Checked during BRK execution to determine vector hijacking
    nmi_pending: bool,

    // --- Interrupt timing (latched at end of CPU cycles) ---
    prev_need_nmi: bool,
    prev_run_irq: bool,
    run_irq: bool,
    /// IRQ pending flag - set by external hardware (APU/mapper)
    /// IRQ is level-triggered and maskable by the I flag
    irq_pending: bool,
    /// Test-only / externally-forced IRQ assertion.
    ///
    /// The NES IRQ line is level-triggered and should be sampled each CPU cycle.
    /// Some unit tests use `set_irq_pending(true)` to force an asserted IRQ.
    forced_irq_pending: bool,
    /// Master clock (timing model)
    master_clock: MasterClock,
    /// When set, the next CPU cycle will not latch IRQ/NMI line state at the end of the cycle.
    ///
    /// This is used to model edge-case timing where certain instructions (notably taken
    /// non-page-crossing branches) ignore interrupts during their final clock.
    skip_interrupt_latch_this_cycle: bool,

    // DMC DMA state machine
    dmc_dma_phase: DmcDmaPhase,

    /// Tracks whether the CPU is currently executing inside an interrupt handler.
    ///
    /// This is derived from interrupt entry (IRQ/NMI) and cleared on RTI.
    interrupt_stack: Vec<InterruptKind>,
    /// Tracks the current tick number and total ticks for tracing
    current_tick_info: Option<(u8, u8)>,
    /// The most recent non-dummy write address during the current instruction, if any.
    last_cpu_write_addr: Option<u16>,
    /// Cached from the inserted cartridge's mapper capabilities.
    /// True when the mapper provides expansion audio channels (e.g. VRC6, MMC5, Namco 163).
    /// Used to skip the expensive nested RefCell borrow chain when no expansion audio is present.
    mapper_has_expansion_audio: bool,
    /// Cached from the inserted cartridge's mapper capabilities.
    /// True when the mapper can generate IRQ interrupts.
    /// Used to skip the mapper IRQ poll when the mapper cannot generate IRQs.
    mapper_has_irq: bool,
}

// Status register flags
const FLAG_CARRY: u8 = 0b0000_0001;
const FLAG_ZERO: u8 = 0b0000_0010;
const FLAG_INTERRUPT: u8 = 0b0000_0100;
const FLAG_DECIMAL: u8 = 0b0000_1000;
const FLAG_BREAK: u8 = 0b0001_0000;
const FLAG_UNUSED: u8 = 0b0010_0000;
const FLAG_OVERFLOW: u8 = 0b0100_0000;
const FLAG_NEGATIVE: u8 = 0b1000_0000;

const NMI_VECTOR: u16 = 0xFFFA;
const RESET_VECTOR: u16 = 0xFFFC;
const IRQ_VECTOR: u16 = 0xFFFE;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptKind {
    Nmi,
    Irq,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DmaReadOutcome {
    NoDma,
    RetryRead,
    ReturnValue(u8),
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DmcDmaPhase {
    #[default]
    Idle,
    Halt,
    Dummy,
    Aligning,
    Reading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub struct CpuRegisters {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub p: u8,
}

impl Cpu {
    /// Create a new CPU with default register values at power-on
    pub fn new(
        tv_system: TimingMode,
        memory: Rc<RefCell<Bus>>,
        ppu: Rc<RefCell<Ppu>>,
        apu: Rc<RefCell<Apu>>,
    ) -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0x00, // Stack pointer starts at 0x00 at power-on. The automatic reset
            // sequence then subtracts 3, resulting in SP=0xFD when the reset
            // handler first runs.
            pc: 0,   // Program counter will be loaded from reset vector
            p: 0x20, // Status at power-on before reset: only unused bit set (bit 5)
            // 0x20 = 0b00100000
            // The reset sequence will set the I flag, resulting in 0x24.
            // Note: B flag (bit 4) is not actually stored in P register,
            // it only appears when P is pushed to stack during BRK/PHP
            bus: memory,
            ppu,
            apu,
            halted: false,
            total_cycles: 0,
            delayed_i_flag: None,
            nmi_pending: false,
            prev_need_nmi: false,
            prev_run_irq: false,
            run_irq: false,
            irq_pending: false,
            forced_irq_pending: false,
            master_clock: MasterClock::new(tv_system),

            skip_interrupt_latch_this_cycle: false,

            // DMC DMA state machine
            dmc_dma_phase: DmcDmaPhase::Idle,

            interrupt_stack: Vec::with_capacity(2),
            current_tick_info: None,
            last_cpu_write_addr: None,
            mapper_has_expansion_audio: false,
            mapper_has_irq: false,
        }
    }
}

mod alu;
mod bus;
mod dma;
mod execute;
mod interrupts;
mod state;
mod timing;

#[cfg(test)]
mod tests;
