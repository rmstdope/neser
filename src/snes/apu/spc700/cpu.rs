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

    /// Read `PC` and advance it, consuming one cycle.
    fn fetch(&mut self, bus: &mut impl Spc700Bus, cycles: &mut u8) -> u8 {
        let byte = self.read_cycle(bus, self.pc, cycles);
        self.pc = self.pc.wrapping_add(1);
        byte
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
}
