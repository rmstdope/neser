//! SPC700 CPU core.
//!
//! The SPC700 is the Sony 8-bit CPU inside the SNES APU, running at ~1.024 MHz
//! independently of the main 65816. It has a 16-bit address space backed by the
//! 64 KB ARAM and a small register set.
//!
//! Register / flag layout follows the fullsnes "SNES APU SPC700 CPU" section:
//!
//! ```text
//! A   8-bit accumulator       SP  8-bit stack pointer ($0100..$01FF)
//! X   8-bit index             PC  16-bit program counter
//! Y   8-bit index             YA  16-bit pair (Y = MSB, A = LSB)
//! PSW 8-bit flags
//!
//! PSW bits: N V P B H I Z C
//!   N (bit7) Negative      H (bit3) Half-carry
//!   V (bit6) Overflow      I (bit2) Interrupt enable (unused in SNES APU)
//!   P (bit5) Direct page   Z (bit1) Zero
//!   B (bit4) Break         C (bit0) Carry
//! ```

use crate::snes::apu::spc700::bus::Spc700Bus;

/// PSW carry flag (bit 0).
pub const FLAG_CARRY: u8 = 0b0000_0001;
/// PSW zero flag (bit 1).
pub const FLAG_ZERO: u8 = 0b0000_0010;
/// PSW interrupt-enable flag (bit 2); has no function in the SNES APU.
pub const FLAG_INTERRUPT: u8 = 0b0000_0100;
/// PSW half-carry flag (bit 3).
pub const FLAG_HALF_CARRY: u8 = 0b0000_1000;
/// PSW break flag (bit 4).
pub const FLAG_BREAK: u8 = 0b0001_0000;
/// PSW direct-page selection flag (bit 5): 0 = `$00xx`, 1 = `$01xx`.
pub const FLAG_DIRECT_PAGE: u8 = 0b0010_0000;
/// PSW overflow flag (bit 6).
pub const FLAG_OVERFLOW: u8 = 0b0100_0000;
/// PSW negative/sign flag (bit 7).
pub const FLAG_NEGATIVE: u8 = 0b1000_0000;

/// SPC700 CPU core.
///
/// The core is generic over an [`Spc700Bus`] so it can be unit-tested with a
/// flat-RAM bus and verified against SingleStepTests vectors, while the real
/// APU wires it to ARAM, the I/O ports, timers, and the boot ROM overlay.
#[derive(Debug, Clone)]
pub struct Spc700 {
    /// Accumulator.
    a: u8,
    /// X index register.
    x: u8,
    /// Y index register.
    y: u8,
    /// Stack pointer (addresses `$0100 + sp`).
    sp: u8,
    /// Program counter.
    pc: u16,
    /// Program status word (flags).
    psw: u8,
}

impl Spc700 {
    /// SPC700 reset vector address (low byte at `$FFFE`, high byte at `$FFFF`).
    pub const RESET_VECTOR: u16 = 0xFFFE;

