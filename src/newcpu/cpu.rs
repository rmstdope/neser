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

#[allow(dead_code)]
const NMI_VECTOR: u16 = 0xFFFA;
const RESET_VECTOR: u16 = 0xFFFC;
#[allow(dead_code)]
const IRQ_VECTOR: u16 = 0xFFFE;

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
    pub fn tick(&mut self) -> bool {
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

    /// Fetch the next opcode and initialize instruction state
    fn fetch_opcode(&mut self) {
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
}
