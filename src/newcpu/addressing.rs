//! Addressing mode implementations for cycle-accurate CPU
//!
//! This module implements all 11 6502 addressing modes as concrete types
//! that implement the AddressingMode trait.

use super::traits::AddressingMode;
use super::types::AddressingState;

/// Implied/Accumulator addressing mode
///
/// Used by instructions that operate on the accumulator or have no operand.
/// Examples: NOP, CLC, INX, TAX, ASL A
///
/// Cycles: 0 (no address resolution needed)
#[derive(Debug, Clone, Copy)]
pub struct Implied;

impl AddressingMode for Implied {
    fn address_cycles(&self) -> u8 {
        0
    }

    fn tick_addressing(
        &self,
        _cycle: u8,
        _pc: &mut u16,
        _x: u8,
        _y: u8,
        _state: &mut AddressingState,
        _read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        // Implied mode has no address - return immediately
        Some(0)
    }

    fn has_page_cross_penalty(&self) -> bool {
        false
    }
}

/// Immediate addressing mode
///
/// The operand is the byte immediately following the opcode.
/// Examples: LDA #$42, ADC #$10
///
/// Cycles: 1 (fetch operand byte)
#[derive(Debug, Clone, Copy)]
pub struct Immediate;

impl AddressingMode for Immediate {
    fn address_cycles(&self) -> u8 {
        1
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        _x: u8,
        _y: u8,
        state: &mut AddressingState,
        _read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch the operand address (which is PC itself)
                let addr = *pc;
                *pc = pc.wrapping_add(1);
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("Immediate addressing mode only takes 1 cycle"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        false
    }
}

/// Zero Page addressing mode
///
/// The operand address is in the zero page ($00-$FF).
/// Examples: LDA $42, STA $10
///
/// Cycles: 1 (fetch zero page address)
#[derive(Debug, Clone, Copy)]
pub struct ZeroPage;

impl AddressingMode for ZeroPage {
    fn address_cycles(&self) -> u8 {
        1
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        _x: u8,
        _y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch zero page address
                let addr = read_fn(*pc) as u16;
                *pc = pc.wrapping_add(1);
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("ZeroPage addressing mode only takes 1 cycle"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        false
    }
}

/// Zero Page,X addressing mode
///
/// The operand address is in the zero page, indexed by X register.
/// Examples: LDA $42,X, STA $10,X
///
/// Cycles: 2 (fetch base address, add X with wrap)
#[derive(Debug, Clone, Copy)]
pub struct ZeroPageX;

impl AddressingMode for ZeroPageX {
    fn address_cycles(&self) -> u8 {
        2
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        x: u8,
        _y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch base address
                let base = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                state.temp_bytes[0] = base;
                None
            }
            1 => {
                // Add X register (wraps in zero page)
                let addr = state.temp_bytes[0].wrapping_add(x) as u16;
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("ZeroPageX addressing mode only takes 2 cycles"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        false
    }
}

/// Zero Page,Y addressing mode
///
/// The operand address is in the zero page, indexed by Y register.
/// Examples: LDX $42,Y, STX $10,Y
///
/// Cycles: 2 (fetch base address, add Y with wrap)
#[derive(Debug, Clone, Copy)]
pub struct ZeroPageY;

impl AddressingMode for ZeroPageY {
    fn address_cycles(&self) -> u8 {
        2
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        _x: u8,
        y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch base address
                let base = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                state.temp_bytes[0] = base;
                None
            }
            1 => {
                // Add Y register (wraps in zero page)
                let addr = state.temp_bytes[0].wrapping_add(y) as u16;
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("ZeroPageY addressing mode only takes 2 cycles"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        false
    }
}

/// Absolute addressing mode
///
/// The operand address is a 16-bit absolute address.
/// Examples: LDA $1234, JMP $8000
///
/// Cycles: 2 (fetch low byte, fetch high byte)
#[derive(Debug, Clone, Copy)]
pub struct Absolute;

impl AddressingMode for Absolute {
    fn address_cycles(&self) -> u8 {
        2
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        _x: u8,
        _y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch low byte
                let low = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                state.temp_bytes[0] = low;
                None
            }
            1 => {
                // Fetch high byte
                let high = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                let addr = (high as u16) << 8 | state.temp_bytes[0] as u16;
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("Absolute addressing mode only takes 2 cycles"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        false
    }
}

/// Absolute,X addressing mode
///
/// The operand address is a 16-bit absolute address indexed by X.
/// Examples: LDA $1234,X, STA $2000,X
///
/// Cycles: 2-3 (fetch low, fetch high, [+1 if page crossed for reads])
#[derive(Debug, Clone, Copy)]
pub struct AbsoluteX;

impl AddressingMode for AbsoluteX {
    fn address_cycles(&self) -> u8 {
        2 // Base cycles; page cross adds 1 for reads
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        x: u8,
        _y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch low byte
                let low = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                state.temp_bytes[0] = low;
                None
            }
            1 => {
                // Fetch high byte and add X
                let high = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                let base = (high as u16) << 8 | state.temp_bytes[0] as u16;
                let addr = base.wrapping_add(x as u16);
                state.base_addr = Some(base);
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("AbsoluteX addressing mode only takes 2 cycles"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        true
    }
}

/// Absolute,Y addressing mode
///
/// The operand address is a 16-bit absolute address indexed by Y.
/// Examples: LDA $1234,Y, STA $2000,Y
///
/// Cycles: 2-3 (fetch low, fetch high, [+1 if page crossed for reads])
#[derive(Debug, Clone, Copy)]
pub struct AbsoluteY;

impl AddressingMode for AbsoluteY {
    fn address_cycles(&self) -> u8 {
        2 // Base cycles; page cross adds 1 for reads
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        _x: u8,
        y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch low byte
                let low = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                state.temp_bytes[0] = low;
                None
            }
            1 => {
                // Fetch high byte and add Y
                let high = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                let base = (high as u16) << 8 | state.temp_bytes[0] as u16;
                let addr = base.wrapping_add(y as u16);
                state.base_addr = Some(base);
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("AbsoluteY addressing mode only takes 2 cycles"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        true
    }
}

/// Indirect addressing mode (used only by JMP)
///
/// The operand is a pointer to the actual address.
/// Example: JMP ($1234)
///
/// Cycles: 4 (fetch ptr low, fetch ptr high, fetch addr low, fetch addr high)
#[derive(Debug, Clone, Copy)]
pub struct Indirect;

impl AddressingMode for Indirect {
    fn address_cycles(&self) -> u8 {
        4
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        _x: u8,
        _y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch pointer low byte
                let ptr_low = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                state.temp_bytes[0] = ptr_low;
                None
            }
            1 => {
                // Fetch pointer high byte
                let ptr_high = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                let ptr = (ptr_high as u16) << 8 | state.temp_bytes[0] as u16;
                state.temp_bytes[1] = ptr_high;
                state.base_addr = Some(ptr);
                None
            }
            2 => {
                // Fetch target address low byte
                let ptr = state.base_addr.unwrap();
                let addr_low = read_fn(ptr);
                state.temp_bytes[2] = addr_low;
                None
            }
            3 => {
                // Fetch target address high byte
                // Note: 6502 has a bug - if pointer is at page boundary (e.g., $12FF),
                // it fetches high byte from $1200 instead of $1300
                let ptr = state.base_addr.unwrap();
                let ptr_high = (ptr & 0xFF00) | ((ptr + 1) & 0x00FF);
                let addr_high = read_fn(ptr_high);
                let addr = (addr_high as u16) << 8 | state.temp_bytes[2] as u16;
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("Indirect addressing mode only takes 4 cycles"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        false
    }
}

/// Indexed Indirect addressing mode (Indirect,X)
///
/// The zero page pointer is indexed by X, then dereferenced.
/// Example: LDA ($20,X)
///
/// Cycles: 4 (fetch ptr, add X, fetch addr low, fetch addr high)
#[derive(Debug, Clone, Copy)]
pub struct IndexedIndirect;

impl AddressingMode for IndexedIndirect {
    fn address_cycles(&self) -> u8 {
        4
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        x: u8,
        _y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch base pointer
                let base = read_fn(*pc);
                *pc = pc.wrapping_add(1);
                state.temp_bytes[0] = base;
                None
            }
            1 => {
                // Add X (wraps in zero page)
                let ptr = state.temp_bytes[0].wrapping_add(x);
                state.temp_bytes[1] = ptr;
                None
            }
            2 => {
                // Fetch address low byte from zero page
                let ptr = state.temp_bytes[1] as u16;
                let addr_low = read_fn(ptr);
                state.temp_bytes[2] = addr_low;
                None
            }
            3 => {
                // Fetch address high byte from zero page (wraps)
                let ptr = state.temp_bytes[1].wrapping_add(1) as u16;
                let addr_high = read_fn(ptr);
                let addr = (addr_high as u16) << 8 | state.temp_bytes[2] as u16;
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("IndexedIndirect addressing mode only takes 4 cycles"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        false
    }
}

/// Indirect Indexed addressing mode (Indirect),Y
///
/// The zero page pointer is dereferenced, then indexed by Y.
/// Example: LDA ($20),Y
///
/// Cycles: 3-4 (fetch ptr, fetch addr low, fetch addr high, [+1 if page crossed for reads])
#[derive(Debug, Clone, Copy)]
pub struct IndirectIndexed;

impl AddressingMode for IndirectIndexed {
    fn address_cycles(&self) -> u8 {
        3 // Base cycles; page cross adds 1 for reads
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        _x: u8,
        y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch pointer address
                let ptr = read_fn(*pc) as u16;
                *pc = pc.wrapping_add(1);
                state.temp_bytes[0] = ptr as u8;
                None
            }
            1 => {
                // Fetch target address low byte
                let ptr = state.temp_bytes[0] as u16;
                let addr_low = read_fn(ptr);
                state.temp_bytes[1] = addr_low;
                None
            }
            2 => {
                // Fetch target address high byte and add Y
                let ptr = state.temp_bytes[0].wrapping_add(1) as u16;
                let addr_high = read_fn(ptr);
                let base = (addr_high as u16) << 8 | state.temp_bytes[1] as u16;
                let addr = base.wrapping_add(y as u16);
                state.base_addr = Some(base);
                state.addr = Some(addr);
                Some(addr)
            }
            _ => panic!("IndirectIndexed addressing mode only takes 3 cycles"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        true
    }
}

/// Relative addressing mode (used only by branch instructions)
///
/// The operand is a signed 8-bit offset from PC.
/// Examples: BNE +5, BEQ -10
///
/// Cycles: 1 (fetch offset)
/// Note: Branch instructions add cycles for branch taken and page crossing
#[derive(Debug, Clone, Copy)]
pub struct Relative;

impl AddressingMode for Relative {
    fn address_cycles(&self) -> u8 {
        1
    }

