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
use crate::trace_apu;
use serde::{Deserialize, Serialize};

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

/// In-progress opcode state used by [`Spc700::step_one_cycle`].
///
/// When the per-cycle stepper starts a new instruction it stores the opcode
/// and a cycle-since-opcode-fetch counter here; subsequent calls advance the
/// counter and dispatch into a per-opcode cycle handler that consumes one bus
/// operation (read / write / idle) per call. When the last cycle of the
/// opcode runs, the handler clears this slot back to `None`.
#[derive(Debug, Clone, Default)]
pub(crate) struct InProgressOp {
    pub(crate) opcode: u8,
    /// Cycle index within the instruction. Cycle 1 is the opcode fetch (done
    /// when this struct is created); per-opcode handlers run for cycles 2..N.
    pub(crate) cycle: u8,
    /// Scratch operand byte fetched mid-instruction.
    pub(crate) operand: u8,
}

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
    /// Halt state entered by SLEEP/STOP.
    halted: bool,
    /// In-progress per-cycle opcode state (used by [`Self::step_one_cycle`]).
    ///
    /// `None` between instructions; `Some` while a cycle-scripted opcode is
    /// mid-execution. Not part of `Spc700State` (transient between ticks); a
    /// save state taken mid-instruction would lose this.
    pub(crate) in_progress: Option<InProgressOp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Spc700State {
    #[serde(default)]
    pub a: u8,
    #[serde(default)]
    pub x: u8,
    #[serde(default)]
    pub y: u8,
    #[serde(default)]
    pub sp: u8,
    #[serde(default)]
    pub pc: u16,
    #[serde(default)]
    pub psw: u8,
    #[serde(default)]
    pub halted: bool,
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
            halted: false,
            in_progress: None,
        }
    }

    /// Force the CPU into the halted state.
    pub(crate) fn halt(&mut self) {
        self.halted = true;
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
        self.halted = false;
        self.in_progress = None;
        let lo = bus.read(Self::RESET_VECTOR) as u16;
        let hi = bus.read(Self::RESET_VECTOR.wrapping_add(1)) as u16;
        self.pc = (hi << 8) | lo;
    }

    pub fn capture_state(&self) -> Spc700State {
        Spc700State {
            a: self.a,
            x: self.x,
            y: self.y,
            sp: self.sp,
            pc: self.pc,
            psw: self.psw,
            halted: self.halted,
        }
    }

    pub fn restore_state(&mut self, state: &Spc700State) {
        self.a = state.a;
        self.x = state.x;
        self.y = state.y;
        self.sp = state.sp;
        self.pc = state.pc;
        self.psw = state.psw;
        self.halted = state.halted;
        // restore_state always lands on an instruction boundary.
        self.in_progress = None;
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

    /// Add immediate to A with carry bit included, updating N/Z/V/C flags.
    fn add_with_carry_to_a(&mut self, imm: u8) {
        let carry_bit = if self.flag(FLAG_CARRY) { 1 } else { 0 };
        let (temp, carry1) = self.a.overflowing_add(imm);
        let (result, carry2) = temp.overflowing_add(carry_bit);
        let overflow = (self.a ^ result) & (imm ^ result) & 0x80 != 0;
        self.a = result;
        self.set_flag(FLAG_CARRY, carry1 || carry2);
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.update_nz8(self.a);
    }

    /// Subtract immediate from A, updating N/Z/V/C flags.
    fn subtract_from_a(&mut self, imm: u8) {
        let (result, borrow) = self.a.overflowing_sub(imm);
        let overflow = (self.a ^ result) & (!imm ^ result) & 0x80 != 0;
        self.a = result;
        self.set_flag(FLAG_CARRY, !borrow);
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.update_nz8(self.a);
    }

    /// Subtract immediate from A with borrow, updating N/Z/V/C flags.
    fn subtract_with_borrow_from_a(&mut self, imm: u8) {
        let carry_bit = if self.flag(FLAG_CARRY) { 0 } else { 1 };
        let (temp, borrow1) = self.a.overflowing_sub(imm);
        let (result, borrow2) = temp.overflowing_sub(carry_bit);
        let overflow = (self.a ^ result) & (!imm ^ result) & 0x80 != 0;
        self.a = result;
        self.set_flag(FLAG_CARRY, !(borrow1 || borrow2));
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.update_nz8(self.a);
    }

    /// Update C, V, Z, N flags based on subtraction (left - right).
    /// Used by CMP instructions.
    fn update_flags_on_compare(&mut self, left: u8, right: u8) {
        let (result, borrow) = left.overflowing_sub(right);
        let overflow = (left ^ result) & (!right ^ result) & 0x80 != 0;
        self.set_flag(FLAG_CARRY, !borrow);
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.set_flag(FLAG_ZERO, result == 0);
        self.set_flag(FLAG_NEGATIVE, result & 0x80 != 0);
    }

    /// Compare A with immediate (subtract without storing), updating N/Z/V/C flags.
    fn compare_a(&mut self, imm: u8) {
        self.update_flags_on_compare(self.a, imm);
    }

    /// Compare X with immediate, updating N/Z/V/C flags.
    fn compare_x(&mut self, imm: u8) {
        self.update_flags_on_compare(self.x, imm);
    }

    /// Compare Y with immediate, updating N/Z/V/C flags.
    fn compare_y(&mut self, imm: u8) {
        self.update_flags_on_compare(self.y, imm);
    }

    /// Compare two 8-bit values (left - right), updating N/Z/V/C flags.
    fn compare_values(&mut self, left: u8, right: u8) {
        self.update_flags_on_compare(left, right);
    }

    /// Read `PC` and advance it, consuming the bus-defined read cycle cost.
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

    /// Fetch SPC700 mem.bit operand (13-bit address + 3-bit bit index).
    fn fetch_mem_bit_operand(&mut self, bus: &mut impl Spc700Bus, cycles: &mut u8) -> (u16, u8) {
        let lo = self.fetch(bus, cycles);
        let hi = self.fetch(bus, cycles);
        let bit_index = hi >> 5;
        let addr = (u16::from(hi & 0x1F) << 8) | u16::from(lo);
        (addr, bit_index)
    }

    /// Read a byte, consuming the bus-defined read cycle cost.
    fn read_cycle(&mut self, bus: &mut impl Spc700Bus, addr: u16, cycles: &mut u8) -> u8 {
        *cycles = cycles.wrapping_add(bus.read_cycles(addr));
        bus.read(addr)
    }

    /// Write a byte, consuming the bus-defined write cycle cost.
    #[allow(dead_code)] // Used as opcodes are added in subsequent slices.
    fn write_cycle(&mut self, bus: &mut impl Spc700Bus, addr: u16, value: u8, cycles: &mut u8) {
        *cycles = cycles.wrapping_add(bus.write_cycles(addr));
        bus.write(addr, value);
    }

    /// Consume one internal (idle) cycle.
    fn idle_cycle(&mut self, bus: &mut impl Spc700Bus, cycles: &mut u8) {
        *cycles = cycles.wrapping_add(bus.idle_cycles());
        bus.idle();
    }

    /// Perform a branch using a signed 8-bit offset (relative to PC+2).
    /// The offset is interpreted as signed: positive goes forward, negative goes backward.
    fn branch(&mut self, offset: i8) {
        let signed_offset = offset as i16;
        self.pc = self.pc.wrapping_add(signed_offset as u16);
    }

    /// Push a byte onto the stack: write to [SP], then decrement SP.
    fn push(&mut self, bus: &mut impl Spc700Bus, value: u8, cycles: &mut u8) {
        let addr = 0x0100u16 | (self.sp as u16);
        self.write_cycle(bus, addr, value, cycles);
        self.sp = self.sp.wrapping_sub(1);
    }

    /// Pop a byte from the stack: increment SP, then read from [SP].
    fn pop(&mut self, bus: &mut impl Spc700Bus, cycles: &mut u8) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        let addr = 0x0100u16 | (self.sp as u16);
        self.read_cycle(bus, addr, cycles)
    }

    /// Read a 16-bit little-endian word from direct-page address `dp`.
    fn read_word_direct_page(&mut self, bus: &mut impl Spc700Bus, dp: u8, cycles: &mut u8) -> u16 {
        let base = self.direct_page_base();
        let lo = self.read_cycle(bus, base | u16::from(dp), cycles);
        let hi = self.read_cycle(bus, base | u16::from(dp.wrapping_add(1)), cycles);
        u16::from(lo) | (u16::from(hi) << 8)
    }

    /// Write a 16-bit little-endian word to direct-page address `dp`.
    fn write_word_direct_page(
        &mut self,
        bus: &mut impl Spc700Bus,
        dp: u8,
        value: u16,
        cycles: &mut u8,
    ) {
        let base = self.direct_page_base();
        self.write_cycle(bus, base | u16::from(dp), value as u8, cycles);
        self.write_cycle(
            bus,
            base | u16::from(dp.wrapping_add(1)),
            (value >> 8) as u8,
            cycles,
        );
    }

    /// ADDW YA,[dp] — add 16-bit direct-page word to YA.
    /// Updates N/Z/V/H/C flags based on the result.
    fn add_to_ya(&mut self, value: u16) {
        let ya = (u16::from(self.y) << 8) | u16::from(self.a);
        let (result, carry) = ya.overflowing_add(value);
        let overflow = (!(ya ^ value) & (ya ^ result) & 0x8000) != 0;
        let half_carry = ((ya & 0x0FFF) + (value & 0x0FFF)) > 0x0FFF;
        self.y = (result >> 8) as u8;
        self.a = result as u8;
        self.set_flag(FLAG_CARRY, carry);
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.set_flag(FLAG_HALF_CARRY, half_carry);
        self.update_nz8(self.y);
    }

    /// SUBW YA,[dp] — subtract 16-bit direct-page word from YA.
    /// Updates N/Z/V/H/C flags based on the result.
    fn subtract_from_ya(&mut self, value: u16) {
        let ya = (u16::from(self.y) << 8) | u16::from(self.a);
        let (result, borrow) = ya.overflowing_sub(value);
        let overflow = ((ya ^ value) & (ya ^ result) & 0x8000) != 0;
        let half_borrow = (ya & 0x0FFF) < (value & 0x0FFF);
        self.y = (result >> 8) as u8;
        self.a = result as u8;
        self.set_flag(FLAG_CARRY, !borrow);
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.set_flag(FLAG_HALF_CARRY, !half_borrow);
        self.update_nz8(self.y);
    }

    /// CMPW YA,[dp] — compare 16-bit direct-page word with YA.
    /// Updates N/Z/V/C flags based on the comparison result (YA - value).
    fn compare_ya(&mut self, value: u16) {
        let ya = (u16::from(self.y) << 8) | u16::from(self.a);
        let (result, borrow) = ya.overflowing_sub(value);
        let overflow = ((ya ^ value) & (ya ^ result) & 0x8000) != 0;
        self.set_flag(FLAG_CARRY, !borrow);
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.update_nz8((result >> 8) as u8);
    }

    /// ASL — shift left and update N/Z/C flags.
    fn asl(&mut self, value: u8) -> u8 {
        self.set_flag(FLAG_CARRY, value & 0x80 != 0);
        let result = value << 1;
        self.update_nz8(result);
        result
    }

    /// LSR — shift right and update N/Z/C flags.
    fn lsr(&mut self, value: u8) -> u8 {
        self.set_flag(FLAG_CARRY, value & 0x01 != 0);
        let result = value >> 1;
        self.update_nz8(result);
        result
    }

    /// ROL — rotate left through carry and update N/Z/C flags.
    fn rol(&mut self, value: u8) -> u8 {
        let carry_in = if self.flag(FLAG_CARRY) { 1 } else { 0 };
        let carry_out = value & 0x80 != 0;
        let result = (value << 1) | carry_in;
        self.set_flag(FLAG_CARRY, carry_out);
        self.update_nz8(result);
        result
    }

    /// ROR — rotate right through carry and update N/Z/C flags.
    fn ror(&mut self, value: u8) -> u8 {
        let carry_in = if self.flag(FLAG_CARRY) { 0x80 } else { 0 };
        let carry_out = value & 0x01 != 0;
        let result = (value >> 1) | carry_in;
        self.set_flag(FLAG_CARRY, carry_out);
        self.update_nz8(result);
        result
    }

    /// MUL YA — multiply Y * A, store result in YA (9 cycles).
    /// YA = Y * A; updates N/Z flags based on high byte of result.
    fn mul_ya(&mut self) {
        let product = u16::from(self.y) * u16::from(self.a);
        self.y = (product >> 8) as u8;
        self.a = product as u8;
        self.update_nz8(self.y); // Set flags based on high byte
    }

    /// DIV YA,X — divide YA / X, store quotient in A and remainder in Y (12 cycles).
    /// A = YA / X; Y = YA mod X. Sets overflow flag if X == 0 (division by zero).
    fn div_ya(&mut self) {
        let ya = (u16::from(self.y) << 8) | u16::from(self.a);
        if self.x == 0 {
            self.set_flag(FLAG_OVERFLOW, true);
        } else {
            let quotient = (ya / u16::from(self.x)) as u8;
            let remainder = (ya % u16::from(self.x)) as u8;
            self.a = quotient;
            self.y = remainder;
            self.set_flag(FLAG_OVERFLOW, false);
        }
        self.update_nz8(self.a); // Set flags based on quotient
    }

    /// Return `true` if the given opcode has a per-cycle script and should be
    /// driven by [`Self::step_one_cycle`] instead of the atomic [`Self::step`].
    ///
    /// Only opcodes critical for the blargg IPL-hack trampoline test
    /// reproducer are cycle-scripted in this commit; the remainder still run
    /// atomically via `step`. Subsequent commits in #2908 expand coverage to
    /// the full instruction set.
    pub fn opcode_is_cycle_scripted(opcode: u8) -> bool {
        matches!(
            opcode,
            // BRA rel — the trampoline wait-loop opcode. Cycle-scripted so the
            // operand byte (port-3 in the trampoline) is read at the correct
            // sub-cycle, observing brief host pulses.
            0x2F
            // MOV A,#imm — the queued micro-op behind the trampoline. Cycle-
            // scripted so the operand byte (port-1 in the trampoline) is read
            // when the host expects.
            | 0xE8
        )
    }

    /// `true` while a cycle-scripted opcode is mid-execution.
    pub fn has_in_progress_op(&self) -> bool {
        self.in_progress.is_some()
    }

    /// Advance the CPU by exactly one SPC700 cycle.
    ///
    /// Caller is responsible for ensuring the next opcode is cycle-scripted
    /// (see [`Self::opcode_is_cycle_scripted`]) before calling this when no
    /// instruction is in progress; the dispatcher in `SnesApu::tick` checks
    /// this and falls back to atomic [`Self::step`] otherwise.
    pub fn step_one_cycle(&mut self, bus: &mut impl Spc700Bus) {
        if self.halted {
            bus.idle();
            return;
        }

        if self.in_progress.is_none() {
            // Cycle 1: opcode fetch. Triggers a bus read at PC and bumps PC.
            let opcode_pc = self.pc;
            let opcode = bus.read(opcode_pc);
            self.pc = opcode_pc.wrapping_add(1);
            debug_assert!(
                Self::opcode_is_cycle_scripted(opcode),
                "step_one_cycle called for non-cycle-scripted opcode ${:02X}",
                opcode
            );
            self.in_progress = Some(InProgressOp {
                opcode,
                cycle: 1,
                operand: 0,
            });
            return;
        }

        let mut op = self.in_progress.take().expect("checked above");
        op.cycle = op.cycle.wrapping_add(1);
        let done = match op.opcode {
            0x2F => self.bra_cycle(bus, &mut op),
            0xE8 => self.mov_a_imm_cycle(bus, &mut op),
            _ => unreachable!(
                "in_progress holds non-cycle-scripted opcode ${:02X}",
                op.opcode
            ),
        };
        if !done {
            self.in_progress = Some(op);
        }
    }

    /// BRA rel cycle handler. 4 cycles total (cycle 1 = opcode fetch elsewhere).
    fn bra_cycle(&mut self, bus: &mut impl Spc700Bus, op: &mut InProgressOp) -> bool {
        match op.cycle {
            2 => {
                // Operand fetch.
                op.operand = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                false
            }
            3 => {
                // First idle + take branch.
                self.branch(op.operand as i8);
                bus.idle();
                false
            }
            4 => {
                // Second idle. Done.
                bus.idle();
                true
            }
            _ => unreachable!("BRA cycle {} out of range", op.cycle),
        }
    }

    /// MOV A,#imm cycle handler. 2 cycles total (cycle 1 = opcode fetch elsewhere).
    fn mov_a_imm_cycle(&mut self, bus: &mut impl Spc700Bus, op: &mut InProgressOp) -> bool {
        match op.cycle {
            2 => {
                let imm = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.a = imm;
                self.update_nz8(self.a);
                true
            }
            _ => unreachable!("MOV A,#imm cycle {} out of range", op.cycle),
        }
    }

    /// Execute a single instruction or halted cycle, returning cycles consumed.
    ///
    /// Opcode coverage is added incrementally with dedicated tests and
    /// SingleStepTests coverage.
    pub fn step(&mut self, bus: &mut impl Spc700Bus) -> u8 {
        if self.halted {
            let cycles = bus.idle_cycles();
            bus.idle();
            return cycles;
        }
        let mut cycles = 0u8;
        let opcode_pc = self.pc;
        let opcode = self.fetch(bus, &mut cycles);
        trace_apu!(
            6;
            "SPC exec ${:04X}: op=${:02X} A=${:02X} X=${:02X} Y=${:02X} PSW=${:02X}",
            opcode_pc,
            opcode,
            self.a,
            self.x,
            self.y,
            self.psw
        );
        if (0xFFDA..=0xFFFF).contains(&opcode_pc) {
            trace_apu!(
                4;
                "SPC exec ${:04X}: op=${:02X} A=${:02X} X=${:02X} Y=${:02X} PSW=${:02X}",
                opcode_pc,
                opcode,
                self.a,
                self.x,
                self.y,
                self.psw
            );
        }
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
            // MOV dp,dp — copy source direct-page byte into destination direct-page byte.
            0xFA => {
                let src_dp = self.fetch(bus, &mut cycles);
                let dst_dp = self.fetch(bus, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_addr = self.direct_page_base() | u16::from(dst_dp);
                self.write_cycle(bus, dst_addr, value, &mut cycles);
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
            // MOV A,[dp+X] — load A via direct-page pointer indexed by X, update N/Z.
            0xE7 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let ptr = dp.wrapping_add(self.x);
                let lo = self.read_cycle(bus, self.direct_page_base() | ptr as u16, &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | ptr.wrapping_add(1) as u16,
                    &mut cycles,
                );
                let addr = u16::from(lo) | (u16::from(hi) << 8);
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
            }
            // MOV A,[dp]+Y — load A via direct-page pointer plus Y, update N/Z.
            0xF7 => {
                let dp = self.fetch(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | dp as u16, &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | dp.wrapping_add(1) as u16,
                    &mut cycles,
                );
                self.idle_cycle(bus, &mut cycles);
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(self.y as u16);
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
            }
            // MOV [dp+X],A — store A via direct-page pointer indexed by X; flags unchanged.
            0xC7 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let ptr = dp.wrapping_add(self.x);
                let lo = self.read_cycle(bus, self.direct_page_base() | ptr as u16, &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | ptr.wrapping_add(1) as u16,
                    &mut cycles,
                );
                let addr = u16::from(lo) | (u16::from(hi) << 8);
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
            }
            // MOV [dp]+Y,A — store A via direct-page pointer plus Y; flags unchanged.
            0xD7 => {
                let dp = self.fetch(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | dp as u16, &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | dp.wrapping_add(1) as u16,
                    &mut cycles,
                );
                self.idle_cycle(bus, &mut cycles);
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(self.y as u16);
                if dp == 0x00 && (0xFFDA..=0xFFFF).contains(&opcode_pc) {
                    trace_apu!(
                        4;
                        "SPC D7 base=${:04X} ptr=${:02X}{:02X} Y=${:02X} -> addr=${:04X} A=${:02X} PSW=${:02X}",
                        self.direct_page_base(),
                        hi,
                        lo,
                        self.y,
                        addr,
                        self.a,
                        self.psw
                    );
                }
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
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
            // MOV X,dp+Y — load X from direct page indexed by Y, update N/Z.
            0xF9 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | dp.wrapping_add(self.y) as u16;
                self.x = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.x);
            }
            // MOV Y,dp+X — load Y from direct page indexed by X, update N/Z.
            0xFB => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | dp.wrapping_add(self.x) as u16;
                self.y = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.y);
            }
            // MOV !abs,X — store X to absolute address; flags unchanged.
            0xC9 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.x, &mut cycles);
            }
            // MOV !abs,Y — store Y to absolute address; flags unchanged.
            0xCC => {
                let addr = self.fetch_u16(bus, &mut cycles);
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.y, &mut cycles);
            }
            // MOV dp,#imm — store immediate to direct-page address; flags unchanged.
            0x8F => {
                let imm = self.fetch(bus, &mut cycles);
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, imm, &mut cycles);
            }
            // MOVW YA,dp — load 16-bit direct-page word into YA, update N/Z from high byte.
            0xBA => {
                let dp = self.fetch(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                self.a = lo;
                self.y = hi;
                self.update_nz8(self.y);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOVW dp,YA — store YA as 16-bit direct-page word; flags unchanged.
            0xDA => {
                let dp = self.fetch(bus, &mut cycles);
                let base = self.direct_page_base() | u16::from(dp);
                self.read_cycle(bus, base, &mut cycles);
                self.write_cycle(bus, base, self.a, &mut cycles);
                self.write_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    self.y,
                    &mut cycles,
                );
            }
            // INC A — increment A, update N/Z.
            0xBC => {
                self.a = self.a.wrapping_add(1);
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // DEC A — decrement A, update N/Z.
            0x9C => {
                self.a = self.a.wrapping_sub(1);
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // INC X — increment X, update N/Z.
            0x3D => {
                self.x = self.x.wrapping_add(1);
                self.update_nz8(self.x);
                self.idle_cycle(bus, &mut cycles);
            }
            // DEC X — decrement X, update N/Z.
            0x1D => {
                self.x = self.x.wrapping_sub(1);
                self.update_nz8(self.x);
                self.idle_cycle(bus, &mut cycles);
            }
            // INC Y — increment Y, update N/Z.
            0xFC => {
                self.y = self.y.wrapping_add(1);
                self.update_nz8(self.y);
                self.idle_cycle(bus, &mut cycles);
            }
            // DEC Y — decrement Y, update N/Z.
            0xDC => {
                self.y = self.y.wrapping_sub(1);
                self.update_nz8(self.y);
                self.idle_cycle(bus, &mut cycles);
            }
            // INC dp — increment direct-page byte, update N/Z.
            0xAB => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value.wrapping_add(1), &mut cycles);
                self.update_nz8(value.wrapping_add(1));
            }
            // INC dp+X — increment direct-page byte indexed by X, update N/Z.
            0xBB => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp.wrapping_add(self.x));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value.wrapping_add(1), &mut cycles);
                self.update_nz8(value.wrapping_add(1));
            }
            // INC !abs — increment absolute byte, update N/Z.
            0xAC => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value.wrapping_add(1), &mut cycles);
                self.update_nz8(value.wrapping_add(1));
            }
            // DEC dp — decrement direct-page byte, update N/Z.
            0x8B => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value.wrapping_sub(1), &mut cycles);
                self.update_nz8(value.wrapping_sub(1));
            }
            // DEC dp+X — decrement direct-page byte indexed by X, update N/Z.
            0x9B => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp.wrapping_add(self.x));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value.wrapping_sub(1), &mut cycles);
                self.update_nz8(value.wrapping_sub(1));
            }
            // DEC !abs — decrement absolute byte, update N/Z.
            0x8C => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value.wrapping_sub(1), &mut cycles);
                self.update_nz8(value.wrapping_sub(1));
            }
            // ASL A — shift left accumulator, bit 7 to carry.
            0x1C => {
                self.a = self.asl(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // ROL A — rotate left accumulator through carry.
            0x3C => {
                self.a = self.rol(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // LSR A — shift right accumulator, bit 0 to carry.
            0x5C => {
                self.a = self.lsr(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // ROR A — rotate right accumulator through carry.
            0x7C => {
                self.a = self.ror(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // ASL dp — shift left direct-page byte.
            0x0B => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.asl(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // ASL dp+X — shift left direct-page byte indexed by X.
            0x1B => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp.wrapping_add(self.x));
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.asl(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // ASL !abs — shift left absolute byte.
            0x0C => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.asl(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // ROL dp — rotate left direct-page byte through carry.
            0x2B => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.rol(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // ROL dp+X — rotate left direct-page byte indexed by X through carry.
            0x3B => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp.wrapping_add(self.x));
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.rol(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // ROL !abs — rotate left absolute byte through carry.
            0x2C => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.rol(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // LSR dp — shift right direct-page byte.
            0x4B => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.lsr(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // LSR dp+X — shift right direct-page byte indexed by X.
            0x5B => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp.wrapping_add(self.x));
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.lsr(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // LSR !abs — shift right absolute byte.
            0x4C => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.lsr(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // ROR dp — rotate right direct-page byte through carry.
            0x6B => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.ror(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // ROR dp+X — rotate right direct-page byte indexed by X through carry.
            0x7B => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp.wrapping_add(self.x));
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.ror(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // ROR !abs — rotate right absolute byte through carry.
            0x6C => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = self.ror(value);
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // AND A,#imm — bitwise AND of immediate into A, update N/Z.
            0x24 => {
                let imm = self.fetch(bus, &mut cycles);
                self.a &= imm;
                self.update_nz8(self.a);
            }
            // AND A,#imm — canonical opcode form.
            0x28 => {
                let imm = self.fetch(bus, &mut cycles);
                self.a &= imm;
                self.update_nz8(self.a);
            }
            // AND dp,dp — [dst] &= [src], update N/Z from destination result.
            0x29 => {
                let src_dp = self.fetch(bus, &mut cycles);
                let dst_dp = self.fetch(bus, &mut cycles);
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_addr = self.direct_page_base() | u16::from(dst_dp);
                let dst = self.read_cycle(bus, dst_addr, &mut cycles);
                let result = dst & src;
                self.write_cycle(bus, dst_addr, result, &mut cycles);
                self.update_nz8(result);
            }
            // AND dp,#imm — [dp] &= imm, update N/Z from destination result.
            0x38 => {
                let imm = self.fetch(bus, &mut cycles);
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = value & imm;
                self.write_cycle(bus, addr, result, &mut cycles);
                self.update_nz8(result);
            }
            // AND A,(X).
            0x26 => {
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.a &= value;
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // AND A,dp+X.
            0x34 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(self.x)),
                    &mut cycles,
                );
                self.a &= value;
                self.update_nz8(self.a);
            }
            // AND A,!abs.
            0x25 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.a &= value;
                self.update_nz8(self.a);
            }
            // AND A,!abs+X.
            0x35 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.x)), &mut cycles);
                self.a &= value;
                self.update_nz8(self.a);
            }
            // AND A,!abs+Y.
            0x36 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.y)), &mut cycles);
                self.a &= value;
                self.update_nz8(self.a);
            }
            // AND A,[dp+X].
            0x27 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let ptr = dp.wrapping_add(self.x);
                let lo =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(ptr), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(ptr.wrapping_add(1)),
                    &mut cycles,
                );
                let value = self.read_cycle(bus, u16::from(lo) | (u16::from(hi) << 8), &mut cycles);
                self.a &= value;
                self.update_nz8(self.a);
            }
            // AND A,[dp]+Y.
            0x37 => {
                let dp = self.fetch(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                self.idle_cycle(bus, &mut cycles);
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.a &= value;
                self.update_nz8(self.a);
            }
            // AND (X),(Y) — (X) &= (Y).
            0x39 => {
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let result = x_value & y_value;
                self.write_cycle(bus, x_addr, result, &mut cycles);
                self.update_nz8(result);
                self.idle_cycle(bus, &mut cycles);
            }
            // OR A,#imm — bitwise OR of immediate into A, update N/Z.
            0x04 => {
                let imm = self.fetch(bus, &mut cycles);
                self.a |= imm;
                self.update_nz8(self.a);
            }
            // OR A,#imm — canonical opcode form.
            0x08 => {
                let imm = self.fetch(bus, &mut cycles);
                self.a |= imm;
                self.update_nz8(self.a);
            }
            // OR dp,dp — [dst] |= [src], update N/Z from destination result.
            0x09 => {
                let src_dp = self.fetch(bus, &mut cycles);
                let dst_dp = self.fetch(bus, &mut cycles);
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_addr = self.direct_page_base() | u16::from(dst_dp);
                let dst = self.read_cycle(bus, dst_addr, &mut cycles);
                let result = dst | src;
                self.write_cycle(bus, dst_addr, result, &mut cycles);
                self.update_nz8(result);
            }
            // OR dp,#imm — [dp] |= imm, update N/Z from destination result.
            0x18 => {
                let imm = self.fetch(bus, &mut cycles);
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = value | imm;
                self.write_cycle(bus, addr, result, &mut cycles);
                self.update_nz8(result);
            }
            // OR A,(X).
            0x06 => {
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.a |= value;
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // OR A,dp+X.
            0x14 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(self.x)),
                    &mut cycles,
                );
                self.a |= value;
                self.update_nz8(self.a);
            }
            // OR A,!abs.
            0x05 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.a |= value;
                self.update_nz8(self.a);
            }
            // OR A,!abs+X.
            0x15 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.x)), &mut cycles);
                self.a |= value;
                self.update_nz8(self.a);
            }
            // OR A,!abs+Y.
            0x16 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.y)), &mut cycles);
                self.a |= value;
                self.update_nz8(self.a);
            }
            // OR A,[dp+X].
            0x07 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let ptr = dp.wrapping_add(self.x);
                let lo =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(ptr), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(ptr.wrapping_add(1)),
                    &mut cycles,
                );
                let value = self.read_cycle(bus, u16::from(lo) | (u16::from(hi) << 8), &mut cycles);
                self.a |= value;
                self.update_nz8(self.a);
            }
            // OR A,[dp]+Y.
            0x17 => {
                let dp = self.fetch(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                self.idle_cycle(bus, &mut cycles);
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.a |= value;
                self.update_nz8(self.a);
            }
            // OR (X),(Y) — (X) |= (Y).
            0x19 => {
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let result = x_value | y_value;
                self.write_cycle(bus, x_addr, result, &mut cycles);
                self.update_nz8(result);
                self.idle_cycle(bus, &mut cycles);
            }
            // EOR A,#imm — bitwise XOR of immediate into A, update N/Z.
            0x44 => {
                let imm = self.fetch(bus, &mut cycles);
                self.a ^= imm;
                self.update_nz8(self.a);
            }
            // EOR A,#imm — canonical opcode form.
            0x48 => {
                let imm = self.fetch(bus, &mut cycles);
                self.a ^= imm;
                self.update_nz8(self.a);
            }
            // EOR dp,dp — [dst] ^= [src], update N/Z from destination result.
            0x49 => {
                let src_dp = self.fetch(bus, &mut cycles);
                let dst_dp = self.fetch(bus, &mut cycles);
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_addr = self.direct_page_base() | u16::from(dst_dp);
                let dst = self.read_cycle(bus, dst_addr, &mut cycles);
                let result = dst ^ src;
                self.write_cycle(bus, dst_addr, result, &mut cycles);
                self.update_nz8(result);
            }
            // EOR dp,#imm — [dp] ^= imm, update N/Z from destination result.
            0x58 => {
                let imm = self.fetch(bus, &mut cycles);
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = value ^ imm;
                self.write_cycle(bus, addr, result, &mut cycles);
                self.update_nz8(result);
            }
            // EOR A,(X).
            0x46 => {
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.a ^= value;
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
            }
            // EOR A,dp+X.
            0x54 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(self.x)),
                    &mut cycles,
                );
                self.a ^= value;
                self.update_nz8(self.a);
            }
            // EOR A,!abs.
            0x45 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.a ^= value;
                self.update_nz8(self.a);
            }
            // EOR A,!abs+X.
            0x55 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.x)), &mut cycles);
                self.a ^= value;
                self.update_nz8(self.a);
            }
            // EOR A,!abs+Y.
            0x56 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.y)), &mut cycles);
                self.a ^= value;
                self.update_nz8(self.a);
            }
            // EOR A,[dp+X].
            0x47 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let ptr = dp.wrapping_add(self.x);
                let lo =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(ptr), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(ptr.wrapping_add(1)),
                    &mut cycles,
                );
                let value = self.read_cycle(bus, u16::from(lo) | (u16::from(hi) << 8), &mut cycles);
                self.a ^= value;
                self.update_nz8(self.a);
            }
            // EOR A,[dp]+Y.
            0x57 => {
                let dp = self.fetch(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                self.idle_cycle(bus, &mut cycles);
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.a ^= value;
                self.update_nz8(self.a);
            }
            // EOR (X),(Y) — (X) ^= (Y).
            0x59 => {
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let result = x_value ^ y_value;
                self.write_cycle(bus, x_addr, result, &mut cycles);
                self.update_nz8(result);
                self.idle_cycle(bus, &mut cycles);
            }
            // ADC A,#imm — add immediate and carry to A, update N/Z/V/C/H.
            0x88 => {
                let imm = self.fetch(bus, &mut cycles);
                self.add_with_carry_to_a(imm);
            }
            // ADC A,dp — add direct-page byte and carry to A, update N/Z/V/C/H.
            0x84 => {
                let dp = self.fetch(bus, &mut cycles);
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.add_with_carry_to_a(value);
            }
            // ADC A,(X).
            0x86 => {
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.add_with_carry_to_a(value);
                self.idle_cycle(bus, &mut cycles);
            }
            // ADC A,dp+X.
            0x94 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(self.x)),
                    &mut cycles,
                );
                self.add_with_carry_to_a(value);
            }
            // ADC A,!abs.
            0x85 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.add_with_carry_to_a(value);
            }
            // ADC A,!abs+X.
            0x95 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.x)), &mut cycles);
                self.add_with_carry_to_a(value);
            }
            // ADC A,!abs+Y.
            0x96 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.y)), &mut cycles);
                self.add_with_carry_to_a(value);
            }
            // ADC A,[dp+X].
            0x87 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let ptr = dp.wrapping_add(self.x);
                let lo =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(ptr), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(ptr.wrapping_add(1)),
                    &mut cycles,
                );
                let value = self.read_cycle(bus, u16::from(lo) | (u16::from(hi) << 8), &mut cycles);
                self.add_with_carry_to_a(value);
            }
            // ADC A,[dp]+Y.
            0x97 => {
                let dp = self.fetch(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                self.idle_cycle(bus, &mut cycles);
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.add_with_carry_to_a(value);
            }
            // ADC dp,dp — [dst] = [dst] + [src] + C, update N/Z/V/C from result.
            0x89 => {
                let src_dp = self.fetch(bus, &mut cycles);
                let dst_dp = self.fetch(bus, &mut cycles);
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_addr = self.direct_page_base() | u16::from(dst_dp);
                let dst = self.read_cycle(bus, dst_addr, &mut cycles);
                let saved_a = self.a;
                self.a = dst;
                self.add_with_carry_to_a(src);
                let result = self.a;
                self.a = saved_a;
                self.write_cycle(bus, dst_addr, result, &mut cycles);
            }
            // ADC dp,#imm — [dp] = [dp] + imm + C, update N/Z/V/C from result.
            0x98 => {
                let imm = self.fetch(bus, &mut cycles);
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let saved_a = self.a;
                self.a = value;
                self.add_with_carry_to_a(imm);
                let result = self.a;
                self.a = saved_a;
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // ADC (X),(Y) — (X) = (X) + (Y) + C.
            0x99 => {
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let saved_a = self.a;
                self.a = x_value;
                self.add_with_carry_to_a(y_value);
                let result = self.a;
                self.a = saved_a;
                self.write_cycle(bus, x_addr, result, &mut cycles);
            }
            // SUB A,#imm — subtract immediate from A, update N/Z/V/C.
            0xA8 => {
                let imm = self.fetch(bus, &mut cycles);
                self.subtract_from_a(imm);
            }
            // SBC A,#imm — subtract immediate from A with borrow, update N/Z/V/C.
            0xA4 => {
                let imm = self.fetch(bus, &mut cycles);
                self.subtract_with_borrow_from_a(imm);
            }
            // SBC A,(X).
            0xA6 => {
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.subtract_with_borrow_from_a(value);
                self.idle_cycle(bus, &mut cycles);
            }
            // SBC A,dp+X.
            0xB4 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(self.x)),
                    &mut cycles,
                );
                self.subtract_with_borrow_from_a(value);
            }
            // SBC A,!abs.
            0xA5 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.subtract_with_borrow_from_a(value);
            }
            // SBC A,!abs+X.
            0xB5 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.x)), &mut cycles);
                self.subtract_with_borrow_from_a(value);
            }
            // SBC A,!abs+Y.
            0xB6 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.y)), &mut cycles);
                self.subtract_with_borrow_from_a(value);
            }
            // SBC A,[dp+X].
            0xA7 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let ptr = dp.wrapping_add(self.x);
                let lo =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(ptr), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(ptr.wrapping_add(1)),
                    &mut cycles,
                );
                let value = self.read_cycle(bus, u16::from(lo) | (u16::from(hi) << 8), &mut cycles);
                self.subtract_with_borrow_from_a(value);
            }
            // SBC A,[dp]+Y.
            0xB7 => {
                let dp = self.fetch(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                self.idle_cycle(bus, &mut cycles);
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.subtract_with_borrow_from_a(value);
            }
            // SBC dp,dp — [dst] = [dst] - [src] - !C, update N/Z/V/C from result.
            0xA9 => {
                let src_dp = self.fetch(bus, &mut cycles);
                let dst_dp = self.fetch(bus, &mut cycles);
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_addr = self.direct_page_base() | u16::from(dst_dp);
                let dst = self.read_cycle(bus, dst_addr, &mut cycles);
                let saved_a = self.a;
                self.a = dst;
                self.subtract_with_borrow_from_a(src);
                let result = self.a;
                self.a = saved_a;
                self.write_cycle(bus, dst_addr, result, &mut cycles);
            }
            // SBC dp,#imm — [dp] = [dp] - imm - !C, update N/Z/V/C from result.
            0xB8 => {
                let imm = self.fetch(bus, &mut cycles);
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let saved_a = self.a;
                self.a = value;
                self.subtract_with_borrow_from_a(imm);
                let result = self.a;
                self.a = saved_a;
                self.write_cycle(bus, addr, result, &mut cycles);
            }
            // SBC (X),(Y) — (X) = (X) - (Y) - !C.
            0xB9 => {
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let saved_a = self.a;
                self.a = x_value;
                self.subtract_with_borrow_from_a(y_value);
                let result = self.a;
                self.a = saved_a;
                self.write_cycle(bus, x_addr, result, &mut cycles);
            }
            // CMP X,#imm — compare X with immediate, update N/Z/V/C.
            0xC8 => {
                let imm = self.fetch(bus, &mut cycles);
                self.compare_x(imm);
            }
            // DI — clear interrupt-enable flag (logical only on SNES APU).
            0xC0 => {
                self.set_flag(FLAG_INTERRUPT, false);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // CMP Y,#imm — compare Y with immediate, update N/Z/V/C.
            0xAD => {
                let imm = self.fetch(bus, &mut cycles);
                self.compare_y(imm);
            }
            // CMP A,#imm — compare A with immediate.
            0x68 => {
                let imm = self.fetch(bus, &mut cycles);
                self.compare_a(imm);
            }
            // CMP dp,dp — compare [dst] with [src], update N/Z/V/C.
            0x69 => {
                let src_dp = self.fetch(bus, &mut cycles);
                let dst_dp = self.fetch(bus, &mut cycles);
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dst_dp),
                    &mut cycles,
                );
                self.compare_values(dst, src);
                self.idle_cycle(bus, &mut cycles);
            }
            // CMP dp,#imm — compare [dp] with immediate, update N/Z/V/C.
            0x78 => {
                let imm = self.fetch(bus, &mut cycles);
                let dp = self.fetch(bus, &mut cycles);
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.compare_values(value, imm);
                self.idle_cycle(bus, &mut cycles);
            }
            // CMP A,(X) — compare A with direct-page address in X.
            0x66 => {
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.compare_a(value);
                self.idle_cycle(bus, &mut cycles);
            }
            // CMP A,dp — compare A with direct-page byte.
            0x64 => {
                let dp = self.fetch(bus, &mut cycles);
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.compare_a(value);
            }
            // CMP A,dp+X — compare A with direct-page indexed by X.
            0x74 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(self.x)),
                    &mut cycles,
                );
                self.compare_a(value);
            }
            // CMP A,!abs — compare A with absolute byte.
            0x65 => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.compare_a(value);
            }
            // CMP A,!abs+X — compare A with absolute indexed by X.
            0x75 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.x)), &mut cycles);
                self.compare_a(value);
            }
            // CMP A,!abs+Y — compare A with absolute indexed by Y.
            0x76 => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let value = self.read_cycle(bus, base.wrapping_add(u16::from(self.y)), &mut cycles);
                self.compare_a(value);
            }
            // CMP A,[dp+X] — compare A with indirect direct-page pointer indexed by X.
            0x67 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let ptr = dp.wrapping_add(self.x);
                let lo =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(ptr), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(ptr.wrapping_add(1)),
                    &mut cycles,
                );
                let value = self.read_cycle(bus, u16::from(lo) | (u16::from(hi) << 8), &mut cycles);
                self.compare_a(value);
            }
            // CMP A,[dp]+Y — compare A with indirect direct-page pointer plus Y.
            0x77 => {
                let dp = self.fetch(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                self.idle_cycle(bus, &mut cycles);
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.compare_a(value);
            }
            // CMP X,dp — compare X with direct-page byte.
            0x3E => {
                let dp = self.fetch(bus, &mut cycles);
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.compare_x(value);
            }
            // CMP X,!abs — compare X with absolute byte.
            0x1E => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.compare_x(value);
            }
            // CMP Y,dp — compare Y with direct-page byte.
            0x7E => {
                let dp = self.fetch(bus, &mut cycles);
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                if dp == 0xF4 {
                    trace_apu!(
                        3;
                        "SPC CMP Y,$F4 at ${:04X}: Y=${:02X} F4=${:02X}",
                        opcode_pc,
                        self.y,
                        value
                    );
                }
                self.compare_y(value);
            }
            // CMP Y,!abs — compare Y with absolute byte.
            0x5E => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.compare_y(value);
            }
            // CMP (X),(Y) — compare direct-page bytes addressed by X and Y.
            0x79 => {
                let x_value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                let y_value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.y),
                    &mut cycles,
                );
                self.compare_values(x_value, y_value);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // CLRP — clear direct-page select flag (P=0).
            0x20 => {
                self.set_flag(FLAG_DIRECT_PAGE, false);
                self.idle_cycle(bus, &mut cycles);
            }
            // SETP — set direct-page select flag (P=1).
            0x40 => {
                self.set_flag(FLAG_DIRECT_PAGE, true);
                self.idle_cycle(bus, &mut cycles);
            }
            // CLRC — clear carry flag.
            0x60 => {
                self.set_flag(FLAG_CARRY, false);
                self.idle_cycle(bus, &mut cycles);
            }
            // SETC — set carry flag.
            0x80 => {
                self.set_flag(FLAG_CARRY, true);
                self.idle_cycle(bus, &mut cycles);
            }
            // CLRV — clear overflow and half-carry flags.
            0xE0 => {
                self.set_flag(FLAG_OVERFLOW, false);
                self.set_flag(FLAG_HALF_CARRY, false);
                self.idle_cycle(bus, &mut cycles);
            }
            // EI — set interrupt-enable flag (logical only on SNES APU).
            0xA0 => {
                self.set_flag(FLAG_INTERRUPT, true);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // NOTC — complement carry flag.
            0xED => {
                self.set_flag(FLAG_CARRY, !self.flag(FLAG_CARRY));
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // AND1 C,mem.bit — C &= bit.
            0x4A => {
                let (addr, bit) = self.fetch_mem_bit_operand(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let bit_set = value & (1 << bit) != 0;
                self.set_flag(FLAG_CARRY, self.flag(FLAG_CARRY) && bit_set);
            }
            // AND1 C,/mem.bit — C &= !bit.
            0x6A => {
                let (addr, bit) = self.fetch_mem_bit_operand(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let bit_set = value & (1 << bit) != 0;
                self.set_flag(FLAG_CARRY, self.flag(FLAG_CARRY) && !bit_set);
            }
            // OR1 C,mem.bit — C |= bit.
            0x0A => {
                let (addr, bit) = self.fetch_mem_bit_operand(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let bit_set = value & (1 << bit) != 0;
                self.set_flag(FLAG_CARRY, self.flag(FLAG_CARRY) || bit_set);
                self.idle_cycle(bus, &mut cycles);
            }
            // OR1 C,/mem.bit — C |= !bit.
            0x2A => {
                let (addr, bit) = self.fetch_mem_bit_operand(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let bit_set = value & (1 << bit) != 0;
                self.set_flag(FLAG_CARRY, self.flag(FLAG_CARRY) || !bit_set);
                self.idle_cycle(bus, &mut cycles);
            }
            // EOR1 C,mem.bit — C ^= bit.
            0x8A => {
                let (addr, bit) = self.fetch_mem_bit_operand(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let bit_set = value & (1 << bit) != 0;
                self.set_flag(FLAG_CARRY, self.flag(FLAG_CARRY) ^ bit_set);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV1 C,mem.bit — C = bit.
            0xAA => {
                let (addr, bit) = self.fetch_mem_bit_operand(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.set_flag(FLAG_CARRY, value & (1 << bit) != 0);
            }
            // MOV1 mem.bit,C — bit = C.
            0xCA => {
                let (addr, bit) = self.fetch_mem_bit_operand(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let mask = 1 << bit;
                let result = if self.flag(FLAG_CARRY) {
                    value | mask
                } else {
                    value & !mask
                };
                self.write_cycle(bus, addr, result, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // NOT1 mem.bit — bit = !bit.
            0xEA => {
                let (addr, bit) = self.fetch_mem_bit_operand(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value ^ (1 << bit), &mut cycles);
            }
            // SET1 dp.bit — set bit in direct-page byte.
            op @ (0x02 | 0x22 | 0x42 | 0x62 | 0x82 | 0xA2 | 0xC2 | 0xE2) => {
                let bit = (op >> 5) & 0x07;
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value | (1 << bit), &mut cycles);
            }
            // CLR1 dp.bit — clear bit in direct-page byte.
            op @ (0x12 | 0x32 | 0x52 | 0x72 | 0x92 | 0xB2 | 0xD2 | 0xF2) => {
                let bit = (op >> 5) & 0x07;
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value & !(1 << bit), &mut cycles);
            }
            // BBS dp.bit,rel — branch if direct-page bit is set.
            op @ (0x03 | 0x23 | 0x43 | 0x63 | 0x83 | 0xA3 | 0xC3 | 0xE3) => {
                let bit = (op >> 5) & 0x07;
                let dp = self.fetch(bus, &mut cycles);
                let rel = self.fetch(bus, &mut cycles) as i8;
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                if value & (1 << bit) != 0 {
                    self.branch(rel);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // BBC dp.bit,rel — branch if direct-page bit is clear.
            op @ (0x13 | 0x33 | 0x53 | 0x73 | 0x93 | 0xB3 | 0xD3 | 0xF3) => {
                let bit = (op >> 5) & 0x07;
                let dp = self.fetch(bus, &mut cycles);
                let rel = self.fetch(bus, &mut cycles) as i8;
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                if value & (1 << bit) == 0 {
                    self.branch(rel);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // BRA rel — Branch Always.
            0x2F => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                self.branch(offset);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // BEQ rel — Branch if Equal (Z flag set).
            0xF0 => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                if self.flag(FLAG_ZERO) {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // BNE rel — Branch if Not Equal (Z flag clear).
            0xD0 => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                if !self.flag(FLAG_ZERO) {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // BCS rel — Branch if Carry Set (C flag set).
            0xB0 => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                if self.flag(FLAG_CARRY) {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // BCC rel — Branch if Carry Clear (C flag clear).
            0x90 => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                if !self.flag(FLAG_CARRY) {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // BVS rel — Branch if Overflow Set (V flag set).
            0x70 => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                if self.flag(FLAG_OVERFLOW) {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // BVC rel — Branch if Overflow Clear (V flag clear).
            0x50 => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                if !self.flag(FLAG_OVERFLOW) {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // BMI rel — Branch if Minus (N flag set).
            0x30 => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                if self.flag(FLAG_NEGATIVE) {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // BPL rel — Branch if Plus (N flag clear).
            0x10 => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                if !self.flag(FLAG_NEGATIVE) {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // JMP [!abs+X] — jump via absolute indirect indexed by X.
            0x1F => {
                let base = self.fetch_u16(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let ptr = base.wrapping_add(u16::from(self.x));
                let lo = self.read_cycle(bus, ptr, &mut cycles);
                let hi = self.read_cycle(bus, ptr.wrapping_add(1), &mut cycles);
                self.pc = u16::from(lo) | (u16::from(hi) << 8);
            }
            // CBNE dp,rel — compare A with direct-page byte and branch if not equal.
            0x2E => {
                let dp = self.fetch(bus, &mut cycles);
                let offset = self.fetch(bus, &mut cycles) as i8;
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                if self.a != value {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // PUSH PSW — push flags onto stack (4 cycles).
            0x0D => {
                self.push(bus, self.psw, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // PUSH A — push accumulator onto stack (4 cycles).
            0x2D => {
                self.push(bus, self.a, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // PUSH X — push X register onto stack (4 cycles).
            0x4D => {
                self.push(bus, self.x, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // PUSH Y — push Y register onto stack (4 cycles).
            0x6D => {
                self.push(bus, self.y, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // POP PSW — pop flags from stack (4 cycles).
            0x8E => {
                self.psw = self.pop(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // POP A — pop accumulator from stack (4 cycles).
            0xAE => {
                self.a = self.pop(bus, &mut cycles);
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // POP X — pop X register from stack (4 cycles).
            0xCE => {
                self.x = self.pop(bus, &mut cycles);
                self.update_nz8(self.x);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // POP Y — pop Y register from stack (4 cycles).
            0xEE => {
                self.y = self.pop(bus, &mut cycles);
                self.update_nz8(self.y);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // BRK — software interrupt: push PC+1 and PSW; set B, clear I; jump via vector.
            0x0F => {
                self.read_cycle(bus, self.pc, &mut cycles);
                let return_addr = self.pc;
                self.push(bus, (return_addr >> 8) as u8, &mut cycles);
                self.push(bus, return_addr as u8, &mut cycles);
                self.push(bus, self.psw, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.set_flag(FLAG_BREAK, true);
                self.set_flag(FLAG_INTERRUPT, false);
                let lo = self.read_cycle(bus, 0xFFDE, &mut cycles);
                let hi = self.read_cycle(bus, 0xFFDF, &mut cycles);
                self.pc = u16::from(lo) | (u16::from(hi) << 8);
            }
            // CALL !abs — jump to subroutine at absolute address (8 cycles).
            0x3F => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let return_addr = self.pc;
                self.push(bus, (return_addr >> 8) as u8, &mut cycles);
                self.push(bus, return_addr as u8, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.pc = addr;
            }
            // PCALL u8 — push return address and jump to $FF00+u.
            0x4F => {
                let upage = self.fetch(bus, &mut cycles);
                let return_addr = self.pc;
                self.push(bus, (return_addr >> 8) as u8, &mut cycles);
                self.push(bus, return_addr as u8, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.pc = 0xFF00 | u16::from(upage);
            }
            // TCALL n — table call via vector at $FFDE - 2*n.
            op @ (0x01 | 0x11 | 0x21 | 0x31 | 0x41 | 0x51 | 0x61 | 0x71 | 0x81 | 0x91 | 0xA1
            | 0xB1 | 0xC1 | 0xD1 | 0xE1 | 0xF1) => {
                let n = (op >> 4) & 0x0F;
                let vector = 0xFFDEu16.wrapping_sub(u16::from(n) * 2);
                let return_addr = self.pc;
                self.push(bus, (return_addr >> 8) as u8, &mut cycles);
                self.push(bus, return_addr as u8, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let lo = self.read_cycle(bus, vector, &mut cycles);
                let hi = self.read_cycle(bus, vector.wrapping_add(1), &mut cycles);
                self.pc = u16::from(lo) | (u16::from(hi) << 8);
            }
            // RTS — return from subroutine (5 cycles).
            // Pops return address from stack and jumps.
            0x6F => {
                let lo = self.pop(bus, &mut cycles) as u16;
                let hi = self.pop(bus, &mut cycles) as u16;
                self.pc = (hi << 8) | lo;
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // RETI — return from interrupt: pop PSW, then PC.
            0x7F => {
                self.psw = self.pop(bus, &mut cycles);
                let lo = self.pop(bus, &mut cycles) as u16;
                let hi = self.pop(bus, &mut cycles) as u16;
                self.pc = (hi << 8) | lo;
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // DBNZ dp,rel — decrement direct-page byte and branch if result is non-zero.
            0x6E => {
                let dp = self.fetch(bus, &mut cycles);
                let offset = self.fetch(bus, &mut cycles) as i8;
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = value.wrapping_sub(1);
                self.write_cycle(bus, addr, result, &mut cycles);
                if result != 0 {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // CBNE dp+X,rel — compare A with direct-page byte indexed by X and branch if not equal.
            0xDE => {
                let dp = self.fetch(bus, &mut cycles);
                let offset = self.fetch(bus, &mut cycles) as i8;
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp.wrapping_add(self.x));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                if self.a != value {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // DBNZ Y,rel — decrement Y and branch if result is non-zero.
            0xFE => {
                let offset = self.fetch(bus, &mut cycles) as i8;
                self.y = self.y.wrapping_sub(1);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                if self.y != 0 {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // JMP !abs — jump to absolute address.
            0x5F => {
                self.pc = self.fetch_u16(bus, &mut cycles);
            }
            // TSET1 !abs — set bits in memory by A; N/Z from A - M.
            0x0E => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a.wrapping_sub(value));
                self.write_cycle(bus, addr, value | self.a, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // TCLR1 !abs — clear bits in memory by A; N/Z from A - M.
            0x4E => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a.wrapping_sub(value));
                self.write_cycle(bus, addr, value & !self.a, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // INCW dp — increment 16-bit direct-page word, update N/Z from high byte.
            0x3A => {
                let dp = self.fetch(bus, &mut cycles);
                let value = self.read_word_direct_page(bus, dp, &mut cycles);
                let result = value.wrapping_add(1);
                self.write_word_direct_page(bus, dp, result, &mut cycles);
                self.update_nz8((result >> 8) as u8);
            }
            // DECW dp — decrement 16-bit direct-page word, update N/Z from high byte.
            0x1A => {
                let dp = self.fetch(bus, &mut cycles);
                let value = self.read_word_direct_page(bus, dp, &mut cycles);
                let result = value.wrapping_sub(1);
                self.write_word_direct_page(bus, dp, result, &mut cycles);
                self.update_nz8((result >> 8) as u8);
            }
            // ADDW YA,dp — add 16-bit direct-page word to YA (5 cycles).
            0x7A => {
                let dp = self.fetch(bus, &mut cycles);
                let value = self.read_word_direct_page(bus, dp, &mut cycles);
                self.add_to_ya(value);
                self.idle_cycle(bus, &mut cycles);
            }
            // SUBW YA,dp — subtract 16-bit direct-page word from YA (5 cycles).
            0x9A => {
                let dp = self.fetch(bus, &mut cycles);
                let value = self.read_word_direct_page(bus, dp, &mut cycles);
                self.subtract_from_ya(value);
                self.idle_cycle(bus, &mut cycles);
            }
            // CMPW YA,dp — compare 16-bit direct-page word with YA (4 cycles).
            0x5A => {
                let dp = self.fetch(bus, &mut cycles);
                let value = self.read_word_direct_page(bus, dp, &mut cycles);
                self.compare_ya(value);
            }
            // MUL YA — multiply Y * A, store result in YA (9 cycles).
            0xCF => {
                self.mul_ya();
                // MUL takes 9 cycles: 1 fetch + 8 operation/idle
                for _ in 0..8 {
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // DIV YA,X — divide YA / X, quotient in A, remainder in Y (12 cycles).
            0x9E => {
                self.div_ya();
                // DIV takes 12 cycles: 1 fetch + 11 operation/idle
                for _ in 0..11 {
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // XCN A — exchange high/low nibbles in A, update N/Z.
            0x9F => {
                self.a = self.a.rotate_left(4);
                self.update_nz8(self.a);
                for _ in 0..4 {
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // DAS A — BCD adjust after subtraction.
            0xBE => {
                let mut value = self.a;
                if !self.flag(FLAG_CARRY) || value > 0x99 {
                    value = value.wrapping_sub(0x60);
                    self.set_flag(FLAG_CARRY, false);
                }
                if !self.flag(FLAG_HALF_CARRY) || (value & 0x0F) > 0x09 {
                    value = value.wrapping_sub(0x06);
                }
                self.a = value;
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // DAA A — BCD adjust after addition.
            0xDF => {
                let mut value = self.a;
                if self.flag(FLAG_CARRY) || value > 0x99 {
                    value = value.wrapping_add(0x60);
                    self.set_flag(FLAG_CARRY, true);
                }
                if self.flag(FLAG_HALF_CARRY) || (value & 0x0F) > 0x09 {
                    value = value.wrapping_add(0x06);
                }
                self.a = value;
                self.update_nz8(self.a);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // SLEEP — halt CPU until external wakeup source.
            0xEF => {
                trace_apu!(1; "SPC entered SLEEP at ${:04X}", opcode_pc);
                self.halted = true;
            }
            // STOP — halt CPU clock until reset.
            0xFF => {
                trace_apu!(1; "SPC entered STOP at ${:04X}", opcode_pc);
                self.halted = true;
            }
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
        self.halted = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snes::apu::spc700::bus::FlatRamBus;

    struct VariableIdleBus {
        ram: Box<[u8; 0x1_0000]>,
        cycles: u64,
        idle_cost: u8,
    }

    impl VariableIdleBus {
        fn new(idle_cost: u8) -> Self {
            Self {
                ram: Box::new([0; 0x1_0000]),
                cycles: 0,
                idle_cost,
            }
        }

        fn load(&mut self, addr: u16, data: &[u8]) {
            for (i, &byte) in data.iter().enumerate() {
                self.ram[addr.wrapping_add(i as u16) as usize] = byte;
            }
        }

        fn cycles(&self) -> u64 {
            self.cycles
        }
    }

    impl Spc700Bus for VariableIdleBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.cycles = self.cycles.wrapping_add(1);
            self.ram[addr as usize]
        }

        fn write(&mut self, addr: u16, value: u8) {
            self.cycles = self.cycles.wrapping_add(1);
            self.ram[addr as usize] = value;
        }

        fn idle_cycles(&self) -> u8 {
            self.idle_cost
        }

        fn idle(&mut self) {
            self.cycles = self.cycles.wrapping_add(u64::from(self.idle_cost));
        }
    }

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

    #[test]
    fn mov_x_dp_plus_y_wraps_within_page_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0396, &[0xF9, 0xFF]); // MOV X,$FF+Y
        bus.set(0x0101, 0x80); // base=$0100, $FF+2 wraps to $01
        cpu.load_state_for_processor_test(
            0x00,
            0x00,
            0x02,
            0xEF,
            0x0396,
            FLAG_DIRECT_PAGE | FLAG_CARRY,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0398);
        assert_eq!(cpu.x(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_y_dp_plus_x_wraps_within_page_and_sets_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0398, &[0xFB, 0xFF]); // MOV Y,$FF+X
        bus.set(0x0101, 0x00); // base=$0100, $FF+2 wraps to $01
        cpu.load_state_for_processor_test(
            0x00,
            0x02,
            0xFF,
            0xEF,
            0x0398,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x039A);
        assert_eq!(cpu.y(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_abs_x_stores_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x039A, &[0xC9, 0x34, 0x12]); // MOV $1234,X
        bus.set(0x1234, 0x00);
        cpu.load_state_for_processor_test(
            0x00,
            0x66,
            0x00,
            0xEF,
            0x039A,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x039D);
        assert_eq!(bus.get(0x1234), 0x66);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_abs_y_stores_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x039D, &[0xCC, 0x35, 0x12]); // MOV $1235,Y
        bus.set(0x1235, 0x00);
        cpu.load_state_for_processor_test(
            0x00,
            0x00,
            0x77,
            0xEF,
            0x039D,
            FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x03A0);
        assert_eq!(bus.get(0x1235), 0x77);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_a_indirect_dp_plus_x_loads_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x03A0, &[0xE7, 0x80]); // MOV A,[$80+X]
        bus.set(0x0182, 0x34); // pointer low ($80 + X=2)
        bus.set(0x0183, 0x12); // pointer high
        bus.set(0x1234, 0x80);
        cpu.load_state_for_processor_test(
            0x00,
            0x02,
            0x00,
            0xEF,
            0x03A0,
            FLAG_DIRECT_PAGE | FLAG_CARRY,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.pc(), 0x03A2);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_a_indirect_dp_plus_y_loads_and_sets_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x03A2, &[0xF7, 0x84]); // MOV A,[$84]+Y
        bus.set(0x0184, 0x34);
        bus.set(0x0185, 0x12);
        bus.set(0x1236, 0x00); // Y=2
        cpu.load_state_for_processor_test(
            0xFF,
            0x00,
            0x02,
            0xEF,
            0x03A2,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.pc(), 0x03A4);
        assert_eq!(cpu.a(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_indirect_dp_plus_x_a_stores_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x03A4, &[0xC7, 0x88]); // MOV [$88+X],A
        bus.set(0x018A, 0x40); // $88 + X(2) = $8A
        bus.set(0x018B, 0x12);
        bus.set(0x1240, 0x00);
        cpu.load_state_for_processor_test(
            0x66,
            0x02,
            0x00,
            0xEF,
            0x03A4,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 7);
        assert_eq!(cpu.pc(), 0x03A6);
        assert_eq!(bus.get(0x1240), 0x66);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn mov_indirect_dp_plus_y_a_stores_and_preserves_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x03A6, &[0xD7, 0x8C]); // MOV [$8C]+Y,A
        bus.set(0x018C, 0x40);
        bus.set(0x018D, 0x12);
        bus.set(0x1242, 0x00); // base $1240 + Y(2)
        cpu.load_state_for_processor_test(
            0x77,
            0x00,
            0x02,
            0xEF,
            0x03A6,
            FLAG_DIRECT_PAGE | FLAG_CARRY | FLAG_NEGATIVE,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 7);
        assert_eq!(cpu.pc(), 0x03A8);
        assert_eq!(bus.get(0x1242), 0x77);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn inc_a_increments_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xBC]); // INC A
        cpu.load_state_for_processor_test(0x7F, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0201);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn inc_a_wraps_and_sets_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xBC]); // INC A
        cpu.load_state_for_processor_test(0xFF, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.a(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn dec_a_decrements_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x9C]); // DEC A
        cpu.load_state_for_processor_test(0x80, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0201);
        assert_eq!(cpu.a(), 0x7F);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn dec_a_underflows_and_sets_negative() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x9C]); // DEC A
        cpu.load_state_for_processor_test(0x00, 0, 0, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0xFF);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn rol_a_rotates_through_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x3C]); // ROL A
        cpu.load_state_for_processor_test(0x80, 0, 0, 0xEF, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.a(), 0x01);
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn ror_a_rotates_through_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x7C]); // ROR A
        cpu.load_state_for_processor_test(0x01, 0, 0, 0xEF, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn inc_dp_increments_memory_and_sets_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xAB, 0x20]); // INC $20
        bus.set(0x0020, 0xFF);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(bus.get(0x0020), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn dec_abs_decrements_memory_and_sets_negative() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x8C, 0x00, 0x30]); // DEC $3000
        bus.set(0x3000, 0x00);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.get(0x3000), 0xFF);
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn inc_dp_x_wraps_and_uses_direct_page_flag() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xBB, 0xF0]); // INC $F0+X
        bus.set(0x0110, 0x7F); // ($F0 + $20) in page 1
        cpu.load_state_for_processor_test(0x00, 0x20, 0x00, 0xEF, 0x0200, FLAG_DIRECT_PAGE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.get(0x0110), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn asl_dp_shifts_memory_and_sets_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x0B, 0x22]); // ASL $22
        bus.set(0x0022, 0x81);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(bus.get(0x0022), 0x02);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn rol_dp_x_rotates_memory_through_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x3B, 0x20]); // ROL $20+X
        bus.set(0x0023, 0x80);
        cpu.load_state_for_processor_test(0x00, 0x03, 0x00, 0xEF, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.get(0x0023), 0x01);
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn lsr_abs_shifts_memory_right() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x4C, 0x34, 0x12]); // LSR $1234
        bus.set(0x1234, 0x03);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.get(0x1234), 0x01);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn ror_dp_rotates_memory_right_through_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x6B, 0x44]); // ROR $44
        bus.set(0x0044, 0x01);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(bus.get(0x0044), 0x80);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn and_a_imm_bitwise_and() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x24, 0x0F]); // AND A,#$0F
        cpu.load_state_for_processor_test(0xFF, 0, 0, 0xEF, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0202);
        assert_eq!(cpu.a(), 0x0F);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn and_a_imm_results_in_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x24, 0x00]); // AND A,#$00
        cpu.load_state_for_processor_test(0xFF, 0, 0, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn or_a_imm_bitwise_or() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x04, 0x0F]); // OR A,#$0F
        cpu.load_state_for_processor_test(0xF0, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0202);
        assert_eq!(cpu.a(), 0xFF);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn or_a_imm_zero_result() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x04, 0x00]); // OR A,#$00
        cpu.load_state_for_processor_test(0x00, 0, 0, 0xEF, 0x0200, FLAG_NEGATIVE);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn eor_a_imm_bitwise_xor() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x44, 0xFF]); // EOR A,#$FF
        cpu.load_state_for_processor_test(0x0F, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0202);
        assert_eq!(cpu.a(), 0xF0);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn eor_a_imm_zero_result() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x44, 0x55]); // EOR A,#$55
        cpu.load_state_for_processor_test(0x55, 0, 0, 0xEF, 0x0200, FLAG_NEGATIVE);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn add_a_imm_simple_no_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x88, 0x10]); // ADD A,#$10
        cpu.load_state_for_processor_test(0x20, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0202);
        assert_eq!(cpu.a(), 0x30);
        assert!(!cpu.flag(FLAG_CARRY));
        assert!(!cpu.flag(FLAG_OVERFLOW));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn add_a_imm_with_carry_out() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x88, 0x01]); // ADD A,#$01
        cpu.load_state_for_processor_test(0xFF, 0, 0, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0x00);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(!cpu.flag(FLAG_OVERFLOW));
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn add_a_imm_with_overflow() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x88, 0x02]); // ADD A,#$02
        cpu.load_state_for_processor_test(0x7F, 0, 0, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0x81);
        assert!(!cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_OVERFLOW));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn adc_a_imm_with_carry_flag_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x88, 0x10]); // ADC A,#$10
        cpu.load_state_for_processor_test(0x20, 0, 0, 0xEF, 0x0200, FLAG_CARRY);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0x31);
        assert!(!cpu.flag(FLAG_CARRY));
        assert!(!cpu.flag(FLAG_OVERFLOW));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn adc_a_dp_uses_direct_page_operand_not_immediate() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x84, 0x10]); // ADC A,$10
        bus.set(0x0010, 0x05);
        cpu.load_state_for_processor_test(0x01, 0, 0, 0xEF, 0x0200, FLAG_CARRY);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0x07);
        assert!(!cpu.flag(FLAG_CARRY));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn sub_a_imm_simple_no_borrow() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xA8, 0x10]); // SUB A,#$10
        cpu.load_state_for_processor_test(0x30, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0202);
        assert_eq!(cpu.a(), 0x20);
        assert!(cpu.flag(FLAG_CARRY)); // Carry is set on no borrow
        assert!(!cpu.flag(FLAG_OVERFLOW));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn sub_a_imm_with_borrow() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xA8, 0x10]); // SUB A,#$10
        cpu.load_state_for_processor_test(0x05, 0, 0, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0xF5);
        assert!(!cpu.flag(FLAG_CARRY)); // Carry is clear on borrow
        assert!(!cpu.flag(FLAG_OVERFLOW));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn cmp_a_imm_equal_sets_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x68, 0x55]); // CMP A,#$55
        cpu.load_state_for_processor_test(0x55, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0202);
        assert_eq!(cpu.a(), 0x55); // A unchanged
        assert!(cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY)); // No borrow
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn cmp_a_imm_less_sets_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x68, 0x50]); // CMP A,#$50
        cpu.load_state_for_processor_test(0x40, 0, 0, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0x40); // A unchanged
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(!cpu.flag(FLAG_CARRY)); // Borrow occurred
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn cmp_x_imm_comparison() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xC8, 0x30]); // CMP X,#$30
        cpu.load_state_for_processor_test(0x00, 0x30, 0x00, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.x(), 0x30); // X unchanged
        assert!(cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn di_opcode_clears_interrupt_flag() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xC0]); // DI
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0200, FLAG_INTERRUPT);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.pc(), 0x0201);
        assert!(!cpu.flag(FLAG_INTERRUPT));
    }

    #[test]
    fn cmp_y_imm_comparison() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xAD, 0x40]); // CMP Y,#$40
        cpu.load_state_for_processor_test(0x00, 0x00, 0x40, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.y(), 0x40); // Y unchanged
        assert!(cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn bra_always_branches_by_relative_offset() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0100, &[0x2F, 0x10]); // BRA +$10
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0100, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0112);
    }

    #[test]
    fn beq_branches_when_zero_flag_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xF0, 0x10]); // BEQ rel +16
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0200, FLAG_ZERO);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0212); // 0x0200 + 2 (pc after fetch) + 0x10
    }

    #[test]
    fn beq_does_not_branch_when_zero_flag_clear() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xF0, 0x10]); // BEQ rel +16
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0200, 0); // Zero flag clear

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0202); // Just advance past opcode + operand
    }

    #[test]
    fn bne_branches_when_zero_flag_clear() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0300, &[0xD0, 0xFE]); // BNE rel -2
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0300, 0); // Zero flag clear

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0300); // 0x0300 + 2 + (-2) = 0x0300
    }

    #[test]
    fn bne_does_not_branch_when_zero_flag_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0300, &[0xD0, 0xFE]); // BNE rel -2
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0300, FLAG_ZERO);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0302); // Just advance past opcode + operand
    }

    #[test]
    fn bcs_branches_when_carry_flag_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0400, &[0xB0, 0x20]); // BCS rel +32
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0400, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0422); // 0x0400 + 2 + 0x20
    }

    #[test]
    fn bcc_branches_when_carry_flag_clear() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0500, &[0x90, 0x30]); // BCC rel +48
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0500, 0); // Carry flag clear

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0532); // 0x0500 + 2 + 0x30
    }

    #[test]
    fn bcc_does_not_branch_when_carry_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0500, &[0x90, 0x30]); // BCC rel +48
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0500, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0502); // Just advance past opcode + operand
    }

    #[test]
    fn bvs_branches_when_overflow_flag_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0600, &[0x70, 0x10]); // BVS rel +16
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0600, FLAG_OVERFLOW);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0612); // 0x0600 + 2 + 0x10
    }

    #[test]
    fn bvc_branches_when_overflow_flag_clear() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0700, &[0x50, 0x20]); // BVC rel +32
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0700, 0); // Overflow clear

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0722); // 0x0700 + 2 + 0x20
    }

    #[test]
    fn bmi_branches_when_negative_flag_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0800, &[0x30, 0x08]); // BMI rel +8
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0800, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x080A); // 0x0800 + 2 + 0x08
    }

    #[test]
    fn bpl_branches_when_negative_flag_clear() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0900, &[0x10, 0x40]); // BPL rel +64
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0900, 0); // Negative clear

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.pc(), 0x0942); // 0x0900 + 2 + 0x40
    }

    #[test]
    fn bmi_does_not_branch_when_negative_clear() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0800, &[0x30, 0x08]); // BMI rel +8
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0800, 0); // Negative clear

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0802); // Just advance past opcode + operand
    }

    #[test]
    fn jmp_abs_loads_pc_from_absolute_operand() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x5F, 0x34, 0x12]); // JMP $1234
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.pc(), 0x1234);
    }

    #[test]
    fn jmp_indirect_abs_x_reads_pointer_target() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x1F, 0x00, 0x20]); // JMP [$2000+X]
        bus.set(0x2002, 0x78);
        bus.set(0x2003, 0x56);
        cpu.load_state_for_processor_test(0x00, 0x02, 0x00, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.pc(), 0x5678);
    }

    #[test]
    fn cbne_dp_branches_when_a_differs() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x2E, 0x20, 0x10]); // CBNE $20,+$10
        bus.set(0x0020, 0x33);
        cpu.load_state_for_processor_test(0x44, 0x00, 0x00, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 7);
        assert_eq!(cpu.pc(), 0x0213);
    }

    #[test]
    fn cbne_dp_x_not_taken_when_equal() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xDE, 0x20, 0x10]); // CBNE $20+X,+$10
        bus.set(0x0023, 0x44);
        cpu.load_state_for_processor_test(0x44, 0x03, 0x00, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.pc(), 0x0203);
    }

    #[test]
    fn dbnz_y_branches_until_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0300, &[0xFE, 0xFC]); // DBNZ Y,-4
        cpu.load_state_for_processor_test(0x00, 0x00, 0x02, 0xEF, 0x0300, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.y(), 0x01);
        assert_eq!(cpu.pc(), 0x02FE);
    }

    #[test]
    fn dbnz_dp_not_taken_when_result_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0400, &[0x6E, 0x40, 0x20]); // DBNZ $40,+$20
        bus.set(0x0040, 0x01);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x0400, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.get(0x0040), 0x00);
        assert_eq!(cpu.pc(), 0x0403);
    }

    #[test]
    fn push_psw_pushes_flags_onto_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x0D]); // PUSH PSW
        cpu.load_state_for_processor_test(0xAA, 0xBB, 0xCC, 0xF0, 0x0200, FLAG_ZERO | FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xEF); // SP decremented
        assert_eq!(bus.read(0x01F0), FLAG_ZERO | FLAG_CARRY); // PSW pushed to stack
    }

    #[test]
    fn push_a_pushes_accumulator_onto_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x2D]); // PUSH A
        cpu.load_state_for_processor_test(0x42, 0xBB, 0xCC, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xEF); // SP decremented
        assert_eq!(cpu.a(), 0x42); // A unchanged
        assert_eq!(bus.read(0x01F0), 0x42); // A pushed to stack
    }

    #[test]
    fn push_x_pushes_x_onto_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x4D]); // PUSH X
        cpu.load_state_for_processor_test(0xAA, 0x55, 0xCC, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xEF); // SP decremented
        assert_eq!(cpu.x(), 0x55); // X unchanged
        assert_eq!(bus.read(0x01F0), 0x55); // X pushed to stack
    }

    #[test]
    fn push_y_pushes_y_onto_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x6D]); // PUSH Y
        cpu.load_state_for_processor_test(0xAA, 0xBB, 0x77, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xEF); // SP decremented
        assert_eq!(cpu.y(), 0x77); // Y unchanged
        assert_eq!(bus.read(0x01F0), 0x77); // Y pushed to stack
    }

    #[test]
    fn pop_psw_pops_flags_from_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x8E]); // POP PSW
        bus.write(0x01F0, FLAG_NEGATIVE | FLAG_OVERFLOW); // Stack contains flags
        cpu.load_state_for_processor_test(0xAA, 0xBB, 0xCC, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xF0); // SP incremented
        assert_eq!(cpu.psw(), FLAG_NEGATIVE | FLAG_OVERFLOW); // PSW loaded from stack
    }

    #[test]
    fn pop_a_pops_accumulator_from_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xAE]); // POP A
        bus.write(0x01F0, 0x88); // Stack contains value
        cpu.load_state_for_processor_test(0x00, 0xBB, 0xCC, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xF0); // SP incremented
        assert_eq!(cpu.a(), 0x88); // A loaded from stack
        assert!(cpu.flag(FLAG_NEGATIVE)); // N flag set (0x88 is negative)
    }

    #[test]
    fn pop_x_pops_x_from_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xCE]); // POP X
        bus.write(0x01F0, 0x44); // Stack contains value
        cpu.load_state_for_processor_test(0xAA, 0x00, 0xCC, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xF0); // SP incremented
        assert_eq!(cpu.x(), 0x44); // X loaded from stack
        assert!(!cpu.flag(FLAG_NEGATIVE)); // N flag clear (0x44 is positive)
    }

    #[test]
    fn pop_y_pops_y_from_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xEE]); // POP Y
        bus.write(0x01F0, 0x99); // Stack contains value
        cpu.load_state_for_processor_test(0xAA, 0xBB, 0x00, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xF0); // SP incremented
        assert_eq!(cpu.y(), 0x99); // Y loaded from stack
        assert!(cpu.flag(FLAG_NEGATIVE)); // N flag set (0x99 is negative)
    }

    #[test]
    fn call_pushes_return_address_and_jumps() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x3F, 0x34, 0x12]); // CALL $1234
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 8);
        assert_eq!(cpu.pc(), 0x1234); // Jumped to target
        assert_eq!(cpu.sp(), 0xEE); // SP decremented by 2
        assert_eq!(bus.read(0x01F0), 0x02); // High byte of return address
        assert_eq!(bus.read(0x01EF), 0x03); // Low byte of return address
    }

    #[test]
    fn brk_pushes_pc_and_psw_then_jumps_to_vector() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x2000, &[0x0F, 0xAA]); // BRK + padding byte
        bus.set(0xFFDE, 0x78);
        bus.set(0xFFDF, 0x56);
        cpu.load_state_for_processor_test(
            0x00,
            0x00,
            0x00,
            0xF0,
            0x2000,
            FLAG_INTERRUPT | FLAG_CARRY,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 8);
        assert_eq!(cpu.pc(), 0x5678);
        assert_eq!(cpu.sp(), 0xED);
        assert_eq!(bus.read(0x01F0), 0x20); // pushed PCH of return address ($2001)
        assert_eq!(bus.read(0x01EF), 0x01); // pushed PCL of return address
        assert_eq!(bus.read(0x01EE), FLAG_INTERRUPT | FLAG_CARRY); // pushed original PSW
        assert!(cpu.flag(FLAG_BREAK));
        assert!(!cpu.flag(FLAG_INTERRUPT));
    }

    #[test]
    fn rts_pops_return_address_and_jumps() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x6F]); // RTS
        bus.write(0x01EF, 0x03); // High byte of return address
        bus.write(0x01EE, 0x40); // Low byte of return address (0x0340)
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xED, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x0340); // Returned to stored address
        assert_eq!(cpu.sp(), 0xEF); // SP incremented by 2
    }

    #[test]
    fn incw_dp_increments_16bit_word_in_memory() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x3A, 0x20]); // INCW $20
        bus.set(0x0020, 0xFF);
        bus.set(0x0021, 0x00);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0020), 0x00);
        assert_eq!(bus.read(0x0021), 0x01);
        assert!(!cpu.flag(FLAG_ZERO)); // N/Z based on Y (0x01)
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn incw_dp_sets_negative_flag_from_high_byte() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x3A, 0x20]); // INCW $20
        bus.set(0x0020, 0xFF);
        bus.set(0x0021, 0x7F);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0020), 0x00);
        assert_eq!(bus.read(0x0021), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE)); // N flag set (0x80 is negative)
    }

    #[test]
    fn decw_dp_decrements_16bit_word_in_memory() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x1A, 0x20]); // DECW $20
        bus.set(0x0020, 0x00);
        bus.set(0x0021, 0x00);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0020), 0xFF);
        assert_eq!(bus.read(0x0021), 0xFF);
        assert!(cpu.flag(FLAG_NEGATIVE)); // N flag set (0xFF is negative)
    }

    #[test]
    fn decw_dp_normal_decrement() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x1A, 0x20]); // DECW $20
        bus.set(0x0020, 0x00);
        bus.set(0x0021, 0x80);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0020), 0xFF);
        assert_eq!(bus.read(0x0021), 0x7F);
        assert!(!cpu.flag(FLAG_NEGATIVE)); // N flag clear (0x7F is positive)
    }

    #[test]
    fn addw_ya_simple_addition() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x7A, 0x20]); // ADDW YA,$20
        bus.set(0x0020, 0x34);
        bus.set(0x0021, 0x12);
        cpu.load_state_for_processor_test(0x01, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.a(), 0x35); // Low byte
        assert_eq!(cpu.y(), 0x12); // High byte
        assert!(!cpu.flag(FLAG_CARRY)); // No carry
        assert!(!cpu.flag(FLAG_OVERFLOW)); // No overflow
    }

    #[test]
    fn addw_ya_with_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x7A, 0x20]); // ADDW YA,$20
        bus.set(0x0020, 0x00);
        bus.set(0x0021, 0xFF);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x01, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.a(), 0x00); // Low byte wrapped
        assert_eq!(cpu.y(), 0x00); // High byte wrapped
        assert!(cpu.flag(FLAG_CARRY)); // Carry flag set
    }

    #[test]
    fn subw_ya_simple_subtraction() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x9A, 0x20]); // SUBW YA,$20
        bus.set(0x0020, 0x01);
        bus.set(0x0021, 0x00);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x01, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.a(), 0xFF); // Low byte
        assert_eq!(cpu.y(), 0x00); // High byte
        assert!(cpu.flag(FLAG_CARRY)); // No borrow
    }

    #[test]
    fn subw_ya_with_borrow() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x9A, 0x20]); // SUBW YA,$20
        bus.set(0x0020, 0x00);
        bus.set(0x0021, 0x01);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.a(), 0x00); // Low byte
        assert_eq!(cpu.y(), 0xFF); // High byte wrapped
        assert!(!cpu.flag(FLAG_CARRY)); // Borrow occurred
    }

    #[test]
    fn cmpw_ya_equal_comparison() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x5A, 0x20]); // CMPW YA,$20
        bus.set(0x0020, 0x34);
        bus.set(0x0021, 0x12);
        cpu.load_state_for_processor_test(0x34, 0x00, 0x12, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert!(cpu.flag(FLAG_ZERO)); // Zero flag set (result is 0)
        assert!(cpu.flag(FLAG_CARRY)); // No borrow
    }

    #[test]
    fn cmpw_ya_less_than_comparison() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x5A, 0x20]); // CMPW YA,$20
        bus.set(0x0020, 0x00);
        bus.set(0x0021, 0x01);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert!(!cpu.flag(FLAG_CARRY)); // Borrow occurred (YA < memory word)
    }

    #[test]
    fn mul_ya_simple_multiplication() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xCF]); // MUL YA
        // Y = 0x04, A = 0x05, result = 0x0014 (4 * 5 = 20)
        cpu.load_state_for_processor_test(0x05, 0x00, 0x04, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 9);
        assert_eq!(cpu.a(), 0x14); // Low byte of product
        assert_eq!(cpu.y(), 0x00); // High byte of product
    }

    #[test]
    fn mul_ya_larger_product() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xCF]); // MUL YA
        // Y = 0x12, A = 0x34, result = 0x03A8 (18 * 52 = 936)
        cpu.load_state_for_processor_test(0x34, 0x00, 0x12, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 9);
        assert_eq!(cpu.a(), 0xA8); // Low byte of product
        assert_eq!(cpu.y(), 0x03); // High byte of product
        assert!(!cpu.flag(FLAG_NEGATIVE)); // High byte (0x03) is positive
        assert!(!cpu.flag(FLAG_ZERO)); // Result is not zero
    }

    #[test]
    fn div_ya_simple_division() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x9E]); // DIV YA,X
        // YA = 0x0014 (20), X = 4, quotient = 5, remainder = 0
        cpu.load_state_for_processor_test(0x14, 0x04, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 12);
        assert_eq!(cpu.a(), 0x05); // Quotient
        assert_eq!(cpu.y(), 0x00); // Remainder
        assert!(!cpu.flag(FLAG_OVERFLOW)); // No overflow (X != 0)
    }

    #[test]
    fn div_ya_with_remainder() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x9E]); // DIV YA,X
        // YA = 0x0017 (23), X = 5, quotient = 4, remainder = 3
        cpu.load_state_for_processor_test(0x17, 0x05, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 12);
        assert_eq!(cpu.a(), 0x04); // Quotient
        assert_eq!(cpu.y(), 0x03); // Remainder
        assert!(!cpu.flag(FLAG_OVERFLOW)); // No overflow (X != 0)
    }

    #[test]
    fn div_ya_division_by_zero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x9E]); // DIV YA,X
        // YA = 0x0100, X = 0 (division by zero)
        cpu.load_state_for_processor_test(0x00, 0x00, 0x01, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 12);
        assert!(cpu.flag(FLAG_OVERFLOW)); // Overflow flag set (division by zero error)
    }

    #[test]
    fn clrc_clears_carry_flag() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x60]); // CLRC
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, FLAG_CARRY | FLAG_ZERO);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert!(!cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn notc_flips_carry_flag() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xED]); // NOTC
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn clrv_clears_overflow_and_half_carry_only() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xE0]); // CLRV
        cpu.load_state_for_processor_test(
            0x00,
            0x00,
            0x00,
            0xF0,
            0x0200,
            FLAG_OVERFLOW | FLAG_HALF_CARRY | FLAG_NEGATIVE | FLAG_CARRY,
        );

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert!(!cpu.flag(FLAG_OVERFLOW));
        assert!(!cpu.flag(FLAG_HALF_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn set1_dp3_sets_only_target_bit() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x62, 0x20]); // SET1 $20.3
        bus.set(0x0020, 0b1000_0001);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, FLAG_ZERO);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(bus.read(0x0020), 0b1000_1001);
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn clr1_dp6_clears_only_target_bit() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xD2, 0x31]); // CLR1 $31.6
        bus.set(0x0031, 0b1111_1111);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(bus.read(0x0031), 0b1011_1111);
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn bbs0_branches_when_bit_is_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x03, 0x40, 0x10]); // BBS0 $40,+$10
        bus.set(0x0040, 0x01);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 7);
        assert_eq!(cpu.pc(), 0x0213);
    }

    #[test]
    fn bbc7_does_not_branch_when_bit_is_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xF3, 0x40, 0x10]); // BBC7 $40,+$10
        bus.set(0x0040, 0x80);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.pc(), 0x0203);
    }

    #[test]
    fn mov_dp_imm_writes_immediate_value() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x8F, 0x5A, 0x20]); // MOV $20,#$5A
        bus.set(0x0020, 0x00);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0020), 0x5A);
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn movw_ya_dp_loads_word_and_sets_negative_from_high_byte() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xBA, 0x30]); // MOVW YA,$30
        bus.set(0x0030, 0x11);
        bus.set(0x0031, 0x80);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.a(), 0x11);
        assert_eq!(cpu.y(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn movw_dp_ya_stores_word_low_then_high() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xDA, 0x40]); // MOVW $40,YA
        cpu.load_state_for_processor_test(0x34, 0x00, 0x12, 0xF0, 0x0200, FLAG_ZERO);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0040), 0x34);
        assert_eq!(bus.read(0x0041), 0x12);
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn xcn_a_swaps_nibbles_and_updates_nz() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x9F]); // XCN A
        cpu.load_state_for_processor_test(0xF0, 0x00, 0x00, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.a(), 0x0F);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn ei_sets_interrupt_flag() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xA0]); // EI
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert!(cpu.flag(FLAG_INTERRUPT));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn pcall_pushes_return_address_and_jumps_to_ff_page() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x2000, &[0x4F, 0x34]); // PCALL $34
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x2000, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.pc(), 0xFF34);
        assert_eq!(cpu.sp(), 0xEE);
        assert_eq!(bus.read(0x01F0), 0x20); // return PCH ($2002)
        assert_eq!(bus.read(0x01EF), 0x02); // return PCL
    }

    #[test]
    fn tcall0_uses_ffde_vector() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x3000, &[0x01]); // TCALL 0
        bus.set(0xFFDE, 0x78);
        bus.set(0xFFDF, 0x56);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x3000, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 8);
        assert_eq!(cpu.pc(), 0x5678);
        assert_eq!(cpu.sp(), 0xED);
        assert_eq!(bus.read(0x01EF), 0x30); // return PCH ($3001)
        assert_eq!(bus.read(0x01EE), 0x01); // return PCL
    }

    #[test]
    fn tcall15_uses_ffc0_vector() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x3000, &[0xF1]); // TCALL 15
        bus.set(0xFFC0, 0xCD);
        bus.set(0xFFC1, 0xAB);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xEF, 0x3000, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 8);
        assert_eq!(cpu.pc(), 0xABCD);
    }

    #[test]
    fn reti_restores_psw_and_pc_from_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x4000, &[0x7F]); // RETI
        bus.write(0x01EE, FLAG_NEGATIVE | FLAG_CARRY); // PSW
        bus.write(0x01EF, 0x34); // PCL
        bus.write(0x01F0, 0x12); // PCH
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xED, 0x4000, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.psw(), FLAG_NEGATIVE | FLAG_CARRY);
        assert_eq!(cpu.pc(), 0x1234);
        assert_eq!(cpu.sp(), 0xF0);
    }

    #[test]
    fn mov1_c_mem_bit_loads_carry_from_selected_bit() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        // MOV1 C,$0123.5 => low=0x23, high=(bit<<5)|(addr>>8)=0xA1
        bus.load(0x0200, &[0xAA, 0x23, 0xA1]);
        bus.set(0x0123, 0b0010_0000);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov1_mem_bit_c_writes_selected_bit_from_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        // MOV1 $0456.3,C => low=0x56, high=(3<<5)|0x04 = 0x64
        bus.load(0x0200, &[0xCA, 0x56, 0x64]);
        bus.set(0x0456, 0x00);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0456), 0b0000_1000);
    }

    #[test]
    fn not1_mem_bit_toggles_selected_bit() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        // NOT1 $0456.0 => low=0x56, high=0x04
        bus.load(0x0200, &[0xEA, 0x56, 0x04]);
        bus.set(0x0456, 0xFE);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0456), 0xFF);
    }

    #[test]
    fn or1_c_not_mem_bit_sets_carry_when_bit_is_clear() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        // OR1 C,/$0120.2 => low=0x20, high=(2<<5)|0x01 = 0x41
        bus.load(0x0200, &[0x2A, 0x20, 0x41]);
        bus.set(0x0120, 0x00);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn tset1_abs_sets_bits_and_updates_nz_from_subtract() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x0E, 0x00, 0x30]); // TSET1 $3000
        bus.set(0x3000, 0b0000_1111);
        cpu.load_state_for_processor_test(0b1111_0000, 0, 0, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x3000), 0xFF);
        assert!(!cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn tclr1_abs_clears_bits_and_sets_zero_on_equal() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x4E, 0x00, 0x30]); // TCLR1 $3000
        bus.set(0x3000, 0x12);
        cpu.load_state_for_processor_test(0x12, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x3000), 0x00);
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn cmp_a_dp_compares_accumulator_with_direct_page_value() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x64, 0x20]); // CMP A,$20
        bus.set(0x0020, 0x40);
        cpu.load_state_for_processor_test(0x40, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn cmp_a_abs_y_reads_indexed_address_and_sets_negative_on_borrow() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x76, 0x00, 0x30]); // CMP A,$3000+Y
        bus.set(0x3002, 0x20);
        cpu.load_state_for_processor_test(0x10, 0, 0x02, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert!(!cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn cmp_a_indirect_dp_plus_x_follows_pointer() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x67, 0x40]); // CMP A,[$40+X]
        bus.set(0x0043, 0x00); // ptr low
        bus.set(0x0044, 0x30); // ptr high -> $3000
        bus.set(0x3000, 0x22);
        cpu.load_state_for_processor_test(0x22, 0x03, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn cmp_x_abs_uses_absolute_operand() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x1E, 0x34, 0x12]); // CMP X,$1234
        bus.set(0x1234, 0x10);
        cpu.load_state_for_processor_test(0x00, 0x20, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn cmp_y_dp_uses_direct_page_base_flag() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x7E, 0x80]); // CMP Y,$80
        bus.set(0x0180, 0x10);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x10, 0xF0, 0x0200, FLAG_DIRECT_PAGE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn cmp_indirect_x_y_compares_memory_values() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x79]); // CMP (X),(Y)
        bus.set(0x0010, 0x44);
        bus.set(0x0011, 0x44);
        cpu.load_state_for_processor_test(0x00, 0x10, 0x11, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn or_a_imm_canonical_opcode_08_updates_accumulator() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x08, 0x0F]); // OR A,#$0F
        cpu.load_state_for_processor_test(0xF0, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.a(), 0xFF);
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn and_a_abs_canonical_opcode_25_masks_accumulator() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x25, 0x00, 0x30]); // AND A,$3000
        bus.set(0x3000, 0x0F);
        cpu.load_state_for_processor_test(0xF3, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.a(), 0x03);
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn eor_a_indirect_dp_plus_y_canonical_opcode_57() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x57, 0x40]); // EOR A,[$40]+Y
        bus.set(0x0040, 0x00);
        bus.set(0x0041, 0x30); // pointer = $3000
        bus.set(0x3002, 0xFF); // +Y
        cpu.load_state_for_processor_test(0x55, 0x00, 0x02, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.a(), 0xAA);
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn or_x_y_memory_opcode_19_writes_back_to_x_location() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x19]); // OR (X),(Y)
        bus.set(0x0010, 0x0F);
        bus.set(0x0020, 0xF0);
        cpu.load_state_for_processor_test(0x00, 0x10, 0x20, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0010), 0xFF);
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn and_x_y_memory_opcode_39_writes_back_to_x_location() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x39]); // AND (X),(Y)
        bus.set(0x0010, 0x3C);
        bus.set(0x0020, 0x0F);
        cpu.load_state_for_processor_test(0x00, 0x10, 0x20, 0xF0, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0010), 0x0C);
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn eor_x_y_memory_opcode_59_writes_back_to_x_location() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x59]); // EOR (X),(Y)
        bus.set(0x0010, 0xAA);
        bus.set(0x0020, 0xFF);
        cpu.load_state_for_processor_test(0x00, 0x10, 0x20, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0010), 0x55);
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn or_dp_dp_opcode_09_combines_source_into_destination() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x09, 0x20, 0x10]); // OR $10,$20
        bus.set(0x0010, 0x0F);
        bus.set(0x0020, 0xF0);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0010), 0xFF);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn and_dp_dp_opcode_29_masks_destination_with_source() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x29, 0x20, 0x10]); // AND $10,$20
        bus.set(0x0010, 0x3C);
        bus.set(0x0020, 0x0F);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0010), 0x0C);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn eor_dp_dp_opcode_49_xors_source_into_destination() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x49, 0x20, 0x10]); // EOR $10,$20
        bus.set(0x0010, 0xAA);
        bus.set(0x0020, 0xFF);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0010), 0x55);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn or_dp_imm_opcode_18_updates_destination_and_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x18, 0xF0, 0x10]); // OR $10,#$F0
        bus.set(0x0010, 0x0F);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0010), 0xFF);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn and_dp_imm_opcode_38_can_set_zero_flag() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x38, 0x0F, 0x10]); // AND $10,#$0F
        bus.set(0x0010, 0xF0);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0010), 0x00);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn eor_dp_imm_opcode_58_updates_destination_and_flags() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x58, 0xFF, 0x10]); // EOR $10,#$FF
        bus.set(0x0010, 0x0F);
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0010), 0xF0);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn cmp_dp_dp_opcode_69_compares_destination_against_source() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x69, 0x20, 0x10]); // CMP $10,$20
        bus.set(0x0010, 0x44);
        bus.set(0x0020, 0x44);
        cpu.load_state_for_processor_test(0xAB, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert!(cpu.flag(FLAG_ZERO));
        assert!(cpu.flag(FLAG_CARRY));
        assert_eq!(cpu.a(), 0xAB);
    }

    #[test]
    fn cmp_dp_imm_opcode_78_sets_negative_when_destination_is_less() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x78, 0x20, 0x10]); // CMP $10,#$20
        bus.set(0x0010, 0x10);
        cpu.load_state_for_processor_test(0xAB, 0, 0, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert!(!cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
        assert_eq!(cpu.a(), 0xAB);
    }

    #[test]
    fn adc_dp_dp_opcode_89_writes_result_to_destination() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x89, 0x20, 0x10]); // ADC $10,$20
        bus.set(0x0010, 0x0F);
        bus.set(0x0020, 0x01);
        cpu.load_state_for_processor_test(0x80, 0, 0, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0010), 0x11);
        assert!(!cpu.flag(FLAG_CARRY));
        assert_eq!(cpu.a(), 0x80);
    }

    #[test]
    fn adc_dp_imm_opcode_98_sets_zero_and_carry_on_overflow() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x98, 0x00, 0x10]); // ADC $10,#$00
        bus.set(0x0010, 0xFF);
        cpu.load_state_for_processor_test(0x11, 0, 0, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0010), 0x00);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_ZERO));
        assert_eq!(cpu.a(), 0x11);
    }

    #[test]
    fn sbc_dp_dp_opcode_a9_writes_result_to_destination() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xA9, 0x20, 0x10]); // SBC $10,$20
        bus.set(0x0010, 0x10);
        bus.set(0x0020, 0x01);
        cpu.load_state_for_processor_test(0x80, 0, 0, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0010), 0x0F);
        assert!(cpu.flag(FLAG_CARRY));
        assert_eq!(cpu.a(), 0x80);
    }

    #[test]
    fn sbc_dp_imm_opcode_b8_applies_borrow_when_carry_is_clear() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xB8, 0x00, 0x10]); // SBC $10,#$00
        bus.set(0x0010, 0x00);
        cpu.load_state_for_processor_test(0x11, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0010), 0xFF);
        assert!(!cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert_eq!(cpu.a(), 0x11);
    }

    #[test]
    fn adc_a_abs_opcode_85_adds_memory_into_accumulator_with_carry() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x85, 0x00, 0x30]); // ADC A,$3000
        bus.set(0x3000, 0x0F);
        cpu.load_state_for_processor_test(0x70, 0, 0, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.a(), 0x80);
        assert!(cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn adc_x_y_memory_opcode_99_writes_sum_back_to_x_location() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x99]); // ADC (X),(Y)
        bus.set(0x0010, 0x01);
        bus.set(0x0020, 0x01);
        cpu.load_state_for_processor_test(0xCC, 0x10, 0x20, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(bus.read(0x0010), 0x03);
        assert_eq!(cpu.a(), 0xCC);
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn sbc_a_indirect_dp_plus_y_opcode_b7_subtracts_pointed_value() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xB7, 0x40]); // SBC A,[$40]+Y
        bus.set(0x0040, 0x00);
        bus.set(0x0041, 0x30); // pointer = $3000
        bus.set(0x3002, 0x10); // +Y
        cpu.load_state_for_processor_test(0x22, 0, 0x02, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(cpu.a(), 0x12);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn sbc_x_y_memory_opcode_b9_writes_difference_back_to_x_location() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xB9]); // SBC (X),(Y)
        bus.set(0x0010, 0x10);
        bus.set(0x0020, 0x01);
        cpu.load_state_for_processor_test(0xCC, 0x10, 0x20, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(bus.read(0x0010), 0x0F);
        assert_eq!(cpu.a(), 0xCC);
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn mov_dp_dp_opcode_fa_copies_source_to_destination() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xFA, 0x20, 0x10]); // MOV $10,$20
        bus.set(0x0010, 0x00);
        bus.set(0x0020, 0xA5);
        cpu.load_state_for_processor_test(0x11, 0x22, 0x33, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(bus.read(0x0010), 0xA5);
        assert_eq!(cpu.a(), 0x11);
        assert!(cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn daa_opcode_df_bcd_adjusts_accumulator_after_addition() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xDF]); // DAA A
        cpu.load_state_for_processor_test(0x9A, 0, 0, 0xF0, 0x0200, FLAG_HALF_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.a(), 0x00);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn das_opcode_be_bcd_adjusts_accumulator_after_subtraction() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xBE]); // DAS A
        cpu.load_state_for_processor_test(0x0F, 0, 0, 0xF0, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.a(), 0x09);
        assert!(cpu.flag(FLAG_CARRY));
        assert!(!cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn sleep_opcode_ef_halts_cpu_and_freezes_pc() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xEF]); // SLEEP
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, 0);

        let first_cycles = cpu.step(&mut bus);
        let pc_after_sleep = cpu.pc();
        let second_cycles = cpu.step(&mut bus);

        assert_eq!(first_cycles, 1);
        assert_eq!(second_cycles, 1);
        assert_eq!(bus.cycles(), 2);
        assert_eq!(pc_after_sleep, cpu.pc());
    }

    #[test]
    fn stop_opcode_ff_halts_cpu_and_freezes_pc() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xFF]); // STOP
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, 0);

        let first_cycles = cpu.step(&mut bus);
        let pc_after_stop = cpu.pc();
        let second_cycles = cpu.step(&mut bus);

        assert_eq!(first_cycles, 1);
        assert_eq!(second_cycles, 1);
        assert_eq!(bus.cycles(), 2);
        assert_eq!(pc_after_stop, cpu.pc());
    }

    #[test]
    fn halted_step_uses_bus_idle_cycle_cost() {
        let mut cpu = Spc700::new();
        let mut bus = VariableIdleBus::new(5);
        bus.load(0x0200, &[0xFF]); // STOP
        cpu.load_state_for_processor_test(0, 0, 0, 0xF0, 0x0200, 0);

        let first_cycles = cpu.step(&mut bus);
        let second_cycles = cpu.step(&mut bus);

        assert_eq!(first_cycles, 1);
        assert_eq!(second_cycles, 5);
        assert_eq!(bus.cycles(), 6);
    }
}
