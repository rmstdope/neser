//! Addressing mode implementations for cycle-accurate CPU
//!
//! This module implements all 11 6502 addressing modes as concrete types
//! that implement the AddressingMode trait.

use super::traits::AddressingMode;
use super::types::CpuState;
use crate::mem_controller::MemController;
use std::cell::RefCell;
use std::rc::Rc;
// use super::types::AddressingState;

/// Implied/Accumulator addressing mode
///
/// Used by instructions that operate on the accumulator or have no operand.
/// Examples: NOP, CLC, INX, TAX, ASL A
///
/// Cycles: 0 (no address resolution needed)
#[derive(Debug, Clone, Copy)]
pub struct Implied;

impl AddressingMode for Implied {
    fn is_done(&self) -> bool {
        true
    }

    fn tick(&mut self, _cpu_state: &mut CpuState, _memory: Rc<RefCell<MemController>>) {
        // No action needed for implied mode
    }
}

/// Immediate addressing mode
///
/// The operand is the byte immediately following the opcode.
/// Examples: LDA #$42, ADC #$10
///
/// Cycles: 1 (fetch operand from PC, advance PC)
#[derive(Debug, Clone, Copy, Default)]
pub struct Immediate {
    has_read: bool,
    address: u16,
}

impl Immediate {
    /// Create a new Immediate addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for Immediate {
    fn is_done(&self) -> bool {
        self.has_read
    }

    fn tick(&mut self, cpu_state: &mut CpuState, _memory: Rc<RefCell<MemController>>) {
        debug_assert!(!self.has_read, "Immediate::tick called after already done");

        // The operand address is PC itself (pointing to the immediate value byte)
        self.address = cpu_state.pc;
        cpu_state.pc = cpu_state.pc.wrapping_add(1);
        self.has_read = true;
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.has_read,
            "Immediate::get_address called before addressing complete"
        );
        self.address
    }
}

/// Zero Page addressing mode
///
/// The operand address is in the zero page ($00-$FF).
/// The byte at PC is used as the low byte of the address, with high byte being $00.
/// Examples: LDA $42, STA $10
///
/// Cycles: 1 (fetch zero page address from PC)
#[derive(Debug, Clone, Copy, Default)]
pub struct ZeroPage {
    has_read: bool,
    address: u16,
}

impl ZeroPage {
    /// Create a new ZeroPage addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for ZeroPage {
    fn is_done(&self) -> bool {
        self.has_read
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(!self.has_read, "ZeroPage::tick called after already done");

        // Read zero page address (low byte only, high byte is always 0x00)
        let zp_addr = memory.borrow().read(cpu_state.pc);
        self.address = zp_addr as u16;
        cpu_state.pc = cpu_state.pc.wrapping_add(1);
        self.has_read = true;
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.has_read,
            "ZeroPage::get_address called before addressing complete"
        );
        self.address
    }
}

/// Absolute addressing mode
///
/// The operand address is formed from two bytes following the opcode.
/// First byte is low byte, second byte is high byte (little-endian).
/// Examples: LDA $1234, JMP $C000, STA $2000
///
/// Cycles: 2 (fetch low byte, then high byte)
#[derive(Debug, Clone, Copy, Default)]
pub struct Absolute {
    cycle: u8,
    address: u16,
}

impl Absolute {
    /// Create a new Absolute addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for Absolute {
    fn is_done(&self) -> bool {
        self.cycle == 2
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(self.cycle < 2, "Absolute::tick called after already done");

        match self.cycle {
            0 => {
                // Fetch low byte of address
                let low = memory.borrow().read(cpu_state.pc);
                self.address = low as u16;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 1;
            }
            1 => {
                // Fetch high byte of address
                let high = memory.borrow().read(cpu_state.pc);
                self.address |= (high as u16) << 8;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 2;
            }
            _ => unreachable!(),
        }
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.cycle == 2,
            "Absolute::get_address called before addressing complete"
        );
        self.address
    }
}

/// Zero Page X addressing mode
///
/// The operand address is in zero page, indexed by X register.
/// Base address is read from PC, then X is added (wrapping within zero page).
/// Examples: LDA $42,X, STX $10,X
///
/// Cycles: 2 (fetch base address, dummy read while adding index)
#[derive(Debug, Clone, Copy, Default)]
pub struct ZeroPageX {
    cycle: u8,
    address: u16,
}

impl ZeroPageX {
    /// Create a new ZeroPageX addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for ZeroPageX {
    fn is_done(&self) -> bool {
        self.cycle == 2
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(self.cycle < 2, "ZeroPageX::tick called after already done");

        match self.cycle {
            0 => {
                // Fetch base zero page address
                let base = memory.borrow().read(cpu_state.pc);
                self.address = base as u16;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 1;
            }
            1 => {
                // Dummy read from base address while adding X index
                // The address wraps within zero page (stays in $00-$FF)
                self.address = (self.address.wrapping_add(cpu_state.x as u16)) & 0xFF;
                self.cycle = 2;
            }
            _ => unreachable!(),
        }
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.cycle == 2,
            "ZeroPageX::get_address called before addressing complete"
        );
        self.address
    }
}

/// Zero Page Y addressing mode
///
/// The operand address is in zero page, indexed by Y register.
/// Base address is read from PC, then Y is added (wrapping within zero page).
/// Examples: LDX $42,Y, STX $10,Y
///
/// Cycles: 2 (fetch base address, dummy read while adding index)
#[derive(Debug, Clone, Copy, Default)]
pub struct ZeroPageY {
    cycle: u8,
    address: u16,
}