    fn tick_addressing(
        &self,
        cycle: u8,
        pc: &mut u16,
        _x: u8,
        _y: u8,
        state: &mut AddressingState,
        read_fn: &dyn Fn(u16) -> u8,
    ) -> Option<u16> {
        match cycle {
            0 => {
                // Fetch signed offset
                let offset = read_fn(*pc) as i8;
                *pc = pc.wrapping_add(1);

                // Calculate target address
                let target = if offset >= 0 {
                    pc.wrapping_add(offset as u16)
                } else {
                    pc.wrapping_sub((-offset) as u16)
                };

                state.base_addr = Some(*pc); // Store PC for page cross detection
                state.addr = Some(target);
                Some(target)
            }
            _ => panic!("Relative addressing mode only takes 1 cycle"),
        }
    }

    fn has_page_cross_penalty(&self) -> bool {
        false // Branch instructions handle page crossing differently
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_implied_address_cycles() {
        let mode = Implied;
        assert_eq!(mode.address_cycles(), 0);
    }

    #[test]
    fn test_implied_tick_addressing_returns_immediately() {
        let mode = Implied;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |_addr: u16| 0x00;

        // Should return address immediately on first cycle
        let result = mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);
        assert_eq!(result, Some(0));
    }

    #[test]
    fn test_implied_no_page_cross_penalty() {
        let mode = Implied;
        assert_eq!(mode.has_page_cross_penalty(), false);
    }