    /// Create a new SPC700 with all registers cleared.
    ///
    /// Use [`Spc700::reset`] to load the reset vector before stepping.
    pub fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            sp: 0,
            pc: 0,
            psw: 0,
        }
    }

    /// Reset the CPU: clear registers/flags and load `PC` from the reset vector.
    ///
    /// On real hardware the boot ROM is mapped at reset, so the vector at
    /// `$FFFE/$FFFF` points at the IPL entry (`$FFC0`). The core simply reads the
    /// vector through the bus, leaving region selection to the bus implementation.
    pub fn reset(&mut self, bus: &mut impl Spc700Bus) {
        self.a = 0;
        self.x = 0;
        self.y = 0;
        self.sp = 0;
        self.psw = 0;
        let lo = bus.read(Self::RESET_VECTOR) as u16;
        let hi = bus.read(Self::RESET_VECTOR.wrapping_add(1)) as u16;
        self.pc = (hi << 8) | lo;
    }

    /// Accumulator register.
    pub fn a(&self) -> u8 {
        self.a
    }

    /// X index register.
    pub fn x(&self) -> u8 {
        self.x
    }

    /// Y index register.
    pub fn y(&self) -> u8 {
        self.y
    }

    /// Stack pointer.
    pub fn sp(&self) -> u8 {
        self.sp
    }

    /// Program counter.
    pub fn pc(&self) -> u16 {
        self.pc
    }

    /// Program status word (flags byte).
    pub fn psw(&self) -> u8 {
        self.psw
    }

    /// `YA` register pair (Y = high byte, A = low byte).
    pub fn ya(&self) -> u16 {
        ((self.y as u16) << 8) | self.a as u16
    }

    /// Returns `true` if the given PSW flag mask is set.
    pub fn flag(&self, mask: u8) -> bool {
        self.psw & mask != 0
    }

    /// Set or clear the given PSW flag mask.
    #[allow(dead_code)] // Consumed by ALU/MOV opcodes added in the next slice.
    fn set_flag(&mut self, mask: u8, value: bool) {
        if value {
            self.psw |= mask;
        } else {
            self.psw &= !mask;
        }
    }

    /// Update N and Z according to an 8-bit result value.
    fn update_nz8(&mut self, value: u8) {
        self.set_flag(FLAG_ZERO, value == 0);
        self.set_flag(FLAG_NEGATIVE, value & 0x80 != 0);
    }

    /// Base address for direct-page addressing (`$00xx` or `$01xx`).
    fn direct_page_base(&self) -> u16 {
        if self.flag(FLAG_DIRECT_PAGE) {
            0x0100
        } else {
            0x0000
        }
    }

    /// Read `PC` and advance it, consuming one cycle.
    fn fetch(&mut self, bus: &mut impl Spc700Bus, cycles: &mut u8) -> u8 {
        let byte = self.read_cycle(bus, self.pc, cycles);
        self.pc = self.pc.wrapping_add(1);
        byte
    }

    /// Fetch a 16-bit little-endian immediate operand from PC.
    fn fetch_u16(&mut self, bus: &mut impl Spc700Bus, cycles: &mut u8) -> u16 {
        let lo = self.fetch(bus, cycles) as u16;
        let hi = self.fetch(bus, cycles) as u16;
        (hi << 8) | lo
    }

    /// Read a byte, consuming one cycle.
    fn read_cycle(&mut self, bus: &mut impl Spc700Bus, addr: u16, cycles: &mut u8) -> u8 {
        *cycles = cycles.wrapping_add(1);
        bus.read(addr)
    }

    /// Write a byte, consuming one cycle.
    #[allow(dead_code)] // Used as opcodes are added in subsequent slices.
    fn write_cycle(&mut self, bus: &mut impl Spc700Bus, addr: u16, value: u8, cycles: &mut u8) {
        *cycles = cycles.wrapping_add(1);
        bus.write(addr, value);
    }

    /// Consume one internal (idle) cycle.
    fn idle_cycle(&mut self, bus: &mut impl Spc700Bus, cycles: &mut u8) {
        *cycles = cycles.wrapping_add(1);
        bus.idle();
    }

    /// Execute a single instruction, returning the number of cycles consumed.
    ///
    /// Only `NOP` is implemented in this first slice; further opcodes are added
    /// incrementally with their own tests and SingleStepTests coverage.
    pub fn step(&mut self, bus: &mut impl Spc700Bus) -> u8 {
        let mut cycles = 0u8;
        let opcode = self.fetch(bus, &mut cycles);
        match opcode {
            // NOP — no operation (2 cycles: opcode fetch + 1 idle).
            0x00 => {
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV A,#imm — load 8-bit immediate into A, update N/Z.
            0xE8 => {
                let imm = self.fetch(bus, &mut cycles);
                self.a = imm;
                self.update_nz8(self.a);
            }
            // MOV X,#imm — load 8-bit immediate into X, update N/Z.
            0xCD => {
                let imm = self.fetch(bus, &mut cycles);
                self.x = imm;
                self.update_nz8(self.x);
            }
            // MOV Y,#imm — load 8-bit immediate into Y, update N/Z.
            0x8D => {
                let imm = self.fetch(bus, &mut cycles);
                self.y = imm;
                self.update_nz8(self.y);
            }
            // MOV A,X — copy X into A, update N/Z.
            0x7D => {
                self.a = self.x;
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV X,A — copy A into X, update N/Z.
            0x5D => {
                self.x = self.a;
                self.update_nz8(self.x);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV A,Y — copy Y into A, update N/Z.
            0xDD => {
                self.a = self.y;
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV Y,A — copy A into Y, update N/Z.
            0xFD => {
                self.y = self.a;
                self.update_nz8(self.y);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV X,SP — copy SP into X, update N/Z.
            0x9D => {
                self.x = self.sp;
                self.update_nz8(self.x);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV SP,X — copy X into SP; flags are unaffected.
            0xBD => {
                self.sp = self.x;
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV A,dp — load A from direct page, update N/Z.
            0xE4 => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | dp as u16;
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
            }
            // MOV dp,A — store A to direct page; flags unaffected.
            0xC4 => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | dp as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
            }
            // MOV dp,X — store X to direct page; flags unaffected.
            0xD8 => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | dp as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.x, &mut cycles);
            }
            // MOV dp,Y — store Y to direct page; flags unaffected.
            0xCB => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | dp as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.y, &mut cycles);
            }
            // MOV A,(X) — load A from direct-page address in X, update N/Z.
            0xE6 => {
                let addr = self.direct_page_base() | self.x as u16;
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV A,(X)+ — load A from [X], then increment X, update N/Z.
            0xBF => {
                let addr = self.direct_page_base() | self.x as u16;
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
                self.x = self.x.wrapping_add(1);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV (X),A — store A to direct-page address in X; flags unchanged.
            0xC6 => {
                let addr = self.direct_page_base() | self.x as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV (X)+,A — store A to [X], then increment X; flags unchanged.
            0xAF => {
                let addr = self.direct_page_base() | self.x as u16;
                self.write_cycle(bus, addr, self.a, &mut cycles);
                self.x = self.x.wrapping_add(1);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV A,dp+X — load A from direct page indexed by X, update N/Z.
            0xF4 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | dp.wrapping_add(self.x) as u16;
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
            }
            // MOV dp+X,A — store A to direct page indexed by X; flags unchanged.
            0xD4 => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | dp.wrapping_add(self.x) as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV dp+X,Y — store Y to direct page indexed by X; flags unchanged.
            0xDB => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | dp.wrapping_add(self.x) as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.y, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV dp+Y,X — store X to direct page indexed by Y; flags unchanged.
            0xD9 => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | dp.wrapping_add(self.y) as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.x, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV A,!abs — load A from 16-bit absolute address, update N/Z.
            0xE5 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
            }
            // MOV !abs,A — store A to 16-bit absolute address; flags unchanged.
            0xC5 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
            }
            // MOV A,!abs+X — load A from absolute indexed by X, update N/Z.
            0xF5 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = base.wrapping_add(self.x as u16);
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
            }
            // MOV A,!abs+Y — load A from absolute indexed by Y, update N/Z.
            0xF6 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = base.wrapping_add(self.y as u16);
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
            }
            // MOV !abs+X,A — store A to absolute indexed by X; flags unchanged.
            0xD5 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = base.wrapping_add(self.x as u16);
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
            }
            // MOV !abs+Y,A — store A to absolute indexed by Y; flags unchanged.
            0xD6 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = base.wrapping_add(self.y as u16);
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
            }
            // MOV X,dp — load X from direct page, update N/Z.
            0xF8 => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | dp as u16;
                self.x = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.x);
            }
            // MOV Y,dp — load Y from direct page, update N/Z.
            0xEB => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | dp as u16;
                self.y = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.y);
            }
            // MOV X,!abs — load X from absolute, update N/Z.
            0xE9 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                self.x = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.x);
            }
            // MOV Y,!abs — load Y from absolute, update N/Z.
            0xEC => {
                let addr = self.fetch_u16(bus, &mut cycles);
                self.y = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.y);
            }
            other => panic!(
                "SPC700: unimplemented opcode {other:#04X} at PC {:#06X}",
                self.pc.wrapping_sub(1)
            ),
        }
        cycles
    }
}

