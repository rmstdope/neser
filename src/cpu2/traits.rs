//! Traits for cycle-accurate CPU execution
//!
//! This module defines the core traits that separate addressing modes from operations,
//! enabling clean, reusable implementations.

use super::types::CpuState;
use crate::mem_controller::MemController;
use std::cell::RefCell;
use std::rc::Rc;

// use super::types::AddressingState;

/// Trait for addressing modes that resolve addresses cycle-by-cycle
pub trait AddressingMode {
    /// Returns true if the addressing mode has completed address resolution
    /// This is typically determined by the number of cycles taken
    /// and any page crossing penalties.
    fn is_done(&self) -> bool;

    /// Ticks the addressing mode by one cycle
    /// This may involve reading bytes from memory and updating internal state.
    /// Note that is_done should be called first to see if any more ticks are needed.
    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>);

    /// Returns true if this addressing mode has page crossing penalty for reads
    fn has_page_cross_penalty(&self) -> bool {
        false
    }

    /// Returns the address of the operand once addressing is done
    /// # Panics
    /// Panics if called before is_done() returns true
    fn get_address(&self) -> u16 {
        panic!("get_address not implemented for this addressing mode");
    }
}

pub trait InstructionType {
    /// Returns true if the instruction has completed execution
    fn is_done(&self) -> bool;

    /// Ticks the instruction type by one cycle
    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>);
}

