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
    ADC,
    SBC,
    // Logical
    AND,
    ORA,
    EOR,
    // Shift/Rotate
    ASL,
    LSR,
    ROL,
    ROR,
    // Load/Store
    LDA,
    LDX,
    LDY,
    STA,
    STX,
    STY,
    // Transfer
    TAX,
    TAY,
    TXA,
    TYA,
    TSX,
    TXS,
    // Compare
    CMP,
    CPX,
    CPY,
    // Increment/Decrement
    INC,
    INX,
    INY,
    DEC,
    DEX,
    DEY,
    // Stack
    PHA,
    PLA,
    PHP,
    PLP,
    // Flags
    CLC,
    SEC,
    CLI,
    SEI,
    CLD,
    SED,
    CLV,
    // Branches
    BCC,
    BCS,
    BEQ,
    BNE,
    BMI,
    BPL,
    BVC,
    BVS,
    // Control
    JMP,
    JSR,
    RTS,
    RTI,
    BRK,
    NOP,
    // Bit test
    BIT,
    // Unofficial opcodes
    LAX,
    SAX,
    DCP,
    ISC,
    RLA,
    RRA,
    SLO,
    SRE,
    AAC,
    ARR,
    ASR,
    ATX,
    AXS,
    XAA,
    // Unofficial NOP variants
    DOP,
    TOP,
    // KIL (halts CPU)
    KIL,
}

/// CPU state needed for operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// Execute a stack push operation
    ///
    /// # Arguments
    /// * `state` - Mutable CPU state
    ///
    /// # Returns
    /// The value to push onto the stack
    fn execute_stack(&self, state: &mut CpuState) -> u8 {
        // Default implementation - should not be called for non-stack operations
        panic!("execute_stack not implemented for this operation");
    }

    /// Execute a stack pull operation
    ///
    /// # Arguments
    /// * `state` - Mutable CPU state
    /// * `value` - The value pulled from the stack
    fn execute_pull(&self, state: &mut CpuState, value: u8) {
        // Default implementation - should not be called for non-stack operations
        panic!("execute_pull not implemented for this operation");
    }

    /// Execute a control flow operation
    ///
    /// # Arguments
    /// * `state` - Mutable CPU state
    /// * `target_addr` - The target address for the control flow operation
    ///
    /// # Returns
    /// The new PC value, or None if PC should not be modified
    fn execute_control(&self, _state: &mut CpuState, target_addr: u16) -> Option<u16> {
        // Default implementation - return the target address for simple jumps
        Some(target_addr)
    }

    /// Execute JSR (Jump to Subroutine) - pushes return address to stack
    ///
    /// # Arguments
    /// * `state` - Mutable CPU state
    /// * `target_addr` - The target address to jump to
    /// * `current_pc` - The current PC value
    ///
    /// # Returns
    /// Tuple of (high_byte, low_byte) to push to stack
    fn execute_jsr(&self, _state: &mut CpuState, _target_addr: u16, _current_pc: u16) -> (u8, u8) {
        panic!("execute_jsr not implemented for this operation");
    }

    /// Execute RTS (Return from Subroutine) - pulls return address from stack
    ///
    /// # Arguments
    /// * `state` - Mutable CPU state
    /// * `low_byte` - Low byte pulled from stack
    /// * `high_byte` - High byte pulled from stack
    ///
    /// # Returns
    /// The new PC value
    fn execute_rts(&self, _state: &mut CpuState, _low_byte: u8, _high_byte: u8) -> u16 {
        panic!("execute_rts not implemented for this operation");
    }

    /// Execute RTI (Return from Interrupt) - pulls status and return address from stack
    ///
    /// # Arguments
    /// * `state` - Mutable CPU state
    /// * `status` - Status byte pulled from stack
    /// * `pc_low` - Low byte of PC pulled from stack
    /// * `pc_high` - High byte of PC pulled from stack
    ///
    /// # Returns
    /// The new PC value
    fn execute_rti(&self, _state: &mut CpuState, _status: u8, _pc_low: u8, _pc_high: u8) -> u16 {
        panic!("execute_rti not implemented for this operation");
    }

    /// Execute a branch instruction - checks if branch condition is met
    ///
    /// # Arguments
    /// * `state` - CPU state (used to check status flags)
    ///
    /// # Returns
    /// True if the branch should be taken, false otherwise
    fn execute_branch(&self, _state: &CpuState) -> bool {
        panic!("execute_branch not implemented for this operation");
    }

    /// Execute BRK (Break) instruction - pushes PC+2 and status to stack
    ///
    /// # Arguments
    /// * `state` - Mutable CPU state
    /// * `current_pc` - The current PC value
    /// * `nmi_pending` - Whether NMI is pending (for NMI hijacking)
    ///
    /// # Returns
    /// Tuple of (pc_high, pc_low, status) to push to stack
    fn execute_brk(&self, _state: &mut CpuState, _current_pc: u16, _nmi_pending: bool) -> (u8, u8, u8) {
        panic!("execute_brk not implemented for this operation");
    }

    /// Check if this operation should inhibit IRQ for one instruction.
    ///
    /// On the 6502, CLI, SEI, and PLP instructions poll for interrupts at the end
    /// of their first cycle, before the I flag is modified in the second cycle.
    /// This means a pending IRQ will not be serviced until after the next instruction
    /// completes, creating a one-instruction delay.
    ///
    /// RTI does NOT have this behavior - it affects the I flag immediately.
    ///
    /// # Returns
    /// - `true` for CLI, SEI, PLP
    /// - `false` for all other operations (default)
    fn inhibits_irq(&self) -> bool {
        false // Default: most operations don't inhibit IRQ
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