impl Default for Spc700 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl Spc700 {
    /// Test-only helper to seed full CPU state from processor-test vectors.
    pub fn load_state_for_processor_test(&mut self, a: u8, x: u8, y: u8, sp: u8, pc: u16, psw: u8) {
        self.a = a;
        self.x = x;
        self.y = y;
        self.sp = sp;
        self.pc = pc;
        self.psw = psw;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snes::apu::spc700::bus::FlatRamBus;

    #[test]
    fn new_clears_all_registers() {
        let cpu = Spc700::new();
        assert_eq!(cpu.a(), 0);
        assert_eq!(cpu.x(), 0);
        assert_eq!(cpu.y(), 0);
        assert_eq!(cpu.sp(), 0);
        assert_eq!(cpu.pc(), 0);
        assert_eq!(cpu.psw(), 0);
    }

    #[test]
    fn reset_loads_pc_from_reset_vector() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.set(0xFFFE, 0xC0);
        bus.set(0xFFFF, 0xFF);
        cpu.reset(&mut bus);
        assert_eq!(cpu.pc(), 0xFFC0);
        assert_eq!(cpu.psw(), 0);
        assert_eq!(cpu.sp(), 0);
    }

    #[test]
    fn ya_combines_y_and_a() {
        let mut cpu = Spc700::new();
        cpu.a = 0x34;
        cpu.y = 0x12;
        assert_eq!(cpu.ya(), 0x1234);
    }