// Opcode constants for use in match patterns
pub const BRK: u8 = 0x00;
pub const ORA_INDX: u8 = 0x01;
pub const ORA_ZP: u8 = 0x05;
pub const ASL_ZP: u8 = 0x06;
pub const PHP: u8 = 0x08;
pub const ORA_IMM: u8 = 0x09;
pub const ASL_A: u8 = 0x0A;
pub const ORA_ABS: u8 = 0x0D;
pub const ASL_ABS: u8 = 0x0E;
pub const BPL: u8 = 0x10;
pub const ORA_INDY: u8 = 0x11;
pub const ORA_ZPX: u8 = 0x15;
pub const ASL_ZPX: u8 = 0x16;
pub const CLC: u8 = 0x18;
pub const ORA_ABSY: u8 = 0x19;
pub const ORA_ABSX: u8 = 0x1D;
pub const ASL_ABSX: u8 = 0x1E;
pub const JSR: u8 = 0x20;
pub const AND_INDX: u8 = 0x21;
pub const BIT_ZP: u8 = 0x24;
pub const AND_ZP: u8 = 0x25;
pub const ROL_ZP: u8 = 0x26;
pub const PLP: u8 = 0x28;
pub const AND_IMM: u8 = 0x29;
pub const ROL_ACC: u8 = 0x2A;
pub const BIT_ABS: u8 = 0x2C;
pub const AND_ABS: u8 = 0x2D;
pub const ROL_ABS: u8 = 0x2E;
pub const BMI: u8 = 0x30;
pub const AND_INDY: u8 = 0x31;
pub const AND_ZPX: u8 = 0x35;
pub const ROL_ZPX: u8 = 0x36;
pub const SEC: u8 = 0x38;
pub const AND_ABSY: u8 = 0x39;
pub const AND_ABSX: u8 = 0x3D;
pub const ROL_ABSX: u8 = 0x3E;
pub const RTI: u8 = 0x40;
pub const EOR_INDX: u8 = 0x41;
pub const EOR_ZP: u8 = 0x45;
pub const LSR_ZP: u8 = 0x46;
pub const PHA: u8 = 0x48;
pub const EOR_IMM: u8 = 0x49;
pub const LSR_ACC: u8 = 0x4A;
pub const JMP_ABS: u8 = 0x4C;
pub const EOR_ABS: u8 = 0x4D;
pub const LSR_ABS: u8 = 0x4E;
pub const BVC: u8 = 0x50;
pub const EOR_INDY: u8 = 0x51;
pub const EOR_ZPX: u8 = 0x55;
pub const LSR_ZPX: u8 = 0x56;
pub const CLI: u8 = 0x58;
pub const EOR_ABSY: u8 = 0x59;
pub const EOR_ABSX: u8 = 0x5D;
pub const LSR_ABSX: u8 = 0x5E;
pub const RTS: u8 = 0x60;
pub const ADC_INDX: u8 = 0x61;
pub const ADC_ZP: u8 = 0x65;
pub const ROR_ZP: u8 = 0x66;
pub const PLA: u8 = 0x68;
pub const ADC_IMM: u8 = 0x69;
pub const ROR_ACC: u8 = 0x6A;
pub const JMP_IND: u8 = 0x6C;
pub const ADC_ABS: u8 = 0x6D;
pub const ROR_ABS: u8 = 0x6E;
pub const BVS: u8 = 0x70;
pub const ADC_INDY: u8 = 0x71;
pub const ADC_ZPX: u8 = 0x75;
pub const ROR_ZPX: u8 = 0x76;
pub const SEI: u8 = 0x78;
pub const ADC_ABSY: u8 = 0x79;
pub const ADC_ABSX: u8 = 0x7D;
pub const ROR_ABSX: u8 = 0x7E;
pub const STA_INDX: u8 = 0x81;
pub const STY_ZP: u8 = 0x84;
pub const STA_ZP: u8 = 0x85;
pub const STX_ZP: u8 = 0x86;
pub const DEY: u8 = 0x88;
pub const TXA: u8 = 0x8A;
pub const STY_ABS: u8 = 0x8C;
pub const STA_ABS: u8 = 0x8D;
pub const STX_ABS: u8 = 0x8E;
pub const BCC: u8 = 0x90;
pub const STA_INDY: u8 = 0x91;
pub const STY_ZPX: u8 = 0x94;
pub const STA_ZPX: u8 = 0x95;
pub const STX_ZPY: u8 = 0x96;
pub const TYA: u8 = 0x98;
pub const STA_ABSY: u8 = 0x99;
pub const TXS: u8 = 0x9A;
pub const STA_ABSX: u8 = 0x9D;
pub const LDY_IMM: u8 = 0xA0;
pub const LDA_INDX: u8 = 0xA1;
pub const LDX_IMM: u8 = 0xA2;
pub const LDY_ZP: u8 = 0xA4;
pub const LDA_ZP: u8 = 0xA5;
pub const LDX_ZP: u8 = 0xA6;
pub const TAY: u8 = 0xA8;
pub const LDA_IMM: u8 = 0xA9;
pub const TAX: u8 = 0xAA;
pub const LDY_ABS: u8 = 0xAC;
pub const LDA_ABS: u8 = 0xAD;
pub const LDX_ABS: u8 = 0xAE;
pub const BCS: u8 = 0xB0;
pub const LDA_INDY: u8 = 0xB1;
pub const LDY_ZPX: u8 = 0xB4;
pub const LDA_ZPX: u8 = 0xB5;
pub const LDX_ZPY: u8 = 0xB6;
pub const CLV: u8 = 0xB8;
pub const LDA_ABSY: u8 = 0xB9;
pub const TSX: u8 = 0xBA;
pub const LDY_ABSX: u8 = 0xBC;
pub const LDA_ABSX: u8 = 0xBD;
pub const LDX_ABSY: u8 = 0xBE;
pub const CPY_IMM: u8 = 0xC0;
pub const CMP_INDX: u8 = 0xC1;
pub const CPY_ZP: u8 = 0xC4;
pub const CMP_ZP: u8 = 0xC5;
pub const DEC_ZP: u8 = 0xC6;
pub const INY: u8 = 0xC8;
pub const CMP_IMM: u8 = 0xC9;
pub const DEX: u8 = 0xCA;
pub const CPY_ABS: u8 = 0xCC;
pub const CMP_ABS: u8 = 0xCD;
pub const DEC_ABS: u8 = 0xCE;
pub const BNE: u8 = 0xD0;
pub const CMP_INDY: u8 = 0xD1;
pub const CMP_ZPX: u8 = 0xD5;
pub const DEC_ZPX: u8 = 0xD6;
pub const CLD: u8 = 0xD8;
pub const CMP_ABSY: u8 = 0xD9;
pub const CMP_ABSX: u8 = 0xDD;
pub const DEC_ABSX: u8 = 0xDE;
pub const CPX_IMM: u8 = 0xE0;
pub const SBC_INDX: u8 = 0xE1;
pub const CPX_ZP: u8 = 0xE4;
pub const SBC_ZP: u8 = 0xE5;
pub const INC_ZP: u8 = 0xE6;
pub const INX: u8 = 0xE8;
pub const SBC_IMM: u8 = 0xE9;
pub const NOP: u8 = 0xEA;
pub const CPX_ABS: u8 = 0xEC;
pub const SBC_ABS: u8 = 0xED;
pub const INC_ABS: u8 = 0xEE;
pub const BEQ: u8 = 0xF0;
pub const SBC_INDY: u8 = 0xF1;
pub const SBC_ZPX: u8 = 0xF5;
pub const INC_ZPX: u8 = 0xF6;
pub const SED: u8 = 0xF8;
pub const SBC_ABSY: u8 = 0xF9;
pub const SBC_ABSX: u8 = 0xFD;
pub const INC_ABSX: u8 = 0xFE;

