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
    fn is_controller_port2_read(addr: u16) -> bool {
        addr == 0x4017
    }

    fn should_skip_first_input_clock(read_address: u16, dmc_address: u16) -> bool {
        let is_controller_read = matches!(read_address, 0x4016 | 0x4017);
        is_controller_read && (dmc_address & 0x1F) == (read_address & 0x1F)
    }

    fn dmc_pending_single_byte_fetch(&self) -> bool {
        let mut apu = self.apu.borrow_mut();
        let dmc = apu.dmc_mut().capture_state();
        dmc.sample_length == 1 && dmc.bytes_remaining == 1
    }

    pub fn current_interrupt(&self) -> Option<InterruptKind> {
        self.interrupt_stack.last().copied()
    }

    #[allow(dead_code)]
    pub fn state(&self) -> CpuRegisters {
        CpuRegisters {
            a: self.a,
            x: self.x,
            y: self.y,
            sp: self.sp,
            pc: self.pc,
            p: self.p,
        }
    }

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

    pub fn is_halted(&self) -> bool {
        self.halted
    }

    pub fn a(&self) -> u8 {
        self.a
    }

    pub fn x(&self) -> u8 {
        self.x
    }

    pub fn y(&self) -> u8 {
        self.y
    }

    pub fn sp(&self) -> u8 {
        self.sp
    }

    pub fn pc(&self) -> u16 {
        self.pc
    }

    pub fn p(&self) -> u8 {
        self.p
    }

    /// Get the total number of cycles executed since last reset
    pub fn get_total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Returns the address of the most recent non-dummy CPU write during the last instruction,
    /// or `None` if no write occurred.
    pub fn last_cpu_write_addr(&self) -> Option<u16> {
        self.last_cpu_write_addr
    }

    #[cfg(test)]
    pub fn set_total_cycles(&mut self, cycles: u64) {
        self.total_cycles = cycles;
    }

    /// Update `mapper_has_expansion_audio` and `mapper_has_irq` from the currently
    /// inserted cartridge's mapper capabilities.
    ///
    /// Must be called whenever a new cartridge is inserted. These flags gate
    /// expensive per-cycle nested `RefCell` borrow chains so they are only paid
    /// when the mapper actually requires them.
    pub fn update_mapper_capability_flags(&mut self) {
        if let Some(caps) = self.bus.borrow().cartridge_mapper_capabilities() {
            self.mapper_has_expansion_audio = caps.has_expansion_audio;
            self.mapper_has_irq = caps.has_irq;
        } else {
            self.mapper_has_expansion_audio = false;
            self.mapper_has_irq = false;
        }
    }

    #[cfg(test)]
    pub fn test_mapper_has_expansion_audio(&self) -> bool {
        self.mapper_has_expansion_audio
    }

    #[cfg(test)]
    pub fn test_mapper_has_irq(&self) -> bool {
        self.mapper_has_irq
    }

    #[cfg(test)]
    pub fn set_a_register(&mut self, value: u8) {
        self.a = value;
    }

    #[cfg(test)]
    pub fn set_x(&mut self, value: u8) {
        self.x = value;
    }

    #[cfg(test)]
    pub fn set_y(&mut self, value: u8) {
        self.y = value;
    }

    #[cfg(test)]
    pub fn set_sp(&mut self, value: u8) {
        self.sp = value;
    }

    #[cfg(test)]
    pub fn set_pc(&mut self, value: u16) {
        self.pc = value;
    }

    #[cfg(test)]
    pub fn set_p(&mut self, value: u8) {
        self.p = value;
    }

    /// Simulate a JSR to $7003 for trainer execution.
    /// Pushes `(game_vector − 1)` onto the stack (hi byte first) and sets PC to $7003.
    /// The trainer must end with RTS to return execution to `game_vector`.
    pub fn divert_to_trainer(&mut self, game_vector: u16) {
        let return_addr = game_vector.wrapping_sub(1);
        let hi = (return_addr >> 8) as u8;
        let lo = return_addr as u8;
        let addr_hi = 0x0100 | self.sp as u16;
        self.bus.borrow_mut().write(addr_hi, hi, false);
        self.sp = self.sp.wrapping_sub(1);
        let addr_lo = 0x0100 | self.sp as u16;
        self.bus.borrow_mut().write(addr_lo, lo, false);
        self.sp = self.sp.wrapping_sub(1);
        self.pc = 0x7003;
    }

    pub fn add_cycles(&mut self, cycles: u64) {
        self.total_cycles += cycles;
    }

    /// Capture the current CPU state for save-state.
    pub fn capture_state(&self) -> CpuState {
        CpuState {
            a: self.a,
            x: self.x,
            y: self.y,
            sp: self.sp,
            pc: self.pc,
            p: self.p,
            total_cycles: self.total_cycles,
            halted: self.halted,
            nmi_pending: self.nmi_pending,
            irq_pending: self.irq_pending,
            prev_need_nmi: self.prev_need_nmi,
            prev_run_irq: self.prev_run_irq,
            run_irq: self.run_irq,
            delayed_i_flag: self.delayed_i_flag,
            forced_irq_pending: self.forced_irq_pending,
            skip_interrupt_latch_this_cycle: self.skip_interrupt_latch_this_cycle,
            master_clock: self.master_clock.master_cycles(),
            master_clock_ppu: self.master_clock.ppu_cycles(),
            dmc_dma_phase: self.dmc_dma_phase,
            interrupt_stack: self.interrupt_stack.clone(),
            current_tick_info: self.current_tick_info,
        }
    }

    /// Restore CPU state from a save-state.
    pub fn restore_state(&mut self, state: &CpuState) {
        self.a = state.a;
        self.x = state.x;
        self.y = state.y;
        self.sp = state.sp;
        self.pc = state.pc;
        self.p = state.p;
        self.total_cycles = state.total_cycles;
        self.halted = state.halted;
        self.nmi_pending = state.nmi_pending;
        self.irq_pending = state.irq_pending;
        self.prev_need_nmi = state.prev_need_nmi;
        self.prev_run_irq = state.prev_run_irq;
        self.run_irq = state.run_irq;
        self.delayed_i_flag = state.delayed_i_flag;
        self.forced_irq_pending = state.forced_irq_pending;
        self.skip_interrupt_latch_this_cycle = state.skip_interrupt_latch_this_cycle;
        self.master_clock.set_master_cycles(state.master_clock);
        self.master_clock.set_ppu_cycles(state.master_clock_ppu);
        self.dmc_dma_phase = state.dmc_dma_phase;
        self.interrupt_stack = state.interrupt_stack.clone();
        self.current_tick_info = state.current_tick_info;
    }

    fn end_cpu_cycle_latch_interrupt_lines(&mut self) {
        // Capture previous-cycle state, then update
        // edge/level detections based on the current end-of-cycle line status.

        self.prev_need_nmi = self.nmi_pending;
        if self.ppu.borrow_mut().poll_nmi() {
            self.nmi_pending = true;
        }

        self.prev_run_irq = self.run_irq;

        // Level-triggered IRQ line: sample hardware lines each CPU cycle.
        // Unit tests may force an asserted IRQ via `forced_irq_pending`.
        let irq_asserted_from_apu = self.apu.borrow().poll_irq();
        let irq_asserted_from_mapper = if self.mapper_has_irq {
            self.bus.borrow().mapper_irq_pending()
        } else {
            false
        };
        self.irq_pending =
            irq_asserted_from_apu || irq_asserted_from_mapper || self.forced_irq_pending;

        // The value that will be used for the *next* instruction's interrupt check.
        self.run_irq = self.should_poll_irq();
    }

    fn service_irq_or_nmi_sequence(&mut self) {
        // Shared interrupt sequence used after an instruction completes.
        // Two dummy reads with suppressed PC increment, then push PC, push PS, set I, vector.
        // PAL DMA nuances omitted for now.
        // NMI can interrupt IRQ vectoring, but only if it becomes pending early enough.
        // This behavior is required for blargg cpu_interrupts_v2/3-nmi_and_irq.
        let mut nmi_hijack = self.nmi_pending;

        let pc = self.pc;
        let _sp = self.sp;
        let _p = self.p;
        let _cycle = self.total_cycles;
        let _frame = self.ppu.borrow().timing().frame_count();
        let _scanline = self.ppu.borrow().timing().scanline();
        let _pixel = self.ppu.borrow().timing().pixel();
        self.dummy_read(pc);
        nmi_hijack |= self.nmi_pending;
        self.dummy_read(pc);
        nmi_hijack |= self.nmi_pending;

        // Push PC (high then low). Sample NMI between the two stack writes.
        self.push_byte((self.pc >> 8) as u8);
        nmi_hijack |= self.nmi_pending;
        self.push_byte(self.pc as u8);
        nmi_hijack |= self.nmi_pending;

        // For IRQ/NMI, the pushed status has B cleared and bit 5 set.
        let flags = (self.p & !FLAG_BREAK) | FLAG_UNUSED;
        self.push_byte(flags);
        self.p |= FLAG_INTERRUPT;
        // Interrupt entry sets I immediately; any pending CLI/SEI/PLP "delay" state
        // must not leak into the handler.
        self.delayed_i_flag = None;

        if nmi_hijack {
            self.nmi_pending = false;
            self.interrupt_stack.push(InterruptKind::Nmi);
            self.pc = self.read_u16(NMI_VECTOR);
        } else {
            // IRQ has been serviced; clear any forced IRQ. Hardware IRQ remains asserted
            // only if the APU line is still high and will be re-sampled next cycle.
            self.forced_irq_pending = false;
            self.interrupt_stack.push(InterruptKind::Irq);
            self.pc = self.read_u16(IRQ_VECTOR);
        }
        trace_cpu!(1;
            "{} PC={:04X}                         A={:02X} X={:02X} Y={:02X} P={:02X} SP={:02X} cyc={:<3} F/S/P={}/{:03}/{:03}",
            if nmi_hijack { "NMI " } else { "IRQ " },
            pc,
            self.a,
            self.x,
            self.y,
            _p,
            _sp,
            _cycle,
            _frame,
            _scanline,
            _pixel
        );
    }

    /// If an OAM DMA is pending (triggered by a write to $4014), execute it and
    /// return the number of CPU cycles consumed.
    ///
    /// This is a cycle-accurate implementation that:
    /// - Allows DMC DMA to interrupt the OAM DMA mid-transfer
    /// - Properly handles alignment cycles
    /// - Maintains correct get/put cycle semantics for DMC DMA collision
    ///
    /// Cycle cost (without DMC collision):
    /// - 513 cycles when DMA starts on a "put" cycle (odd total_cycles at DMA start)
    /// - 514 cycles when DMA starts on a "get" cycle (even total_cycles at DMA start)
    ///
    /// NES DMA timing:
    /// - DMA begins on the next CPU cycle after $4014 write
    /// - If $4014 written on even cycle: DMA starts on odd cycle = "put" = no extra align = 513
    /// - If $4014 written on odd cycle: DMA starts on even cycle = "get" = needs align = 514
    ///
    /// With DMC DMA collision:
    /// - DMC and OAM share cycles where possible
    /// - Extra 2-4 cycles depending on when collision occurs
    pub fn handle_oam_dma_if_pending(&mut self) -> Option<u16> {
        let page = self.bus.borrow_mut().take_oam_dma_page()?;

        let cycles_before = self.total_cycles;

        // Halt cycle - hijack the read the CPU was trying to do
        // This is shared by both OAM and DMC DMA if both are pending
        self.tick_single_dma_cycle();

        // Run the OAM DMA transfer with DMC collision handling
        self.run_oam_dma_internal_no_nmi(page);

        let dma_cycles = (self.total_cycles - cycles_before) as u16;

        // Check for NMI after DMA (outside of cycle count for backward compatibility)
        if self.ppu.borrow_mut().poll_nmi() {
            self.trigger_nmi_without_bus_cycles();
            self.tick_ppu_apu_for_cpu_cycles(7);
            self.add_cycles(7);
        }

        Some(dma_cycles)
    }

    /// Tick a single DMA cycle (advances PPU/APU by one CPU cycle).
    /// Also increments the internal CPU cycle counter for get/put cycle tracking.
    fn tick_single_dma_cycle(&mut self) {
        self.master_clock.advance_cpu_cycles(1);
        let ppu_cycles = self.master_clock.ppu_cycles_since_last();
        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles);

        let expansion = if self.mapper_has_expansion_audio {
            self.bus.borrow().mapper_expansion_audio_sample()
        } else {
            0.0
        };
        self.apu.borrow_mut().clock_with_expansion(expansion);

        // Synthetic cycles (DMA stalls) still advance mapper IRQ counters and expansion audio.
        self.bus.borrow_mut().mapper_cpu_cycle();
        self.end_cpu_cycle_latch_irq_line_only();

        // Increment internal cycle counter for get/put cycle tracking
        self.total_cycles += 1;
    }

    // Apply an external stall for the given number of CPU cycles.
    //
    // This is used for synthetic cycles where the CPU does not execute instructions
    // but the PPU/APU must continue to advance (e.g., DMA stalls).
    // pub fn apply_external_stall(&mut self, cpu_cycles: u16) {
    //     if cpu_cycles == 0 {
    //         return;
    //     }

    //     self.tick_ppu_apu_for_cpu_cycles(cpu_cycles);
    //     self.add_cycles(cpu_cycles as u64);
    // }

    fn tick_ppu_apu_for_cpu_cycles(&mut self, cpu_cycles: u16) {
        self.master_clock.advance_cpu_cycles(cpu_cycles as u64);
        let ppu_cycles = self.master_clock.ppu_cycles_since_last();
        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles);

        for _ in 0..cpu_cycles {
            let expansion = if self.mapper_has_expansion_audio {
                self.bus.borrow().mapper_expansion_audio_sample()
            } else {
                0.0
            };
            self.apu.borrow_mut().clock_with_expansion(expansion);

            // Synthetic cycles still advance mapper IRQ counters and expansion audio.
            self.bus.borrow_mut().mapper_cpu_cycle();
            self.end_cpu_cycle_latch_irq_line_only();
        }
    }

    fn end_cpu_cycle_latch_irq_line_only(&mut self) {
        // When ticking synthetic CPU cycles (e.g., DMA stalls), we need IRQ sampling to keep
        // running, but we must not poll/clear the PPU's edge-latched NMI flag.
        self.prev_run_irq = self.run_irq;

        let irq_asserted_from_apu = self.apu.borrow().poll_irq();
        let irq_asserted_from_mapper = if self.mapper_has_irq {
            self.bus.borrow().mapper_irq_pending()
        } else {
            false
        };
        self.irq_pending =
            irq_asserted_from_apu || irq_asserted_from_mapper || self.forced_irq_pending;

        self.run_irq = self.should_poll_irq();
    }

    fn trigger_nmi_without_bus_cycles(&mut self) {
        // Replicates `trigger_nmi` semantics without using `read`/`write` helpers,
        // so we don't accidentally advance CPU cycles or PPU timing here.
        self.delayed_i_flag = None;
        let pc = self.pc;

        // Push PC (high then low)
        let addr = 0x0100 | (self.sp as u16);
        self.bus.borrow_mut().write(addr, (pc >> 8) as u8, false);
        self.sp = self.sp.wrapping_sub(1);

        let addr = 0x0100 | (self.sp as u16);
        self.bus.borrow_mut().write(addr, (pc & 0xFF) as u8, false);
        self.sp = self.sp.wrapping_sub(1);

        // Push status (B clear, unused set)
        let mut p_with_break = self.p & !FLAG_BREAK;
        p_with_break |= FLAG_UNUSED;
        let addr = 0x0100 | (self.sp as u16);
        self.bus.borrow_mut().write(addr, p_with_break, false);
        self.sp = self.sp.wrapping_sub(1);

        // Read NMI vector and set PC
        let lo = self.bus.borrow_mut().read(NMI_VECTOR, false) as u16;
        let hi = self.bus.borrow_mut().read(NMI_VECTOR + 1, false) as u16;
        self.pc = (hi << 8) | lo;

        self.interrupt_stack.push(InterruptKind::Nmi);

        // Set Interrupt Disable flag
        self.p |= FLAG_INTERRUPT;
    }

    /// Reset the CPU.
    ///
    /// - `soft_reset`: true for a reset-button style reset, false for power-on.
    ///
    /// - On soft reset: preserve A/X/Y, decrement SP by 3, set I.
    /// - On hard reset: restore power-on register defaults, then run the reset sequence.
    /// - Takes 7 CPU cycles (5 internal + 2 vector reads)
    pub fn reset(&mut self, soft_reset: bool) {
        if !soft_reset {
            self.a = 0;
            self.x = 0;
            self.y = 0;
            self.sp = 0x00;
            self.p = FLAG_UNUSED;
        }

        // Set I flag (bit 2) and ensure unused bit is set.
        // Note: blargg cpu_reset/registers expects reset to not change other flags (e.g. D).
        self.p |= FLAG_INTERRUPT | FLAG_UNUSED;

        // Subtract 3 from SP (wrapping if necessary)
        self.sp = self.sp.wrapping_sub(3);

        // Clear cycle-accurate instruction state
        self.halted = false;
        self.delayed_i_flag = None;
        self.nmi_pending = false;
        self.irq_pending = false;
        self.forced_irq_pending = false;
        self.prev_need_nmi = false;
        self.prev_run_irq = false;
        self.run_irq = false;
        self.skip_interrupt_latch_this_cycle = false;
        self.interrupt_stack.clear();

        // Reset cycle counters and master clock alignment.
        self.total_cycles = 0;
        self.master_clock.reset();

        // Reset takes 7 CPU cycles total: 5 internal cycles + 2 reset-vector reads.
        for _ in 0..5 {
            self.internal_cycle();
        }

        // Read reset vector and set PC (2 CPU cycles via bus reads)
        self.pc = self.read_reset_vector();
    }

    fn internal_cycle(&mut self) {
        // Advance the CPU/PPU/APU by one CPU cycle without performing a bus read/write.
        if let Some((ref mut tick, total)) = self.current_tick_info {
            trace_cpu!(2;
                "tick ({}/{}) cyc={} [internal]",
                *tick,
                total,
                self.total_cycles
            );
            let _ = total;
            *tick += 1;
        } else {
            trace_cpu!(2; "tick cyc={} [internal]", self.total_cycles);
        }
        self.before_cpu_cycle(false);
        self.after_cpu_cycle(false);
    }

    /// Check if two addresses are on different pages
    fn page_crossed(addr1: u16, addr2: u16) -> bool {
        (addr1 & 0xFF00) != (addr2 & 0xFF00)
    }

    fn before_cpu_cycle(&mut self, is_write: bool) {
        self.master_clock.before_cpu_cycle(is_write);
        self.total_cycles += 1;
        let ppu_cycles = self.master_clock.ppu_cycles_since_last();
        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles);
        let expansion = if self.mapper_has_expansion_audio {
            self.bus.borrow().mapper_expansion_audio_sample()
        } else {
            0.0
        };
        self.apu.borrow_mut().clock_with_expansion(expansion);
    }

    fn after_cpu_cycle(&mut self, is_write: bool) {
        self.master_clock.after_cpu_cycle(is_write);
        let ppu_cycles = self.master_clock.ppu_cycles_since_last();
        self.ppu.borrow_mut().run_ppu_cycles(ppu_cycles);

        // Some mappers (e.g., Konami VRC) use CPU-cycle-driven IRQ counters.
        self.bus.borrow_mut().mapper_cpu_cycle();

        // Latch interrupt lines at the end of each CPU cycle.
        // Some edge cases require skipping latching for a single cycle.
        if self.skip_interrupt_latch_this_cycle {
            self.skip_interrupt_latch_this_cycle = false;
        } else {
            self.end_cpu_cycle_latch_interrupt_lines();
        }
    }

    /// Start a DMC DMA transfer.
    /// Called when the DMC sample buffer becomes empty and needs refilling.
    fn start_dmc_dma(&mut self) {
        self.dmc_dma_phase = DmcDmaPhase::Halt;
    }

    fn cpu_visible_dmc_dma_pending(&self) -> bool {
        matches!(self.dmc_dma_phase, DmcDmaPhase::Idle) && {
            let mut apu = self.apu.borrow_mut();
            apu.dmc_mut().cpu_dma_pending()
        }
    }

    /// Process any pending DMC DMA during a CPU read cycle.
    /// This is called from `read()` and handles the DMA state machine.
    ///
    /// DMC DMA sequence (per NESdev wiki):
    /// 1. Halt cycle: CPU read is repeated (discarded), consumes _needHalt
    /// 2. Dummy read cycle: CPU read is repeated (discarded), consumes _needDummyRead
    /// 3. Optional alignment cycle: if not on a "get" cycle, repeat read
    /// 4. Get cycle: actual DMC sample byte read
    fn process_pending_dmc_dma(&mut self, read_address: u16) -> Option<u8> {
        let mut observed_bus_value = None;

        // Loop until DMC DMA completes
        while !matches!(self.dmc_dma_phase, DmcDmaPhase::Idle) {
            match self.dmc_dma_phase {
                DmcDmaPhase::Idle => break,
                DmcDmaPhase::Halt => {
                    self.before_cpu_cycle(false);
                    let _ = self.bus.borrow_mut().read(read_address, true);
                    self.after_cpu_cycle(false);
                    self.dmc_dma_phase = DmcDmaPhase::Dummy;
                }
                DmcDmaPhase::Dummy => {
                    self.before_cpu_cycle(false);
                    let _ = self.bus.borrow_mut().read(read_address, true);
                    self.after_cpu_cycle(false);
                    self.dmc_dma_phase = DmcDmaPhase::Aligning;
                }
                DmcDmaPhase::Aligning => {
                    if !self.total_cycles.is_multiple_of(2) {
                        self.before_cpu_cycle(false);
                        let _ = self.bus.borrow_mut().read(read_address, true);
                        self.after_cpu_cycle(false);
                    }
                    self.dmc_dma_phase = DmcDmaPhase::Reading;
                }
                DmcDmaPhase::Reading => {
                    let dma_addr = {
                        let mut apu = self.apu.borrow_mut();
                        apu.dmc_mut().dma_address()
                    };

                    if let Some(addr) = dma_addr {
                        self.before_cpu_cycle(false);
                        let value = self.bus.borrow_mut().read(addr, false);
                        self.after_cpu_cycle(false);
                        self.apu.borrow_mut().dmc_mut().complete_dma_read(value);

                        if Self::is_controller_port2_read(read_address) {
                            observed_bus_value = Some(value);
                        }
                    }

                    self.dmc_dma_phase = DmcDmaPhase::Idle;
                }
            }
        }

        observed_bus_value
    }

    /// Process any pending DMA (OAM and/or DMC) during a CPU read cycle.
    /// Returns a DMA read outcome indicating whether to retry the read or return a bus value.
    fn process_pending_dma(&mut self, read_address: u16) -> DmaReadOutcome {
        // Check if OAM DMA is pending
        let oam_dma_pending = self.bus.borrow().oam_dma_pending();
        let dmc_dma_pending = self.cpu_visible_dmc_dma_pending();

        if !oam_dma_pending && !dmc_dma_pending {
            return DmaReadOutcome::NoDma;
        }

        // Note: The caller (read()) has already called before_cpu_cycle().
        // We complete that cycle here with the halt read and after_cpu_cycle().

        // If only DMC is pending (no OAM), handle it separately with existing logic
        if !oam_dma_pending && dmc_dma_pending {
            self.start_dmc_dma();

            let dmc_dma_address = {
                let mut apu = self.apu.borrow_mut();
                apu.dmc_mut().dma_address()
            };

            let is_controller_read = matches!(read_address, 0x4016 | 0x4017);
            let single_byte_dmc_fetch = self.dmc_pending_single_byte_fetch();
            let skip_first_input_clock = dmc_dma_address
                .map(|address| Self::should_skip_first_input_clock(read_address, address))
                .unwrap_or(false);
            let use_dummy_halt_read =
                is_controller_read && (!single_byte_dmc_fetch || skip_first_input_clock);

            // Halt cycle: complete the CPU cycle started by read() - the read value is discarded
            let halted_read_value = self
                .bus
                .borrow_mut()
                .read(read_address, use_dummy_halt_read);
            self.after_cpu_cycle(false);
            self.dmc_dma_phase = DmcDmaPhase::Dummy;

            // Process remaining DMC DMA cycles
            let observed_bus_value = self.process_pending_dmc_dma(read_address);

            if read_address == 0x4016 {
                if observed_bus_value.is_none() {
                    return DmaReadOutcome::RetryRead;
                }
                return DmaReadOutcome::ReturnValue(halted_read_value);
            }

            if let Some(value) = observed_bus_value {
                return DmaReadOutcome::ReturnValue(value);
            }

            return DmaReadOutcome::RetryRead;
        }

        // OAM DMA is pending (possibly with DMC collision)
        // Halt cycle: complete the CPU cycle started by read() - the read value is discarded
        let _ = self.bus.borrow_mut().read(read_address, false);
        self.after_cpu_cycle(false);

        // Now run the OAM DMA (which handles DMC collision internally)
        let page = self.bus.borrow_mut().take_oam_dma_page();
        if let Some(page) = page {
            self.run_oam_dma_internal(page);
        }

        DmaReadOutcome::RetryRead
    }

    /// Run OAM DMA internally (called from process_pending_dma or handle_oam_dma_if_pending).
    /// This is the actual OAM DMA loop with DMC collision handling.
    ///
    /// If `handle_nmi` is true, checks for and handles NMI after DMA completes.
    ///
    /// Note: The caller is responsible for the halt cycle. If DMC is pending at start,
    /// it shares that halt cycle which the caller has already consumed.
    fn run_oam_dma_internal_impl(&mut self, page: u8, handle_nmi: bool) {
        let source_base = (page as u16) << 8;

        // Check if DMC DMA is also pending at the start
        let dmc_pending_at_start = self.cpu_visible_dmc_dma_pending();

        // DMC DMA progress states (like Pinky)
        const DMC_IDLE: u8 = 0;
        const DMC_HALT_DONE: u8 = 1;
        const DMC_READY_TO_READ: u8 = 2;

        // OAM size in bytes
        const OAM_SIZE: u16 = 256;

        // Track DMC DMA progress
        // If DMC is pending at start, it shares the halt cycle that the caller consumed
        let mut dmc_progress: u8 = if dmc_pending_at_start {
            DMC_HALT_DONE
        } else {
            DMC_IDLE
        };
        let mut sprite_dma_value: Option<u8> = None;
        let mut sprite_offset: u16 = 0;
        let mut sprite_dma_done = false;

        loop {
            // Check DMC progress
            let dmc_pending = self.cpu_visible_dmc_dma_pending();

            // If DMC becomes pending during OAM DMA, start tracking it
            if dmc_pending && dmc_progress == DMC_IDLE {
                dmc_progress = DMC_HALT_DONE; // halt done (shared with OAM cycle)
            }

            // Helper: are we on a get or put phase?
            let on_get_phase = self.total_cycles.is_multiple_of(2);
            let on_put_phase = !on_get_phase;

            // Can DMC read this cycle?
            let dmc_ready_to_read =
                dmc_pending && dmc_progress >= DMC_READY_TO_READ && on_get_phase;

            // Can OAM read this cycle?
            let oam_ready_to_read = !sprite_dma_done && sprite_dma_value.is_none() && on_get_phase;

            // Can OAM write this cycle?
            let oam_ready_to_write = !sprite_dma_done && sprite_dma_value.is_some() && on_put_phase;

            if dmc_ready_to_read {
                // DMC takes priority - do the DMC read
                let dma_addr = {
                    let mut apu = self.apu.borrow_mut();
                    apu.dmc_mut().dma_address()
                };

                if let Some(addr) = dma_addr {
                    let value = self.bus.borrow_mut().read(addr, false);
                    self.apu.borrow_mut().dmc_mut().complete_dma_read(value);
                }
                self.tick_single_dma_cycle();
                dmc_progress = DMC_IDLE; // DMC done
            } else if oam_ready_to_read {
                // OAM read cycle - also counts as DMC dummy if DMC is waiting
                if dmc_pending && dmc_progress == DMC_HALT_DONE {
                    dmc_progress = DMC_READY_TO_READ; // dummy done
                }
                let addr = source_base.wrapping_add(sprite_offset);
                let value = self.bus.borrow_mut().read(addr, false);
                sprite_dma_value = Some(value);
                self.tick_single_dma_cycle();
            } else if oam_ready_to_write {
                // OAM write cycle - also counts as DMC alignment if DMC is waiting
                if dmc_pending && dmc_progress == DMC_HALT_DONE {
                    dmc_progress = DMC_READY_TO_READ; // alignment done
                }
                self.ppu
                    .borrow_mut()
                    .write_oam_data_dma(sprite_dma_value.take().unwrap());
                self.tick_single_dma_cycle();
                sprite_offset += 1;
                if sprite_offset >= OAM_SIZE {
                    sprite_dma_done = true;
                }
            } else if dmc_pending || !sprite_dma_done {
                // Alignment/dummy cycle
                if dmc_pending && dmc_progress == DMC_HALT_DONE {
                    dmc_progress = DMC_READY_TO_READ;
                }
                self.tick_single_dma_cycle();
            } else {
                // All done
                break;
            }
        }

        // Check for NMI after DMA (only if requested)
        if handle_nmi && self.ppu.borrow_mut().poll_nmi() {
            self.trigger_nmi_without_bus_cycles();
            self.tick_ppu_apu_for_cpu_cycles(7);
            self.add_cycles(7);
        }
    }

    /// Run OAM DMA without NMI handling (for handle_oam_dma_if_pending which handles NMI separately)
    fn run_oam_dma_internal_no_nmi(&mut self, page: u8) {
        self.run_oam_dma_internal_impl(page, false);
    }

    /// Run OAM DMA with NMI handling (for process_pending_dma)
    fn run_oam_dma_internal(&mut self, page: u8) {
        self.run_oam_dma_internal_impl(page, true);
    }

    /// Read a byte from memory at the specified address
    fn read(&mut self, addr: u16) -> u8 {
        self.read_with_dummy_flag(addr, false)
    }

    /// Dummy read a byte from memory at the specified address
    fn dummy_read(&mut self, addr: u16) -> u8 {
        self.read_with_dummy_flag(addr, true)
    }

    fn read_with_dummy_flag(&mut self, addr: u16, is_dummy_read: bool) -> u8 {
        loop {
            if let Some((ref mut tick, total)) = self.current_tick_info {
                trace_cpu!(2;
                    "tick ({}/{}) cyc={} [read] addr=0x{:04X}",
                    *tick,
                    total,
                    self.total_cycles,
                    addr
                );
                let _ = total;
                *tick += 1;
            } else {
                trace_cpu!(2; "tick cyc={} [read] addr=0x{:04X}", self.total_cycles, addr);
            }

            self.before_cpu_cycle(false);

            // Process any pending DMA (OAM and/or DMC)
            match self.process_pending_dma(addr) {
                DmaReadOutcome::NoDma => {}
                DmaReadOutcome::RetryRead => {
                    // DMA was processed; retry the read from the beginning
                    continue;
                }
                DmaReadOutcome::ReturnValue(value) => return value,
            }

            let value = self.bus.borrow_mut().read(addr, is_dummy_read);

            self.after_cpu_cycle(false);
            return value;
        }
    }

    /// Read a 16-bit word from memory at the specified address
    fn read_u16(&mut self, addr: u16) -> u16 {
        self.read(addr) as u16 | ((self.read(addr + 1) as u16) << 8)
    }

    /// Write a byte to memory at the specified address
    fn write(&mut self, addr: u16, value: u8, dummy: bool) {
        if let Some((ref mut tick, total)) = self.current_tick_info {
            trace_cpu!(2;
                "tick ({}/{}) cyc={} [write{}] addr=0x{:04X} value=0x{:02X}",
                *tick,
                total,
                self.total_cycles,
                if dummy { " (dummy)" } else { "" },
                addr,
                value
            );
            let _ = total;
            *tick += 1;
        } else {
            trace_cpu!(2;
                "tick cyc={} [write{}] addr=0x{:04X} value=0x{:02X}",
                self.total_cycles,
                if dummy { " (dummy)" } else { "" },
                addr,
                value
            );
        }
        self.before_cpu_cycle(true);
        self.bus.borrow_mut().write(addr, value, dummy);
        if !dummy {
            self.last_cpu_write_addr = Some(addr);
        }
        self.after_cpu_cycle(true);
    }

    /// Dummy write a byte to memory at the specified address
    fn dummy_write(&mut self, addr: u16, value: u8) {
        self.write(addr, value, true);
    }

    /// Read a byte from memory at PC and increment PC
    fn read_byte_from_pc(&mut self) -> u8 {
        let value = self.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    /// Perform a read-modify-write operation with dummy write
    /// All RMW instructions on the 6502 first write the original value back,
    /// Read a 16-bit word from memory at PC (little-endian) and increment PC
    fn read_word_from_pc(&mut self) -> u16 {
        let lo = self.read_byte_from_pc() as u16;
        let hi = self.read_byte_from_pc() as u16;
        (hi << 8) | lo
    }

    /// Read a 16-bit address from the reset vector at 0xFFFC-0xFFFD
    fn read_reset_vector(&mut self) -> u16 {
        self.read_u16(RESET_VECTOR)
    }

    /// Read a 16-bit word from zero page (wraps at page boundary)
    fn read_word_from_zp(&mut self, addr: u8) -> u16 {
        let lo = self.read(addr as u16) as u16;
        let hi = self.read(addr.wrapping_add(1) as u16) as u16;
        (hi << 8) | lo
    }

    /// Read a word from an indirect address with 6502 page boundary bug
    /// If the address is at a page boundary (e.g., 0x10FF), the high byte
    /// is read from the start of the same page (0x1000) instead of the next page (0x1100)
    fn read_word_indirect(&mut self, addr: u16) -> u16 {
        let lo = self.read(addr) as u16;
        let hi_addr = if addr & 0xFF == 0xFF {
            // Page boundary bug: wrap within the same page
            addr & 0xFF00
        } else {
            addr + 1
        };
        let hi = self.read(hi_addr) as u16;
        (hi << 8) | lo
    }

    /// Push a byte onto the stack
    fn push_byte(&mut self, value: u8) {
        let addr = 0x0100 | (self.sp as u16);
        self.write(addr, value, false);
        self.sp = self.sp.wrapping_sub(1);
    }

    /// Push a word onto the stack (high byte first)
    fn push_word(&mut self, value: u16) {
        self.push_byte((value >> 8) as u8); // High byte first
        self.push_byte(value as u8); // Low byte second
    }

    /// Pull a byte from the stack
    fn pop_byte(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        let addr = 0x0100 | (self.sp as u16);
        self.read(addr)
    }

    /// Pull a word from the stack (low byte first)
    fn pop_word(&mut self) -> u16 {
        let lo = self.pop_byte() as u16; // Low byte first
        let hi = self.pop_byte() as u16; // High byte second
        (hi << 8) | lo
    }

    /// Update Zero and Negative flags based on a value
    fn update_zero_and_negative_flags(&mut self, value: u8) {
        // Clear Z and N flags
        self.p &= !(FLAG_ZERO | FLAG_NEGATIVE);

        // Set Zero flag if value is 0
        if value == 0 {
            self.p |= FLAG_ZERO;
        }

        // Set Negative flag if bit 7 is set
        if value & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        }
    }

    /// Add with Carry - ADC operation
    fn adc(&mut self, value: u8) {
        let carry = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        let sum = self.a as u16 + value as u16 + carry as u16;

        // Check for carry (result > 255)
        if sum > 0xFF {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        // Check for overflow
        // Overflow occurs when:
        // - Two positive numbers add to a negative result
        // - Two negative numbers add to a positive result
        let result = sum as u8;
        let overflow = (self.a ^ result) & (value ^ result) & 0x80;
        if overflow != 0 {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }

        self.a = result;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Bitwise AND - AND operation
    fn and(&mut self, value: u8) {
        self.a &= value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Arithmetic Shift Left - ASL operation
    fn asl(&mut self, value: u8) -> u8 {
        let carry = if value & 0x80 != 0 { FLAG_CARRY } else { 0 };
        let result = value << 1;
        self.p = (self.p & !FLAG_CARRY) | carry;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Bit Test - BIT operation
    fn bit(&mut self, value: u8) {
        // Test bits: Zero flag is set based on A & value
        let result = self.a & value;
        if result == 0 {
            self.p |= FLAG_ZERO;
        } else {
            self.p &= !FLAG_ZERO;
        }

        // Copy bit 7 of value to Negative flag
        if value & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        } else {
            self.p &= !FLAG_NEGATIVE;
        }

        // Copy bit 6 of value to Overflow flag
        if value & 0x40 != 0 {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }
    }

    /// Compare operation - sets flags based on register - value
    fn compare(&mut self, register_value: u8, value: u8) {
        let result = register_value.wrapping_sub(value);

        // Set Carry flag if register >= value
        if register_value >= value {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        // Set Zero flag if register == value
        if register_value == value {
            self.p |= FLAG_ZERO;
        } else {
            self.p &= !FLAG_ZERO;
        }

        // Set Negative flag based on bit 7 of result
        if result & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        } else {
            self.p &= !FLAG_NEGATIVE;
        }
    }

    /// Compare - CMP operation
    fn cmp(&mut self, value: u8) {
        self.compare(self.a, value);
    }

    /// Compare X Register - CPX operation
    fn cpx(&mut self, value: u8) {
        self.compare(self.x, value);
    }

    /// Compare Y Register - CPY operation
    fn cpy(&mut self, value: u8) {
        self.compare(self.y, value);
    }

    /// Decrement - DEC operation
    fn dec(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Decrement and Compare - DCP undocumented operation
    fn dcp(&mut self, addr: u16) {
        let value = self.read(addr);
        self.dummy_write(addr, value);
        // Real operation and write
        let result = self.dec(value);
        self.write(addr, result, false);
        self.cmp(result);
    }

    /// Load Accumulator and X - LAR undocumented operation
    /// Also known as LAS. ANDs memory with stack pointer, stores result in A, X, and SP
    fn lar(&mut self, value: u8) {
        let result = self.sp & value;
        self.a = result;
        self.x = result;
        self.sp = result;
        self.update_zero_and_negative_flags(result);
    }

    /// AXS - undocumented operation
    /// Also known as SBX. Performs (A & X) - value -> X with carry flag behavior
    fn axs(&mut self, value: u8) {
        let and_result = self.a & self.x;
        let (result, borrow) = and_result.overflowing_sub(value);
        self.x = result;
        // Set carry flag if no borrow occurred (like CMP/CPX/CPY)
        self.p = (self.p & !FLAG_CARRY) | if !borrow { FLAG_CARRY } else { 0 };
        self.update_zero_and_negative_flags(self.x);
    }

    /// ISB - undocumented operation
    /// Also known as ISC. Increments memory then performs SBC
    fn isb(&mut self, addr: u16) {
        let value = self.read(addr);
        self.dummy_write(addr, value);
        // Increment the value
        let result = value.wrapping_add(1);
        // Write back
        self.write(addr, result, false);
        // Perform SBC with the incremented value
        self.sbc(result);
    }

    /// Exclusive OR - EOR operation
    fn eor(&mut self, value: u8) {
        self.a ^= value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Increment - INC operation
    fn inc(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        self.update_zero_and_negative_flags(result);
        result
    }

    /// RLA - Undocumented opcode: Rotate left memory then AND with accumulator
    fn rla(&mut self, addr: u16) {
        let value = self.read(addr);
        // Dummy write
        self.dummy_write(addr, value);
        // Real operation and write
        let rotated = self.rol(value);
        self.write(addr, rotated, false);
        self.a &= rotated;
        self.update_zero_and_negative_flags(self.a);
    }

    /// RRA - Undocumented opcode: Rotate right memory then ADC with accumulator
    fn rra(&mut self, addr: u16) {
        let value = self.read(addr);
        // Dummy write
        self.dummy_write(addr, value);
        // Real operation and write
        let rotated = self.ror(value);
        self.write(addr, rotated, false);
        self.adc(rotated);
    }

    /// SLO - Undocumented opcode: Shift left memory then ORA with accumulator
    fn slo(&mut self, addr: u16) {
        let value = self.read(addr);
        // Dummy write
        self.dummy_write(addr, value);
        // Real operation and write
        let shifted = self.asl(value);
        self.write(addr, shifted, false);
        self.ora(shifted);
    }

    /// SRE - Undocumented opcode: Shift right memory then EOR with accumulator
    fn sre(&mut self, addr: u16) {
        let value = self.read(addr);
        // Dummy write
        self.dummy_write(addr, value);
        // Real operation and write
        let shifted = self.lsr(value);
        self.write(addr, shifted, false);
        self.eor(shifted);
    }

    /// Load Accumulator - LDA operation
    fn lda(&mut self, value: u8) {
        self.a = value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Load X Register - LDX operation
    fn ldx(&mut self, value: u8) {
        self.x = value;
        self.update_zero_and_negative_flags(self.x);
    }

    /// Load Y Register - LDY operation
    fn ldy(&mut self, value: u8) {
        self.y = value;
        self.update_zero_and_negative_flags(self.y);
    }

    /// Logical Shift Right - LSR operation
    fn lsr(&mut self, value: u8) -> u8 {
        // Bit 0 goes into carry flag
        if value & 0b00000001 != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        let result = value >> 1;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Logical Inclusive OR - ORA operation
    fn ora(&mut self, value: u8) {
        self.set_a(self.a | value);
    }

    /// Decrement X Register - DEX operation
    fn dex(&mut self) {
        self.x = self.x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.x);
    }

    /// Decrement Y Register - DEY operation
    fn dey(&mut self) {
        self.y = self.y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.y);
    }

    /// Increment Y Register - INY operation
    fn iny(&mut self) {
        self.y = self.y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.y);
    }

    /// Increment X Register - INX operation
    fn inx(&mut self) {
        self.x = self.x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.x);
    }

    /// Transfer Accumulator to X - TAX operation
    fn tax(&mut self) {
        self.x = self.a;
        self.update_zero_and_negative_flags(self.x);
    }

    /// Transfer Accumulator to Y - TAY operation
    fn tay(&mut self) {
        self.y = self.a;
        self.update_zero_and_negative_flags(self.y);
    }

    /// Transfer X to Accumulator - TXA operation
    fn txa(&mut self) {
        self.a = self.x;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Transfer Y to Accumulator - TYA operation
    fn tya(&mut self) {
        self.a = self.y;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Rotate Left - ROL operation
    fn rol(&mut self, value: u8) -> u8 {
        let old_carry = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        // Bit 7 goes into carry flag
        if value & 0b10000000 != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        let result = (value << 1) | old_carry;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Rotate Right - ROR operation
    fn ror(&mut self, value: u8) -> u8 {
        let old_carry = if self.p & FLAG_CARRY != 0 {
            0b10000000
        } else {
            0
        };
        // Bit 0 goes into carry flag
        if value & 0b00000001 != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        let result = (value >> 1) | old_carry;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Subtract with Carry - SBC operation
    fn sbc(&mut self, value: u8) {
        // SBC is equivalent to ADC with inverted value
        // A - M - (1 - C) = A + ~M + C
        let carry_in = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        let inverted_value = !value;
        let result = self.a as u16 + inverted_value as u16 + carry_in;

        // Set carry flag if no borrow occurred (result >= 0x100)
        if result >= 0x100 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        // Set overflow flag if signed overflow occurred
        // Overflow occurs when subtracting different signs yields wrong sign
        // Same logic as ADC but with inverted value
        let a_sign = self.a & 0x80;
        let m_sign = inverted_value & 0x80;
        let result_sign = (result as u8) & 0x80;
        if a_sign == m_sign && a_sign != result_sign {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }

        self.a = result as u8;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Get the effective I flag value for IRQ polling
    /// If there's a delayed I flag value, use that; otherwise use the current I flag
    fn get_effective_i_flag(&self) -> bool {
        self.delayed_i_flag
            .unwrap_or((self.p & FLAG_INTERRUPT) != 0)
    }

    /// Check if IRQ should be allowed to trigger
    /// Returns true if an IRQ is pending and the effective I flag (considering delays) allows IRQs
    pub fn should_poll_irq(&self) -> bool {
        self.irq_pending && !self.get_effective_i_flag()
    }

    /// Set the NMI pending flag (test-only helper).
    ///
    /// Some unit tests need to force an NMI edge before the next instruction.
    #[cfg(test)]
    pub(crate) fn set_nmi_pending(&mut self, pending: bool) {
        self.nmi_pending = pending;
    }

    /// Set the IRQ pending flag
    /// This should be called by the NES loop when IRQ is detected
    #[cfg(test)]
    pub(crate) fn set_irq_pending(&mut self, pending: bool) {
        self.forced_irq_pending = pending;
        // Preserve prior unit-test behavior where `set_irq_pending(true)` makes
        // `should_poll_irq()` immediately reflect an asserted IRQ.
        self.irq_pending = pending;
    }

    fn get_operand_value(&mut self, op: &OpCode, operand: u16) -> u8 {
        match op.mode {
            AddrMode::IMM => operand as u8,
            AddrMode::ZP
            | AddrMode::ZPX
            | AddrMode::ZPY
            | AddrMode::ABS
            | AddrMode::ABSX
            | AddrMode::ABSY
            | AddrMode::IND
            | AddrMode::INDX
            | AddrMode::INDY => self.read(operand),
            AddrMode::IMP | AddrMode::ACC | AddrMode::REL => operand as u8,
            _ => panic!("Unhandled addressing mode: {}", op.mode),
        }
    }

    fn set_a(&mut self, value: u8) {
        self.a = value;
        self.update_zero_and_negative_flags(self.a);
    }

    fn exec_arr_illegal(&mut self, imm: u8) {
        // ARR (undocumented): AND with immediate, then ROR, with special flag handling.
        // Flags on 2A03:
        // - C = bit 6 of result
        // - V = bit 6 XOR bit 5 of result
        self.a &= imm;

        let old_carry = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        self.a = (self.a >> 1) | (old_carry << 7);

        self.update_zero_and_negative_flags(self.a);

        if (self.a & 0x40) != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        let bit6 = (self.a >> 6) & 1;
        let bit5 = (self.a >> 5) & 1;
        if (bit6 ^ bit5) != 0 {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }
    }

    fn exec_sya_illegal(&mut self, addr: u16) {
        // *SYA/SHY (undocumented): Store Y AND (high byte of BASE address + 1).
        // Quirk: on page crossing, the high byte of the target address is ANDed with Y.
        let base_addr = addr.wrapping_sub(self.x as u16);
        let base_high_byte = (base_addr >> 8) as u8;
        let value = self.y & base_high_byte.wrapping_add(1);

        let page_crossed = Self::page_crossed(base_addr, addr);
        let final_addr = if page_crossed {
            let modified_high = ((addr >> 8) as u8) & self.y;
            ((modified_high as u16) << 8) | (addr & 0x00FF)
        } else {
            addr
        };

        self.write(final_addr, value, false);
    }

    fn exec_sxa_illegal(&mut self, addr: u16) {
        // *SXA/SHX (undocumented): Store X AND (high byte of BASE address + 1).
        // Quirk: on page crossing, the high byte of the target address is ANDed with X.
        let base_addr = addr.wrapping_sub(self.y as u16);
        let base_high_byte = (base_addr >> 8) as u8;
        let value = self.x & base_high_byte.wrapping_add(1);

        let page_crossed = Self::page_crossed(base_addr, addr);
        let final_addr = if page_crossed {
            let modified_high = ((addr >> 8) as u8) & self.x;
            ((modified_high as u16) << 8) | (addr & 0x00FF)
        } else {
            addr
        };

        self.write(final_addr, value, false);
    }

    pub fn execute(&mut self) {
        if self.halted {
            return;
        }

        self.last_cpu_write_addr = None;

        // The CPU's IRQ inhibit flag (I) has a one-instruction delay behavior for
        // CLI/SEI and (conditionally) PLP. We model that using `delayed_i_flag`:
        // when set, `should_poll_irq()` uses the old I value for one instruction.
        let had_delayed_i_flag = self.delayed_i_flag.is_some();
        let mut new_delayed_i_flag: Option<bool> = None;

        // Trace CPU tick before reading opcode (so PC is correct for the instruction)
        // Read instruction bytes for tracing without advancing PC
        // Use read_for_testing to avoid affecting the open bus state
        // Only execute this code when CPU tracing is actually enabled
        #[cfg(debug_assertions)]
        if crate::platform::debugging::is_cpu_tracing_enabled() {
            let pc = self.pc;
            let mut memory = self.bus.borrow_mut();
            let opcode_byte = memory.read_for_testing(pc);
            let op = super::opcode::lookup(opcode_byte);
            let byte1 = if op.bytes() > 1 {
                memory.read_for_testing(pc.wrapping_add(1))
            } else {
                0
            };
            let byte2 = if op.bytes() > 2 {
                memory.read_for_testing(pc.wrapping_add(2))
            } else {
                0
            };
            drop(memory); // Release borrow before trace macro may do other operations
            let hex_dump = match op.bytes() {
                1 => format!("{:02X}", opcode_byte),
                2 => format!("{:02X} {:02X}", opcode_byte, byte1),
                _ => format!("{:02X} {:02X} {:02X}", opcode_byte, byte1, byte2),
            };
            let asm = match op.mode {
                AddrMode::IMP => op.mnemonic.to_string(),
                AddrMode::ACC => format!("{} A", op.mnemonic),
                AddrMode::IMM => format!("{} #${:02X}", op.mnemonic, byte1),
                AddrMode::ZP => format!("{} ${:02X}", op.mnemonic, byte1),
                AddrMode::ZPX => format!("{} ${:02X},X", op.mnemonic, byte1),
                AddrMode::ZPY => format!("{} ${:02X},Y", op.mnemonic, byte1),
                AddrMode::ABS => format!(
                    "{} ${:04X}",
                    op.mnemonic,
                    u16::from_le_bytes([byte1, byte2])
                ),
                AddrMode::ABSX | AddrMode::ABSXW => format!(
                    "{} ${:04X},X",
                    op.mnemonic,
                    u16::from_le_bytes([byte1, byte2])
                ),
                AddrMode::ABSY | AddrMode::ABSYW => format!(
                    "{} ${:04X},Y",
                    op.mnemonic,
                    u16::from_le_bytes([byte1, byte2])
                ),
                AddrMode::IND => format!(
                    "{} (${:04X})",
                    op.mnemonic,
                    u16::from_le_bytes([byte1, byte2])
                ),
                AddrMode::INDX => format!("{} (${:02X},X)", op.mnemonic, byte1),
                AddrMode::INDY | AddrMode::INDYW => format!("{} (${:02X}),Y", op.mnemonic, byte1),
                AddrMode::REL => {
                    let offset = byte1 as i8;
                    let target = pc.wrapping_add(2).wrapping_add(offset as u16);
                    format!("{} ${:04X}", op.mnemonic, target)
                }
            };
            // Set up tick tracking for this instruction
            self.current_tick_info = Some((1, op.cycles));
            trace_cpu!(1;
                "exec PC={:04X} {:08} {:14} A={:02X} X={:02X} Y={:02X} P={:02X} SP={:02X} cyc={:<3} F/S/P={}/{:03}/{:03}",
                pc,
                hex_dump,
                asm,
                self.a,
                self.x,
                self.y,
                self.p,
                self.sp,
                self.total_cycles,
                self.ppu.borrow().timing().frame_count(),
                self.ppu.borrow().timing().scanline(),
                self.ppu.borrow().timing().pixel()
            );
        }

        let opcode = self.read_byte_from_pc();
        let op = super::opcode::lookup(opcode);
        let operand = self.get_operand(*op);

        match op.mnemonic {
            Mnemonic::BRK => {
                // BRK pushes (PC + 1), which corresponds to BRK+2 overall.
                // At this point, PC points to the padding byte, so add 1.
                self.push_word(self.pc.wrapping_add(1));

                let flags = self.p | FLAG_BREAK | FLAG_UNUSED;

                if self.nmi_pending {
                    self.nmi_pending = false;
                    self.push_byte(flags);
                    self.p |= FLAG_INTERRUPT;
                    self.pc = self.read_u16(NMI_VECTOR);
                } else {
                    self.push_byte(flags);
                    self.p |= FLAG_INTERRUPT;
                    self.pc = self.read_u16(IRQ_VECTOR);
                }

                // Ensure we don't start an NMI immediately after BRK.
                self.prev_need_nmi = false;
            }
            Mnemonic::ORA => {
                let value = self.get_operand_value(op, operand);
                self.ora(value);
            }
            Mnemonic::HLT | Mnemonic::KIL => {
                self.halted = true;
                // Halt on instruction, not after
                self.pc -= 1;
            }
            Mnemonic::USLO => {
                self.slo(operand);
            }
            Mnemonic::NOP | Mnemonic::UNOP => {
                // Consume one cycle
                self.get_operand_value(op, operand);
            }
            Mnemonic::ASL => {
                match op.mode {
                    AddrMode::ACC => {
                        self.a = self.asl(self.a);
                    }
                    _ => {
                        let value = self.read(operand);
                        self.dummy_write(operand, value);
                        let result = self.asl(value);
                        self.write(operand, result, false); // real write
                    }
                }
            }
            Mnemonic::PHP => {
                // Push processor status with BREAK and UNUSED flags set
                let flags = self.p | FLAG_BREAK | FLAG_UNUSED;
                self.push_byte(flags);
            }
            Mnemonic::UAAC => {
                // Undocumented: AND with accumulator, then copy bit 7 to carry
                let value = self.get_operand_value(op, operand);
                self.a &= value;
                self.update_zero_and_negative_flags(self.a);
                // Copy bit 7 to carry flag (same pattern as ASL)
                let carry = if self.a & 0x80 != 0 { FLAG_CARRY } else { 0 };
                self.p = (self.p & !FLAG_CARRY) | carry;
            }
            Mnemonic::BPL => {
                // Branch if negative flag is clear
                let offset = operand as i8;
                if self.p & FLAG_NEGATIVE == 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        // Branch taken - do a dummy read
                        self.dummy_read(self.pc);
                        // Page crossing: extra dummy read
                        self.dummy_read(self.pc);
                    } else {
                        // Taken non-page-crossing branches ignore interrupts during their last
                        // clock (blargg cpu_interrupts_v2/5-branch_delays_irq).
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::CLC => {
                self.p &= !FLAG_CARRY;
            }
            Mnemonic::JSR => {
                // JSR takes 6 cycles:
                // 1. Fetch opcode
                // 2. Fetch low byte of address
                // 3. Internal operation (dummy read from stack)
                // 4. Push PCH to stack
                // 5. Push PCL to stack
                // 6. Fetch high byte of address

                // Dummy read from stack pointer for cycle 3
                self.dummy_read(0x0100 | (self.sp as u16));

                // Push return address (PC - 1) to stack
                // PC is already pointing to the next instruction, so PC - 1 is the last byte of JSR
                let return_addr = self.pc.wrapping_sub(1);
                self.push_word(return_addr);

                // Set PC to target address
                self.pc = operand;
            }
            Mnemonic::AND => {
                let value = self.get_operand_value(op, operand);
                self.and(value);
            }
            Mnemonic::URLA => {
                self.rla(operand);
            }
            Mnemonic::BIT => {
                let value = self.read(operand);
                self.bit(value);
            }
            Mnemonic::ROL => {
                match op.mode {
                    AddrMode::ACC => {
                        self.a = self.rol(self.a);
                    }
                    _ => {
                        let value = self.read(operand);
                        self.dummy_write(operand, value);
                        let result = self.rol(value);
                        self.write(operand, result, false); // real write
                    }
                }
            }
            Mnemonic::PLP => {
                // Dummy read from current SP (cycle 2)
                self.dummy_read(0x0100 | (self.sp as u16));
                // Pop status from stack
                let status = self.pop_byte();
                // Restore flags, but always set UNUSED and clear BREAK
                let old_i_flag = (self.p & FLAG_INTERRUPT) != 0;
                self.p = (status & !FLAG_BREAK) | FLAG_UNUSED;
                let new_i_flag = (self.p & FLAG_INTERRUPT) != 0;

                // If PLP changes I, IRQ polling uses the OLD value for the next instruction.
                if old_i_flag != new_i_flag {
                    new_delayed_i_flag = Some(old_i_flag);
                }
            }
            Mnemonic::BMI => {
                // Branch if negative flag is set
                let offset = operand as i8;
                if self.p & FLAG_NEGATIVE != 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::SEC => {
                self.p |= FLAG_CARRY;
            }
            Mnemonic::RTI => {
                // RTI (Return from Interrupt) - 6 cycles
                // Cycle 1: Fetch opcode (already done)
                // Cycle 2: Dummy read from current PC
                self.dummy_read(self.pc);

                // Cycle 3: Increment SP (dummy read happens in pop_byte)
                // Cycle 4: Pull status from stack
                let status = self.pop_byte();
                // Restore flags, ignoring BREAK, always setting UNUSED
                self.p = (status & !FLAG_BREAK) | FLAG_UNUSED;

                // RTI clears the delayed I flag immediately (special case)
                self.delayed_i_flag = None;

                // Cycle 5-6: Pull PC from stack (low byte, then high byte)
                self.pc = self.pop_word();

                // Leaving interrupt handler.
                let _ = self.interrupt_stack.pop();
            }
            Mnemonic::EOR => {
                let value = self.get_operand_value(op, operand);
                self.eor(value);
            }
            Mnemonic::USRE => {
                self.sre(operand);
            }
            Mnemonic::LSR => {
                match op.mode {
                    AddrMode::ACC => {
                        self.a = self.lsr(self.a);
                    }
                    _ => {
                        let value = self.read(operand);
                        self.dummy_write(operand, value);
                        let result = self.lsr(value);
                        self.write(operand, result, false); // real write
                    }
                }
            }
            Mnemonic::PHA => {
                // Push accumulator to stack
                self.push_byte(self.a);
            }
            Mnemonic::UASR => {
                // ASR/ALR (undocumented): AND with immediate, then LSR
                let value = self.get_operand_value(op, operand);
                self.a &= value;
                self.a = self.lsr(self.a);
            }
            Mnemonic::JMP => {
                // Jump to address (operand is already the target address)
                self.pc = operand;
            }
            Mnemonic::BVC => {
                // Branch on overflow clear
                let offset = operand as i8;
                if (self.p & FLAG_OVERFLOW) == 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::CLI => {
                // Save old I before clearing. IRQ polling uses the OLD value for the next instruction.
                let old_i_flag = (self.p & FLAG_INTERRUPT) != 0;
                self.p &= !FLAG_INTERRUPT;
                new_delayed_i_flag = Some(old_i_flag);
            }
            Mnemonic::RTS => {
                // Return from subroutine - 6 cycles:
                // Cycle 1: Fetch opcode (already done)
                // Cycle 2: Dummy read from current S
                self.dummy_read(0x0100 | (self.sp as u16));

                // Cycle 3-4: Pull return address from stack
                let addr = self.pop_word();

                // Cycle 5: Increment PC (PC = popped_value + 1)
                self.pc = addr.wrapping_add(1);

                // Cycle 6: Dummy read at incremented PC
                self.dummy_read(self.pc);
            }
            Mnemonic::ADC => {
                let value = self.get_operand_value(op, operand);
                self.adc(value);
            }
            Mnemonic::URRA => {
                self.rra(operand);
            }
            Mnemonic::ROR => {
                match op.mode {
                    AddrMode::ACC => {
                        self.a = self.ror(self.a);
                    }
                    _ => {
                        let value = self.read(operand);
                        self.dummy_write(operand, value);
                        let result = self.ror(value);
                        self.write(operand, result, false); // real write
                    }
                }
            }
            Mnemonic::PLA => {
                // Pull accumulator from stack - 4 cycles:
                // Cycle 1: Fetch opcode (already done)
                // Cycle 2: Dummy read at current PC
                self.dummy_read(self.pc);

                // Cycle 3: Increment SP (dummy read happens in pop_byte)
                // Cycle 4: Pull value from stack
                self.a = self.pop_byte();
                self.update_zero_and_negative_flags(self.a);
            }
            Mnemonic::UARR => {
                let value = self.get_operand_value(op, operand);
                self.exec_arr_illegal(value);
            }
            Mnemonic::BVS => {
                // Branch on overflow set
                let offset = operand as i8;
                if (self.p & FLAG_OVERFLOW) != 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::SEI => {
                // Save old I before setting. IRQ polling uses the OLD value for the next instruction.
                let old_i_flag = (self.p & FLAG_INTERRUPT) != 0;
                self.p |= FLAG_INTERRUPT;
                new_delayed_i_flag = Some(old_i_flag);
            }
            Mnemonic::STA => {
                self.write(operand, self.a, false);
            }
            Mnemonic::USAX => {
                // SAX: Store A AND X (undocumented)
                let value = self.a & self.x;
                self.write(operand, value, false);
            }
            Mnemonic::STY => {
                self.write(operand, self.y, false);
            }
            Mnemonic::STX => {
                // Store X Register
                self.write(operand, self.x, false);
            }
            Mnemonic::DEY => {
                // Decrement Y Register - already implemented as helper method
                self.dey();
            }
            Mnemonic::TXA => {
                // Transfer X to Accumulator - already implemented as helper method
                self.txa();
            }
            Mnemonic::UXAA => {
                // *XAA (undocumented) - Transfer X to A, then AND with immediate
                self.a = self.x;
                let value = self.get_operand_value(op, operand);
                self.a &= value;
                self.update_zero_and_negative_flags(self.a);
            }
            Mnemonic::BCC => {
                // Branch on Carry Clear
                let offset = operand as i8;
                if self.p & FLAG_CARRY == 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::UXAS => {
                // *XAS / TAS (undocumented) - SP = A & X, then store SP & (high byte of address + 1)
                self.sp = self.a & self.x;
                let high_byte = (operand >> 8) as u8;
                let value = self.sp & high_byte.wrapping_add(1);
                self.write(operand, value, false);
            }
            Mnemonic::TYA => {
                // Transfer Y to Accumulator - already implemented as helper method
                self.tya();
            }
            Mnemonic::TXS => {
                // Transfer X to Stack Pointer - does not affect flags
                self.sp = self.x;
            }
            Mnemonic::USYA => {
                self.exec_sya_illegal(operand);
            }
            Mnemonic::USXA => {
                self.exec_sxa_illegal(operand);
            }
            Mnemonic::UAXA => {
                // *AXA (undocumented) - Store A AND X AND (high byte of address + 1)
                let high_byte = (operand >> 8) as u8;
                let value = self.a & self.x & high_byte.wrapping_add(1);
                self.write(operand, value, false);
            }
            Mnemonic::LDY => {
                let value = self.get_operand_value(op, operand);
                self.ldy(value);
            }
            Mnemonic::LDA => {
                let value = self.get_operand_value(op, operand);
                self.lda(value);
            }
            Mnemonic::LDX => {
                let value = self.get_operand_value(op, operand);
                self.ldx(value);
            }
            Mnemonic::ULAX => {
                // LAX (undocumented): Load A and X with the same value
                let value = self.get_operand_value(op, operand);
                self.lda(value);
                self.ldx(value);
            }
            Mnemonic::TAY => {
                // Transfer Accumulator to Y - already implemented as helper method
                self.tay();
            }
            Mnemonic::TAX => {
                // Transfer Accumulator to X - already implemented as helper method
                self.tax();
            }
            Mnemonic::UATX => {
                // *ATX (undocumented): Load A and X with immediate value
                // Also known as *LAX immediate or *OAL
                let value = self.get_operand_value(op, operand);
                self.a = value;
                self.x = value;
                self.update_zero_and_negative_flags(self.a);
            }
            Mnemonic::BCS => {
                // Branch on Carry Set
                let offset = operand as i8;
                if self.p & FLAG_CARRY != 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::CLV => {
                // Clear overflow flag
                self.p &= !FLAG_OVERFLOW;
            }
            Mnemonic::TSX => {
                // Transfer Stack pointer to X
                self.x = self.sp;
                self.update_zero_and_negative_flags(self.x);
            }
            Mnemonic::ULAR => {
                // Undocumented: AND memory with stack pointer, store in A, X, and SP
                let value = self.get_operand_value(op, operand);
                self.lar(value);
            }
            Mnemonic::CPY => {
                let value = self.get_operand_value(op, operand);
                self.cpy(value);
            }
            Mnemonic::CMP => {
                let value = self.get_operand_value(op, operand);
                self.cmp(value);
            }
            Mnemonic::UDCP => {
                // Undocumented: Decrement memory then compare with A
                self.dcp(operand);
            }
            Mnemonic::INY => {
                self.iny();
            }
            Mnemonic::DEX => {
                // Decrement X Register
                self.dex();
            }
            Mnemonic::UAXS => {
                // *AXS (undocumented): (A & X) - immediate -> X
                let value = self.get_operand_value(op, operand);
                self.axs(value);
            }
            Mnemonic::BNE => {
                // Branch if Not Equal (zero flag clear)
                let offset = operand as i8;
                if self.p & FLAG_ZERO == 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::CLD => {
                // Clear Decimal flag
                self.p &= !FLAG_DECIMAL;
            }
            Mnemonic::CPX => {
                // Compare X with memory
                let value = self.get_operand_value(op, operand);
                self.cpx(value);
            }
            Mnemonic::SBC | Mnemonic::USBC => {
                // Subtract with Carry
                let value = self.get_operand_value(op, operand);
                self.sbc(value);
            }
            Mnemonic::UISB => {
                // *ISB (undocumented): Increment memory then SBC
                self.isb(operand);
            }
            Mnemonic::INX => {
                // Increment X Register
                self.inx();
            }
            Mnemonic::BEQ => {
                // Branch if Equal (zero flag set)
                let offset = operand as i8;
                if self.p & FLAG_ZERO != 0 {
                    let old_pc = self.pc;
                    self.pc = self.pc.wrapping_add(offset as u16);
                    let page_crossed = Self::page_crossed(old_pc, self.pc);
                    if page_crossed {
                        self.dummy_read(self.pc);
                        self.dummy_read(self.pc);
                    } else {
                        self.skip_interrupt_latch_this_cycle = true;
                        self.dummy_read(self.pc);
                    }
                }
            }
            Mnemonic::SED => {
                // Set Decimal flag
                self.p |= FLAG_DECIMAL;
            }
            Mnemonic::INC => {
                // Increment memory
                let value = self.read(operand);
                //   (cycle accurate)
                self.dummy_write(operand, value);
                // Increment and write back
                let result = self.inc(value);
                self.write(operand, result, false);
            }
            Mnemonic::DEC => {
                // Decrement memory
                let value = self.read(operand);
                //   (cycle accurate)
                self.dummy_write(operand, value);
                // Decrement and write back
                let result = self.dec(value);
                self.write(operand, result, false);
            }
        }

        // Clear tick tracking after instruction
        self.current_tick_info = None;

        // Clear the previous delayed-I state after exactly one instruction.
        // If this instruction introduced a new delay, keep that for the next instruction.
        let cleared_delayed_i_flag_this_instruction = had_delayed_i_flag;
        if had_delayed_i_flag {
            self.delayed_i_flag = None;
        }
        if new_delayed_i_flag.is_some() {
            self.delayed_i_flag = new_delayed_i_flag;
        }

        // IRQ/NMI are taken after the instruction completes.
        //
        // Special case: when the delayed-I state just expired (e.g., the instruction after CLI),
        // IRQ recognition must reflect the *new* I state immediately at this boundary.
        // We accomplish this by re-evaluating `should_poll_irq()` only in that case.
        let irq_after_delayed_i_expires =
            cleared_delayed_i_flag_this_instruction && self.should_poll_irq();

        if self.prev_need_nmi || self.prev_run_irq || irq_after_delayed_i_expires {
            self.service_irq_or_nmi_sequence();
        }
    }

    /// Fetch the operand address or value for an instruction
    ///
    /// For memory-accessing modes (ZP, ABS, etc.), returns the effective address.
    /// For immediate mode (IMM), returns the immediate value (low byte only).
    /// For implied/accumulator modes, performs dummy read and returns 0.
    /// For relative mode (REL), returns the immediate byte (offset).
    ///
    /// # Arguments
    /// * `opcode` - The opcode byte to fetch the operand for
    ///
    /// # Returns
    /// The operand address or value (depending on addressing mode)
    pub fn get_operand(&mut self, op: OpCode) -> u16 {
        match op.mode {
            // Implied and Accumulator - perform dummy read
            AddrMode::IMP | AddrMode::ACC => {
                self.dummy_read(self.pc);
                0
            }

            // Immediate, Zero Page and Relative - return the immediate byte
            AddrMode::IMM | AddrMode::REL | AddrMode::ZP => self.read_byte_from_pc() as u16,

            // Zero Page,X - read base, dummy read at base, return base+X
            AddrMode::ZPX => {
                let base = self.read_byte_from_pc();
                self.dummy_read(base as u16);
                base.wrapping_add(self.x) as u16
            }

            // Zero Page,Y - read base, dummy read at base, return base+Y
            AddrMode::ZPY => {
                let base = self.read_byte_from_pc();
                self.dummy_read(base as u16);
                base.wrapping_add(self.y) as u16
            }

            // Absolute - return 16-bit address
            AddrMode::ABS => self.read_word_from_pc(),

            // Absolute,X - return address + X
            // Note: Page crossing dummy read i
            AddrMode::ABSX => {
                let base = self.read_word_from_pc();
                let addr = base.wrapping_add(self.x as u16);
                // Always do dummy read at base + X with wrong high byte if page crossed
                if Self::page_crossed(base, addr) {
                    let dummy_addr = (base & 0xFF00) | (addr & 0x00FF);
                    self.dummy_read(dummy_addr);
                }
                addr
            }

            // Absolute,X (Write/RMW) - return address + X, always do dummy read
            AddrMode::ABSXW => {
                let base = self.read_word_from_pc();
                let addr = base.wrapping_add(self.x as u16);
                // Always do dummy read at base+X with wrong high byte (no carry into high byte)
                // for write/RMW indexed addressing.
                let dummy_addr = (base & 0xFF00) | (addr & 0x00FF);
                self.dummy_read(dummy_addr);
                addr
            }

            // Absolute,Y - return address + Y
            // Note: Page crossing dummy read is handled by instruction for reads
            AddrMode::ABSY => {
                let base = self.read_word_from_pc();
                let addr = base.wrapping_add(self.y as u16);
                // Always do dummy read at base + T with wrong high byte if page crossed
                if Self::page_crossed(base, addr) {
                    let dummy_addr = (base & 0xFF00) | (addr & 0x00FF);
                    self.dummy_read(dummy_addr);
                }
                addr
            }

            // Absolute,Y (Write/RMW) - return address + Y, always do dummy read
            AddrMode::ABSYW => {
                let base = self.read_word_from_pc();
                let addr = base.wrapping_add(self.y as u16);
                // Always do dummy read at base + Y with wrong high byte if page crossed
                let page_crossed = Self::page_crossed(base, addr);
                let dummy_addr = if page_crossed { addr - 0x100 } else { addr };
                self.dummy_read(dummy_addr);
                addr
            }

            // Indirect - JMP ($addr) with 6502 page boundary bug
            AddrMode::IND => {
                let ptr = self.read_word_from_pc();
                self.read_word_indirect(ptr)
            }

            // Indexed Indirect - (ZP,X)
            // Always does dummy read at base address during indexing
            AddrMode::INDX => {
                let base = self.read_byte_from_pc();
                self.dummy_read(base as u16);
                let ptr = base.wrapping_add(self.x);
                self.read_word_from_zp(ptr)
            }

            // Indirect Indexed - (ZP),Y (Read-only)
            // Note: Page crossing means dummy read
            AddrMode::INDY => {
                let ptr = self.read_byte_from_pc();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                // Always do dummy read at base + Y with wrong high byte if page crossed
                if Self::page_crossed(base, addr) {
                    let dummy_addr = (base & 0xFF00) | (addr & 0x00FF);
                    self.dummy_read(dummy_addr);
                }
                addr
            }

            // Indirect Indexed - (ZP),Y (Write/RMW)
            // Always do dummy read at base + Y with wrong high byte if page crossed
            AddrMode::INDYW => {
                let ptr = self.read_byte_from_pc();
                let base = self.read_word_from_zp(ptr);
                let addr = base.wrapping_add(self.y as u16);
                // Always do dummy read - with wrong high byte if page crossed
                let dummy_addr = if Self::page_crossed(base, addr) {
                    (base & 0xFF00) | (addr & 0x00FF)
                } else {
                    addr
                };
                self.dummy_read(dummy_addr);
                addr
            }
        }
    }
}

#[cfg(test)]
mod tests;