    #[test]
    fn flag_get_and_set_roundtrip() {
        let mut cpu = Spc700::new();
        assert!(!cpu.flag(FLAG_CARRY));
        cpu.set_flag(FLAG_CARRY, true);
        assert!(cpu.flag(FLAG_CARRY));
        assert_eq!(cpu.psw(), FLAG_CARRY);
        cpu.set_flag(FLAG_NEGATIVE, true);
        assert!(cpu.flag(FLAG_NEGATIVE));
        cpu.set_flag(FLAG_CARRY, false);
        assert!(!cpu.flag(FLAG_CARRY));
        assert_eq!(cpu.psw(), FLAG_NEGATIVE);
    }

    #[test]
    fn psw_flag_bit_positions_match_fullsnes() {
        assert_eq!(FLAG_CARRY, 1 << 0);
        assert_eq!(FLAG_ZERO, 1 << 1);
        assert_eq!(FLAG_INTERRUPT, 1 << 2);
        assert_eq!(FLAG_HALF_CARRY, 1 << 3);
        assert_eq!(FLAG_BREAK, 1 << 4);
        assert_eq!(FLAG_DIRECT_PAGE, 1 << 5);
        assert_eq!(FLAG_OVERFLOW, 1 << 6);
        assert_eq!(FLAG_NEGATIVE, 1 << 7);
    }