// Undocumented opcodes
pub const AAC_IMM: u8 = 0x0B;
pub const AAC_IMM2: u8 = 0x2B;
pub const ARR_IMM: u8 = 0x6B;
pub const ASR_IMM: u8 = 0x4B;
pub const ATX_IMM: u8 = 0xAB;
pub const SAX_INDX: u8 = 0x83;
pub const SAX_ZP: u8 = 0x87;
pub const SAX_ABS: u8 = 0x8F;
pub const SAX_ZPY: u8 = 0x97;
pub const AXA_INDY: u8 = 0x93;
pub const AXA_ABSY: u8 = 0x9F;
pub const AXS_IMM: u8 = 0xCB;
pub const DCP_INDX: u8 = 0xC3;
pub const DCP_ZP: u8 = 0xC7;
pub const DCP_ABS: u8 = 0xCF;
pub const DCP_INDY: u8 = 0xD3;
pub const DCP_ZPX: u8 = 0xD7;
pub const DCP_ABSY: u8 = 0xDB;
pub const DCP_ABSX: u8 = 0xDF;
pub const DOP_ZP: u8 = 0x04;
pub const DOP_ZPX: u8 = 0x14;
pub const DOP_ZPX2: u8 = 0x34;
pub const DOP_ZP2: u8 = 0x44;
pub const DOP_ZPX3: u8 = 0x54;
pub const DOP_ZP3: u8 = 0x64;
pub const DOP_ZPX4: u8 = 0x74;
pub const DOP_IMM: u8 = 0x80;
pub const DOP_IMM2: u8 = 0x82;
pub const DOP_IMM3: u8 = 0x89;
pub const DOP_IMM4: u8 = 0xC2;
pub const DOP_ZPX5: u8 = 0xD4;
pub const DOP_IMM5: u8 = 0xE2;
pub const DOP_ZPX6: u8 = 0xF4;
pub const ISB_INDX: u8 = 0xE3;
pub const ISB_ZP: u8 = 0xE7;
pub const ISB_ABS: u8 = 0xEF;
pub const ISB_INDY: u8 = 0xF3;
pub const ISB_ZPX: u8 = 0xF7;
pub const ISB_ABSY: u8 = 0xFB;
pub const ISB_ABSX: u8 = 0xFF;
pub const KIL: u8 = 0x02;
pub const KIL2: u8 = 0x12;
pub const KIL3: u8 = 0x22;
pub const KIL4: u8 = 0x32;
pub const KIL5: u8 = 0x42;
pub const KIL6: u8 = 0x52;
pub const KIL7: u8 = 0x62;
pub const KIL8: u8 = 0x72;
pub const KIL9: u8 = 0x92;
pub const KIL10: u8 = 0xB2;
pub const KIL11: u8 = 0xD2;
pub const KIL12: u8 = 0xF2;
pub const LAR_ABSY: u8 = 0xBB;
pub const LAX_INDX: u8 = 0xA3;
pub const LAX_ZP: u8 = 0xA7;
pub const LAX_ABS: u8 = 0xAF;
pub const LAX_INDY: u8 = 0xB3;
pub const LAX_ZPY: u8 = 0xB7;
pub const LAX_ABSY: u8 = 0xBF;
pub const NOP_IMP: u8 = 0x1A;
pub const NOP_IMP2: u8 = 0x3A;
pub const NOP_IMP3: u8 = 0x5A;
pub const NOP_IMP4: u8 = 0x7A;
pub const NOP_IMP5: u8 = 0xDA;
pub const NOP_IMP6: u8 = 0xFA;
pub const RLA_INDX: u8 = 0x23;
pub const RLA_ZP: u8 = 0x27;
pub const RLA_ABS: u8 = 0x2F;
pub const RLA_INDY: u8 = 0x33;
pub const RLA_ZPX: u8 = 0x37;
pub const RLA_ABSY: u8 = 0x3B;
pub const RLA_ABSX: u8 = 0x3F;
pub const RRA_INDX: u8 = 0x63;
pub const RRA_ZP: u8 = 0x67;
pub const RRA_ABS: u8 = 0x6F;
pub const RRA_INDY: u8 = 0x73;
pub const RRA_ZPX: u8 = 0x77;
pub const RRA_ABSY: u8 = 0x7B;
pub const RRA_ABSX: u8 = 0x7F;
pub const SBC_IMM2: u8 = 0xEB;
pub const SLO_INDX: u8 = 0x03;
pub const SLO_ZP: u8 = 0x07;
pub const SLO_ABS: u8 = 0x0F;
pub const SLO_INDY: u8 = 0x13;
pub const SLO_ZPX: u8 = 0x17;
pub const SLO_ABSY: u8 = 0x1B;
pub const SLO_ABSX: u8 = 0x1F;
pub const SRE_INDX: u8 = 0x43;
pub const SRE_ZP: u8 = 0x47;
pub const SRE_ABS: u8 = 0x4F;
pub const SRE_INDY: u8 = 0x53;
pub const SRE_ZPX: u8 = 0x57;
pub const SRE_ABSY: u8 = 0x5B;
pub const SRE_ABSX: u8 = 0x5F;
pub const SXA_ABSY: u8 = 0x9E;
pub const SYA_ABSX: u8 = 0x9C;
pub const TOP_ABS: u8 = 0x0C;
pub const TOP_ABSX: u8 = 0x1C;
pub const TOP_ABSX2: u8 = 0x3C;
pub const TOP_ABSX3: u8 = 0x5C;
pub const TOP_ABSX4: u8 = 0x7C;
pub const TOP_ABSX5: u8 = 0xDC;
pub const TOP_ABSX6: u8 = 0xFC;
pub const XAA_IMM: u8 = 0x8B;
pub const XAS_ABSY: u8 = 0x9B;

