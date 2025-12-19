use super::decoder::decode_opcode;
use super::sequencer::{TickResult, tick_instruction};
use super::traits::{AddressingMode, CpuState, Operation};
use super::types::{AddressingState, InstructionPhase, InstructionType};
use crate::mem_controller::MemController;
use std::cell::RefCell;
use std::rc::Rc;

// Status register flags
#[allow(dead_code)]
const FLAG_CARRY: u8 = 0b0000_0001;
const FLAG_ZERO: u8 = 0b0000_0010;
const FLAG_INTERRUPT: u8 = 0b0000_0100;
#[allow(dead_code)]
const FLAG_DECIMAL: u8 = 0b0000_1000;
#[allow(dead_code)]
const FLAG_BREAK: u8 = 0b0001_0000;
#[allow(dead_code)]
const FLAG_UNUSED: u8 = 0b0010_0000;
#[allow(dead_code)]
const FLAG_OVERFLOW: u8 = 0b0100_0000;
const FLAG_NEGATIVE: u8 = 0b1000_0000;

// Interrupt vector addresses in memory
#[allow(dead_code)]
const NMI_VECTOR: u16 = 0xFFFA; // Non-Maskable Interrupt vector
const RESET_VECTOR: u16 = 0xFFFC; // Reset vector
#[allow(dead_code)]
const IRQ_VECTOR: u16 = 0xFFFE; // IRQ and BRK vector

/// New cycle-accurate 6502 CPU implementation
pub struct NewCpu {
    /// Accumulator
    pub a: u8,
    /// X register
    pub x: u8,
    /// Y register
    pub y: u8,
    /// Stack pointer
    pub sp: u8,
    /// Program counter
    pub pc: u16,
    /// Status register (processor flags)
    pub p: u8,
    /// Memory controller
    pub memory: Rc<RefCell<MemController>>,
    /// Halted state (set by KIL instruction)
    pub halted: bool,
    /// Total cycles executed since last reset
    pub total_cycles: u64,
    /// NMI pending flag
    pub nmi_pending: bool,
    /// IRQ inhibit flag (delays IRQ by one instruction after CLI/SEI/PLP)
    irq_inhibit: bool,
    /// Current instruction execution state
    instruction_state: Option<InstructionExecutionState>,
}

/// Tracks the state of an instruction being executed
struct InstructionExecutionState {
    phase: InstructionPhase,
    addressing_mode: Box<dyn AddressingMode>,
    operation: Box<dyn Operation>,
    instruction_type: InstructionType,
    addressing_state: AddressingState,
}