    #[test]
    fn test_implied_does_not_modify_pc() {
        let mode = Implied;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |_addr: u16| 0x00;

        mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);

        // PC should not be modified for implied mode
        assert_eq!(pc, 0x8000);
    }

    #[test]
    fn test_immediate_address_cycles() {
        let mode = Immediate;
        assert_eq!(mode.address_cycles(), 1);
    }

    #[test]
    fn test_immediate_tick_addressing() {
        let mode = Immediate;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |_addr: u16| 0x42;

        // Cycle 0: fetch operand address (PC)
        let result = mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);
        assert_eq!(result, Some(0x8000));
        assert_eq!(pc, 0x8001); // PC incremented
        assert_eq!(state.addr, Some(0x8000));
    }

    #[test]
    fn test_immediate_no_page_cross_penalty() {
        let mode = Immediate;
        assert_eq!(mode.has_page_cross_penalty(), false);
    }

    #[test]
    fn test_immediate_increments_pc() {
        let mode = Immediate;
        let mut state = AddressingState::default();
        let mut pc = 0x1234;
        let read_fn = |_addr: u16| 0x00;

        mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);

        // PC should be incremented by 1
        assert_eq!(pc, 0x1235);
    }

    #[test]
    fn test_zero_page_address_cycles() {
        let mode = ZeroPage;
        assert_eq!(mode.address_cycles(), 1);
    }