// /// CPU operation mnemonic
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum Mnemonic {
//     // Arithmetic
//     ADC,
//     SBC,
//     // Logical
//     AND,
//     ORA,
//     EOR,
//     // Shift/Rotate
//     ASL,
//     LSR,
//     ROL,
//     ROR,
//     // Load/Store
//     LDA,
//     LDX,
//     LDY,
//     STA,
//     STX,
//     STY,
//     // Transfer
//     TAX,
//     TAY,
//     TXA,
//     TYA,
//     TSX,
//     TXS,
//     // Compare
//     CMP,
//     CPX,
//     CPY,
//     // Increment/Decrement
//     INC,
//     INX,
//     INY,
//     DEC,
//     DEX,
//     DEY,
//     // Stack
//     PHA,
//     PLA,
//     PHP,
//     PLP,
//     // Flags
//     CLC,
//     SEC,
//     CLI,
//     SEI,
//     CLD,
//     SED,
//     CLV,
//     // Branches
//     BCC,
//     BCS,
//     BEQ,
//     BNE,
//     BMI,
//     BPL,
//     BVC,
//     BVS,
//     // Control
//     JMP,
//     JSR,
//     RTS,
//     RTI,
//     BRK,
//     NOP,
//     // Bit test
//     BIT,
//     // Unofficial opcodes
//     LAX,
//     SAX,
//     DCP,
//     ISC,
//     RLA,
//     RRA,
//     SLO,
//     SRE,
//     AAC,
//     ARR,
//     ASR,
//     ATX,
//     AXS,
//     XAA,
//     // Unofficial NOP variants
//     DOP,
//     TOP,
//     // KIL (halts CPU)
//     KIL,
// }

// /// CPU state needed for operations
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub struct CpuState {
//     pub a: u8,
//     pub x: u8,
//     pub y: u8,
//     pub sp: u8,
//     pub p: u8, // Status flags
// }

// /// Trait for CPU operations that execute on operands
// pub trait Operation {
//     /// Execute the operation for a read instruction
//     ///
//     /// # Arguments
//     /// * `state` - Mutable CPU state
//     /// * `operand` - The operand value
//     fn execute(&self, state: &mut CpuState, operand: u8);

//     /// Execute the operation for a read-modify-write instruction
//     ///
//     /// # Arguments
//     /// * `state` - Mutable CPU state
//     /// * `operand` - The operand value read from memory
//     ///
//     /// # Returns
//     /// The modified value to write back to memory
//     fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
//         // Default implementation - most operations are read-only
//         self.execute(state, operand);
//         operand
//     }

//     /// Execute a stack push operation
//     ///
//     /// # Arguments
//     /// * `state` - Mutable CPU state
//     ///
//     /// # Returns
//     /// The value to push onto the stack
//     fn execute_stack(&self, state: &mut CpuState) -> u8 {
//         // Default implementation - should not be called for non-stack operations
//         panic!("execute_stack not implemented for this operation");
//     }

//     /// Check if this operation is a stack pull operation (PLA/PLP)
//     ///
//     /// # Returns
//     /// `true` if this is a pull operation, `false` otherwise
//     fn is_pull(&self) -> bool {
//         false // Default: not a pull operation
//     }

//     /// Execute a stack pull operation
//     ///
//     /// # Arguments
//     /// * `state` - Mutable CPU state
//     /// * `value` - The value pulled from the stack
//     fn execute_pull(&self, state: &mut CpuState, value: u8) {
//         // Default implementation - should not be called for non-stack operations
//         panic!("execute_pull not implemented for this operation");
//     }

//     /// Execute a control flow operation
//     ///
//     /// # Arguments
//     /// * `state` - Mutable CPU state
//     /// * `target_addr` - The target address for the control flow operation
//     ///
//     /// # Returns
//     /// The new PC value, or None if PC should not be modified
//     fn execute_control(&self, _state: &mut CpuState, target_addr: u16) -> Option<u16> {
//         // Default implementation - return the target address for simple jumps
//         Some(target_addr)
//     }

//     /// Execute JSR (Jump to Subroutine) - pushes return address to stack
//     ///
//     /// # Arguments
//     /// * `state` - Mutable CPU state
//     /// * `target_addr` - The target address to jump to
//     /// * `current_pc` - The current PC value
//     ///
//     /// # Returns
//     /// Tuple of (high_byte, low_byte) to push to stack
//     fn execute_jsr(&self, _state: &mut CpuState, _target_addr: u16, _current_pc: u16) -> (u8, u8) {
//         panic!("execute_jsr not implemented for this operation");
//     }

//     /// Execute RTS (Return from Subroutine) - pulls return address from stack
//     ///
//     /// # Arguments
//     /// * `state` - Mutable CPU state
//     /// * `low_byte` - Low byte pulled from stack
//     /// * `high_byte` - High byte pulled from stack
//     ///
//     /// # Returns
//     /// The new PC value
//     fn execute_rts(&self, _state: &mut CpuState, _low_byte: u8, _high_byte: u8) -> u16 {
//         panic!("execute_rts not implemented for this operation");
//     }

//     /// Execute RTI (Return from Interrupt) - pulls status and return address from stack
//     ///
//     /// # Arguments
//     /// * `state` - Mutable CPU state
//     /// * `status` - Status byte pulled from stack
//     /// * `pc_low` - Low byte of PC pulled from stack
//     /// * `pc_high` - High byte of PC pulled from stack
//     ///
//     /// # Returns
//     /// The new PC value
//     fn execute_rti(&self, _state: &mut CpuState, _status: u8, _pc_low: u8, _pc_high: u8) -> u16 {
//         panic!("execute_rti not implemented for this operation");
//     }

//     /// Execute a branch instruction - checks if branch condition is met
//     ///
//     /// # Arguments
//     /// * `state` - CPU state (used to check status flags)
//     ///
//     /// # Returns
//     /// True if the branch should be taken, false otherwise
//     fn execute_branch(&self, _state: &CpuState) -> bool {
//         panic!("execute_branch not implemented for this operation");
//     }

//     /// Execute BRK (Break) instruction - pushes PC+2 and status to stack
//     ///
//     /// # Arguments
//     /// * `state` - Mutable CPU state
//     /// * `current_pc` - The current PC value
//     /// * `nmi_pending` - Whether NMI is pending (for NMI hijacking)
//     ///
//     /// # Returns
//     /// Tuple of (pc_high, pc_low, status) to push to stack
//     fn execute_brk(
//         &self,
//         _state: &mut CpuState,
//         _current_pc: u16,
//         _nmi_pending: bool,
//     ) -> (u8, u8, u8) {
//         panic!("execute_brk not implemented for this operation");
//     }

//     /// Check if this operation is a BRK instruction.
//     ///
//     /// # Returns
//     /// `true` if this is BRK, `false` otherwise
//     fn is_brk(&self) -> bool {
//         false // Default: not BRK
//     }

//     /// Check if this operation should inhibit IRQ for one instruction.
//     ///
//     /// On the 6502, CLI, SEI, and PLP instructions poll for interrupts at the end
//     /// of their first cycle, before the I flag is modified in the second cycle.
//     /// This means a pending IRQ will not be serviced until after the next instruction
//     /// completes, creating a one-instruction delay.
//     ///
//     /// RTI does NOT have this behavior - it affects the I flag immediately.
//     ///
//     /// # Returns
//     /// - `true` for CLI, SEI, PLP
//     /// - `false` for all other operations (default)
//     fn inhibits_irq(&self) -> bool {
//         false // Default: most operations don't inhibit IRQ
//     }

//     /// Check if this operation is a JMP instruction.
//     ///
//     /// JMP completes immediately when addressing finishes, without an Execute phase.
//     ///
//     /// # Returns
//     /// `true` if this is JMP, `false` otherwise
//     fn is_jmp(&self) -> bool {
//         false // Default: not JMP
//     }

//     /// Get the value to write for Write instructions (STA, STX, STY)
//     ///
//     /// # Arguments
//     /// * `state` - CPU state containing register values
//     ///
//     /// # Returns
//     /// The byte value to write to memory
//     fn get_write_value(&self, state: &CpuState) -> u8 {
//         // Default: write accumulator (for STA)
//         state.a
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;

//     // Test struct implementing AddressingMode
//     struct TestAddressingMode {
//         cycles: u8,
//         has_penalty: bool,
//     }

//     impl AddressingMode for TestAddressingMode {
//         fn address_cycles(&self) -> u8 {
//             self.cycles
//         }

//         fn tick_addressing(
//             &self,
//             cycle: u8,
//             _pc: &mut u16,
//             _x: u8,
//             _y: u8,
//             _state: &mut AddressingState,
//             _read_fn: &dyn Fn(u16) -> u8,
//         ) -> Option<u16> {
//             if cycle >= self.cycles - 1 {
//                 Some(0x1234) // Return resolved address
//             } else {
//                 None
//             }
//         }

//         fn has_page_cross_penalty(&self) -> bool {
//             self.has_penalty
//         }
//     }

//     #[test]
//     fn test_addressing_mode_trait() {
//         let mode = TestAddressingMode {
//             cycles: 2,
//             has_penalty: false,
//         };

//         assert_eq!(mode.address_cycles(), 2);
//         assert_eq!(mode.has_page_cross_penalty(), false);

//         let mut state = AddressingState::default();
//         let mut pc = 0x8000;
//         let read_fn = |_addr: u16| 0x00;

//         // First cycle should return None
//         let result = mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);
//         assert_eq!(result, None);

//         // Second cycle should return address
//         let result = mode.tick_addressing(1, &mut pc, 0, 0, &mut state, &read_fn);
//         assert_eq!(result, Some(0x1234));
//     }

//     #[test]
//     fn test_addressing_mode_with_page_cross_penalty() {
//         let mode = TestAddressingMode {
//             cycles: 3,
//             has_penalty: true,
//         };

//         assert_eq!(mode.has_page_cross_penalty(), true);
//     }

//     // Test struct implementing Operation
//     struct TestOperation;

//     impl Operation for TestOperation {
//         fn execute(&self, state: &mut CpuState, operand: u8) {
//             state.a = operand;
//         }
//     }

//     #[test]
//     fn test_operation_trait() {
//         let op = TestOperation;
//         let mut state = CpuState {
//             a: 0,
//             x: 0,
//             y: 0,
//             sp: 0xFF,
//             p: 0,
//         };

//         op.execute(&mut state, 0x42);
//         assert_eq!(state.a, 0x42);
//     }

//     // Test RMW operation
//     struct TestRMWOperation;

//     impl Operation for TestRMWOperation {
//         fn execute(&self, _state: &mut CpuState, _operand: u8) {
//             // No read-side effects
//         }

//         fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
//             state.p |= 0x01; // Set a flag
//             operand.wrapping_add(1) // Increment
//         }
//     }

//     #[test]
//     fn test_operation_rmw() {
//         let op = TestRMWOperation;
//         let mut state = CpuState {
//             a: 0,
//             x: 0,
//             y: 0,
//             sp: 0xFF,
//             p: 0,
//         };

//         let result = op.execute_rmw(&mut state, 0x42);
//         assert_eq!(result, 0x43);
//         assert_eq!(state.p & 0x01, 0x01);
//     }

//     #[test]
//     fn test_mnemonic_enum() {
//         // Test that mnemonics can be compared
//         assert_eq!(Mnemonic::LDA, Mnemonic::LDA);
//         assert_ne!(Mnemonic::LDA, Mnemonic::STA);

//         // Test pattern matching
//         let mnemonic = Mnemonic::ADC;
//         match mnemonic {
//             Mnemonic::ADC => { /* expected */ }
//             _ => panic!("Should match ADC"),
//         }
//     }
// }