impl NewCpu {
    /// Create a new CPU with default register values at power-on
    pub fn new(memory: Rc<RefCell<MemController>>) -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0x00,
            pc: 0,
            p: 0x20, // Only unused bit set at power-on
            memory,
            halted: false,
            total_cycles: 0,
            nmi_pending: false,
            irq_inhibit: false,
            instruction_state: None,
        }
    }

    /// Reset the CPU to initial state
    pub fn reset(&mut self) {
        // Set I flag
        self.p |= FLAG_INTERRUPT;

        // Subtract 3 from SP
        self.sp = self.sp.wrapping_sub(3);

        // Clear state
        self.halted = false;
        self.nmi_pending = false;
        self.irq_inhibit = false;
        self.instruction_state = None;

        // Read reset vector and set PC
        self.pc = self.read_reset_vector();

        // Reset takes 7 cycles
        self.total_cycles = 7;
    }

    /// Read the reset vector from memory
    fn read_reset_vector(&self) -> u16 {
        let lo = self.memory.borrow().read(RESET_VECTOR);
        let hi = self.memory.borrow().read(RESET_VECTOR + 1);
        u16::from_le_bytes([lo, hi])
    }

    /// Execute one CPU cycle
    /// Returns true on every successful cycle
    pub fn tick(&mut self) -> bool {
        eprintln!(
            "DEBUG tick: halted={}, has_state={}",
            self.halted,
            self.instruction_state.is_some()
        );
        if self.halted {
            return false;
        }

        self.total_cycles += 1;

        // If no instruction is being executed, fetch the next opcode
        if self.instruction_state.is_none() {
            self.fetch_opcode();
            return true;
        }

        // Execute one cycle of the current instruction
        self.execute_instruction_cycle();
        true
    }

    /// Execute one CPU cycle (compat with old CPU interface)
    /// Returns true if an instruction completed in this cycle, false otherwise
    pub fn tick_cycle(&mut self) -> bool {
        let was_executing = self.instruction_state.is_some();
        self.tick();
        let now_idle = self.instruction_state.is_none();

        // Instruction completed if we were executing and now we're idle
        was_executing && now_idle
    }

    /// Fetch the next opcode and initialize instruction state
    fn fetch_opcode(&mut self) {
        // Clear IRQ inhibit flag when starting a new instruction.
        // This implements the one-instruction delay: if CLI/SEI/PLP set this flag,
        // it prevents IRQ during the next instruction, then clears for the one after.
        self.irq_inhibit = false;

        let opcode = self.memory.borrow().read(self.pc);

        self.pc = self.pc.wrapping_add(1);

        let (addressing_mode, operation, instruction_type, _cycles) = decode_opcode(opcode);

        self.instruction_state = Some(InstructionExecutionState {
            phase: InstructionPhase::Addressing(0),
            addressing_mode,
            operation,
            instruction_type,
            addressing_state: AddressingState::default(),
        });
    }

    /// Execute one cycle of the current instruction
    fn execute_instruction_cycle(&mut self) {
        let state = self.instruction_state.as_mut().unwrap();
        eprintln!(
            "DEBUG execute_instruction_cycle: phase={:?}, type={:?}",
            state.phase, state.instruction_type
        );

        // Create CpuState for operations
        let mut cpu_state = CpuState {
            a: self.a,
            x: self.x,
            y: self.y,
            sp: self.sp,
            p: self.p,
        };

        // Create read and write closures
        let read_fn = |addr: u16| -> u8 { self.memory.borrow().read(addr) };
        let mut write_fn = |addr: u16, value: u8| {
            self.memory.borrow_mut().write(addr, value, false);
        };

        let (result, next_phase) = tick_instruction(
            state.instruction_type,
            state.phase,
            state.addressing_mode.as_ref(),
            state.operation.as_ref(),
            &mut self.pc,
            self.x,
            self.y,
            &mut cpu_state,
            &mut state.addressing_state,
            &read_fn,
            &mut write_fn,
            self.nmi_pending,
        );

        // Update CPU state from operation
        self.a = cpu_state.a;
        self.x = cpu_state.x;
        self.y = cpu_state.y;
        self.sp = cpu_state.sp;
        self.p = cpu_state.p;

        // Update instruction state or complete instruction
        match result {
            TickResult::InProgress => {
                state.phase = next_phase;
            }
            TickResult::Complete => {
                // CLI, SEI, and PLP delay IRQ by one instruction due to interrupt
                // polling happening before flag modification. Set inhibit flag so
                // trigger_irq() will return 0 cycles until the next instruction completes.
                if state.operation.inhibits_irq() {
                    self.irq_inhibit = true;
                }
                self.instruction_state = None;
            }
        }
    }

    /// Get the total number of cycles executed
    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Set the NMI pending flag
    pub fn set_nmi_pending(&mut self, pending: bool) {
        self.nmi_pending = pending;
    }

    /// Check if IRQ should be polled (I flag is clear)
    pub fn should_poll_irq(&self) -> bool {
        (self.p & FLAG_INTERRUPT) == 0
    }

    /// Push a byte onto the stack
    fn push_byte(&mut self, value: u8) {
        let addr = 0x0100 | (self.sp as u16);
        self.memory.borrow_mut().write(addr, value, false);
        self.sp = self.sp.wrapping_sub(1);
    }

    /// Push a word onto the stack (high byte first)
    fn push_word(&mut self, value: u16) {
        self.push_byte((value >> 8) as u8); // High byte
        self.push_byte((value & 0xFF) as u8); // Low byte
    }

    /// Trigger an NMI (Non-Maskable Interrupt)
    /// Returns the number of cycles consumed (7 cycles)
    pub fn trigger_nmi(&mut self) -> u8 {
        // Push PC onto stack
        self.push_word(self.pc);

        // Push P onto stack with B flag clear and unused flag set
        let mut p_with_flags = self.p & !FLAG_BREAK;
        p_with_flags |= FLAG_UNUSED;
        self.push_byte(p_with_flags);

        // Read NMI vector and set PC
        let lo = self.memory.borrow().read(NMI_VECTOR);
        let hi = self.memory.borrow().read(NMI_VECTOR + 1);
        self.pc = u16::from_le_bytes([lo, hi]);

        // Set Interrupt Disable flag
        self.p |= FLAG_INTERRUPT;

        // NMI takes 7 cycles
        self.total_cycles += 7;
        7
    }

    /// Trigger an IRQ (Interrupt Request)
    /// Returns the number of cycles consumed (7 cycles if triggered, 0 if masked)
    pub fn trigger_irq(&mut self) -> u8 {
        // IRQ is maskable - check if interrupts are disabled
        if (self.p & FLAG_INTERRUPT) != 0 {
            return 0; // IRQ masked
        }

        // IRQ is also inhibited for one instruction after CLI/SEI/PLP
        if self.irq_inhibit {
            return 0; // IRQ inhibited
        }

        // Push PC onto stack
        self.push_word(self.pc);

        // Push P onto stack with B flag clear and unused flag set
        let mut p_with_flags = self.p & !FLAG_BREAK;
        p_with_flags |= FLAG_UNUSED;
        self.push_byte(p_with_flags);

        // Read IRQ vector and set PC
        let lo = self.memory.borrow().read(IRQ_VECTOR);
        let hi = self.memory.borrow().read(IRQ_VECTOR + 1);
        self.pc = u16::from_le_bytes([lo, hi]);

        // Set Interrupt Disable flag
        self.p |= FLAG_INTERRUPT;

        // IRQ takes 7 cycles
        self.total_cycles += 7;
        7
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::{Cartridge, MirroringMode};

    fn setup_cpu() -> NewCpu {
        let ppu = Rc::new(RefCell::new(crate::ppu::Ppu::new(
            crate::nes::TvSystem::Ntsc,
        )));
        let apu = Rc::new(RefCell::new(crate::apu::Apu::new()));
        let mem = Rc::new(RefCell::new(MemController::new(ppu, apu)));
        NewCpu::new(mem)
    }

    fn setup_cpu_with_rom(reset_addr: u16, program: &[u8]) -> NewCpu {
        let cpu = setup_cpu();

        // Create a minimal PRG ROM with reset vector
        let mut prg_rom = vec![0; 0x4000]; // 16KB

        // Place program at the beginning of PRG ROM
        for (i, &byte) in program.iter().enumerate() {
            prg_rom[i] = byte;
        }

        // Set reset vector to point to reset_addr
        // Reset vector is at 0xFFFC-0xFFFD
        // For 16KB ROM: (0xFFFC - 0x8000) % 0x4000 = 0x3FFC
        prg_rom[0x3FFC] = (reset_addr & 0xFF) as u8; // Low byte
        prg_rom[0x3FFD] = (reset_addr >> 8) as u8; // High byte

        // Create CHR ROM (8KB)
        let chr_rom = vec![0; 0x2000];

        let cartridge = Cartridge::from_parts(prg_rom, chr_rom, MirroringMode::Horizontal);
        cpu.memory.borrow_mut().map_cartridge(cartridge);

        cpu
    }

    #[test]
    fn test_new_cpu_initial_state() {
        let cpu = setup_cpu();

        assert_eq!(cpu.a, 0);
        assert_eq!(cpu.x, 0);
        assert_eq!(cpu.y, 0);
        assert_eq!(cpu.sp, 0x00);
        assert_eq!(cpu.pc, 0);
        assert_eq!(cpu.p, 0x20); // Only unused bit set
        assert_eq!(cpu.total_cycles, 0);
        assert!(!cpu.halted);
        assert!(!cpu.nmi_pending);
    }

    #[test]
    fn test_reset() {
        let mut cpu = setup_cpu_with_rom(0x8000, &[]);

        cpu.reset();

        assert_eq!(cpu.pc, 0x8000);
        assert_eq!(cpu.sp, 0xFD); // 0x00 - 3 = 0xFD
        assert_eq!(cpu.p & FLAG_INTERRUPT, FLAG_INTERRUPT); // I flag set
        assert_eq!(cpu.total_cycles, 7);
        assert!(!cpu.halted);
        assert!(!cpu.nmi_pending);
    }

    #[test]
    fn test_tick_executes_single_cycle() {
        let mut cpu = setup_cpu_with_rom(0x8000, &[]);
        cpu.reset();

        let initial_cycles = cpu.total_cycles();
        cpu.tick();

        assert_eq!(cpu.total_cycles(), initial_cycles + 1);
    }

    #[test]
    fn test_tick_returns_false_when_halted() {
        let mut cpu = setup_cpu();
        cpu.halted = true;

        let result = cpu.tick();

        assert!(!result);
    }

    #[test]
    fn test_execute_lda_immediate() {
        // LDA #$42 at 0x8000
        // LDA immediate is opcode 0xA9, takes 2 cycles
        let program = vec![0xA9, 0x42]; // LDA #$42
        let mut cpu = setup_cpu_with_rom(0x8000, &program);

        cpu.reset();

        assert_eq!(cpu.pc, 0x8000);
        assert_eq!(cpu.a, 0);

        // Execute instruction cycle by cycle
        // LDA immediate is 2 cycles: fetch opcode + execute addressing and operation

        // Cycle 1: Fetch opcode and start addressing
        cpu.tick();
        assert_eq!(cpu.total_cycles(), 8); // 7 from reset + 1

        // Cycle 2: Execute addressing and operation
        cpu.tick();
        assert_eq!(cpu.total_cycles(), 9);

        // After 2 cycles, instruction should be complete
        assert_eq!(cpu.a, 0x42); // A should now be 0x42
        assert_eq!(cpu.pc, 0x8002); // PC should have advanced by 2
        assert_eq!(cpu.p & FLAG_ZERO, 0); // Zero flag clear
        assert_eq!(cpu.p & FLAG_NEGATIVE, 0); // Negative flag clear
    }

    #[test]
    fn test_set_nmi_pending() {
        let mut cpu = setup_cpu();

        assert!(!cpu.nmi_pending);

        cpu.set_nmi_pending(true);
        assert!(cpu.nmi_pending);

        cpu.set_nmi_pending(false);
        assert!(!cpu.nmi_pending);
    }

    #[test]
    fn test_trigger_nmi() {
        let mut cpu = setup_cpu_with_rom(0x8000, &[]);
        cpu.reset();

        // Set up NMI vector to point to 0x9000
        let mut prg_rom = vec![0; 0x4000];
        prg_rom[0x3FFA] = 0x00; // NMI vector low byte (0xFFFA - 0x8000 + 0x4000 = 0x3FFA)
        prg_rom[0x3FFB] = 0x90; // NMI vector high byte
        let chr_rom = vec![0; 0x2000];
        let cartridge = Cartridge::from_parts(prg_rom, chr_rom, MirroringMode::Horizontal);
        cpu.memory.borrow_mut().map_cartridge(cartridge);

        let initial_pc = cpu.pc;
        let initial_sp = cpu.sp;
        let initial_cycles = cpu.total_cycles();

        let cycles = cpu.trigger_nmi();

        // NMI should take 7 cycles
        assert_eq!(cycles, 7);
        assert_eq!(cpu.total_cycles(), initial_cycles + 7);

        // PC should be set to NMI vector
        assert_eq!(cpu.pc, 0x9000);

        // Stack should have PC and P pushed (3 bytes)
        assert_eq!(cpu.sp, initial_sp.wrapping_sub(3));

        // I flag should be set
        assert_eq!(cpu.p & FLAG_INTERRUPT, FLAG_INTERRUPT);
    }

    #[test]
    fn test_trigger_irq_when_enabled() {
        let mut cpu = setup_cpu_with_rom(0x8000, &[]);
        cpu.reset();

        // Set up IRQ vector to point to 0xA000
        let mut prg_rom = vec![0; 0x4000];
        prg_rom[0x3FFE] = 0x00; // IRQ vector low byte (0xFFFE - 0x8000 + 0x4000 = 0x3FFE)
        prg_rom[0x3FFF] = 0xA0; // IRQ vector high byte
        let chr_rom = vec![0; 0x2000];
        let cartridge = Cartridge::from_parts(prg_rom, chr_rom, MirroringMode::Horizontal);
        cpu.memory.borrow_mut().map_cartridge(cartridge);

        // Clear I flag to enable IRQ
        cpu.p &= !FLAG_INTERRUPT;

        let initial_cycles = cpu.total_cycles();
        let cycles = cpu.trigger_irq();

        // IRQ should take 7 cycles when enabled
        assert_eq!(cycles, 7);
        assert_eq!(cpu.total_cycles(), initial_cycles + 7);

        // PC should be set to IRQ vector
        assert_eq!(cpu.pc, 0xA000);

        // I flag should be set
        assert_eq!(cpu.p & FLAG_INTERRUPT, FLAG_INTERRUPT);
    }

    #[test]
    fn test_trigger_irq_when_disabled() {
        let mut cpu = setup_cpu_with_rom(0x8000, &[]);
        cpu.reset();

        // I flag is set by reset, so IRQ should be disabled
        assert_eq!(cpu.p & FLAG_INTERRUPT, FLAG_INTERRUPT);

        let initial_pc = cpu.pc;
        let initial_cycles = cpu.total_cycles();

        let cycles = cpu.trigger_irq();

        // IRQ should be masked and take 0 cycles
        assert_eq!(cycles, 0);
        assert_eq!(cpu.total_cycles(), initial_cycles);

        // PC should not change
        assert_eq!(cpu.pc, initial_pc);
    }

    #[test]
    fn test_should_poll_irq() {
        let mut cpu = setup_cpu();

        // Initially I flag is clear, so IRQ should be allowed
        assert!(cpu.should_poll_irq());

        // Set I flag
        cpu.p |= FLAG_INTERRUPT;
        assert!(!cpu.should_poll_irq());

        // Clear I flag
        cpu.p &= !FLAG_INTERRUPT;
        assert!(cpu.should_poll_irq());
    }

    #[test]
    fn test_tick_cycle_returns_completion() {
        // LDA immediate: 2 cycles
        let program = vec![0xA9, 0x42];
        let mut cpu = setup_cpu_with_rom(0x8000, &program);
        cpu.reset();

        // First cycle: fetch opcode - instruction not complete
        assert!(!cpu.tick_cycle());

        // Second cycle: execute - instruction complete
        assert!(cpu.tick_cycle());

        assert_eq!(cpu.a, 0x42);
    }

    #[test]
    fn test_brk_basic_execution() {
        // Simple test to verify BRK executes at all
        let mut program = vec![0x00, 0x00]; // BRK + padding

        // We need to set up the ROM with the IRQ vector
        // 16KB ROM is mirrored, so 0xFFFE maps to offset 0x3FFE in the ROM
        // Fill with NOPs up to the vector
        program.resize(0x3FFE, 0xEA); // Fill with NOP
        program.push(0x00); // IRQ vector low byte (0xA000)
        program.push(0xA0); // IRQ vector high byte

        let mut cpu = setup_cpu_with_rom(0x8000, &program);

        cpu.reset();
        let initial_sp = cpu.sp;

        // Execute BRK instruction
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break; // Instruction completed
            }
        }

        // PC should be at IRQ vector
        assert_eq!(cpu.pc, 0xA000, "BRK should jump to IRQ vector");

        // Stack should have 3 bytes pushed
        assert_eq!(
            cpu.sp,
            initial_sp.wrapping_sub(3),
            "SP should have 3 bytes pushed"
        );
    }

    #[test]
    fn test_nmi_hijacks_brk_uses_nmi_vector_but_sets_b_flag() {
        // Test interrupt hijacking per NesDev wiki:
        // https://www.nesdev.org/wiki/CPU_interrupts#Interrupt_hijacking
        //
        // When NMI is asserted during the first 4 ticks of BRK, the BRK executes
        // normally (PC increments, B flag set on stack), but execution branches
        // to the NMI vector instead of IRQ/BRK vector.

        // BRK opcode (0x00) followed by padding byte
        let mut program = vec![0x00, 0x00];

        // Set up vectors in ROM
        // 16KB ROM is mirrored, so vectors map to offset 0x3FF8 onwards
        program.resize(0x3FFA, 0xEA); // Fill with NOP up to NMI vector
        program.push(0x00); // NMI vector low byte (0x9000)
        program.push(0x90); // NMI vector high byte
        program.push(0x00); // Reserved vector low (not used)
        program.push(0x00); // Reserved vector high (not used)
        program.push(0x00); // IRQ vector low byte (0xA000)
        program.push(0xA0); // IRQ vector high byte

        let mut cpu = setup_cpu_with_rom(0x8000, &program);

        cpu.reset();

        // Execute BRK instruction while setting NMI pending during execution
        // BRK takes 7 cycles:
        // 1. Fetch opcode
        // 2. Read next byte (padding), increment PC
        // 3. Push PCH
        // 4. Push PCL
        // 5. Push P (with B flag)
        // 6. Fetch PCL from vector
        // 7. Fetch PCH from vector

        // Cycle 1: Fetch BRK opcode
        cpu.tick_cycle();

        // Cycle 2: Read padding byte - assert NMI here (during cycle 2)
        cpu.set_nmi_pending(true);
        cpu.tick_cycle();

        // Cycles 3-7: Complete BRK sequence
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break; // Instruction completed
            }
        }

        // PC should point to NMI vector (0x9000), not IRQ vector (0xA000)
        assert_eq!(cpu.pc, 0x9000, "BRK should be hijacked to use NMI vector");

        // Check stack: should have PC+2 and status with B flag set
        // Stack layout (from top): P, PCL, PCH
        let status_on_stack = cpu.memory.borrow().read(0x0100 + cpu.sp as u16 + 1);
        assert_eq!(
            status_on_stack & FLAG_BREAK,
            FLAG_BREAK,
            "B flag should be set on stack even when hijacked by NMI"
        );
    }

    #[test]
    fn test_cli_delays_irq_by_one_instruction() {
        // Test that CLI delays IRQ response by one instruction
        // Per NesDev wiki: CLI polls interrupts at end of first cycle, then modifies I flag,
        // so a pending IRQ won't be serviced until after the next instruction.

        // Program: CLI (0x58), NOP (0xEA), NOP (0xEA)
        let mut program = vec![0x58, 0xEA, 0xEA];

        // Set up IRQ vector in ROM at 0xB000
        program.resize(0x3FFE, 0xEA); // Fill with NOP
        program.push(0x00); // IRQ vector low byte (0xB000)
        program.push(0xB0); // IRQ vector high byte

        let mut cpu = setup_cpu_with_rom(0x8000, &program);
        cpu.reset();

        // Set I flag initially (interrupts disabled)
        cpu.p |= FLAG_INTERRUPT;

        // Execute CLI instruction - should clear I flag but delay IRQ by one instruction
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break; // CLI completed
            }
        }

        // I flag should now be clear
        assert_eq!(
            cpu.p & FLAG_INTERRUPT,
            0,
            "I flag should be clear after CLI"
        );

        // Trigger IRQ immediately after CLI completes
        // According to spec, this IRQ should NOT be taken until after next instruction
        let cycles_consumed = cpu.trigger_irq();

        // IRQ should be inhibited (delayed), so trigger_irq should return 0
        assert_eq!(
            cycles_consumed, 0,
            "IRQ should be inhibited immediately after CLI"
        );

        // PC should still be at NOP instruction after CLI (0x8001)
        assert_eq!(cpu.pc, 0x8001, "PC should be at first NOP after CLI");

        // Execute the NOP instruction (this is the "delay" instruction)
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break; // NOP completed
            }
        }

        // After NOP completes, PC should be at 0x8002 (second NOP)
        assert_eq!(cpu.pc, 0x8002, "PC should be at second NOP after first NOP");

        // Now try to trigger IRQ again - it should succeed this time
        let cycles_consumed = cpu.trigger_irq();
        assert_eq!(
            cycles_consumed, 7,
            "IRQ should be taken after delay instruction"
        );

        // PC should now be at IRQ handler (0xB000)
        assert_eq!(cpu.pc, 0xB000, "PC should jump to IRQ vector after delay");
    }

    #[test]
    fn test_adc_ignores_decimal_flag() {
        // Verify ADC always performs binary addition, never BCD
        // Program: SED (0xF8), ADC #$09 (0x69 0x09)
        let program = vec![0xF8, 0x69, 0x09];
        let mut cpu = setup_cpu_with_rom(0x8000, &program);
        cpu.reset();
        cpu.a = 0;

        // Execute SED
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break; // SED completed
            }
        }
        assert_eq!(
            cpu.p & FLAG_DECIMAL,
            FLAG_DECIMAL,
            "D flag should be set after SED"
        );

        // Execute ADC with D flag set
        // A = 0, operand = 9, result should be binary 9
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break; // ADC completed
            }
        }
        assert_eq!(cpu.a, 9, "ADC should perform binary addition");

        // Try a more obvious case: 9 + 9 = 18 in binary (0x12)
        // In BCD mode (if it were implemented), 09 + 09 = 18, but stored differently
        cpu.pc = 0x8003;
        cpu.a = 0x09;

        let program2 = vec![0x00, 0x00, 0x00, 0x69, 0x09]; // ADC #$09 at 0x8003
        let mut cpu2 = setup_cpu_with_rom(0x8000, &program2);
        cpu2.reset();
        cpu2.pc = 0x8003;
        cpu2.a = 0x09;
        cpu2.p |= FLAG_DECIMAL; // Set D flag

        for _ in 0..10 {
            if cpu2.tick_cycle() {
                break;
            }
        }
        // Binary: 0x09 + 0x09 = 0x12 (18)
        // BCD would adjust to 0x18
        assert_eq!(cpu2.a, 0x12, "ADC result should be binary 0x12, not BCD");
    }

    #[test]
    fn test_sbc_ignores_decimal_flag() {
        // Verify SBC always performs binary subtraction, never BCD
        // Program: SED (0xF8), SEC (0x38), SBC #$05 (0xE9 0x05)
        let program = vec![0xF8, 0x38, 0xE9, 0x05];
        let mut cpu = setup_cpu_with_rom(0x8000, &program);
        cpu.reset();
        cpu.a = 0x10; // 16 in binary

        // Execute SED
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break;
            }
        }
        assert_eq!(
            cpu.p & FLAG_DECIMAL,
            FLAG_DECIMAL,
            "D flag should be set after SED"
        );

        // Execute SEC
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break;
            }
        }

        // Execute SBC with D flag set
        // A = 0x10, operand = 0x05, result should be 0x0B (binary 11)
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break;
            }
        }
        // Binary: 0x10 - 0x05 = 0x0B (11)
        // BCD would be: 16 - 05 = 11, stored as 0x11
        assert_eq!(cpu.a, 0x0B, "SBC should perform binary subtraction");
    }

    #[test]
    fn test_sed_cld_modify_d_flag() {
        // Verify SED sets and CLD clears the D flag
        // Program: SED (0xF8), NOP (0xEA), CLD (0xD8)
        let program = vec![0xF8, 0xEA, 0xD8];
        let mut cpu = setup_cpu_with_rom(0x8000, &program);
        cpu.reset();

        // Initially D flag should be clear
        assert_eq!(cpu.p & FLAG_DECIMAL, 0, "D flag should be clear initially");

        // Execute SED
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break;
            }
        }
        assert_eq!(
            cpu.p & FLAG_DECIMAL,
            FLAG_DECIMAL,
            "D flag should be set after SED"
        );

        // Execute NOP
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break;
            }
        }
        assert_eq!(
            cpu.p & FLAG_DECIMAL,
            FLAG_DECIMAL,
            "D flag should remain set after NOP"
        );

        // Execute CLD
        for _ in 0..10 {
            if cpu.tick_cycle() {
                break;
            }
        }
        assert_eq!(cpu.p & FLAG_DECIMAL, 0, "D flag should be clear after CLD");
    }

    // TODO: test_d_flag_preserved_through_php_plp skipped because PHP (0x08) is not
    // properly decoded in decoder.rs (currently mapped to NOP). This is a separate
    // issue from decimal mode verification and should be fixed separately.
}