    #[test]
    fn test_zero_page_tick_addressing() {
        let mode = ZeroPage;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| {
            if addr == 0x8000 { 0x42 } else { 0x00 }
        };

        // Cycle 0: fetch zero page address
        let result = mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);
        assert_eq!(result, Some(0x42));
        assert_eq!(pc, 0x8001);
        assert_eq!(state.addr, Some(0x42));
    }

    #[test]
    fn test_zero_page_no_page_cross_penalty() {
        let mode = ZeroPage;
        assert_eq!(mode.has_page_cross_penalty(), false);
    }

    #[test]
    fn test_zero_page_x() {
        let mode = ZeroPageX;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| if addr == 0x8000 { 0x80 } else { 0x00 };

        assert_eq!(mode.address_cycles(), 2);

        // Cycle 0: fetch base
        let result = mode.tick_addressing(0, &mut pc, 0x05, 0, &mut state, &read_fn);
        assert_eq!(result, None);
        assert_eq!(pc, 0x8001);

        // Cycle 1: add X
        let result = mode.tick_addressing(1, &mut pc, 0x05, 0, &mut state, &read_fn);
        assert_eq!(result, Some(0x85));
    }

    #[test]
    fn test_zero_page_x_wraps() {
        let mode = ZeroPageX;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| if addr == 0x8000 { 0xFF } else { 0x00 };

        // Cycle 0 and 1
        mode.tick_addressing(0, &mut pc, 0x10, 0, &mut state, &read_fn);
        let result = mode.tick_addressing(1, &mut pc, 0x10, 0, &mut state, &read_fn);

        // Should wrap: 0xFF + 0x10 = 0x0F
        assert_eq!(result, Some(0x0F));
    }

    #[test]
    fn test_zero_page_y() {
        let mode = ZeroPageY;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| if addr == 0x8000 { 0x20 } else { 0x00 };

        assert_eq!(mode.address_cycles(), 2);

        mode.tick_addressing(0, &mut pc, 0, 0x03, &mut state, &read_fn);
        let result = mode.tick_addressing(1, &mut pc, 0, 0x03, &mut state, &read_fn);
        assert_eq!(result, Some(0x23));
    }

