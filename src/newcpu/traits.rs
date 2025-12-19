//! Traits for cycle-accurate CPU execution
//!
//! This module defines the core traits that separate addressing modes from operations,
//! enabling clean, reusable implementations.

use super::types::AddressingState;

/// Trait for addressing modes that resolve addresses cycle-by-cycle
pub trait AddressingMode {
    /// Returns the number of cycles needed to resolve the address
    /// This may vary based on page crossings or instruction type
    fn address_cycles(&self) -> u8;
    
    /// Execute one cycle of address resolution
    /// 
    /// # Arguments
    /// * `cycle` - The current cycle within address resolution (0-indexed)
    /// * `pc` - Current program counter
    /// * `x` - X register (for indexed modes)
    /// * `y` - Y register (for indexed modes)
    /// * `state` - Mutable addressing state for storing intermediate values
    /// * `read_fn` - Function to read a byte from memory
    /// 
    /// # Returns
    /// * `Some(addr)` - The resolved address (when resolution completes)
    /// * `None` - Address not yet resolved, needs more cycles
    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        x: u8,
        y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16>;
    
    /// Returns true if this addressing mode has page crossing penalty for reads
    fn has_page_cross_penalty(&self) -> bool {
        false
    }
}

/// CPU operation mnemonic
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mnemonic {
    // Arithmetic
    ADC, SBC,
    // Logical
    AND, ORA, EOR,
    // Shift/Rotate
    ASL, LSR, ROL, ROR,
    // Load/Store
    LDA, LDX, LDY, STA, STX, STY,
    // Transfer
    TAX, TAY, TXA, TYA, TSX, TXS,
    // Compare
    CMP, CPX, CPY,
    // Increment/Decrement
    INC, INX, INY, DEC, DEX, DEY,
    // Stack
    PHA, PLA, PHP, PLP,
    // Flags
    CLC, SEC, CLI, SEI, CLD, SED, CLV,
    // Branches
    BCC, BCS, BEQ, BNE, BMI, BPL, BVC, BVS,
    // Control
    JMP, JSR, RTS, RTI, BRK, NOP,
    // Bit test
    BIT,
    // Unofficial opcodes
    LAX, SAX, DCP, ISC, RLA, RRA, SLO, SRE,
    AAC, ARR, ASR, ATX, AXS, XAA,
    // Unofficial NOP variants
    DOP, TOP,
    // KIL (halts CPU)
    KIL,
}

/// CPU state needed for operations
pub struct CpuState {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub p: u8, // Status flags
}

/// Trait for CPU operations that execute on operands
pub trait Operation {
    /// Execute the operation for a read instruction
    /// 
    /// # Arguments
    /// * `state` - Mutable CPU state
    /// * `operand` - The operand value
    fn execute(&self, state: &mut CpuState, operand: u8);
    
    /// Execute the operation for a read-modify-write instruction
    /// 
    /// # Arguments
    /// * `state` - Mutable CPU state
    /// * `operand` - The operand value read from memory
    /// 
    /// # Returns
    /// The modified value to write back to memory
    fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
        // Default implementation - most operations are read-only
        self.execute(state, operand);
        operand
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test struct implementing AddressingMode
    struct TestAddressingMode {
        cycles: u8,
        has_penalty: bool,
    }

    impl AddressingMode for TestAddressingMode {
        fn address_cycles(&self) -> u8 {
            self.cycles
        }

        fn tick_addressing(
            &self,
            cycle: u8,
            _pc: &mut u16,
            _x: u8,
            _y: u8,
            _state: &mut AddressingState,
            _read_fn: &dyn Fn(u16) -> u8,
        ) -> Option<u16> {
            if cycle >= self.cycles - 1 {
                Some(0x1234) // Return resolved address
            } else {
                None
            }
        }

        fn has_page_cross_penalty(&self) -> bool {
            self.has_penalty
        }
    }

    #[test]
    fn test_addressing_mode_trait() {
        let mode = TestAddressingMode {
            cycles: 2,
            has_penalty: false,
        };

        assert_eq!(mode.address_cycles(), 2);
        assert_eq!(mode.has_page_cross_penalty(), false);

        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |_addr: u16| 0x00;

        // First cycle should return None
        let result = mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);
        assert_eq!(result, None);

        // Second cycle should return address
        let result = mode.tick_addressing(1, &mut pc, 0, 0, &mut state, &read_fn);
        assert_eq!(result, Some(0x1234));
    }

    #[test]
    fn test_addressing_mode_with_page_cross_penalty() {
        let mode = TestAddressingMode {
            cycles: 3,
            has_penalty: true,
        };

        assert_eq!(mode.has_page_cross_penalty(), true);
    }

    // Test struct implementing Operation
    struct TestOperation;

    impl Operation for TestOperation {
        fn execute(&self, state: &mut CpuState, operand: u8) {
            state.a = operand;
        }
    }

    #[test]
    fn test_operation_trait() {
        let op = TestOperation;
        let mut state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFF,
            p: 0,
        };

        op.execute(&mut state, 0x42);
        assert_eq!(state.a, 0x42);
    }

    // Test RMW operation
    struct TestRMWOperation;

    impl Operation for TestRMWOperation {
        fn execute(&self, _state: &mut CpuState, _operand: u8) {
            // No read-side effects
        }

        fn execute_rmw(&self, state: &mut CpuState, operand: u8) -> u8 {
            state.p |= 0x01; // Set a flag
            operand.wrapping_add(1) // Increment
        }
    }

    #[test]
    fn test_operation_rmw() {
        let op = TestRMWOperation;
        let mut state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFF,
            p: 0,
        };

        let result = op.execute_rmw(&mut state, 0x42);
        assert_eq!(result, 0x43);
        assert_eq!(state.p & 0x01, 0x01);
    }

    #[test]
    fn test_mnemonic_enum() {
        // Test that mnemonics can be compared
        assert_eq!(Mnemonic::LDA, Mnemonic::LDA);
        assert_ne!(Mnemonic::LDA, Mnemonic::STA);

        // Test pattern matching
        let mnemonic = Mnemonic::ADC;
        match mnemonic {
            Mnemonic::ADC => { /* expected */ }
            _ => panic!("Should match ADC"),
        }
    }
}