impl ZeroPageY {
    /// Create a new ZeroPageY addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for ZeroPageY {
    fn is_done(&self) -> bool {
        self.cycle == 2
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(self.cycle < 2, "ZeroPageY::tick called after already done");

        match self.cycle {
            0 => {
                // Fetch base zero page address
                let base = memory.borrow().read(cpu_state.pc);
                self.address = base as u16;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 1;
            }
            1 => {
                // Dummy read from base address while adding Y index
                // The address wraps within zero page (stays in $00-$FF)
                self.address = (self.address.wrapping_add(cpu_state.y as u16)) & 0xFF;
                self.cycle = 2;
            }
            _ => unreachable!(),
        }
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.cycle == 2,
            "ZeroPageY::get_address called before addressing complete"
        );
        self.address
    }
}

/// Absolute X addressing mode
///
/// The operand address is formed from two bytes, then X register is added.
/// If adding X causes a page boundary crossing, an extra cycle is needed.
/// Examples: LDA $1234,X, STA $2000,X
///
/// Cycles: 2 (no page cross) or 3 (page cross)
#[derive(Debug, Clone, Copy, Default)]
pub struct AbsoluteX {
    cycle: u8,
    address: u16,
    page_crossed: bool,
}

impl AbsoluteX {
    /// Create a new AbsoluteX addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for AbsoluteX {
    fn is_done(&self) -> bool {
        self.cycle == 2 && !self.page_crossed || self.cycle == 3
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(self.cycle < 3, "AbsoluteX::tick called after already done");

        match self.cycle {
            0 => {
                // Fetch low byte of base address
                let low = memory.borrow().read(cpu_state.pc);
                self.address = low as u16;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 1;
            }
            1 => {
                // Fetch high byte of base address
                let high = memory.borrow().read(cpu_state.pc);
                let base_addr = self.address | ((high as u16) << 8);
                cpu_state.pc = cpu_state.pc.wrapping_add(1);

                // Add X register and check for page crossing
                let indexed_addr = base_addr.wrapping_add(cpu_state.x as u16);
                self.page_crossed = (base_addr & 0xFF00) != (indexed_addr & 0xFF00);
                self.address = indexed_addr;
                self.cycle = 2;
            }
            2 => {
                // Extra cycle for page crossing (dummy read)
                debug_assert!(
                    self.page_crossed,
                    "Cycle 2 should only happen on page cross"
                );
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.is_done(),
            "AbsoluteX::get_address called before addressing complete"
        );
        self.address
    }

    fn has_page_cross_penalty(&self) -> bool {
        true
    }
}

/// Absolute Y addressing mode
///
/// The operand address is formed from two bytes, then Y register is added.
/// If adding Y causes a page boundary crossing, an extra cycle is needed.
/// Examples: LDA $1234,Y, STA $2000,Y
///
/// Cycles: 2 (no page cross) or 3 (page cross)
#[derive(Debug, Clone, Copy, Default)]
pub struct AbsoluteY {
    cycle: u8,
    address: u16,
    page_crossed: bool,
}

impl AbsoluteY {
    /// Create a new AbsoluteY addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for AbsoluteY {
    fn is_done(&self) -> bool {
        self.cycle == 2 && !self.page_crossed || self.cycle == 3
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(self.cycle < 3, "AbsoluteY::tick called after already done");

        match self.cycle {
            0 => {
                // Fetch low byte of base address
                let low = memory.borrow().read(cpu_state.pc);
                self.address = low as u16;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 1;
            }
            1 => {
                // Fetch high byte of base address
                let high = memory.borrow().read(cpu_state.pc);
                let base_addr = self.address | ((high as u16) << 8);
                cpu_state.pc = cpu_state.pc.wrapping_add(1);

                // Add Y register and check for page crossing
                let indexed_addr = base_addr.wrapping_add(cpu_state.y as u16);
                self.page_crossed = (base_addr & 0xFF00) != (indexed_addr & 0xFF00);
                self.address = indexed_addr;
                self.cycle = 2;
            }
            2 => {
                // Extra cycle for page crossing (dummy read)
                debug_assert!(
                    self.page_crossed,
                    "Cycle 2 should only happen on page cross"
                );
                self.cycle = 3;
            }
            _ => unreachable!(),
        }
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.is_done(),
            "AbsoluteY::get_address called before addressing complete"
        );
        self.address
    }

    fn has_page_cross_penalty(&self) -> bool {
        true
    }
}

/// Indirect addressing mode
///
/// Used exclusively by JMP instruction. The operand is a pointer address.
/// First read 2 bytes for pointer address, then read 2 bytes from that pointer.
/// Has a famous 6502 bug: if pointer is at page boundary (e.g., $02FF),
/// the high byte is fetched from $0200 instead of $0300.
/// Examples: JMP ($1234)
///
/// Cycles: 4 (fetch pointer low, pointer high, target low, target high)
#[derive(Debug, Clone, Copy, Default)]
pub struct Indirect {
    cycle: u8,
    pointer: u16,
    address: u16,
}