    #[test]
    fn test_absolute() {
        let mode = Absolute;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0x34, // Low byte
            0x8001 => 0x12, // High byte
            _ => 0x00,
        };

        assert_eq!(mode.address_cycles(), 2);

        // Cycle 0: fetch low
        let result = mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);
        assert_eq!(result, None);
        assert_eq!(pc, 0x8001);

        // Cycle 1: fetch high
        let result = mode.tick_addressing(1, &mut pc, 0, 0, &mut state, &read_fn);
        assert_eq!(result, Some(0x1234));
        assert_eq!(pc, 0x8002);
    }

    #[test]
    fn test_absolute_x() {
        let mode = AbsoluteX;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0x00, // Low byte
            0x8001 => 0x20, // High byte
            _ => 0x00,
        };

        assert_eq!(mode.address_cycles(), 2);
        assert_eq!(mode.has_page_cross_penalty(), true);

        mode.tick_addressing(0, &mut pc, 0x05, 0, &mut state, &read_fn);
        let result = mode.tick_addressing(1, &mut pc, 0x05, 0, &mut state, &read_fn);

        // 0x2000 + 0x05 = 0x2005
        assert_eq!(result, Some(0x2005));
        assert_eq!(state.base_addr, Some(0x2000));
    }

    #[test]
    fn test_absolute_x_page_cross() {
        let mode = AbsoluteX;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0xFF, // Low byte
            0x8001 => 0x20, // High byte
            _ => 0x00,
        };

        mode.tick_addressing(0, &mut pc, 0x10, 0, &mut state, &read_fn);
        let result = mode.tick_addressing(1, &mut pc, 0x10, 0, &mut state, &read_fn);

        // 0x20FF + 0x10 = 0x210F (page cross)
        assert_eq!(result, Some(0x210F));
    }

    #[test]
    fn test_absolute_y() {
        let mode = AbsoluteY;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0x00,
            0x8001 => 0x30,
            _ => 0x00,
        };

        assert_eq!(mode.has_page_cross_penalty(), true);

        mode.tick_addressing(0, &mut pc, 0, 0x07, &mut state, &read_fn);
        let result = mode.tick_addressing(1, &mut pc, 0, 0x07, &mut state, &read_fn);

        assert_eq!(result, Some(0x3007));
    }

    #[test]
    fn test_indirect() {
        let mode = Indirect;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0x20, // Pointer low
            0x8001 => 0x10, // Pointer high
            0x1020 => 0x34, // Target low
            0x1021 => 0x56, // Target high
            _ => 0x00,
        };

        assert_eq!(mode.address_cycles(), 4);

        mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);
        mode.tick_addressing(1, &mut pc, 0, 0, &mut state, &read_fn);
        mode.tick_addressing(2, &mut pc, 0, 0, &mut state, &read_fn);
        let result = mode.tick_addressing(3, &mut pc, 0, 0, &mut state, &read_fn);

        assert_eq!(result, Some(0x5634));
    }

    #[test]
    fn test_indirect_page_boundary_bug() {
        let mode = Indirect;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0xFF, // Pointer low
            0x8001 => 0x10, // Pointer high
            0x10FF => 0x34, // Target low at page boundary
            0x1000 => 0x56, // Target high wraps to start of page (6502 bug)
            _ => 0x00,
        };

        mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);
        mode.tick_addressing(1, &mut pc, 0, 0, &mut state, &read_fn);
        mode.tick_addressing(2, &mut pc, 0, 0, &mut state, &read_fn);
        let result = mode.tick_addressing(3, &mut pc, 0, 0, &mut state, &read_fn);

        // Should be 0x5634, not 0x??34, due to page wrap bug
        assert_eq!(result, Some(0x5634));
    }

    #[test]
    fn test_indexed_indirect() {
        let mode = IndexedIndirect;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0x20, // Base pointer
            0x25 => 0x34,   // Address low (0x20 + 0x05)
            0x26 => 0x12,   // Address high
            _ => 0x00,
        };

        assert_eq!(mode.address_cycles(), 4);

        mode.tick_addressing(0, &mut pc, 0x05, 0, &mut state, &read_fn);
        mode.tick_addressing(1, &mut pc, 0x05, 0, &mut state, &read_fn);
        mode.tick_addressing(2, &mut pc, 0x05, 0, &mut state, &read_fn);
        let result = mode.tick_addressing(3, &mut pc, 0x05, 0, &mut state, &read_fn);

        assert_eq!(result, Some(0x1234));
    }

    #[test]
    fn test_indexed_indirect_wraps() {
        let mode = IndexedIndirect;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0xFF, // Base pointer
            0x04 => 0x34,   // Address low (0xFF + 0x05 wraps to 0x04)
            0x05 => 0x12,   // Address high
            _ => 0x00,
        };

        mode.tick_addressing(0, &mut pc, 0x05, 0, &mut state, &read_fn);
        mode.tick_addressing(1, &mut pc, 0x05, 0, &mut state, &read_fn);
        mode.tick_addressing(2, &mut pc, 0x05, 0, &mut state, &read_fn);
        let result = mode.tick_addressing(3, &mut pc, 0x05, 0, &mut state, &read_fn);

        assert_eq!(result, Some(0x1234));
    }

    #[test]
    fn test_indirect_indexed() {
        let mode = IndirectIndexed;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0x20, // Pointer
            0x20 => 0x00,   // Base address low
            0x21 => 0x30,   // Base address high
            _ => 0x00,
        };

        assert_eq!(mode.address_cycles(), 3);
        assert_eq!(mode.has_page_cross_penalty(), true);

        mode.tick_addressing(0, &mut pc, 0, 0x05, &mut state, &read_fn);
        mode.tick_addressing(1, &mut pc, 0, 0x05, &mut state, &read_fn);
        let result = mode.tick_addressing(2, &mut pc, 0, 0x05, &mut state, &read_fn);

        // 0x3000 + 0x05 = 0x3005
        assert_eq!(result, Some(0x3005));
        assert_eq!(state.base_addr, Some(0x3000));
    }

    #[test]
    fn test_indirect_indexed_page_cross() {
        let mode = IndirectIndexed;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| match addr {
            0x8000 => 0x20,
            0x20 => 0xFF, // Base low
            0x21 => 0x20, // Base high
            _ => 0x00,
        };

        mode.tick_addressing(0, &mut pc, 0, 0x10, &mut state, &read_fn);
        mode.tick_addressing(1, &mut pc, 0, 0x10, &mut state, &read_fn);
        let result = mode.tick_addressing(2, &mut pc, 0, 0x10, &mut state, &read_fn);

        // 0x20FF + 0x10 = 0x210F (crosses page)
        assert_eq!(result, Some(0x210F));
    }

    #[test]
    fn test_relative_positive_offset() {
        let mode = Relative;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| if addr == 0x8000 { 0x10 } else { 0x00 };

        assert_eq!(mode.address_cycles(), 1);

        let result = mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);

        // PC after fetch = 0x8001, +0x10 = 0x8011
        assert_eq!(result, Some(0x8011));
        assert_eq!(pc, 0x8001);
    }

    #[test]
    fn test_relative_negative_offset() {
        let mode = Relative;
        let mut state = AddressingState::default();
        let mut pc = 0x8000;
        let read_fn = |addr: u16| if addr == 0x8000 { 0xF0 } else { 0x00 }; // -16

        let result = mode.tick_addressing(0, &mut pc, 0, 0, &mut state, &read_fn);

        // PC after fetch = 0x8001, -16 = 0x7FF1
        assert_eq!(result, Some(0x7FF1));
    }
}
