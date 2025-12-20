use super::addressing::Implied;
use super::instruction::Instruction;
use super::instruction_types::Jsr;
use super::traits::JSR;
use crate::cpu2::CpuState;
use crate::mem_controller::MemController;
use std::cell::RefCell;
use std::rc::Rc;

/// NES 6502 CPU
pub struct Cpu {
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

// Status register flags
#[allow(dead_code)]
const FLAG_CARRY: u8 = 0b0000_0001;
#[allow(dead_code)]
const FLAG_ZERO: u8 = 0b0000_0010;
#[allow(dead_code)]
const FLAG_INTERRUPT: u8 = 0b0000_0100;
#[allow(dead_code)]
const FLAG_DECIMAL: u8 = 0b0000_1000;
#[allow(dead_code)]
const FLAG_BREAK: u8 = 0b0001_0000;
const FLAG_UNUSED: u8 = 0b0010_0000;
#[allow(dead_code)]
const FLAG_OVERFLOW: u8 = 0b0100_0000;
#[allow(dead_code)]
const FLAG_NEGATIVE: u8 = 0b1000_0000;

#[allow(dead_code)]
const NMI_VECTOR: u16 = 0xFFFA;
#[allow(dead_code)]
const RESET_VECTOR: u16 = 0xFFFC;
#[allow(dead_code)]
const IRQ_VECTOR: u16 = 0xFFFE;

impl Cpu {
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

    /// Execute a single CPU cycle
    /// Returns true when the current instruction completes
    pub fn tick_cycle(&mut self) -> bool {
        if self.halted {
            return false;
        }

        // If no current instruction, fetch and decode a new one
        if self.current_instruction.is_none() {
            let opcode = self.memory.borrow().read(self.state.pc);
            if let Some(instruction) = Self::decode(opcode) {
                self.state.pc = self.state.pc.wrapping_add(1);
                self.current_instruction = Some(instruction);
                return false;
            } else {
                // Unimplemented opcode - halt
                self.halted = true;
                return false;
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
            JSR => {
                // JSR handles its own address fetching internally, so we use Implied addressing
                Some(Instruction::new(Box::new(Implied), Box::new(Jsr::new())))
            }
            _ => None, // Unimplemented opcode
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apu::Apu;
    use crate::nes::TvSystem;
    use crate::ppu::Ppu;

    #[test]
    fn test_jsr_execution() {
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Set up JSR $1234 instruction at address $0400
        memory.borrow_mut().write(0x0400, JSR, false); // JSR opcode
        memory.borrow_mut().write(0x0401, 0x34, false); // Low byte of target
        memory.borrow_mut().write(0x0402, 0x12, false); // High byte of target

        let mut cpu = Cpu::new(Rc::clone(&memory));
        cpu.state.pc = 0x0400;
        cpu.state.sp = 0xFD;

        // Decode the JSR instruction
        let opcode = memory.borrow().read(cpu.state.pc);
        let mut instruction = Cpu::decode(opcode).expect("JSR should decode successfully");

        // Advance PC past the opcode (JSR reads operands starting from PC+1)
        cpu.state.pc = cpu.state.pc.wrapping_add(1);

        // Tick the instruction until it's done (JSR takes 6 cycles)
        let mut cycles = 0;
        while cycles < 6 {
            instruction.tick(&mut cpu.state, Rc::clone(&memory));
            cycles += 1;
        }

        // Verify the CPU state after JSR execution
        assert_eq!(cpu.state.pc, 0x1234, "PC should jump to target address");
        assert_eq!(cpu.state.sp, 0xFB, "SP should have decremented by 2");

        // Verify return address on stack (should be 0x0402, which is PC after reading both operand bytes)
        let pch = memory.borrow().read(0x01FD);
        let pcl = memory.borrow().read(0x01FC);
        let return_address = ((pch as u16) << 8) | (pcl as u16);
        assert_eq!(
            return_address, 0x0402,
            "Return address on stack should point to the byte after JSR instruction"
        );
    }
}