impl Indirect {
    /// Create a new Indirect addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for Indirect {
    fn is_done(&self) -> bool {
        self.cycle == 4
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(self.cycle < 4, "Indirect::tick called after already done");

        match self.cycle {
            0 => {
                // Fetch low byte of pointer address
                let low = memory.borrow().read(cpu_state.pc);
                self.pointer = low as u16;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 1;
            }
            1 => {
                // Fetch high byte of pointer address
                let high = memory.borrow().read(cpu_state.pc);
                self.pointer |= (high as u16) << 8;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 2;
            }
            2 => {
                // Fetch low byte of target address from pointer
                let low = memory.borrow().read(self.pointer);
                self.address = low as u16;
                self.cycle = 3;
            }
            3 => {
                // Fetch high byte of target address
                // 6502 bug: if pointer low byte is 0xFF, high byte wraps within same page
                let high_addr = if self.pointer & 0xFF == 0xFF {
                    self.pointer & 0xFF00 // Wrap to start of same page
                } else {
                    self.pointer.wrapping_add(1)
                };
                let high = memory.borrow().read(high_addr);
                self.address |= (high as u16) << 8;
                self.cycle = 4;
            }
            _ => unreachable!(),
        }
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.cycle == 4,
            "Indirect::get_address called before addressing complete"
        );
        self.address
    }
}

/// Indexed Indirect addressing mode (Indirect,X)
///
/// The base address is in zero page, X register is added to it (wrapping in zero page).
/// Then a 16-bit pointer is read from that zero-page location.
/// Examples: LDA ($20,X)
///
/// Cycles: 5 (addressing completes, then instruction executes on cycle 6)
///   1. Fetch zero-page base address from PC
///   2. Dummy read at zero page address, add X register to base
///   3. Read pointer low byte from (base+X) in zero page
///   4. Read pointer high byte from (base+X+1) in zero page
///   5. Addressing complete (instruction reads and operates on cycle 6)
#[derive(Debug, Clone, Copy, Default)]
pub struct IndexedIndirect {
    cycle: u8,
    pointer_addr: u8,
    address: u16,
}

impl IndexedIndirect {
    /// Create a new IndexedIndirect addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for IndexedIndirect {
    fn is_done(&self) -> bool {
        self.cycle == 4
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(
            self.cycle < 4,
            "IndexedIndirect::tick called after already done"
        );

        match self.cycle {
            0 => {
                // Cycle 1: Fetch zero-page base address from PC
                let base = memory.borrow().read(cpu_state.pc);
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                // Add X register to base (wraps within zero page)
                self.pointer_addr = base.wrapping_add(cpu_state.x);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Dummy read at zero page base address (CPU performs internal X addition)
                let _ = memory.borrow().read(self.pointer_addr as u16);
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Read low byte of pointer from zero page
                let low = memory.borrow().read(self.pointer_addr as u16);
                self.address = low as u16;
                self.cycle = 3;
            }
            3 => {
                // Cycle 4: Read high byte of pointer from zero page (wraps within zero page)
                let high_addr = self.pointer_addr.wrapping_add(1);
                let high = memory.borrow().read(high_addr as u16);
                self.address |= (high as u16) << 8;
                self.cycle = 4;
            }
            _ => unreachable!(),
        }
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.cycle == 4,
            "IndexedIndirect::get_address called before addressing complete"
        );
        self.address
    }
}

/// Indirect Indexed addressing mode ((Indirect),Y)
///
/// Read a zero-page address, fetch a 16-bit pointer from there,
/// then add Y register to form the final address.
/// Has a page-cross penalty: adds 1 cycle if Y addition crosses page boundary.
/// Examples: LDA ($20),Y
///
/// Cycles: 5-6
///   1. Fetch zero-page pointer address from PC
///   2. Read pointer low byte from zero page
///   3. Read pointer high byte from zero page (wraps)
///   4. Add Y to pointer (may cross page)
///   5. Dummy read if page crossed (penalty cycle)
///   6. Ready (only if page crossed)
#[derive(Debug, Clone, Copy, Default)]
pub struct IndirectIndexed {
    cycle: u8,
    pointer_addr: u8,
    base_address: u16,
    address: u16,
    page_crossed: bool,
}

impl IndirectIndexed {
    /// Create a new IndirectIndexed addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for IndirectIndexed {
    fn is_done(&self) -> bool {
        if self.page_crossed {
            self.cycle == 6
        } else {
            self.cycle == 5
        }
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(
            !self.is_done(),
            "IndirectIndexed::tick called after already done"
        );

        match self.cycle {
            0 => {
                // Cycle 1: Fetch zero-page pointer address from PC
                self.pointer_addr = memory.borrow().read(cpu_state.pc);
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Read pointer low byte from zero page
                let low = memory.borrow().read(self.pointer_addr as u16);
                self.base_address = low as u16;
                self.cycle = 2;
            }
            2 => {
                // Cycle 3: Read pointer high byte from zero page (wraps within zero page)
                let high_addr = self.pointer_addr.wrapping_add(1);
                let high = memory.borrow().read(high_addr as u16);
                self.base_address |= (high as u16) << 8;
                self.cycle = 3;
            }
            3 => {
                // Cycle 4: Add Y register to base address and check for page cross
                self.address = self.base_address.wrapping_add(cpu_state.y as u16);
                self.page_crossed = (self.base_address & 0xFF00) != (self.address & 0xFF00);
                self.cycle = 4;
            }
            4 => {
                // Cycle 5: Complete (or continue to cycle 6 if page crossed)
                // Note: is_done() checks if we need another cycle
                self.cycle = 5;
            }
            5 => {
                // Cycle 6: Penalty cycle for page crossing
                self.cycle = 6;
            }
            _ => unreachable!(),
        }
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.is_done(),
            "IndirectIndexed::get_address called before addressing complete"
        );
        self.address
    }

    fn has_page_cross_penalty(&self) -> bool {
        self.page_crossed
    }
}

