use super::addressing::{Implied, IndexedIndirect, Indirect, ZeroPage};
use super::instruction::Instruction;
use super::instruction_types::{Brk, Dop, Jmp, Jsr, Kil, Ora, Slo};
use super::traits::{
    BRK, DOP_ZP, JMP_ABS, JMP_IND, JSR, KIL, KIL2, KIL3, KIL4, KIL5, KIL6, KIL7, KIL8, KIL9, KIL10,
    KIL11, KIL12, ORA_INDX, ORA_ZP, SLO_INDX,
};
use super::types::{
    FLAG_BREAK, FLAG_CARRY, FLAG_DECIMAL, FLAG_INTERRUPT, FLAG_NEGATIVE, FLAG_OVERFLOW,
    FLAG_UNUSED, FLAG_ZERO, IRQ_VECTOR, NMI_VECTOR, RESET_VECTOR,
};
use crate::cpu2::CpuState;
use crate::cpu2::addressing::Absolute;
use crate::mem_controller::MemController;
use core::panic;
use std::cell::RefCell;
use std::rc::Rc;

/// NES 6502 CPU
pub struct Cpu2 {
    /// State of the CPU
    state: CpuState,
    /// Memory
    memory: Rc<RefCell<MemController>>,
    /// Halted state (set by KIL instruction)
    halted: bool,
    /// Total cycles executed since last reset
    total_cycles: u64,
    /// Current instruction being executed
    current_instruction: Option<Instruction>,
}

impl Cpu2 {
    /// Create a new CPU with default register values at power-on
    pub fn new(memory: Rc<RefCell<MemController>>) -> Self {
        Self {
            state: CpuState {
                a: 0,
                x: 0,
                y: 0,
                sp: 0x00, // Stack pointer starts at 0x00 at power-on. The automatic reset
                // sequence then subtracts 3, resulting in SP=0xFD when the reset
                // handler first runs.
                pc: 0,          // Program counter will be loaded from reset vector
                p: FLAG_UNUSED, // Status at power-on before reset: only unused bit set (bit 5)
            },
            memory,
            halted: false,
            total_cycles: 0,
            current_instruction: None,
        }
    }

    /// Check if an opcode is a KIL instruction (any of the 12 variants)
    fn is_kil_opcode(opcode: u8) -> bool {
        matches!(
            opcode,
            KIL | KIL2 | KIL3 | KIL4 | KIL5 | KIL6 | KIL7 | KIL8 | KIL9 | KIL10 | KIL11 | KIL12
        )
    }

    /// Execute a single CPU cycle
    /// Returns true when the current instruction completes
    pub fn tick_cycle(&mut self) -> bool {
        if self.halted {
            return true;
        }

        // If no current instruction, fetch and decode a new one
        if self.current_instruction.is_none() {
            let opcode = self.memory.borrow().read(self.state.pc);
            if let Some(instruction) = Self::decode(opcode) {
                self.state.pc = self.state.pc.wrapping_add(1);
                self.current_instruction = Some(instruction);
                self.total_cycles += 1;

                // Check if this is KIL - it halts the CPU immediately
                if Self::is_kil_opcode(opcode) {
                    self.halted = true;
                }

                return false;
            } else {
                // Unimplemented opcode - halt
                panic!(
                    "Unimplemented opcode {:02X} at address {:04X}",
                    opcode, self.state.pc
                );
            }
        }

        // Execute one cycle of the current instruction
        if let Some(ref mut instruction) = self.current_instruction {
            instruction.tick(&mut self.state, Rc::clone(&self.memory));

            // Check if both addressing and instruction are done
            if instruction.is_done() {
                self.current_instruction = None;
                self.total_cycles += 1;
                return true; // Instruction completed
            }
        }

        self.total_cycles += 1;
        false // Instruction not yet complete
    }