    #[test]
    fn nop_advances_pc_by_one_and_takes_two_cycles() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x00]); // NOP
        cpu.load_state_for_processor_test(0, 0, 0, 0, 0x0200, 0);
        let cycles = cpu.step(&mut bus);
        assert_eq!(cpu.pc(), 0x0201);
        assert_eq!(cycles, 2);
        assert_eq!(bus.cycles(), 2);
    }

    #[test]
    fn mov_a_immediate_loads_a_sets_zero_and_preserves_other_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0300, &[0xE8, 0x00]); // MOV A,#$00
        cpu.load_state_for_processor_test(0x12, 0x00, 0x00, 0xEF, 0x0300, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0302);
        assert_eq!(cpu.a(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_a_immediate_sets_negative_clears_zero_and_preserves_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0310, &[0xE8, 0x80]); // MOV A,#$80
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0310, FLAG_CARRY | FLAG_ZERO);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0312);
        assert_eq!(cpu.a(), 0x80);
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_x_immediate_sets_negative_and_preserves_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0320, &[0xCD, 0x80]); // MOV X,#$80
        cpu.load_state_for_processor_test(0x12, 0x34, 0x56, 0xEF, 0x0320, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0322);
        assert_eq!(cpu.x(), 0x80);
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_x_immediate_sets_zero_clears_negative_and_preserves_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0322, &[0xCD, 0x00]); // MOV X,#$00
        cpu.load_state_for_processor_test(
            0x12,
            0x34,
            0x56,
            0xEF,
            0x0322,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0324);
        assert_eq!(cpu.x(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_y_immediate_sets_negative_and_preserves_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0340, &[0x8D, 0x80]); // MOV Y,#$80
        cpu.load_state_for_processor_test(0x12, 0x34, 0x56, 0xEF, 0x0340, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0342);
        assert_eq!(cpu.y(), 0x80);
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_y_immediate_sets_zero_clears_negative_and_preserves_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0342, &[0x8D, 0x00]); // MOV Y,#$00
        cpu.load_state_for_processor_test(
            0x12,
            0x34,
            0x56,
            0xEF,
            0x0342,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0344);
        assert_eq!(cpu.y(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_a_x_copies_x_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0360, &[0x7D]); // MOV A,X
        cpu.load_state_for_processor_test(0x10, 0x80, 0x00, 0xEF, 0x0360, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0361);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_x_a_copies_a_and_sets_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0361, &[0x5D]); // MOV X,A
        cpu.load_state_for_processor_test(
            0x00,
            0xFF,
            0x00,
            0xEF,
            0x0361,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0362);
        assert_eq!(cpu.x(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_a_y_copies_y_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0362, &[0xDD]); // MOV A,Y
        cpu.load_state_for_processor_test(0x10, 0x00, 0x80, 0xEF, 0x0362, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0363);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_y_a_copies_a_and_sets_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0363, &[0xFD]); // MOV Y,A
        cpu.load_state_for_processor_test(
            0x00,
            0x00,
            0xFF,
            0xEF,
            0x0363,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0364);
        assert_eq!(cpu.y(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_x_sp_copies_sp_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0364, &[0x9D]); // MOV X,SP
        cpu.load_state_for_processor_test(0x12, 0x00, 0x00, 0x80, 0x0364, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0365);
        assert_eq!(cpu.x(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_sp_x_copies_x_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0365, &[0xBD]); // MOV SP,X
        cpu.load_state_for_processor_test(
            0x12,
            0x34,
            0x56,
            0xEF,
            0x0365,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0366);
        assert_eq!(cpu.sp(), 0x34);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_a_dp_uses_p_flag_page_1_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0366, &[0xE4, 0x80]); // MOV A,$80
        bus.set(0x0180, 0x80);
        cpu.load_state_for_processor_test(
            0x00,
            0x00,
            0x00,
            0xEF,
            0x0366,
            FLAG_DIRECT_PAGE | FLAG_CARRY,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.pc(), 0x0368);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_dp_a_uses_p_flag_page_1_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0368, &[0xC4, 0x81]); // MOV $81,A
        cpu.load_state_for_processor_test(
            0x42,
            0x00,
            0x00,
            0xEF,
            0x0368,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x036A);
        assert_eq!(bus.get(0x0181), 0x42);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_DIRECT_PAGE));
    }

    #[test]
    fn mov_dp_x_uses_p_flag_page_1_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x036A, &[0xD8, 0x82]); // MOV $82,X
        cpu.load_state_for_processor_test(
            0x00,
            0x37,
            0x00,
            0xEF,
            0x036A,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x036C);
        assert_eq!(bus.get(0x0182), 0x37);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_DIRECT_PAGE));
    }

    #[test]
    fn mov_dp_y_uses_p_flag_page_1_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x036C, &[0xCB, 0x83]); // MOV $83,Y
        cpu.load_state_for_processor_test(
            0x00,
            0x00,
            0x91,
            0xEF,
            0x036C,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x036E);
        assert_eq!(bus.get(0x0183), 0x91);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_DIRECT_PAGE));
    }

    #[test]
    fn mov_a_indirect_x_loads_from_page_selected_by_p_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x036E, &[0xE6]); // MOV A,(X)
        bus.set(0x0192, 0x80);
        cpu.load_state_for_processor_test(
            0x00,
            0x92,
            0x00,
            0xEF,
            0x036E,
            FLAG_DIRECT_PAGE | FLAG_CARRY,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.pc(), 0x036F);
        assert_eq!(cpu.a(), 0x80);
        assert_eq!(cpu.x(), 0x92);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_a_indirect_x_postinc_increments_x_and_sets_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x036F, &[0xBF]); // MOV A,(X)+
        bus.set(0x01FE, 0x00);
        cpu.load_state_for_processor_test(
            0x12,
            0xFE,
            0x00,
            0xEF,
            0x036F,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0370);
        assert_eq!(cpu.a(), 0x00);
        assert_eq!(cpu.x(), 0xFF);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_indirect_x_a_stores_to_page_selected_by_p_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0370, &[0xC6]); // MOV (X),A
        bus.set(0x0184, 0x00);
        cpu.load_state_for_processor_test(
            0x66,
            0x84,
            0x00,
            0xEF,
            0x0370,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0371);
        assert_eq!(bus.get(0x0184), 0x66);
        assert_eq!(cpu.x(), 0x84);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_indirect_x_postinc_a_stores_and_increments_x() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0371, &[0xAF]); // MOV (X)+,A
        bus.set(0x01FF, 0x00);
        cpu.load_state_for_processor_test(
            0x77,
            0xFF,
            0x00,
            0xEF,
            0x0371,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0372);
        assert_eq!(bus.get(0x01FF), 0x77);
        assert_eq!(cpu.x(), 0x00);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_a_dp_plus_x_wraps_within_page_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0372, &[0xF4, 0xFE]); // MOV A,$FE+X
        bus.set(0x0100, 0x80); // with X=2 wraps FE+2 -> 00 within page $01xx
        cpu.load_state_for_processor_test(
            0x00,
            0x02,
            0x00,
            0xEF,
            0x0372,
            FLAG_DIRECT_PAGE | FLAG_CARRY,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0374);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_dp_plus_x_a_wraps_within_page_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0374, &[0xD4, 0xFE]); // MOV $FE+X,A
        bus.set(0x0100, 0x00); // X=2 -> wraps to $0100
        cpu.load_state_for_processor_test(
            0x5A,
            0x02,
            0x00,
            0xEF,
            0x0374,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x0376);
        assert_eq!(bus.get(0x0100), 0x5A);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_dp_plus_x_y_stores_y_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0376, &[0xDB, 0xFE]); // MOV $FE+X,Y
        bus.set(0x0100, 0x00); // X=2 -> wraps to $0100
        cpu.load_state_for_processor_test(
            0x00,
            0x02,
            0x91,
            0xEF,
            0x0376,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x0378);
        assert_eq!(bus.get(0x0100), 0x91);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_dp_plus_y_x_stores_x_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0378, &[0xD9, 0xFE]); // MOV $FE+Y,X
        bus.set(0x0100, 0x00); // Y=2 -> wraps to $0100
        cpu.load_state_for_processor_test(
            0x00,
            0x77,
            0x02,
            0xEF,
            0x0378,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x037A);
        assert_eq!(bus.get(0x0100), 0x77);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_a_abs_loads_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x037A, &[0xE5, 0x34, 0x12]); // MOV A,$1234
        bus.set(0x1234, 0x80);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x037A, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x037D);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_abs_a_stores_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x037D, &[0xC5, 0x35, 0x12]); // MOV $1235,A
        bus.set(0x1235, 0x00);
        cpu.load_state_for_processor_test(
            0x66,
            0x00,
            0x00,
            0xEF,
            0x037D,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x0380);
        assert_eq!(bus.get(0x1235), 0x66);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_a_abs_plus_x_loads_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0380, &[0xF5, 0x34, 0x12]); // MOV A,$1234+X
        bus.set(0x1236, 0x80); // X=2
        cpu.load_state_for_processor_test(0x00, 0x02, 0x00, 0xEF, 0x0380, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x0383);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_a_abs_plus_y_loads_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0383, &[0xF6, 0x34, 0x12]); // MOV A,$1234+Y
        bus.set(0x1236, 0x80); // Y=2
        cpu.load_state_for_processor_test(0x00, 0x00, 0x02, 0xEF, 0x0383, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x0386);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_abs_plus_x_a_stores_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0386, &[0xD5, 0x34, 0x12]); // MOV $1234+X,A
        bus.set(0x1236, 0x00); // X=2
        cpu.load_state_for_processor_test(
            0x66,
            0x02,
            0x00,
            0xEF,
            0x0386,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.pc(), 0x0389);
        assert_eq!(bus.get(0x1236), 0x66);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_abs_plus_y_a_stores_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0389, &[0xD6, 0x34, 0x12]); // MOV $1234+Y,A
        bus.set(0x1236, 0x00); // Y=2
        cpu.load_state_for_processor_test(
            0x66,
            0x00,
            0x02,
            0xEF,
            0x0389,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.pc(), 0x038C);
        assert_eq!(bus.get(0x1236), 0x66);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_x_dp_loads_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x038C, &[0xF8, 0x80]); // MOV X,$80
        bus.set(0x0180, 0x80);
        cpu.load_state_for_processor_test(
            0x00,
            0x00,
            0x00,
            0xEF,
            0x038C,
            FLAG_DIRECT_PAGE | FLAG_CARRY,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.pc(), 0x038E);
        assert_eq!(cpu.x(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_y_dp_loads_and_sets_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x038E, &[0xEB, 0x81]); // MOV Y,$81
        bus.set(0x0181, 0x00);
        cpu.load_state_for_processor_test(
            0x00,
            0x00,
            0xFF,
            0xEF,
            0x038E,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.pc(), 0x0390);
        assert_eq!(cpu.y(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_x_abs_loads_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0390, &[0xE9, 0x34, 0x12]); // MOV X,$1234
        bus.set(0x1234, 0x80);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0390, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0393);
        assert_eq!(cpu.x(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_y_abs_loads_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0393, &[0xEC, 0x35, 0x12]); // MOV Y,$1235
        bus.set(0x1235, 0x80);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0393, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0396);
        assert_eq!(cpu.y(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }
}