/// Relative addressing mode
///
/// Used exclusively by branch instructions. Reads a signed 8-bit offset
/// from PC and computes the target address relative to the current PC.
/// Examples: BEQ label, BNE label
///
/// Cycles: 2
///   1. Fetch signed offset from PC
///   2. Compute target address (PC + offset)
#[derive(Debug, Clone, Copy, Default)]
pub struct Relative {
    cycle: u8,
    address: u16,
}

impl Relative {
    /// Create a new Relative addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for Relative {
    fn is_done(&self) -> bool {
        self.cycle == 2
    }

    fn tick(&mut self, cpu_state: &mut CpuState, memory: Rc<RefCell<MemController>>) {
        debug_assert!(self.cycle < 2, "Relative::tick called after already done");

        match self.cycle {
            0 => {
                // Cycle 1: Fetch signed offset from PC and compute target address
                let offset = memory.borrow().read(cpu_state.pc) as i8;
                cpu_state.pc = cpu_state.pc.wrapping_add(1);
                // Cast i8 -> i16 for sign extension, then to u16 for wrapping arithmetic
                // Target address = (PC after increment) + signed offset
                self.address = cpu_state.pc.wrapping_add(offset as i16 as u16);
                self.cycle = 1;
            }
            1 => {
                // Cycle 2: Complete (address already computed)
                self.cycle = 2;
            }
            _ => unreachable!(),
        }
    }

    fn get_address(&self) -> u16 {
        debug_assert!(
            self.cycle == 2,
            "Relative::get_address called before addressing complete"
        );
        self.address
    }
}

/// Accumulator addressing mode
///
/// Used by instructions that operate directly on the accumulator register.
/// No memory access or address calculation needed.
/// Examples: ROL A, ROR A, LSR A, ASL A
///
/// Cycles: 0 (no addressing cycles, operation happens during instruction execution)
#[derive(Debug, Clone, Copy, Default)]
pub struct Accumulator {}

impl Accumulator {
    /// Create a new Accumulator addressing mode instance
    pub fn new() -> Self {
        Self::default()
    }
}

impl AddressingMode for Accumulator {
    fn is_done(&self) -> bool {
        // Always done immediately - no addressing needed
        true
    }

    fn tick(&mut self, _cpu_state: &mut CpuState, _memory: Rc<RefCell<MemController>>) {
        // Should never be called since is_done() always returns true
        panic!("Accumulator::tick should never be called - addressing is always done");
    }

    fn get_address(&self) -> u16 {
        // Accumulator mode doesn't have an address - this should not be called
        // Return 0 as a placeholder, but operations should use the accumulator directly
        panic!("Accumulator::get_address should not be called - no address in accumulator mode");
    }
}

#[cfg(test)]
mod new_tests {
    use super::*;

    // Tests for new cycle-accurate addressing modes

    #[test]
    fn test_immediate_starts_not_done() {
        let mode = Immediate::new();
        assert!(
            !mode.is_done(),
            "Immediate mode should not be done initially"
        );
    }

    #[test]
    fn test_immediate_completes_after_one_tick() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        // Setup memory with a value at PC
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write test value to address 0x8001
        memory.borrow_mut().write(0x8001, 0x42, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x8001, // PC points to the immediate value
            p: 0,
        };

        let mut mode = Immediate::new();