    /// Decode an opcode into an Instruction
    ///
    /// Creates the appropriate InstructionType and AddressingMode based on the opcode.
    /// Returns None if the opcode is not implemented.
    ///
    /// This is an associated function (not a method) since it doesn't depend on instance state.
    pub fn decode(opcode: u8) -> Option<Instruction> {
        match opcode {
            BRK => {
                // BRK uses Implied addressing since it doesn't use operands
                Some(Instruction::new(Box::new(Implied), Box::new(Brk::new())))
            }
            ORA_INDX => {
                // ORA Indexed Indirect: ORA (zp,X)
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new()),
                    Box::new(Ora::new()),
                ))
            }
            KIL => {
                // KIL uses Implied addressing - it halts the CPU
                Some(Instruction::new(Box::new(Implied), Box::new(Kil::new())))
            }
            SLO_INDX => {
                // SLO Indexed Indirect: SLO (zp,X) - shift left and OR
                Some(Instruction::new(
                    Box::new(IndexedIndirect::new()),
                    Box::new(Slo::new()),
                ))
            }
            DOP_ZP => {
                // DOP Zero Page - read and discard (illegal opcode)
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Dop::new()),
                ))
            }
            ORA_ZP => {
                // ORA Zero Page: ORA zp
                Some(Instruction::new(
                    Box::new(ZeroPage::new()),
                    Box::new(Ora::new()),
                ))
            }
            JSR => {
                // JSR handles its own address fetching internally, so we use Implied addressing
                Some(Instruction::new(Box::new(Implied), Box::new(Jsr::new())))
            }
            JMP_ABS => {
                // JMP Absolute handles its own address fetching internally, like JSR
                Some(Instruction::new(
                    Box::new(Absolute::new()),
                    Box::new(Jmp::new()),
                ))
            }
            JMP_IND => {
                // JMP Indirect uses the Indirect addressing mode to resolve the target address
                Some(Instruction::new(
                    Box::new(Indirect::new()),
                    Box::new(Jmp::new()),
                ))
            }
            _ => None, // Unimplemented opcode
        }
    }

    /// Check if the CPU is halted
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Get number of total cycles executed
    pub fn get_total_cycles(&self) -> u64 {
        self.total_cycles
    }

    /// Reset the CPU to initial state
    pub fn reset(&mut self) {
        // Set I flag (bit 2)
        self.state.p |= FLAG_INTERRUPT;

        // Subtract 3 from SP (wrapping if necessary)
        self.state.sp = self.state.sp.wrapping_sub(3);

        // Clear cycle-accurate instruction state
        self.halted = false;
        // self.delayed_i_flag = None;
        self.current_instruction = None;
        // self.cycle_in_instruction = 0;

        // Read reset vector and set PC
        self.state.pc = self.read_reset_vector();

        // Reset takes 7 cycles
        self.total_cycles = 7;
    }

    /// Trigger an NMI (Non-Maskable Interrupt)
    /// Returns the number of cycles consumed (7 cycles)
    pub fn trigger_nmi(&mut self) -> u8 {
        // TODO Implement NMI logic
        // // Push PC and P onto stack
        // self.push_word(self.state.pc);
        // let mut p_with_break = self.state.p & !FLAG_BREAK; // Clear Break flag
        // p_with_break |= FLAG_UNUSED; // Set unused flag
        // self.push_byte(p_with_break);

        // // Set PC to NMI vector
        // self.state.pc = self.memory.borrow().read_u16(NMI_VECTOR);

        // // Set Interrupt Disable flag
        // self.state.p |= FLAG_INTERRUPT;

        // // NMI takes 7 CPU cycles
        // self.total_cycles += 7;
        7
    }

    /// Read a 16-bit address from the reset vector at 0xFFFC-0xFFFD
    fn read_reset_vector(&self) -> u16 {
        self.memory.borrow().read_u16(RESET_VECTOR)
    }

    /// Push a byte onto the stack
    fn push_byte(&mut self, value: u8) {
        let addr = 0x0100 | (self.state.sp as u16);
        self.memory.borrow_mut().write(addr, value, false);
        self.state.sp = self.state.sp.wrapping_sub(1);
    }

    /// Push a word onto the stack (high byte first)
    fn push_word(&mut self, value: u16) {
        self.push_byte((value >> 8) as u8); // High byte first
        self.push_byte(value as u8); // Low byte second
    }

    /// Add cycles to the total cycle count
    pub fn add_cycles(&mut self, cycles: u64) {
        self.total_cycles += cycles;
    }

    /// Get the current CPU state
    pub fn get_state(&mut self) -> &mut CpuState {
        &mut self.state
    }

    /// Set the current CPU state
    pub fn set_state(&mut self, state: CpuState) {
        self.state = state;
    }

    pub fn total_cycles(&self) -> u64 {
        self.total_cycles
    }

    #[cfg(test)]
    pub fn set_total_cycles(&mut self, cycles: u64) {
        self.total_cycles = cycles;
    }
    /// Set the NMI pending flag
    /// This should be called by the NES loop when NMI is detected
    pub fn set_nmi_pending(&mut self, pending: bool) {
        // TODO implement NMI pending logic
    }

    /// Check if an NMI is pending
    pub fn is_nmi_pending(&self) -> bool {
        // TODO implement NMI pending logic
        false
    }

    pub fn should_poll_irq(&self) -> bool {
        // TODO implement IRQ polling logic
        false
    }

    pub fn trigger_irq(&mut self) -> u8 {
        // TODO Implement IRQ logic
        7
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apu::Apu;
    use crate::nes::TvSystem;
    use crate::ppu::Ppu;

    // Helper function to create a test memory controller
    fn create_test_memory() -> Rc<RefCell<MemController>> {
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        Rc::new(RefCell::new(MemController::new(ppu, apu)))
    }

    // Helper function to execute CPU until instruction completes
    fn execute_instruction(cpu: &mut Cpu2) -> u64 {
        let start_cycles = cpu.total_cycles();
        let mut instruction_complete = false;
        let mut safety = 0;
        while !instruction_complete && safety < 100 {
            instruction_complete = cpu.tick_cycle();
            safety += 1;
        }
        assert!(instruction_complete, "Instruction did not complete");
        cpu.total_cycles() - start_cycles
    }

    #[test]
    fn test_opcode_00() {
        use crate::cartridge::Cartridge;

        let memory = create_test_memory();

        // Create a 32KB PRG ROM cartridge with IRQ vector at $FFFE-$FFFF
        let mut prg_rom = vec![0; 0x8000]; // 32KB

        // Set up BRK instruction at address $8400 (mapped to $0400 in ROM)
        prg_rom[0x0400] = BRK; // BRK opcode
        prg_rom[0x0401] = 0x00; // Padding byte

        // Set up IRQ vector at $FFFE-$FFFF (end of ROM) to point to $8000
        prg_rom[0x7FFE] = 0x00; // Low byte of IRQ handler ($8000)
        prg_rom[0x7FFF] = 0x80; // High byte of IRQ handler ($8000)

        let cartridge =
            Cartridge::from_parts(prg_rom, vec![], crate::cartridge::MirroringMode::Horizontal);
        memory.borrow_mut().map_cartridge(cartridge);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x8400; // Start at ROM address (not $0400)
        cpu.state.sp = 0xFD;
        cpu.state.p = 0b0010_0000; // Only unused flag set
        cpu.state.a = 0x42; // Some value to verify registers aren't affected

        let cycles = execute_instruction(&mut cpu);

        // PC should now point to the IRQ handler at $8000
        assert_eq!(cpu.state.pc, 0x8000, "PC should be loaded from IRQ vector");

        // Stack should have three values pushed:
        // SP was 0xFD, after pushing 3 bytes it should be 0xFA
        assert_eq!(
            cpu.state.sp, 0xFA,
            "Stack pointer should have decremented by 3"
        );

        // Check return address on stack (PC+2 = $8402)
        let pch = memory.borrow().read(0x01FD); // High byte at original SP
        let pcl = memory.borrow().read(0x01FC); // Low byte at SP-1
        let return_address = ((pch as u16) << 8) | (pcl as u16);
        assert_eq!(return_address, 0x8402, "Return address should be PC+2");

        // Check status register on stack (should have B flag set)
        let status_on_stack = memory.borrow().read(0x01FB); // Status at SP-2
        assert_eq!(
            status_on_stack & FLAG_BREAK,
            FLAG_BREAK,
            "B flag should be set in pushed status"
        );
        assert_eq!(
            status_on_stack & FLAG_UNUSED,
            FLAG_UNUSED,
            "Unused flag should be set in pushed status"
        );

        // I flag should be set in CPU
        assert_eq!(
            cpu.state.p & FLAG_INTERRUPT,
            FLAG_INTERRUPT,
            "I flag should be set after BRK"
        );

        // A register should be unchanged
        assert_eq!(cpu.state.a, 0x42, "A register should not be affected");

        // BRK takes 7 cycles
        assert_eq!(cycles, 7, "BRK should take 7 cycles");
    }

    #[test]
    fn test_opcode_01() {
        let memory = create_test_memory();

        // Set up ORA ($20,X) instruction at address $0400
        // With X=0x04, reads from zero page address ($20+$04) = $24
        // At $24-$25 we store the pointer $1234
        // At $1234 we store the value to ORA with
        memory.borrow_mut().write(0x0400, ORA_INDX, false); // ORA (zp,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at zero page $24 (base $20 + X register $04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte of target address
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte of target address

        // Set up value to ORA at address $1234
        memory.borrow_mut().write(0x1234, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.a = 0b1100_1100;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_1100 | 0b1010_1010 = 0b1110_1110
        assert_eq!(cpu.state.a, 0b1110_1110, "A should contain result of ORA");
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cycles, 6, "ORA indexed indirect should take 6 cycles");
    }

    #[test]
    fn test_opcode_02() {
        let memory = create_test_memory();

        // Set up KIL instruction at address $0400
        memory.borrow_mut().write(0x0400, KIL, false); // KIL opcode

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0x42;
        cpu.state.x = 0x55;
        cpu.state.y = 0x66;
        cpu.state.p = 0x24;

        // KIL should halt the CPU - it never completes normally
        // After executing, the CPU should be halted
        let start_cycles = cpu.total_cycles();
        let mut ticks = 0;
        let max_ticks = 10;

        // Tick once to fetch and start the KIL instruction
        cpu.tick_cycle();
        ticks += 1;

        // The CPU should now be halted and subsequent ticks should not advance
        let pc_after_fetch = cpu.state.pc;

        while ticks < max_ticks {
            cpu.tick_cycle();
            ticks += 1;
        }

        // PC should not have advanced beyond the KIL instruction
        assert_eq!(
            cpu.state.pc, pc_after_fetch,
            "PC should not advance after KIL"
        );

        // Registers should remain unchanged
        assert_eq!(cpu.state.a, 0x42, "A should not change");
        assert_eq!(cpu.state.x, 0x55, "X should not change");
        assert_eq!(cpu.state.y, 0x66, "Y should not change");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.p, 0x24, "P should not change");

        // The CPU should be in a halted state
        assert!(cpu.is_halted(), "CPU should be halted after KIL");
    }

    #[test]
    fn test_opcode_03() {
        let memory = create_test_memory();

        // Set up SLO ($20,X) instruction at address $0400
        // With X=0x04, reads from zero page address ($20+$04) = $24
        // At $24-$25 we store the pointer $1234
        // At $1234 we store the value to shift and OR with
        memory.borrow_mut().write(0x0400, SLO_INDX, false); // SLO (zp,X) opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page base address

        // Set up pointer at zero page $24 (base $20 + X register $04)
        memory.borrow_mut().write(0x0024, 0x34, false); // Low byte of target address
        memory.borrow_mut().write(0x0025, 0x12, false); // High byte of target address

        // Set up value at target address $1234
        memory.borrow_mut().write(0x1234, 0b0101_0101, false); // Value to shift left

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.x = 0x04;
        cpu.state.a = 0b1100_0011; // Accumulator value to OR with
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // Memory at $1234 should be shifted left: 0b0101_0101 << 1 = 0b1010_1010
        let memory_value = memory.borrow().read(0x1234);
        assert_eq!(
            memory_value, 0b1010_1010,
            "Memory should contain shifted value"
        );

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of OR");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero), C=0 (bit 7 of original was 0)
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");
        assert_eq!(cpu.state.p & 0x01, 0, "C flag should be clear");

        // SLO with indexed indirect should take 8 cycles
        // (5 for addressing + 3 for read-modify-write operation)
        assert_eq!(cycles, 8, "SLO indexed indirect should take 8 cycles");
    }

    #[test]
    fn test_opcode_04() {
        let memory = create_test_memory();

        // Set up DOP $20 instruction at address $0400
        memory.borrow_mut().write(0x0400, DOP_ZP, false); // DOP Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up a value at zero page $20 (will be read but ignored)
        memory.borrow_mut().write(0x0020, 0x42, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0x11;
        cpu.state.x = 0x22;
        cpu.state.y = 0x33;
        cpu.state.p = 0x44;

        let cycles = execute_instruction(&mut cpu);

        // DOP reads from memory but does nothing - all registers should be unchanged
        assert_eq!(cpu.state.a, 0x11, "A should not change");
        assert_eq!(cpu.state.x, 0x22, "X should not change");
        assert_eq!(cpu.state.y, 0x33, "Y should not change");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.p, 0x44, "P should not change");

        // Verify the memory value is still there (not modified)
        let memory_value = memory.borrow().read(0x0020);
        assert_eq!(memory_value, 0x42, "Memory should not be modified");

        // DOP with zero page should take 3 cycles
        // (1 opcode fetch + 1 ZP addressing + 1 read)
        assert_eq!(cycles, 3, "DOP zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_05() {
        let memory = create_test_memory();

        // Set up ORA $20 instruction at address $0400
        memory.borrow_mut().write(0x0400, ORA_ZP, false); // ORA Zero Page opcode
        memory.borrow_mut().write(0x0401, 0x20, false); // Zero page address

        // Set up value at zero page $20
        memory.borrow_mut().write(0x0020, 0b1010_1010, false);

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.a = 0b1100_0011;
        cpu.state.p = 0;

        let cycles = execute_instruction(&mut cpu);

        // A should be 0b1100_0011 | 0b1010_1010 = 0b1110_1011
        assert_eq!(cpu.state.a, 0b1110_1011, "A should contain result of ORA");

        // Flags: N=1 (bit 7 set), Z=0 (non-zero)
        assert_eq!(cpu.state.p & 0x80, 0x80, "N flag should be set");
        assert_eq!(cpu.state.p & 0x02, 0, "Z flag should be clear");

        // ORA with zero page should take 3 cycles
        // (1 opcode fetch + 1 ZP addressing + 1 read/operate)
        assert_eq!(cycles, 3, "ORA zero page should take 3 cycles");
    }

    #[test]
    fn test_opcode_4c() {
        let memory = create_test_memory();

        // Set up JMP $5678 instruction at address $0800
        memory.borrow_mut().write(0x0800, JMP_ABS, false); // JMP Absolute opcode (0x4C)
        memory.borrow_mut().write(0x0801, 0x78, false); // Low byte of target
        memory.borrow_mut().write(0x0802, 0x56, false); // High byte of target

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0800;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0xAA;
        cpu.state.x = 0xBB;
        cpu.state.y = 0xCC;
        cpu.state.p = 0xDD;

        let cycles = execute_instruction(&mut cpu);

        // Verify the CPU state after JMP execution
        assert_eq!(cpu.state.pc, 0x5678, "PC should jump to target address");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.a, 0xAA, "A should not change");
        assert_eq!(cpu.state.x, 0xBB, "X should not change");
        assert_eq!(cpu.state.y, 0xCC, "Y should not change");
        assert_eq!(cpu.state.p, 0xDD, "P should not change");
        assert_eq!(
            cycles, 3,
            "JMP Absolute should take 3 cycles total (2 addressing + 1 execution overlapped)"
        );
    }

    #[test]
    fn test_opcode_6c() {
        let memory = create_test_memory();

        // Set up JMP ($1200) instruction at address $0800
        memory.borrow_mut().write(0x0800, JMP_IND, false); // JMP Indirect opcode (0x6C)
        memory.borrow_mut().write(0x0801, 0x00, false); // Low byte of indirect address
        memory.borrow_mut().write(0x0802, 0x12, false); // High byte of indirect address

        // Set up the target address at the indirect location $1200
        memory.borrow_mut().write(0x1200, 0x34, false); // Low byte of target
        memory.borrow_mut().write(0x1201, 0x56, false); // High byte of target

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0800;
        cpu.state.sp = 0xFD;
        cpu.state.a = 0xAA;
        cpu.state.x = 0xBB;
        cpu.state.y = 0xCC;
        cpu.state.p = 0xDD;

        let cycles = execute_instruction(&mut cpu);

        // Verify the CPU state after JMP execution
        assert_eq!(cpu.state.pc, 0x5634, "PC should jump to target address");
        assert_eq!(cpu.state.sp, 0xFD, "SP should not change");
        assert_eq!(cpu.state.a, 0xAA, "A should not change");
        assert_eq!(cpu.state.x, 0xBB, "X should not change");
        assert_eq!(cpu.state.y, 0xCC, "Y should not change");
        assert_eq!(cpu.state.p, 0xDD, "P should not change");
        assert_eq!(
            cycles, 5,
            "JMP Indirect should take 5 cycles total (4 addressing + 1 execution overlapped)"
        );
    }

    #[test]
    fn test_opcode_6c_boundary_bug() {
        let memory = create_test_memory();

        // Set up JMP ($12FF) instruction at address $0800
        // This tests the page boundary bug where high byte is read from $1200 instead of $1300
        memory.borrow_mut().write(0x0800, JMP_IND, false); // JMP Indirect opcode (0x6C)
        memory.borrow_mut().write(0x0801, 0xFF, false); // Low byte of indirect address
        memory.borrow_mut().write(0x0802, 0x12, false); // High byte of indirect address

        // Set up target address with page boundary bug
        memory.borrow_mut().write(0x12FF, 0x34, false); // Low byte at $12FF
        memory.borrow_mut().write(0x1200, 0x56, false); // High byte wraps to $1200 (bug)
        memory.borrow_mut().write(0x1300, 0x99, false); // This would be correct but is not used

        let mut cpu = Cpu2::new(Rc::clone(&memory));
        cpu.state.pc = 0x0800;
        cpu.state.sp = 0xFD;

        execute_instruction(&mut cpu);

        // Verify the CPU jumps to $5634 (using $1200 for high byte, not $1300)
        assert_eq!(
            cpu.state.pc, 0x5634,
            "PC should use page boundary bug (high byte from $1200, not $1300)"
        );
    }
}