        // After one tick, it should be done
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(
            mode.is_done(),
            "Immediate mode should be done after one tick"
        );
        assert_eq!(mode.get_address(), 0x8001, "Address should be the PC value");
        assert_eq!(cpu_state.pc, 0x8002, "PC should have advanced by 1");
    }

    #[test]
    #[should_panic(expected = "Immediate::get_address called before addressing complete")]
    fn test_immediate_get_address_before_done_panics() {
        let mode = Immediate::new();
        mode.get_address(); // Should panic in debug builds
    }

    // ZeroPage addressing mode tests

    #[test]
    fn test_zeropage_starts_not_done() {
        let mode = ZeroPage::new();
        assert!(
            !mode.is_done(),
            "ZeroPage mode should not be done initially"
        );
    }

    #[test]
    fn test_zeropage_completes_after_one_tick() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        // Setup memory
        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write zero page address at PC (use RAM address 0x0200 instead of ROM)
        memory.borrow_mut().write(0x0200, 0x42, false); // Zero page address

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0200, // Start in RAM
            p: 0,
        };

        let mut mode = ZeroPage::new();

        // After one tick, it should be done
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(
            mode.is_done(),
            "ZeroPage mode should be done after one tick"
        );
        assert_eq!(
            mode.get_address(),
            0x0042,
            "Address should be zero page 0x42"
        );
        assert_eq!(cpu_state.pc, 0x0201, "PC should have advanced by 1");
    }

    #[test]
    #[should_panic(expected = "ZeroPage::get_address called before addressing complete")]
    fn test_zeropage_get_address_before_done_panics() {
        let mode = ZeroPage::new();
        mode.get_address(); // Should panic in debug builds
    }

    // Absolute addressing mode tests

    #[test]
    fn test_absolute_starts_not_done() {
        let mode = Absolute::new();
        assert!(
            !mode.is_done(),
            "Absolute mode should not be done initially"
        );
    }

    #[test]
    fn test_absolute_not_done_after_one_tick() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write low byte at PC
        memory.borrow_mut().write(0x0200, 0x34, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = Absolute::new();
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(
            !mode.is_done(),
            "Absolute mode should not be done after one tick"
        );
        assert_eq!(cpu_state.pc, 0x0201, "PC should have advanced by 1");
    }

    #[test]
    fn test_absolute_completes_after_two_ticks() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write address bytes at PC (little-endian: low byte first, high byte second)
        memory.borrow_mut().write(0x0200, 0x34, false); // Low byte
        memory.borrow_mut().write(0x0201, 0x12, false); // High byte

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = Absolute::new();

        // First tick - read low byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after first tick");

        // Second tick - read high byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(mode.is_done(), "Should be done after second tick");
        assert_eq!(
            mode.get_address(),
            0x1234,
            "Address should be 0x1234 (little-endian)"
        );
        assert_eq!(cpu_state.pc, 0x0202, "PC should have advanced by 2");
    }

    #[test]
    #[should_panic(expected = "Absolute::get_address called before addressing complete")]
    fn test_absolute_get_address_before_done_panics() {
        let mode = Absolute::new();
        mode.get_address(); // Should panic in debug builds
    }

    // ZeroPageX addressing mode tests

    #[test]
    fn test_zeropagex_starts_not_done() {
        let mode = ZeroPageX::new();
        assert!(
            !mode.is_done(),
            "ZeroPageX mode should not be done initially"
        );
    }

    #[test]
    fn test_zeropagex_not_done_after_one_tick() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write base zero page address at PC
        memory.borrow_mut().write(0x0200, 0x80, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0x05,
            y: 0,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = ZeroPageX::new();
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(
            !mode.is_done(),
            "ZeroPageX mode should not be done after one tick"
        );
        assert_eq!(cpu_state.pc, 0x0201, "PC should have advanced by 1");
    }

    #[test]
    fn test_zeropagex_completes_after_two_ticks() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write base zero page address at PC
        memory.borrow_mut().write(0x0200, 0x80, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0x05,
            y: 0,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = ZeroPageX::new();

        // First tick - read base address
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after first tick");

        // Second tick - dummy read and add index
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(mode.is_done(), "Should be done after second tick");
        assert_eq!(
            mode.get_address(),
            0x0085,
            "Address should be 0x80 + 0x05 = 0x85"
        );
        assert_eq!(cpu_state.pc, 0x0201, "PC should have advanced by 1");
    }

    #[test]
    fn test_zeropagex_wraps_around() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write base zero page address at PC
        memory.borrow_mut().write(0x0200, 0xFF, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0x05,
            y: 0,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = ZeroPageX::new();

        mode.tick(&mut cpu_state, Rc::clone(&memory));
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(mode.is_done(), "Should be done after two ticks");
        // 0xFF + 0x05 = 0x104, but wraps to 0x04 in zero page
        assert_eq!(
            mode.get_address(),
            0x0004,
            "Address should wrap around to 0x04"
        );
    }

    #[test]
    #[should_panic(expected = "ZeroPageX::get_address called before addressing complete")]
    fn test_zeropagex_get_address_before_done_panics() {
        let mode = ZeroPageX::new();
        mode.get_address(); // Should panic in debug builds
    }

    // ZeroPageY addressing mode tests

    #[test]
    fn test_zeropagey_starts_not_done() {
        let mode = ZeroPageY::new();
        assert!(
            !mode.is_done(),
            "ZeroPageY mode should not be done initially"
        );
    }

    #[test]
    fn test_zeropagey_completes_after_two_ticks() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write base zero page address at PC
        memory.borrow_mut().write(0x0200, 0x80, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0x07,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = ZeroPageY::new();

        // First tick - read base address
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after first tick");

        // Second tick - dummy read and add index
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(mode.is_done(), "Should be done after second tick");
        assert_eq!(
            mode.get_address(),
            0x0087,
            "Address should be 0x80 + 0x07 = 0x87"
        );
        assert_eq!(cpu_state.pc, 0x0201, "PC should have advanced by 1");
    }

    #[test]
    fn test_zeropagey_wraps_around() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write base zero page address at PC
        memory.borrow_mut().write(0x0200, 0xFE, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0x10,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = ZeroPageY::new();

        mode.tick(&mut cpu_state, Rc::clone(&memory));
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(mode.is_done(), "Should be done after two ticks");
        // 0xFE + 0x10 = 0x10E, but wraps to 0x0E in zero page
        assert_eq!(
            mode.get_address(),
            0x000E,
            "Address should wrap around to 0x0E"
        );
    }

    #[test]
    #[should_panic(expected = "ZeroPageY::get_address called before addressing complete")]
    fn test_zeropagey_get_address_before_done_panics() {
        let mode = ZeroPageY::new();
        mode.get_address(); // Should panic in debug builds
    }

    // AbsoluteX addressing mode tests

    #[test]
    fn test_absolutex_starts_not_done() {
        let mode = AbsoluteX::new();
        assert!(
            !mode.is_done(),
            "AbsoluteX mode should not be done initially"
        );
    }

    #[test]
    fn test_absolutex_completes_after_two_ticks_no_page_cross() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write address bytes (little-endian)
        memory.borrow_mut().write(0x0200, 0x00, false); // Low byte
        memory.borrow_mut().write(0x0201, 0x12, false); // High byte

        let mut cpu_state = CpuState {
            a: 0,
            x: 0x05,
            y: 0,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = AbsoluteX::new();

        // First tick - read low byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after first tick");

        // Second tick - read high byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(
            mode.is_done(),
            "Should be done after second tick (no page cross)"
        );
        assert_eq!(
            mode.get_address(),
            0x1205,
            "Address should be 0x1200 + 0x05 = 0x1205"
        );
        assert_eq!(cpu_state.pc, 0x0202, "PC should have advanced by 2");
    }

    #[test]
    fn test_absolutex_completes_after_three_ticks_with_page_cross() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write address bytes (little-endian)
        memory.borrow_mut().write(0x0200, 0xFF, false); // Low byte
        memory.borrow_mut().write(0x0201, 0x12, false); // High byte

        let mut cpu_state = CpuState {
            a: 0,
            x: 0x05,
            y: 0,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = AbsoluteX::new();

        // First tick - read low byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after first tick");

        // Second tick - read high byte, detect page cross
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(
            !mode.is_done(),
            "Should not be done after second tick (page crossed)"
        );

        // Third tick - fix high byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(mode.is_done(), "Should be done after third tick");
        assert_eq!(
            mode.get_address(),
            0x1304,
            "Address should be 0x12FF + 0x05 = 0x1304"
        );
        assert_eq!(cpu_state.pc, 0x0202, "PC should have advanced by 2");
    }

    #[test]
    #[should_panic(expected = "AbsoluteX::get_address called before addressing complete")]
    fn test_absolutex_get_address_before_done_panics() {
        let mode = AbsoluteX::new();
        mode.get_address(); // Should panic in debug builds
    }

    // AbsoluteY addressing mode tests

    #[test]
    fn test_absolutey_starts_not_done() {
        let mode = AbsoluteY::new();
        assert!(
            !mode.is_done(),
            "AbsoluteY mode should not be done initially"
        );
    }

    #[test]
    fn test_absolutey_completes_after_two_ticks_no_page_cross() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write address bytes (little-endian)
        memory.borrow_mut().write(0x0200, 0x00, false); // Low byte
        memory.borrow_mut().write(0x0201, 0x12, false); // High byte

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0x08,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = AbsoluteY::new();

        // First tick - read low byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after first tick");

        // Second tick - read high byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(
            mode.is_done(),
            "Should be done after second tick (no page cross)"
        );
        assert_eq!(
            mode.get_address(),
            0x1208,
            "Address should be 0x1200 + 0x08 = 0x1208"
        );
        assert_eq!(cpu_state.pc, 0x0202, "PC should have advanced by 2");
    }

    #[test]
    fn test_absolutey_completes_after_three_ticks_with_page_cross() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // Write address bytes (little-endian)
        memory.borrow_mut().write(0x0200, 0xFE, false); // Low byte
        memory.borrow_mut().write(0x0201, 0x12, false); // High byte

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0x10,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = AbsoluteY::new();

        // First tick - read low byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after first tick");

        // Second tick - read high byte, detect page cross
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(
            !mode.is_done(),
            "Should not be done after second tick (page crossed)"
        );

        // Third tick - fix high byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(mode.is_done(), "Should be done after third tick");
        assert_eq!(
            mode.get_address(),
            0x130E,
            "Address should be 0x12FE + 0x10 = 0x130E"
        );
        assert_eq!(cpu_state.pc, 0x0202, "PC should have advanced by 2");
    }

    #[test]
    #[should_panic(expected = "AbsoluteY::get_address called before addressing complete")]
    fn test_absolutey_get_address_before_done_panics() {
        let mode = AbsoluteY::new();
        mode.get_address(); // Should panic in debug builds
    }

    // Indirect addressing mode tests

    #[test]
    fn test_indirect_starts_not_done() {
        let mode = Indirect::new();
        assert!(
            !mode.is_done(),
            "Indirect mode should not be done initially"
        );
    }

    #[test]
    fn test_indirect_completes_after_four_ticks() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // JMP ($0210) - indirect address at $0210
        memory.borrow_mut().write(0x0200, 0x10, false); // Pointer low byte
        memory.borrow_mut().write(0x0201, 0x02, false); // Pointer high byte
        // Target address at $0210
        memory.borrow_mut().write(0x0210, 0x34, false); // Target low byte
        memory.borrow_mut().write(0x0211, 0x12, false); // Target high byte

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0200,
            p: 0,
        };

        let mut mode = Indirect::new();

        // Tick 1 - read pointer low byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after tick 1");

        // Tick 2 - read pointer high byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after tick 2");

        // Tick 3 - read target low byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after tick 3");

        // Tick 4 - read target high byte
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(mode.is_done(), "Should be done after tick 4");
        assert_eq!(
            mode.get_address(),
            0x1234,
            "Address should be 0x1234 from indirect pointer"
        );
        assert_eq!(cpu_state.pc, 0x0202, "PC should have advanced by 2");
    }

    #[test]
    fn test_indirect_page_boundary_bug() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // JMP ($02FF) - pointer at page boundary
        // PC starts at 0x0400
        memory.borrow_mut().write(0x0400, 0xFF, false); // Pointer low byte at PC
        memory.borrow_mut().write(0x0401, 0x02, false); // Pointer high byte at PC+1
        // The pointer is $02FF
        memory.borrow_mut().write(0x02FF, 0x34, false); // Target low byte at $02FF
        memory.borrow_mut().write(0x0200, 0x56, false); // Target high byte at $0200 (page wrap bug)
        memory.borrow_mut().write(0x0300, 0x78, false); // This would be read if there was no bug

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = Indirect::new();

        mode.tick(&mut cpu_state, Rc::clone(&memory));
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(mode.is_done(), "Should be done after four ticks");
        // Because of the page boundary bug, high byte comes from $0200 (0x56), not $0300 (0x78)
        assert_eq!(
            mode.get_address(),
            0x5634,
            "Address should exhibit page boundary bug (0x56 from $0200, not 0x78 from $0300)"
        );
    }

    #[test]
    #[should_panic(expected = "Indirect::get_address called before addressing complete")]
    fn test_indirect_get_address_before_done_panics() {
        let mode = Indirect::new();
        mode.get_address(); // Should panic in debug builds
    }

    // IndexedIndirect tests
    #[test]
    fn test_indexed_indirect_starts_not_done() {
        let mode = IndexedIndirect::new();
        assert!(!mode.is_done(), "Should not be done initially");
    }

    #[test]
    fn test_indexed_indirect_completes_after_six_ticks() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // LDA ($20,X) where X=0x05
        memory.borrow_mut().write(0x0400, 0x20, false); // Base address at PC
        memory.borrow_mut().write(0x25, 0x34, false); // Pointer low at $25 ($20 + X)
        memory.borrow_mut().write(0x26, 0x12, false); // Pointer high at $26
        // Final address should be $1234

        let mut cpu_state = CpuState {
            a: 0,
            x: 0x05,
            y: 0,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = IndexedIndirect::new();

        for i in 1..=4 {
            mode.tick(&mut cpu_state, Rc::clone(&memory));
            if i < 4 {
                assert!(!mode.is_done(), "Should not be done after tick {}", i);
            }
        }

        assert!(mode.is_done(), "Should be done after four ticks");
        assert_eq!(mode.get_address(), 0x1234, "Should return correct address");
    }

    #[test]
    #[should_panic(expected = "IndexedIndirect::get_address called before addressing complete")]
    fn test_indexed_indirect_get_address_before_done_panics() {
        let mode = IndexedIndirect::new();
        mode.get_address(); // Should panic
    }

    #[test]
    fn test_indexed_indirect_wraps_in_zero_page() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // LDA ($FF,X) where X=0x02 - should wrap to $01
        memory.borrow_mut().write(0x0400, 0xFF, false); // Base address at PC
        memory.borrow_mut().write(0x01, 0x78, false); // Pointer low at $01 ($FF + 2 = $01 wrapped)
        memory.borrow_mut().write(0x02, 0x56, false); // Pointer high at $02
        // Final address should be $5678

        let mut cpu_state = CpuState {
            a: 0,
            x: 0x02,
            y: 0,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = IndexedIndirect::new();

        for _ in 0..4 {
            mode.tick(&mut cpu_state, Rc::clone(&memory));
        }

        assert!(mode.is_done(), "Should be done after four ticks");
        assert_eq!(
            mode.get_address(),
            0x5678,
            "Should wrap within zero page when adding X"
        );
    }

    #[test]
    fn test_indexed_indirect_pointer_wraps_in_zero_page() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // LDA ($FE,X) where X=0x00 - pointer at $FE/$FF, high byte wraps to $00
        memory.borrow_mut().write(0x0400, 0xFE, false); // Base address at PC
        memory.borrow_mut().write(0xFE, 0xAB, false); // Pointer low at $FE
        memory.borrow_mut().write(0xFF, 0xCD, false); // Pointer high at $FF
        memory.borrow_mut().write(0x00, 0xEF, false); // Would wrap to $00 if pointer read wraps
        // When reading pointer from $FE/$FF, high byte should come from $FF, not wrap to $00
        // Final address should be $CDAB

        let mut cpu_state = CpuState {
            a: 0,
            x: 0x00,
            y: 0,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = IndexedIndirect::new();

        for _ in 0..4 {
            mode.tick(&mut cpu_state, Rc::clone(&memory));
        }

        assert!(mode.is_done(), "Should be done after four ticks");
        assert_eq!(
            mode.get_address(),
            0xCDAB,
            "Pointer read should wrap high byte within zero page"
        );
    }

    // IndirectIndexed tests
    #[test]
    fn test_indirect_indexed_starts_not_done() {
        let mode = IndirectIndexed::new();
        assert!(!mode.is_done(), "Should not be done initially");
    }

    #[test]
    fn test_indirect_indexed_completes_after_five_ticks_no_page_cross() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // LDA ($20),Y where Y=0x05
        memory.borrow_mut().write(0x0400, 0x20, false); // Zero-page pointer address at PC
        memory.borrow_mut().write(0x20, 0x30, false); // Pointer low at $20
        memory.borrow_mut().write(0x21, 0x12, false); // Pointer high at $21
        // Base pointer is $1230, Y=0x05, final address = $1235 (no page cross)

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0x05,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = IndirectIndexed::new();

        for i in 1..=5 {
            mode.tick(&mut cpu_state, Rc::clone(&memory));
            if i < 5 {
                assert!(!mode.is_done(), "Should not be done after tick {}", i);
            }
        }

        assert!(
            mode.is_done(),
            "Should be done after five ticks (no page cross)"
        );
        assert_eq!(mode.get_address(), 0x1235, "Should return correct address");
        assert!(
            !mode.has_page_cross_penalty(),
            "Should not have page cross penalty"
        );
    }

    #[test]
    fn test_indirect_indexed_completes_after_six_ticks_with_page_cross() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // LDA ($20),Y where Y=0xFF causes page cross
        memory.borrow_mut().write(0x0400, 0x20, false); // Zero-page pointer address at PC
        memory.borrow_mut().write(0x20, 0x80, false); // Pointer low at $20
        memory.borrow_mut().write(0x21, 0x12, false); // Pointer high at $21
        // Base pointer is $1280, Y=0xFF, final address = $137F (page cross: $12 -> $13)

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0xFF,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = IndirectIndexed::new();

        for i in 1..=6 {
            mode.tick(&mut cpu_state, Rc::clone(&memory));
            if i < 6 {
                assert!(!mode.is_done(), "Should not be done after tick {}", i);
            }
        }

        assert!(
            mode.is_done(),
            "Should be done after six ticks (page cross)"
        );
        assert_eq!(mode.get_address(), 0x137F, "Should return correct address");
        assert!(
            mode.has_page_cross_penalty(),
            "Should have page cross penalty"
        );
    }

    #[test]
    #[should_panic(expected = "IndirectIndexed::get_address called before addressing complete")]
    fn test_indirect_indexed_get_address_before_done_panics() {
        let mode = IndirectIndexed::new();
        mode.get_address(); // Should panic
    }

    #[test]
    fn test_indirect_indexed_pointer_wraps_in_zero_page() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // LDA ($FF),Y - pointer at $FF/$00 (wraps in zero page)
        memory.borrow_mut().write(0x0400, 0xFF, false); // Zero-page pointer address at PC
        memory.borrow_mut().write(0xFF, 0x34, false); // Pointer low at $FF
        memory.borrow_mut().write(0x00, 0x12, false); // Pointer high at $00 (wrapped)
        memory.borrow_mut().write(0x01, 0x56, false); // Should not read this
        // Base pointer is $1234, Y=0x05, final address = $1239

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0x05,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = IndirectIndexed::new();

        for _ in 0..5 {
            mode.tick(&mut cpu_state, Rc::clone(&memory));
        }

        assert!(mode.is_done(), "Should be done after five ticks");
        assert_eq!(
            mode.get_address(),
            0x1239,
            "Pointer read should wrap high byte within zero page"
        );
    }

    // Relative tests
    #[test]
    fn test_relative_starts_not_done() {
        let mode = Relative::new();
        assert!(!mode.is_done(), "Should not be done initially");
    }

    #[test]
    fn test_relative_completes_after_two_ticks() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // BEQ with offset +10 (0x0A)
        memory.borrow_mut().write(0x0400, 0x0A, false); // Positive offset at PC

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = Relative::new();

        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(!mode.is_done(), "Should not be done after tick 1");

        mode.tick(&mut cpu_state, Rc::clone(&memory));
        assert!(mode.is_done(), "Should be done after tick 2");
        // PC was 0x0400, after reading offset it's 0x0401, offset is +10, so target is 0x040B
        assert_eq!(
            mode.get_address(),
            0x040B,
            "Should compute correct target address"
        );
    }

    #[test]
    #[should_panic(expected = "Relative::get_address called before addressing complete")]
    fn test_relative_get_address_before_done_panics() {
        let mode = Relative::new();
        mode.get_address(); // Should panic
    }

    #[test]
    fn test_relative_positive_offset() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // BEQ with offset +127 (0x7F, maximum positive)
        memory.borrow_mut().write(0x0400, 0x7F, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = Relative::new();
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(mode.is_done(), "Should be done");
        // PC was 0x0400, after read it's 0x0401, +127 = 0x0480
        assert_eq!(
            mode.get_address(),
            0x0480,
            "Should handle maximum positive offset"
        );
    }

    #[test]
    fn test_relative_negative_offset() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // BEQ with offset -2 (0xFE in two's complement)
        memory.borrow_mut().write(0x0400, 0xFE, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = Relative::new();
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(mode.is_done(), "Should be done");
        // PC was 0x0400, after read it's 0x0401, -2 = 0x03FF
        assert_eq!(mode.get_address(), 0x03FF, "Should handle negative offset");
    }

    #[test]
    fn test_relative_negative_offset_crosses_page() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        // BEQ with offset -128 (0x80, maximum negative)
        memory.borrow_mut().write(0x0450, 0x80, false);

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0450,
            p: 0,
        };

        let mut mode = Relative::new();
        mode.tick(&mut cpu_state, Rc::clone(&memory));
        mode.tick(&mut cpu_state, Rc::clone(&memory));

        assert!(mode.is_done(), "Should be done");
        // PC was 0x0450, after read it's 0x0451, -128 = 0x03D1
        assert_eq!(
            mode.get_address(),
            0x03D1,
            "Should handle maximum negative offset with page cross"
        );
    }

    // Accumulator tests
    #[test]
    fn test_accumulator_is_always_done() {
        let mode = Accumulator::new();
        assert!(mode.is_done(), "Should always be done immediately");
    }

    #[test]
    #[should_panic(expected = "Accumulator::tick should never be called")]
    fn test_accumulator_tick_panics() {
        use super::super::types::CpuState;
        use crate::apu::Apu;
        use crate::mem_controller::MemController;
        use crate::nes::TvSystem;
        use crate::ppu::Ppu;
        use std::cell::RefCell;
        use std::rc::Rc;

        let ppu = Rc::new(RefCell::new(Ppu::new(TvSystem::Ntsc)));
        let apu = Rc::new(RefCell::new(Apu::new()));
        let memory = Rc::new(RefCell::new(MemController::new(ppu, apu)));

        let mut cpu_state = CpuState {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: 0x0400,
            p: 0,
        };

        let mut mode = Accumulator::new();
        mode.tick(&mut cpu_state, Rc::clone(&memory)); // Should panic
    }

    #[test]
    #[should_panic(expected = "Accumulator::get_address should not be called")]
    fn test_accumulator_get_address_panics() {
        let mode = Accumulator::new();
        mode.get_address(); // Should panic
    }
}
