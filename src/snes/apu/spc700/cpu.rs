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
/// counter and execute the next [`MicroOp`] of the opcode's cycle script,
/// consuming exactly one bus operation (read / write / idle) per call. When
/// the last micro-op runs, the [`Finish`] action applies the result and this
/// slot clears back to `None`.
#[derive(Debug, Clone, Default)]
pub(crate) struct InProgressOp {
    pub(crate) opcode: u8,
    /// Cycle index within the instruction. Cycle 1 is the opcode fetch (done
    /// when this struct is created); micro-ops run for cycles 2..N.
    pub(crate) cycle: u8,
    /// First operand byte fetched mid-instruction.
    pub(crate) operand: u8,
    /// Second operand byte (absolute-address high byte).
    pub(crate) operand2: u8,
    /// Effective address computed by an addressing micro-op.
    pub(crate) addr: u16,
    /// Value loaded by a read micro-op.
    pub(crate) value: u8,
    /// Set by a conditional micro-op to terminate the script after the
    /// current cycle (a not-taken branch is 2 cycles instead of 4).
    pub(crate) done_early: bool,
}

/// Index register applied by an addressing micro-op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Index {
    None,
    X,
    Y,
}

/// One bus operation of a cycle-scripted instruction (see
/// [`Spc700::cycle_script`]). Each micro-op performs exactly one bus access
/// (read, write, or idle), mirroring the bus-op sequence of the atomic
/// implementation in [`Spc700::step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MicroOp {
    /// Read the immediate byte at PC into `value` (PC += 1).
    ReadImm,
    /// Read the operand byte at PC into `operand` (PC += 1).
    FetchOperand,
    /// Read the second operand byte at PC into `operand2` (PC += 1).
    FetchOperand2,
    /// One internal idle cycle.
    Idle,
    /// Take the relative branch using `operand` and spend one idle cycle.
    BranchIdle,
    /// Compute `addr` = direct page | (`operand` + index) and read `value`.
    ReadDp(Index),
    /// Compute `addr` = direct page | (`operand` + index) and dummy-read it
    /// (the read-before-write cycle of store instructions).
    ReadDpForStore(Index),
    /// Compute `addr` = direct page | `operand2` and dummy-read it (the
    /// read-before-write cycle of `MOV dp,#imm`, whose dp byte is the
    /// second operand).
    ReadDp2ForStore,
    /// Compute `addr` = direct page | `operand2` and read `value` (the
    /// memory operand of `CMP dp,#imm`, whose dp byte is the second
    /// operand).
    ReadDp2,
    /// Read the high byte at direct page | (`operand` + 1) into `operand2`
    /// (the second read of `MOVW YA,dp`).
    ReadDpHigh,
    /// Dummy-read the opcode byte at PC without advancing it (the internal
    /// cycle of implied/register instructions).
    DummyReadPc,
    /// Fetch the branch displacement, then end the instruction unless the
    /// PSW flag in `.0` equals `.1` (conditional branches are 2 cycles when
    /// not taken, 4 when taken).
    FetchOperandBranch(u8, bool),
    /// Read-modify-write completion: transform `value` per the finish kind
    /// (with flag updates) and write the result back to `addr`.
    WriteRmw,
    /// Compute `addr` = direct page | X and read `value` (the `(X)`
    /// indirect operand).
    ReadAtX,
    /// Compute `addr` = direct page | X and dummy-read it (store form).
    ReadAtXForStore,
    /// Compute `addr` = direct page | X and write the finish's source
    /// register there (`MOV (X)+,A` has no read-before-write).
    WriteAtX,
    /// Compute `addr` = `operand2`:`operand` + index and read `value`
    /// (absolute indexed loads).
    ReadAbsIdx(Index),
    /// Compute `addr` = `operand2`:`operand` + index and dummy-read it
    /// (absolute indexed stores).
    ReadAbsIdxForStore(Index),
    /// Read direct page | `operand` and stash the byte back into `operand`
    /// (the source read of two-operand memory instructions).
    ReadDpSrcToOperand,
    /// Read direct page | Y and stash the byte into `operand` (the source
    /// read of `(X),(Y)` instructions).
    ReadAtYToOperand,
    /// Compute `addr` = direct page | `operand2` and write `operand` there
    /// without a read-before-write (`MOV dp,dp`).
    WriteDp2Operand,
    /// Fetch the branch displacement into `operand`, then end the
    /// instruction unless bit `.0` of `value` equals `.1` (BBS/BBC).
    FetchOperandBranchBit(u8, bool),
    /// Push the finish's source register onto the stack (SP decrements).
    PushReg,
    /// Pop a byte from the stack into `value` (SP increments first).
    PopByte,
    /// Write Y to direct page | (`operand` + 1) (the high write of
    /// `MOVW dp,YA`).
    WriteDpHighY,
    /// INCW/DECW low half: write `value` +/- 1 back to `addr` and keep the
    /// low result in `value` (`.0` = increment).
    WordRmwLoWrite(bool),
    /// INCW/DECW high half: write `operand2` +/- carry/borrow from the low
    /// result back to `addr`+1 and set N/Z from the 16-bit result
    /// (`.0` = increment).
    WordRmwHiWrite(bool),
    /// Fetch the branch displacement, then end the instruction if A equals
    /// `value` (CBNE).
    FetchOperandBranchANe,
    /// Write `value` - 1 back to `addr` without touching flags, keeping the
    /// result in `value` (DBNZ dp).
    WriteDecNoFlags,
    /// Fetch the branch displacement, then end the instruction if `value`
    /// is zero (DBNZ dp tail).
    FetchOperandBranchValueNz,
    /// Decrement Y, fetch the branch displacement, then end the instruction
    /// if Y is zero (DBNZ Y tail).
    FetchOperandBranchDecY,
    /// Compute `addr` = `operand2`:`operand` and read `value`.
    ReadAbs,
    /// Compute `addr` = `operand2`:`operand` and dummy-read it.
    ReadAbsForStore,
    /// Read the pointer low byte at direct page | (`operand` + index) into
    /// `addr` (low half).
    ReadPtrLo(Index),
    /// Read the pointer high byte at the following direct-page address.
    ReadPtrHi(Index),
    /// Read `value` from `addr` (+ Y for `Index::Y`).
    ReadTarget(Index),
    /// Write A/X/Y (per `Finish`) to `addr`.
    WriteReg,
}

/// Result action applied together with the final micro-op of a cycle script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Finish {
    /// No register result (branches, stores).
    None,
    /// `A = value`, update N/Z.
    MovA,
    /// `X = value`, update N/Z.
    MovX,
    /// `Y = value`, update N/Z.
    MovY,
    /// Compare A with `value`.
    CmpA,
    /// Compare X with `value`.
    CmpX,
    /// Compare Y with `value`.
    CmpY,
    /// Source register for [`MicroOp::WriteReg`] is A.
    StoreA,
    /// Source register for [`MicroOp::WriteReg`] is X.
    StoreX,
    /// Source register for [`MicroOp::WriteReg`] is Y.
    StoreY,
    /// Source for [`MicroOp::WriteReg`] is the first operand byte (the
    /// immediate of `MOV dp,#imm`).
    StoreImm,
    /// Compare the memory `value` with the first operand byte (the
    /// immediate of `CMP dp,#imm`).
    CmpMemImm,
    /// `YA = operand2:value`, update N/Z from the 16-bit result
    /// (`MOVW YA,dp`).
    MovwYa,
    /// `A |= value`, update N/Z.
    OrA,
    /// `A &= value`, update N/Z.
    AndA,
    /// `A ^= value`, update N/Z.
    EorA,
    /// `A = A + value + C` with full flag update.
    AdcA,
    /// `A = A - value - !C` with full flag update.
    SbcA,
    /// `A += 1`, update N/Z.
    IncA,
    /// `A -= 1`, update N/Z.
    DecA,
    /// `X += 1`, update N/Z.
    IncX,
    /// `X -= 1`, update N/Z.
    DecX,
    /// `Y += 1`, update N/Z.
    IncY,
    /// `Y -= 1`, update N/Z.
    DecY,
    /// `A = X`, update N/Z.
    AFromX,
    /// `X = A`, update N/Z.
    XFromA,
    /// `A = Y`, update N/Z.
    AFromY,
    /// `Y = A`, update N/Z.
    YFromA,
    /// `X = SP`, update N/Z.
    XFromSp,
    /// `SP = X`, flags unaffected.
    SpFromX,
    /// Set or clear the carry flag.
    SetCarry(bool),
    /// Complement the carry flag.
    NotCarry,
    /// Clear overflow and half-carry.
    ClrV,
    /// Set or clear the direct-page select flag.
    SetDirectPage(bool),
    /// Set or clear the interrupt-enable flag.
    SetInterrupt(bool),
    /// RMW: `mem += 1`, update N/Z.
    RmwInc,
    /// RMW: `mem -= 1`, update N/Z.
    RmwDec,
    /// RMW: arithmetic shift left, C/N/Z.
    RmwAsl,
    /// RMW: logical shift right, C/N/Z.
    RmwLsr,
    /// RMW: rotate left through carry, C/N/Z.
    RmwRol,
    /// RMW: rotate right through carry, C/N/Z.
    RmwRor,
    /// RMW: `mem |= imm` (first operand), update N/Z.
    OrMemImm,
    /// RMW: `mem &= imm`, update N/Z.
    AndMemImm,
    /// RMW: `mem ^= imm`, update N/Z.
    EorMemImm,
    /// RMW: `mem = mem + imm + C` with full flag update.
    AdcMemImm,
    /// RMW: `mem = mem - imm - !C` with full flag update.
    SbcMemImm,
    /// `A = value`, update N/Z, then `X += 1` (`MOV A,(X)+`).
    MovAXInc,
    /// Store A then `X += 1` (`MOV (X)+,A`).
    StoreAXInc,
    /// RMW: set bit `.0` of `mem` (no flags).
    RmwSet1(u8),
    /// RMW: clear bit `.0` of `mem` (no flags).
    RmwClr1(u8),
    /// Source register for [`MicroOp::PushReg`] is PSW.
    StorePsw,
    /// `A = value` from the stack (no flags).
    PopA,
    /// `X = value` from the stack (no flags).
    PopX,
    /// `Y = value` from the stack (no flags).
    PopY,
    /// `PSW = value` from the stack.
    PopPsw,
    /// `YA += word` (`value` lo, `operand2` hi) with full flag update.
    AddwYa,
    /// `YA -= word` with full flag update.
    SubwYa,
    /// Compare YA with the word, update N/Z/C (16-bit).
    CmpwYa,
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

    #[cfg(test)]
    pub(crate) fn is_halted(&self) -> bool {
        self.halted
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

    #[cfg(test)]
    pub(crate) fn set_pc_for_test(&mut self, pc: u16) {
        self.pc = pc;
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

    /// Update N and Z according to a 16-bit result value.
    fn update_nz16(&mut self, value: u16) {
        self.set_flag(FLAG_ZERO, value == 0);
        self.set_flag(FLAG_NEGATIVE, value & 0x8000 != 0);
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
        let half_carry =
            (u16::from(self.a & 0x0F) + u16::from(imm & 0x0F) + u16::from(carry_bit)) > 0x0F;
        self.a = result;
        self.set_flag(FLAG_CARRY, carry1 || carry2);
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.set_flag(FLAG_HALF_CARRY, half_carry);
        self.update_nz8(self.a);
    }

    /// Subtract immediate from A with borrow, updating N/Z/V/C flags.
    fn subtract_with_borrow_from_a(&mut self, imm: u8) {
        let carry_bit = if self.flag(FLAG_CARRY) { 0 } else { 1 };
        let (temp, borrow1) = self.a.overflowing_sub(imm);
        let (result, borrow2) = temp.overflowing_sub(carry_bit);
        let overflow = (self.a ^ result) & (!imm ^ result) & 0x80 != 0;
        let subtrahend_low = u16::from(imm & 0x0F) + u16::from(carry_bit);
        let half_borrow = u16::from(self.a & 0x0F) < subtrahend_low;
        self.a = result;
        self.set_flag(FLAG_CARRY, !(borrow1 || borrow2));
        self.set_flag(FLAG_OVERFLOW, overflow);
        self.set_flag(FLAG_HALF_CARRY, !half_borrow);
        self.update_nz8(self.a);
    }

    /// Update C, Z, N flags based on subtraction (left - right).
    /// Used by CMP instructions.
    fn update_flags_on_compare(&mut self, left: u8, right: u8) {
        let (result, borrow) = left.overflowing_sub(right);
        self.set_flag(FLAG_CARRY, !borrow);
        self.set_flag(FLAG_ZERO, result == 0);
        self.set_flag(FLAG_NEGATIVE, result & 0x80 != 0);
    }

    /// Compare A with immediate (subtract without storing), updating N/Z/C flags.
    fn compare_a(&mut self, imm: u8) {
        self.update_flags_on_compare(self.a, imm);
    }

    /// Compare X with immediate, updating N/Z/C flags.
    fn compare_x(&mut self, imm: u8) {
        self.update_flags_on_compare(self.x, imm);
    }

    /// Compare Y with immediate, updating N/Z/C flags.
    fn compare_y(&mut self, imm: u8) {
        self.update_flags_on_compare(self.y, imm);
    }

    /// Compare two 8-bit values (left - right), updating N/Z/C flags.
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
        if (0x00F0..=0x00FF).contains(&addr) {
            trace_apu!(
                3;
                "SPC I/O write-cycle ${:04X}=${:02X} PC=${:04X} A=${:02X} X=${:02X} Y=${:02X} PSW=${:02X}",
                addr,
                value,
                self.pc,
                self.a,
                self.x,
                self.y,
                self.psw
            );
        }
        bus.write(addr, value);
    }

    /// Consume one internal (idle) cycle.
    fn idle_cycle(&mut self, bus: &mut impl Spc700Bus, cycles: &mut u8) {
        *cycles = cycles.wrapping_add(bus.idle_cycles());
        bus.idle();
    }

    /// Consume one dummy-read cycle. ProcessorTests expose these as read-signal
    /// cycles with no value; Mesen charges the displayed address wait class.
    fn dummy_read_cycle(&mut self, bus: &mut impl Spc700Bus, addr: u16, cycles: &mut u8) {
        *cycles = cycles.wrapping_add(bus.read_cycles(addr));
        bus.dummy_read(addr);
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

    /// Read a 16-bit direct-page word with the SPC700's extra internal cycle
    /// between low and high byte reads used by MOVW/ADDW/SUBW.
    fn read_word_direct_page_with_idle(
        &mut self,
        bus: &mut impl Spc700Bus,
        dp: u8,
        cycles: &mut u8,
    ) -> u16 {
        let base = self.direct_page_base();
        let lo = self.read_cycle(bus, base | u16::from(dp), cycles);
        self.idle_cycle(bus, cycles);
        let hi = self.read_cycle(bus, base | u16::from(dp.wrapping_add(1)), cycles);
        u16::from(lo) | (u16::from(hi) << 8)
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
        self.update_nz16(result);
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
        self.update_nz16(result);
    }

    /// CMPW YA,[dp] — compare 16-bit direct-page word with YA.
    /// Updates N/Z/C flags based on the comparison result (YA - value).
    fn compare_ya(&mut self, value: u16) {
        let ya = (u16::from(self.y) << 8) | u16::from(self.a);
        let (result, borrow) = ya.overflowing_sub(value);
        self.set_flag(FLAG_CARRY, !borrow);
        self.update_nz16(result);
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

    /// DIV YA,X — divide YA by X, store quotient in A and remainder in Y.
    ///
    /// The SPC700 quotient is 9-bit internally. Bit 8 is reported in V, and H
    /// follows the low-nibble compare between Y and X.
    fn div_ya(&mut self) {
        let x = self.x;
        let ya = (u16::from(self.y) << 8) | u16::from(self.a);

        self.set_flag(FLAG_OVERFLOW, self.y >= x);
        self.set_flag(FLAG_HALF_CARRY, (self.y & 0x0F) >= (x & 0x0F));

        let x_u16 = u16::from(x);
        if u16::from(self.y) < (x_u16 << 1) {
            self.a = (ya / x_u16) as u8;
            self.y = (ya % x_u16) as u8;
        } else {
            let denom = 0x0100u16 - x_u16;
            let adjusted = ya.wrapping_sub(x_u16 << 9);
            self.a = (0x00FFu16.wrapping_sub(adjusted / denom)) as u8;
            self.y = (x_u16.wrapping_add(adjusted % denom)) as u8;
        }

        self.update_nz8(self.a);
    }

    /// Cycle script for the given opcode, or `None` when the opcode is only
    /// implemented atomically.
    ///
    /// The scripted set covers the blargg IPL-hack trampoline opcodes (#2908)
    /// plus every opcode the blargg shells use to poll or post through the
    /// `$F4-$F7` port window (#2914) — the dispatcher in `SnesApu::tick`
    /// cycle-steps these so port accesses land on the correct wall-clock SPC
    /// cycle instead of executing atomically at the instruction start.
    fn cycle_script(opcode: u8) -> Option<(&'static [MicroOp], Finish)> {
        use Finish as F;
        use MicroOp::*;
        const DP_READ: &[MicroOp] = &[FetchOperand, ReadDp(Index::None)];
        const DP_X_READ: &[MicroOp] = &[FetchOperand, Idle, ReadDp(Index::X)];
        const ABS_READ: &[MicroOp] = &[FetchOperand, FetchOperand2, ReadAbs];
        const IND_X_READ: &[MicroOp] = &[
            FetchOperand,
            Idle,
            ReadPtrLo(Index::X),
            ReadPtrHi(Index::X),
            ReadTarget(Index::None),
        ];
        const IND_Y_READ: &[MicroOp] = &[
            FetchOperand,
            Idle,
            ReadPtrLo(Index::None),
            ReadPtrHi(Index::None),
            ReadTarget(Index::Y),
        ];
        const DP_WRITE: &[MicroOp] = &[FetchOperand, ReadDpForStore(Index::None), WriteReg];
        const DP_IMM_WRITE: &[MicroOp] = &[FetchOperand, FetchOperand2, ReadDp2ForStore, WriteReg];
        const DP_IMM_CMP: &[MicroOp] = &[FetchOperand, FetchOperand2, ReadDp2, Idle];
        const DP_WORD_READ: &[MicroOp] = &[FetchOperand, ReadDp(Index::None), Idle, ReadDpHigh];
        const IMPLIED: &[MicroOp] = &[DummyReadPc];
        const IMPLIED3: &[MicroOp] = &[DummyReadPc, Idle];
        const DP_RMW: &[MicroOp] = &[FetchOperand, ReadDp(Index::None), WriteRmw];
        const DP_X_RMW: &[MicroOp] = &[FetchOperand, Idle, ReadDp(Index::X), WriteRmw];
        const ABS_RMW: &[MicroOp] = &[FetchOperand, FetchOperand2, ReadAbs, WriteRmw];
        const DP_IMM_RMW: &[MicroOp] = &[FetchOperand, FetchOperand2, ReadDp2, WriteRmw];
        const AT_X_READ: &[MicroOp] = &[DummyReadPc, ReadAtX];
        const ABS_X_READ: &[MicroOp] = &[FetchOperand, FetchOperand2, Idle, ReadAbsIdx(Index::X)];
        const ABS_Y_READ: &[MicroOp] = &[FetchOperand, FetchOperand2, Idle, ReadAbsIdx(Index::Y)];
        const DP_Y_READ: &[MicroOp] = &[FetchOperand, Idle, ReadDp(Index::Y)];
        const DP_DP_RMW: &[MicroOp] = &[
            FetchOperand,
            ReadDpSrcToOperand,
            FetchOperand2,
            ReadDp2,
            WriteRmw,
        ];
        const DP_DP_CMP: &[MicroOp] = &[
            FetchOperand,
            ReadDpSrcToOperand,
            FetchOperand2,
            ReadDp2,
            Idle,
        ];
        const XY_RMW: &[MicroOp] = &[DummyReadPc, ReadAtYToOperand, ReadAtX, WriteRmw];
        const XY_CMP: &[MicroOp] = &[DummyReadPc, ReadAtYToOperand, ReadAtX, Idle];
        const PUSH: &[MicroOp] = &[DummyReadPc, PushReg, Idle];
        const POP: &[MicroOp] = &[DummyReadPc, Idle, PopByte];
        const DP_WORD_ARITH: &[MicroOp] = &[FetchOperand, ReadDp(Index::None), Idle, ReadDpHigh];
        const DP_WORD_CMP: &[MicroOp] = &[FetchOperand, ReadDp(Index::None), ReadDpHigh];
        const DP_WORD_INCDEC_INC: &[MicroOp] = &[
            FetchOperand,
            ReadDp(Index::None),
            WordRmwLoWrite(true),
            ReadDpHigh,
            WordRmwHiWrite(true),
        ];
        const DP_WORD_INCDEC_DEC: &[MicroOp] = &[
            FetchOperand,
            ReadDp(Index::None),
            WordRmwLoWrite(false),
            ReadDpHigh,
            WordRmwHiWrite(false),
        ];
        const DP_X_WRITE: &[MicroOp] = &[FetchOperand, Idle, ReadDpForStore(Index::X), WriteReg];
        const ABS_WRITE: &[MicroOp] = &[FetchOperand, FetchOperand2, ReadAbsForStore, WriteReg];

        Some(match opcode {
            // BRA rel — the trampoline wait-loop opcode.
            0x2F => (&[FetchOperand, BranchIdle, Idle], F::None),
            // Conditional branches: 2 cycles not taken, 4 taken.
            0xF0 => (
                &[FetchOperandBranch(FLAG_ZERO, true), BranchIdle, Idle],
                F::None,
            ),
            0xD0 => (
                &[FetchOperandBranch(FLAG_ZERO, false), BranchIdle, Idle],
                F::None,
            ),
            0xB0 => (
                &[FetchOperandBranch(FLAG_CARRY, true), BranchIdle, Idle],
                F::None,
            ),
            0x90 => (
                &[FetchOperandBranch(FLAG_CARRY, false), BranchIdle, Idle],
                F::None,
            ),
            0x30 => (
                &[FetchOperandBranch(FLAG_NEGATIVE, true), BranchIdle, Idle],
                F::None,
            ),
            0x10 => (
                &[FetchOperandBranch(FLAG_NEGATIVE, false), BranchIdle, Idle],
                F::None,
            ),
            0x70 => (
                &[FetchOperandBranch(FLAG_OVERFLOW, true), BranchIdle, Idle],
                F::None,
            ),
            0x50 => (
                &[FetchOperandBranch(FLAG_OVERFLOW, false), BranchIdle, Idle],
                F::None,
            ),
            // MOV A,#imm — the queued micro-op behind the trampoline.
            0xE8 => (&[ReadImm], F::MovA),
            // Direct-page reads.
            0xE4 => (DP_READ, F::MovA),
            0xF8 => (DP_READ, F::MovX),
            0xEB => (DP_READ, F::MovY),
            0x64 => (DP_READ, F::CmpA),
            // ALU A,dp family (same bus shape as MOV A,dp).
            0x04 => (DP_READ, F::OrA),
            0x24 => (DP_READ, F::AndA),
            0x44 => (DP_READ, F::EorA),
            0x84 => (DP_READ, F::AdcA),
            0xA4 => (DP_READ, F::SbcA),
            // ALU A,dp+X family.
            0x14 => (DP_X_READ, F::OrA),
            0x34 => (DP_X_READ, F::AndA),
            0x54 => (DP_X_READ, F::EorA),
            0x94 => (DP_X_READ, F::AdcA),
            0xB4 => (DP_X_READ, F::SbcA),
            // ALU A,!abs family.
            0x05 => (ABS_READ, F::OrA),
            0x25 => (ABS_READ, F::AndA),
            0x45 => (ABS_READ, F::EorA),
            0x85 => (ABS_READ, F::AdcA),
            0xA5 => (ABS_READ, F::SbcA),
            // ALU A,#imm family (same shape as MOV A,#imm).
            0x08 => (&[ReadImm], F::OrA),
            0x28 => (&[ReadImm], F::AndA),
            0x48 => (&[ReadImm], F::EorA),
            0x88 => (&[ReadImm], F::AdcA),
            0xA8 => (&[ReadImm], F::SbcA),
            0x68 => (&[ReadImm], F::CmpA),
            // Implied / register ops (dummy read of PC as the internal cycle).
            0x00 => (IMPLIED, F::None),
            0xBC => (IMPLIED, F::IncA),
            0x9C => (IMPLIED, F::DecA),
            0x3D => (IMPLIED, F::IncX),
            0x1D => (IMPLIED, F::DecX),
            0xFC => (IMPLIED, F::IncY),
            0xDC => (IMPLIED, F::DecY),
            0x7D => (IMPLIED, F::AFromX),
            0x5D => (IMPLIED, F::XFromA),
            0xDD => (IMPLIED, F::AFromY),
            0xFD => (IMPLIED, F::YFromA),
            0x9D => (IMPLIED, F::XFromSp),
            0xBD => (IMPLIED, F::SpFromX),
            0x60 => (IMPLIED, F::SetCarry(false)),
            0x80 => (IMPLIED, F::SetCarry(true)),
            0x20 => (IMPLIED, F::SetDirectPage(false)),
            0x40 => (IMPLIED, F::SetDirectPage(true)),
            0xE0 => (IMPLIED, F::ClrV),
            // EI/DI/NOTC take one extra internal cycle.
            0xA0 => (IMPLIED3, F::SetInterrupt(true)),
            0xC0 => (IMPLIED3, F::SetInterrupt(false)),
            0xED => (IMPLIED3, F::NotCarry),
            // Read-modify-write dp.
            0xAB => (DP_RMW, F::RmwInc),
            0x8B => (DP_RMW, F::RmwDec),
            0x0B => (DP_RMW, F::RmwAsl),
            0x4B => (DP_RMW, F::RmwLsr),
            0x2B => (DP_RMW, F::RmwRol),
            0x6B => (DP_RMW, F::RmwRor),
            // Read-modify-write dp+X.
            0xBB => (DP_X_RMW, F::RmwInc),
            0x9B => (DP_X_RMW, F::RmwDec),
            0x1B => (DP_X_RMW, F::RmwAsl),
            0x5B => (DP_X_RMW, F::RmwLsr),
            0x3B => (DP_X_RMW, F::RmwRol),
            0x7B => (DP_X_RMW, F::RmwRor),
            // Read-modify-write !abs.
            0xAC => (ABS_RMW, F::RmwInc),
            0x8C => (ABS_RMW, F::RmwDec),
            0x0C => (ABS_RMW, F::RmwAsl),
            0x4C => (ABS_RMW, F::RmwLsr),
            0x2C => (ABS_RMW, F::RmwRol),
            0x6C => (ABS_RMW, F::RmwRor),
            // (X) indirect forms.
            0xE6 => (AT_X_READ, F::MovA),
            0xBF => (&[DummyReadPc, ReadAtX, Idle], F::MovAXInc),
            0xC6 => (&[DummyReadPc, ReadAtXForStore, WriteReg], F::StoreA),
            0xAF => (&[DummyReadPc, Idle, WriteAtX], F::StoreAXInc),
            0x06 => (AT_X_READ, F::OrA),
            0x26 => (AT_X_READ, F::AndA),
            0x46 => (AT_X_READ, F::EorA),
            0x66 => (AT_X_READ, F::CmpA),
            0x86 => (AT_X_READ, F::AdcA),
            0xA6 => (AT_X_READ, F::SbcA),
            // Absolute stores of X/Y and indexed stores of A.
            0xC9 => (ABS_WRITE, F::StoreX),
            0xCC => (ABS_WRITE, F::StoreY),
            0xD5 => (
                &[
                    FetchOperand,
                    FetchOperand2,
                    Idle,
                    ReadAbsIdxForStore(Index::X),
                    WriteReg,
                ],
                F::StoreA,
            ),
            0xD6 => (
                &[
                    FetchOperand,
                    FetchOperand2,
                    Idle,
                    ReadAbsIdxForStore(Index::Y),
                    WriteReg,
                ],
                F::StoreA,
            ),
            // MOV dp+Y,X / MOV dp+X,Y.
            0xD9 => (
                &[FetchOperand, Idle, ReadDpForStore(Index::Y), WriteReg],
                F::StoreX,
            ),
            0xDB => (
                &[FetchOperand, Idle, ReadDpForStore(Index::X), WriteReg],
                F::StoreY,
            ),
            // Absolute-indexed and dp-indexed loads.
            0xF5 => (ABS_X_READ, F::MovA),
            0xF6 => (ABS_Y_READ, F::MovA),
            0xF9 => (DP_Y_READ, F::MovX),
            0xFB => (DP_X_READ, F::MovY),
            // ALU A,!abs+X / !abs+Y.
            0x15 => (ABS_X_READ, F::OrA),
            0x16 => (ABS_Y_READ, F::OrA),
            0x35 => (ABS_X_READ, F::AndA),
            0x36 => (ABS_Y_READ, F::AndA),
            0x55 => (ABS_X_READ, F::EorA),
            0x56 => (ABS_Y_READ, F::EorA),
            0x75 => (ABS_X_READ, F::CmpA),
            0x76 => (ABS_Y_READ, F::CmpA),
            0x95 => (ABS_X_READ, F::AdcA),
            0x96 => (ABS_Y_READ, F::AdcA),
            0xB5 => (ABS_X_READ, F::SbcA),
            0xB6 => (ABS_Y_READ, F::SbcA),
            // SET1/CLR1 dp.bit, BBS/BBC dp.bit,rel, dp<-dp ALU, (X)<-(Y) ALU.
            0x02 => (DP_RMW, F::RmwSet1(0)),
            0x22 => (DP_RMW, F::RmwSet1(1)),
            0x42 => (DP_RMW, F::RmwSet1(2)),
            0x62 => (DP_RMW, F::RmwSet1(3)),
            0x82 => (DP_RMW, F::RmwSet1(4)),
            0xA2 => (DP_RMW, F::RmwSet1(5)),
            0xC2 => (DP_RMW, F::RmwSet1(6)),
            0xE2 => (DP_RMW, F::RmwSet1(7)),
            0x12 => (DP_RMW, F::RmwClr1(0)),
            0x32 => (DP_RMW, F::RmwClr1(1)),
            0x52 => (DP_RMW, F::RmwClr1(2)),
            0x72 => (DP_RMW, F::RmwClr1(3)),
            0x92 => (DP_RMW, F::RmwClr1(4)),
            0xB2 => (DP_RMW, F::RmwClr1(5)),
            0xD2 => (DP_RMW, F::RmwClr1(6)),
            0xF2 => (DP_RMW, F::RmwClr1(7)),
            0x03 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(0, true),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x23 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(1, true),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x43 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(2, true),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x63 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(3, true),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x83 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(4, true),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0xA3 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(5, true),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0xC3 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(6, true),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0xE3 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(7, true),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x13 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(0, false),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x33 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(1, false),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x53 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(2, false),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x73 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(3, false),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x93 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(4, false),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0xB3 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(5, false),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0xD3 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(6, false),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0xF3 => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchBit(7, false),
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x09 => (DP_DP_RMW, F::OrMemImm),
            0x29 => (DP_DP_RMW, F::AndMemImm),
            0x49 => (DP_DP_RMW, F::EorMemImm),
            0x89 => (DP_DP_RMW, F::AdcMemImm),
            0xA9 => (DP_DP_RMW, F::SbcMemImm),
            0x69 => (DP_DP_CMP, F::CmpMemImm),
            0xFA => (
                &[
                    FetchOperand,
                    ReadDpSrcToOperand,
                    FetchOperand2,
                    WriteDp2Operand,
                ],
                F::None,
            ),
            0x19 => (XY_RMW, F::OrMemImm),
            0x39 => (XY_RMW, F::AndMemImm),
            0x59 => (XY_RMW, F::EorMemImm),
            0x99 => (XY_RMW, F::AdcMemImm),
            0xB9 => (XY_RMW, F::SbcMemImm),
            0x79 => (XY_CMP, F::CmpMemImm),
            // Stack.
            0x2D => (PUSH, F::StoreA),
            0x4D => (PUSH, F::StoreX),
            0x6D => (PUSH, F::StoreY),
            0x0D => (PUSH, F::StorePsw),
            0xAE => (POP, F::PopA),
            0xCE => (POP, F::PopX),
            0xEE => (POP, F::PopY),
            0x8E => (POP, F::PopPsw),
            // 16-bit word ops on dp.
            0xDA => (
                &[
                    FetchOperand,
                    ReadDpForStore(Index::None),
                    WriteReg,
                    WriteDpHighY,
                ],
                F::StoreA,
            ),
            0x3A => (DP_WORD_INCDEC_INC, F::None),
            0x1A => (DP_WORD_INCDEC_DEC, F::None),
            0x7A => (DP_WORD_ARITH, F::AddwYa),
            0x9A => (DP_WORD_ARITH, F::SubwYa),
            0x5A => (DP_WORD_CMP, F::CmpwYa),
            // ALU A,[dp+X] / A,[dp]+Y (pointer shapes shared with MOV/CMP).
            0x07 => (IND_X_READ, F::OrA),
            0x27 => (IND_X_READ, F::AndA),
            0x47 => (IND_X_READ, F::EorA),
            0x87 => (IND_X_READ, F::AdcA),
            0xA7 => (IND_X_READ, F::SbcA),
            0x17 => (IND_Y_READ, F::OrA),
            0x37 => (IND_Y_READ, F::AndA),
            0x57 => (IND_Y_READ, F::EorA),
            0x97 => (IND_Y_READ, F::AdcA),
            0xB7 => (IND_Y_READ, F::SbcA),
            // CBNE / DBNZ.
            0x2E => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    Idle,
                    FetchOperandBranchANe,
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0xDE => (
                &[
                    FetchOperand,
                    Idle,
                    ReadDp(Index::X),
                    Idle,
                    FetchOperandBranchANe,
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0x6E => (
                &[
                    FetchOperand,
                    ReadDp(Index::None),
                    WriteDecNoFlags,
                    FetchOperandBranchValueNz,
                    BranchIdle,
                    Idle,
                ],
                F::None,
            ),
            0xFE => (
                &[DummyReadPc, Idle, FetchOperandBranchDecY, BranchIdle, Idle],
                F::None,
            ),
            // ALU dp,#imm read-modify-write (imm first, dp second).
            0x18 => (DP_IMM_RMW, F::OrMemImm),
            0x38 => (DP_IMM_RMW, F::AndMemImm),
            0x58 => (DP_IMM_RMW, F::EorMemImm),
            0x98 => (DP_IMM_RMW, F::AdcMemImm),
            0xB8 => (DP_IMM_RMW, F::SbcMemImm),
            0x3E => (DP_READ, F::CmpX),
            0x7E => (DP_READ, F::CmpY),
            // Direct-page indexed reads.
            0xF4 => (DP_X_READ, F::MovA),
            0x74 => (DP_X_READ, F::CmpA),
            // Absolute reads.
            0xE5 => (ABS_READ, F::MovA),
            0xE9 => (ABS_READ, F::MovX),
            0xEC => (ABS_READ, F::MovY),
            0x65 => (ABS_READ, F::CmpA),
            // Indirect reads through a direct-page pointer.
            0xE7 => (IND_X_READ, F::MovA),
            0x67 => (IND_X_READ, F::CmpA),
            0xF7 => (IND_Y_READ, F::MovA),
            0x77 => (IND_Y_READ, F::CmpA),
            // Stores (port posts).
            0xC4 => (DP_WRITE, F::StoreA),
            // MOV dp,#imm (imm fetched first, dp second).
            0x8F => (DP_IMM_WRITE, F::StoreImm),
            // CMP dp,#imm (imm fetched first, dp second) — the IPL's $CC
            // handshake wait loop polls $F4 with this opcode.
            0x78 => (DP_IMM_CMP, F::CmpMemImm),
            // MOVW YA,dp — the IPL's 16-bit port reads ($F4/$F6).
            0xBA => (DP_WORD_READ, F::MovwYa),
            0xD8 => (DP_WRITE, F::StoreX),
            0xCB => (DP_WRITE, F::StoreY),
            0xD4 => (DP_X_WRITE, F::StoreA),
            0xC5 => (ABS_WRITE, F::StoreA),
            _ => return None,
        })
    }

    /// Return `true` if the given opcode has a per-cycle script and can be
    /// driven by [`Self::step_one_cycle`] instead of the atomic [`Self::step`].
    pub fn opcode_is_cycle_scripted(opcode: u8) -> bool {
        Self::cycle_script(opcode).is_some()
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
                ..InProgressOp::default()
            });
            return;
        }

        let mut op = self.in_progress.take().expect("checked above");
        let (script, finish) =
            Self::cycle_script(op.opcode).expect("in_progress holds non-scripted opcode");
        // op.cycle counts executed cycles; cycle 1 was the opcode fetch, so
        // micro-op k (0-based) runs as cycle k+2.
        let micro_index = usize::from(op.cycle) - 1;
        op.cycle = op.cycle.wrapping_add(1);
        self.run_micro_op(bus, &mut op, script[micro_index], finish);
        let done = op.done_early || micro_index + 1 == script.len();
        if done {
            self.apply_finish(finish, &op);
        } else {
            self.in_progress = Some(op);
        }
    }

    /// Execute one micro-op of a cycle script (exactly one bus operation).
    fn run_micro_op(
        &mut self,
        bus: &mut impl Spc700Bus,
        op: &mut InProgressOp,
        micro: MicroOp,
        finish: Finish,
    ) {
        let index_of = |index: Index, x: u8, y: u8| match index {
            Index::None => 0,
            Index::X => x,
            Index::Y => y,
        };
        match micro {
            MicroOp::ReadImm => {
                op.value = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
            }
            MicroOp::FetchOperand => {
                op.operand = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
            }
            MicroOp::FetchOperand2 => {
                op.operand2 = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
            }
            MicroOp::Idle => {
                bus.idle();
            }
            MicroOp::BranchIdle => {
                self.branch(op.operand as i8);
                bus.idle();
            }
            MicroOp::ReadDp(index) => {
                op.addr = self.direct_page_base()
                    | u16::from(op.operand.wrapping_add(index_of(index, self.x, self.y)));
                op.value = bus.read(op.addr);
            }
            MicroOp::ReadDpForStore(index) => {
                op.addr = self.direct_page_base()
                    | u16::from(op.operand.wrapping_add(index_of(index, self.x, self.y)));
                bus.read(op.addr);
            }
            MicroOp::ReadDp2ForStore => {
                op.addr = self.direct_page_base() | u16::from(op.operand2);
                bus.read(op.addr);
            }
            MicroOp::ReadDp2 => {
                op.addr = self.direct_page_base() | u16::from(op.operand2);
                op.value = bus.read(op.addr);
            }
            MicroOp::ReadDpHigh => {
                let addr = self.direct_page_base() | u16::from(op.operand.wrapping_add(1));
                op.operand2 = bus.read(addr);
            }
            MicroOp::DummyReadPc => {
                bus.read(self.pc);
            }
            MicroOp::FetchOperandBranch(flag, wanted) => {
                op.operand = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if self.flag(flag) != wanted {
                    op.done_early = true;
                }
            }
            MicroOp::ReadAtX => {
                op.addr = self.direct_page_base() | u16::from(self.x);
                op.value = bus.read(op.addr);
            }
            MicroOp::ReadAtXForStore => {
                op.addr = self.direct_page_base() | u16::from(self.x);
                bus.read(op.addr);
            }
            MicroOp::WriteAtX => {
                op.addr = self.direct_page_base() | u16::from(self.x);
                bus.write(op.addr, self.a);
            }
            MicroOp::ReadDpSrcToOperand => {
                let addr = self.direct_page_base() | u16::from(op.operand);
                op.operand = bus.read(addr);
            }
            MicroOp::ReadAtYToOperand => {
                let addr = self.direct_page_base() | u16::from(self.y);
                op.operand = bus.read(addr);
            }
            MicroOp::WriteDp2Operand => {
                op.addr = self.direct_page_base() | u16::from(op.operand2);
                bus.write(op.addr, op.operand);
            }
            MicroOp::FetchOperandBranchBit(bit, wanted) => {
                op.operand = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if (op.value & (1 << bit) != 0) != wanted {
                    op.done_early = true;
                }
            }
            MicroOp::PushReg => {
                let value = match finish {
                    Finish::StoreA => self.a,
                    Finish::StoreX => self.x,
                    Finish::StoreY => self.y,
                    Finish::StorePsw => self.psw,
                    _ => unreachable!("PushReg without a store finish"),
                };
                let addr = 0x0100u16 | u16::from(self.sp);
                bus.write(addr, value);
                self.sp = self.sp.wrapping_sub(1);
            }
            MicroOp::PopByte => {
                self.sp = self.sp.wrapping_add(1);
                let addr = 0x0100u16 | u16::from(self.sp);
                op.value = bus.read(addr);
            }
            MicroOp::WriteDpHighY => {
                let addr = self.direct_page_base() | u16::from(op.operand.wrapping_add(1));
                bus.write(addr, self.y);
            }
            MicroOp::WordRmwLoWrite(inc) => {
                let result = if inc {
                    op.value.wrapping_add(1)
                } else {
                    op.value.wrapping_sub(1)
                };
                bus.write(op.addr, result);
                op.value = result;
            }
            MicroOp::WordRmwHiWrite(inc) => {
                // High byte was loaded into `operand2` by ReadDpHigh; the
                // carry/borrow propagates when the low byte wrapped.
                let hi_addr = self.direct_page_base() | u16::from(op.operand.wrapping_add(1));
                let result = if inc {
                    op.operand2.wrapping_add(u8::from(op.value == 0x00))
                } else {
                    op.operand2.wrapping_sub(u8::from(op.value == 0xFF))
                };
                bus.write(hi_addr, result);
                self.update_nz16((u16::from(result) << 8) | u16::from(op.value));
            }
            MicroOp::FetchOperandBranchANe => {
                op.operand = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if self.a == op.value {
                    op.done_early = true;
                }
            }
            MicroOp::WriteDecNoFlags => {
                let result = op.value.wrapping_sub(1);
                bus.write(op.addr, result);
                op.value = result;
            }
            MicroOp::FetchOperandBranchValueNz => {
                op.operand = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                if op.value == 0 {
                    op.done_early = true;
                }
            }
            MicroOp::FetchOperandBranchDecY => {
                op.operand = bus.read(self.pc);
                self.pc = self.pc.wrapping_add(1);
                self.y = self.y.wrapping_sub(1);
                if self.y == 0 {
                    op.done_early = true;
                }
            }
            MicroOp::ReadAbsIdx(index) => {
                let base = u16::from(op.operand) | (u16::from(op.operand2) << 8);
                op.addr = base.wrapping_add(u16::from(index_of(index, self.x, self.y)));
                op.value = bus.read(op.addr);
            }
            MicroOp::ReadAbsIdxForStore(index) => {
                let base = u16::from(op.operand) | (u16::from(op.operand2) << 8);
                op.addr = base.wrapping_add(u16::from(index_of(index, self.x, self.y)));
                bus.read(op.addr);
            }
            MicroOp::WriteRmw => {
                let result = match finish {
                    Finish::RmwInc => {
                        let r = op.value.wrapping_add(1);
                        self.update_nz8(r);
                        r
                    }
                    Finish::RmwDec => {
                        let r = op.value.wrapping_sub(1);
                        self.update_nz8(r);
                        r
                    }
                    Finish::RmwAsl => self.asl(op.value),
                    Finish::RmwLsr => self.lsr(op.value),
                    Finish::RmwRol => self.rol(op.value),
                    Finish::RmwRor => self.ror(op.value),
                    Finish::OrMemImm => {
                        let r = op.value | op.operand;
                        self.update_nz8(r);
                        r
                    }
                    Finish::AndMemImm => {
                        let r = op.value & op.operand;
                        self.update_nz8(r);
                        r
                    }
                    Finish::EorMemImm => {
                        let r = op.value ^ op.operand;
                        self.update_nz8(r);
                        r
                    }
                    Finish::AdcMemImm => {
                        let saved = self.a;
                        self.a = op.value;
                        self.add_with_carry_to_a(op.operand);
                        let r = self.a;
                        self.a = saved;
                        r
                    }
                    Finish::RmwSet1(bit) => op.value | (1 << bit),
                    Finish::RmwClr1(bit) => op.value & !(1 << bit),
                    Finish::SbcMemImm => {
                        let saved = self.a;
                        self.a = op.value;
                        self.subtract_with_borrow_from_a(op.operand);
                        let r = self.a;
                        self.a = saved;
                        r
                    }
                    _ => unreachable!("WriteRmw without an RMW finish"),
                };
                bus.write(op.addr, result);
            }
            MicroOp::ReadAbs => {
                op.addr = u16::from(op.operand) | (u16::from(op.operand2) << 8);
                op.value = bus.read(op.addr);
            }
            MicroOp::ReadAbsForStore => {
                op.addr = u16::from(op.operand) | (u16::from(op.operand2) << 8);
                bus.read(op.addr);
            }
            MicroOp::ReadPtrLo(index) => {
                let ptr = op.operand.wrapping_add(index_of(index, self.x, self.y));
                op.addr = u16::from(bus.read(self.direct_page_base() | u16::from(ptr)));
            }
            MicroOp::ReadPtrHi(index) => {
                let ptr = op
                    .operand
                    .wrapping_add(index_of(index, self.x, self.y))
                    .wrapping_add(1);
                let hi = bus.read(self.direct_page_base() | u16::from(ptr));
                op.addr |= u16::from(hi) << 8;
            }
            MicroOp::ReadTarget(index) => {
                let addr = op
                    .addr
                    .wrapping_add(u16::from(index_of(index, self.x, self.y)));
                op.value = bus.read(addr);
            }
            MicroOp::WriteReg => {
                let value = match finish {
                    Finish::StoreA | Finish::StoreAXInc => self.a,
                    Finish::StoreX => self.x,
                    Finish::StoreY => self.y,
                    Finish::StoreImm => op.operand,
                    _ => unreachable!("WriteReg without a store finish"),
                };
                bus.write(op.addr, value);
            }
        }
    }

    /// Apply the result action of a completed cycle script.
    fn apply_finish(&mut self, finish: Finish, op: &InProgressOp) {
        match finish {
            Finish::None
            | Finish::StoreA
            | Finish::StoreX
            | Finish::StoreY
            | Finish::StoreImm
            | Finish::RmwInc
            | Finish::RmwDec
            | Finish::RmwAsl
            | Finish::RmwLsr
            | Finish::RmwRol
            | Finish::RmwRor
            | Finish::OrMemImm
            | Finish::AndMemImm
            | Finish::EorMemImm
            | Finish::AdcMemImm
            | Finish::SbcMemImm => {}
            Finish::RmwSet1(_) | Finish::RmwClr1(_) | Finish::StorePsw => {}
            Finish::PopA => {
                self.a = op.value;
            }
            Finish::PopX => {
                self.x = op.value;
            }
            Finish::PopY => {
                self.y = op.value;
            }
            Finish::PopPsw => {
                self.psw = op.value;
            }
            Finish::AddwYa => {
                self.add_to_ya(u16::from(op.value) | (u16::from(op.operand2) << 8));
            }
            Finish::SubwYa => {
                self.subtract_from_ya(u16::from(op.value) | (u16::from(op.operand2) << 8));
            }
            Finish::CmpwYa => {
                self.compare_ya(u16::from(op.value) | (u16::from(op.operand2) << 8));
            }
            Finish::MovA => {
                self.a = op.value;
                self.update_nz8(self.a);
            }
            Finish::CmpMemImm => {
                self.compare_values(op.value, op.operand);
            }
            Finish::OrA => {
                self.a |= op.value;
                self.update_nz8(self.a);
            }
            Finish::AndA => {
                self.a &= op.value;
                self.update_nz8(self.a);
            }
            Finish::EorA => {
                self.a ^= op.value;
                self.update_nz8(self.a);
            }
            Finish::AdcA => {
                self.add_with_carry_to_a(op.value);
            }
            Finish::SbcA => {
                self.subtract_with_borrow_from_a(op.value);
            }
            Finish::MovAXInc => {
                self.a = op.value;
                self.update_nz8(self.a);
                self.x = self.x.wrapping_add(1);
            }
            Finish::StoreAXInc => {
                self.x = self.x.wrapping_add(1);
            }
            Finish::IncA => {
                self.a = self.a.wrapping_add(1);
                self.update_nz8(self.a);
            }
            Finish::DecA => {
                self.a = self.a.wrapping_sub(1);
                self.update_nz8(self.a);
            }
            Finish::IncX => {
                self.x = self.x.wrapping_add(1);
                self.update_nz8(self.x);
            }
            Finish::DecX => {
                self.x = self.x.wrapping_sub(1);
                self.update_nz8(self.x);
            }
            Finish::IncY => {
                self.y = self.y.wrapping_add(1);
                self.update_nz8(self.y);
            }
            Finish::DecY => {
                self.y = self.y.wrapping_sub(1);
                self.update_nz8(self.y);
            }
            Finish::AFromX => {
                self.a = self.x;
                self.update_nz8(self.a);
            }
            Finish::XFromA => {
                self.x = self.a;
                self.update_nz8(self.x);
            }
            Finish::AFromY => {
                self.a = self.y;
                self.update_nz8(self.a);
            }
            Finish::YFromA => {
                self.y = self.a;
                self.update_nz8(self.y);
            }
            Finish::XFromSp => {
                self.x = self.sp;
                self.update_nz8(self.x);
            }
            Finish::SpFromX => {
                self.sp = self.x;
            }
            Finish::SetCarry(v) => {
                self.set_flag(FLAG_CARRY, v);
            }
            Finish::NotCarry => {
                self.set_flag(FLAG_CARRY, !self.flag(FLAG_CARRY));
            }
            Finish::ClrV => {
                self.set_flag(FLAG_OVERFLOW, false);
                self.set_flag(FLAG_HALF_CARRY, false);
            }
            Finish::SetDirectPage(v) => {
                self.set_flag(FLAG_DIRECT_PAGE, v);
            }
            Finish::SetInterrupt(v) => {
                self.set_flag(FLAG_INTERRUPT, v);
            }
            Finish::MovwYa => {
                self.a = op.value;
                self.y = op.operand2;
                self.update_nz16((u16::from(self.y) << 8) | u16::from(self.a));
            }
            Finish::MovX => {
                self.x = op.value;
                self.update_nz8(self.x);
            }
            Finish::MovY => {
                self.y = op.value;
                self.update_nz8(self.y);
            }
            Finish::CmpA => self.compare_a(op.value),
            Finish::CmpX => self.compare_x(op.value),
            Finish::CmpY => self.compare_y(op.value),
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // MOV X,A — copy A into X, update N/Z.
            0x5D => {
                self.x = self.a;
                self.update_nz8(self.x);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // MOV A,Y — copy Y into A, update N/Z.
            0xDD => {
                self.a = self.y;
                self.update_nz8(self.a);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // MOV Y,A — copy A into Y, update N/Z.
            0xFD => {
                self.y = self.a;
                self.update_nz8(self.y);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // MOV X,SP — copy SP into X, update N/Z.
            0x9D => {
                self.x = self.sp;
                self.update_nz8(self.x);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // MOV SP,X — copy X into SP; flags are unaffected.
            0xBD => {
                self.sp = self.x;
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
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
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_dp = self.fetch(bus, &mut cycles);
                let dst_addr = self.direct_page_base() | u16::from(dst_dp);
                self.write_cycle(bus, dst_addr, value, &mut cycles);
            }
            // MOV A,(X) — load A from direct-page address in X, update N/Z.
            0xE6 => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let addr = self.direct_page_base() | self.x as u16;
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
            }
            // MOV A,(X)+ — load A from [X], then increment X, update N/Z.
            0xBF => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let addr = self.direct_page_base() | self.x as u16;
                self.a = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a);
                self.x = self.x.wrapping_add(1);
                self.idle_cycle(bus, &mut cycles);
            }
            // MOV (X),A — store A to direct-page address in X; flags unchanged.
            0xC6 => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let addr = self.direct_page_base() | self.x as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
            }
            // MOV (X)+,A — store A to [X], then increment X; flags unchanged.
            0xAF => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | self.x as u16;
                self.write_cycle(bus, addr, self.a, &mut cycles);
                self.x = self.x.wrapping_add(1);
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
                self.idle_cycle(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | dp as u16, &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | dp.wrapping_add(1) as u16,
                    &mut cycles,
                );
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
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | dp.wrapping_add(self.x) as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.a, &mut cycles);
            }
            // MOV dp+X,Y — store Y to direct page indexed by X; flags unchanged.
            0xDB => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | dp.wrapping_add(self.x) as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.y, &mut cycles);
            }
            // MOV dp+Y,X — store X to direct page indexed by Y; flags unchanged.
            0xD9 => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | dp.wrapping_add(self.y) as u16;
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, self.x, &mut cycles);
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
            // MOVW YA,dp — load 16-bit direct-page word into YA, update N/Z from YA.
            0xBA => {
                let dp = self.fetch(bus, &mut cycles);
                let value = self.read_word_direct_page_with_idle(bus, dp, &mut cycles);
                self.a = value as u8;
                self.y = (value >> 8) as u8;
                self.update_nz16((u16::from(self.y) << 8) | u16::from(self.a));
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // DEC A — decrement A, update N/Z.
            0x9C => {
                self.a = self.a.wrapping_sub(1);
                self.update_nz8(self.a);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // INC X — increment X, update N/Z.
            0x3D => {
                self.x = self.x.wrapping_add(1);
                self.update_nz8(self.x);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // DEC X — decrement X, update N/Z.
            0x1D => {
                self.x = self.x.wrapping_sub(1);
                self.update_nz8(self.x);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // INC Y — increment Y, update N/Z.
            0xFC => {
                self.y = self.y.wrapping_add(1);
                self.update_nz8(self.y);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // DEC Y — decrement Y, update N/Z.
            0xDC => {
                self.y = self.y.wrapping_sub(1);
                self.update_nz8(self.y);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // ROL A — rotate left accumulator through carry.
            0x3C => {
                self.a = self.rol(self.a);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // LSR A — shift right accumulator, bit 0 to carry.
            0x5C => {
                self.a = self.lsr(self.a);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // ROR A — rotate right accumulator through carry.
            0x7C => {
                self.a = self.ror(self.a);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
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
            // AND A,dp — bitwise AND of direct-page byte into A, update N/Z.
            0x24 => {
                let dp = self.fetch(bus, &mut cycles);
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.a &= value;
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
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_dp = self.fetch(bus, &mut cycles);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.a &= value;
                self.update_nz8(self.a);
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
                self.idle_cycle(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.a &= value;
                self.update_nz8(self.a);
            }
            // AND (X),(Y) — (X) &= (Y).
            0x39 => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let result = x_value & y_value;
                self.write_cycle(bus, x_addr, result, &mut cycles);
                self.update_nz8(result);
            }
            // OR A,dp — bitwise OR of direct-page byte into A, update N/Z.
            0x04 => {
                let dp = self.fetch(bus, &mut cycles);
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.a |= value;
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
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_dp = self.fetch(bus, &mut cycles);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.a |= value;
                self.update_nz8(self.a);
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
                self.idle_cycle(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.a |= value;
                self.update_nz8(self.a);
            }
            // OR (X),(Y) — (X) |= (Y).
            0x19 => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let result = x_value | y_value;
                self.write_cycle(bus, x_addr, result, &mut cycles);
                self.update_nz8(result);
            }
            // EOR A,dp — bitwise XOR of direct-page byte into A, update N/Z.
            0x44 => {
                let dp = self.fetch(bus, &mut cycles);
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.a ^= value;
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
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_dp = self.fetch(bus, &mut cycles);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.a ^= value;
                self.update_nz8(self.a);
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
                self.idle_cycle(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.a ^= value;
                self.update_nz8(self.a);
            }
            // EOR (X),(Y) — (X) ^= (Y).
            0x59 => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let result = x_value ^ y_value;
                self.write_cycle(bus, x_addr, result, &mut cycles);
                self.update_nz8(result);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.add_with_carry_to_a(value);
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
                self.idle_cycle(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.add_with_carry_to_a(value);
            }
            // ADC dp,dp — [dst] = [dst] + [src] + C, update N/Z/V/C from result.
            0x89 => {
                let src_dp = self.fetch(bus, &mut cycles);
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_dp = self.fetch(bus, &mut cycles);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let saved_a = self.a;
                self.a = x_value;
                self.add_with_carry_to_a(y_value);
                let result = self.a;
                self.a = saved_a;
                self.write_cycle(bus, x_addr, result, &mut cycles);
            }
            // SBC A,#imm — subtract immediate from A with borrow, update N/Z/V/C.
            0xA8 => {
                let imm = self.fetch(bus, &mut cycles);
                self.subtract_with_borrow_from_a(imm);
            }
            // SBC A,dp — subtract direct-page byte from A with borrow, update N/Z/V/C.
            0xA4 => {
                let dp = self.fetch(bus, &mut cycles);
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.subtract_with_borrow_from_a(value);
            }
            // SBC A,(X).
            0xA6 => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.subtract_with_borrow_from_a(value);
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
                self.idle_cycle(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
                let addr = (u16::from(lo) | (u16::from(hi) << 8)).wrapping_add(u16::from(self.y));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.subtract_with_borrow_from_a(value);
            }
            // SBC dp,dp — [dst] = [dst] - [src] - !C, update N/Z/V/C from result.
            0xA9 => {
                let src_dp = self.fetch(bus, &mut cycles);
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_dp = self.fetch(bus, &mut cycles);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let x_addr = self.direct_page_base() | u16::from(self.x);
                let y_addr = self.direct_page_base() | u16::from(self.y);
                let y_value = self.read_cycle(bus, y_addr, &mut cycles);
                let x_value = self.read_cycle(bus, x_addr, &mut cycles);
                let saved_a = self.a;
                self.a = x_value;
                self.subtract_with_borrow_from_a(y_value);
                let result = self.a;
                self.a = saved_a;
                self.write_cycle(bus, x_addr, result, &mut cycles);
            }
            // CMP X,#imm — compare X with immediate, update N/Z/C (V unchanged).
            0xC8 => {
                let imm = self.fetch(bus, &mut cycles);
                self.compare_x(imm);
            }
            // DI — clear interrupt-enable flag (logical only on SNES APU).
            0xC0 => {
                self.set_flag(FLAG_INTERRUPT, false);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
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
                let src = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(src_dp),
                    &mut cycles,
                );
                let dst_dp = self.fetch(bus, &mut cycles);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.compare_a(value);
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
                self.idle_cycle(bus, &mut cycles);
                let lo = self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                let hi = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(dp.wrapping_add(1)),
                    &mut cycles,
                );
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                let y_value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.y),
                    &mut cycles,
                );
                let x_value = self.read_cycle(
                    bus,
                    self.direct_page_base() | u16::from(self.x),
                    &mut cycles,
                );
                self.compare_values(x_value, y_value);
                self.idle_cycle(bus, &mut cycles);
            }
            // CLRP — clear direct-page select flag (P=0).
            0x20 => {
                self.set_flag(FLAG_DIRECT_PAGE, false);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // SETP — set direct-page select flag (P=1).
            0x40 => {
                self.set_flag(FLAG_DIRECT_PAGE, true);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // CLRC — clear carry flag.
            0x60 => {
                self.set_flag(FLAG_CARRY, false);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // SETC — set carry flag.
            0x80 => {
                self.set_flag(FLAG_CARRY, true);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // CLRV — clear overflow and half-carry flags.
            0xE0 => {
                self.set_flag(FLAG_OVERFLOW, false);
                self.set_flag(FLAG_HALF_CARRY, false);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
            }
            // EI — set interrupt-enable flag (logical only on SNES APU).
            0xA0 => {
                self.set_flag(FLAG_INTERRUPT, true);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // NOTC — complement carry flag.
            0xED => {
                self.set_flag(FLAG_CARRY, !self.flag(FLAG_CARRY));
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
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
                self.idle_cycle(bus, &mut cycles);
                self.write_cycle(bus, addr, result, &mut cycles);
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
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let rel = self.fetch(bus, &mut cycles) as i8;
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
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let rel = self.fetch(bus, &mut cycles) as i8;
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
                let value =
                    self.read_cycle(bus, self.direct_page_base() | u16::from(dp), &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let offset = self.fetch(bus, &mut cycles) as i8;
                if self.a != value {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // PUSH PSW — push flags onto stack (4 cycles).
            0x0D => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.push(bus, self.psw, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // PUSH A — push accumulator onto stack (4 cycles).
            0x2D => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.push(bus, self.a, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // PUSH X — push X register onto stack (4 cycles).
            0x4D => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.push(bus, self.x, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // PUSH Y — push Y register onto stack (4 cycles).
            0x6D => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.push(bus, self.y, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // POP PSW — pop flags from stack (4 cycles).
            0x8E => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.psw = self.pop(bus, &mut cycles);
            }
            // POP A — pop accumulator from stack (4 cycles).
            0xAE => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.a = self.pop(bus, &mut cycles);
            }
            // POP X — pop X register from stack (4 cycles).
            0xCE => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.x = self.pop(bus, &mut cycles);
            }
            // POP Y — pop Y register from stack (4 cycles).
            0xEE => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.y = self.pop(bus, &mut cycles);
            }
            // BRK — software interrupt: push PC+1 and PSW; set B, clear I; jump via vector.
            0x0F => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
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
                self.idle_cycle(bus, &mut cycles);
                self.push(bus, (return_addr >> 8) as u8, &mut cycles);
                self.push(bus, return_addr as u8, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.pc = addr;
            }
            // PCALL u8 — push return address and jump to $FF00+u.
            0x4F => {
                let upage = self.fetch(bus, &mut cycles);
                let return_addr = self.pc;
                self.idle_cycle(bus, &mut cycles);
                self.push(bus, (return_addr >> 8) as u8, &mut cycles);
                self.push(bus, return_addr as u8, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.pc = 0xFF00 | u16::from(upage);
            }
            // TCALL n — table call via vector at $FFDE - 2*n.
            op @ (0x01 | 0x11 | 0x21 | 0x31 | 0x41 | 0x51 | 0x61 | 0x71 | 0x81 | 0x91 | 0xA1
            | 0xB1 | 0xC1 | 0xD1 | 0xE1 | 0xF1) => {
                let n = (op >> 4) & 0x0F;
                let vector = 0xFFDEu16.wrapping_sub(u16::from(n) * 2);
                let return_addr = self.pc;
                self.read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.push(bus, (return_addr >> 8) as u8, &mut cycles);
                self.push(bus, return_addr as u8, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let lo = self.read_cycle(bus, vector, &mut cycles);
                let hi = self.read_cycle(bus, vector.wrapping_add(1), &mut cycles);
                self.pc = u16::from(lo) | (u16::from(hi) << 8);
            }
            // RTS — return from subroutine (5 cycles).
            // Pops return address from stack and jumps.
            0x6F => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let lo = self.pop(bus, &mut cycles) as u16;
                let hi = self.pop(bus, &mut cycles) as u16;
                self.pc = (hi << 8) | lo;
            }
            // RETI — return from interrupt: pop PSW, then PC.
            0x7F => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                self.psw = self.pop(bus, &mut cycles);
                let lo = self.pop(bus, &mut cycles) as u16;
                let hi = self.pop(bus, &mut cycles) as u16;
                self.pc = (hi << 8) | lo;
            }
            // DBNZ dp,rel — decrement direct-page byte and branch if result is non-zero.
            0x6E => {
                let dp = self.fetch(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp);
                let value = self.read_cycle(bus, addr, &mut cycles);
                let result = value.wrapping_sub(1);
                self.write_cycle(bus, addr, result, &mut cycles);
                let offset = self.fetch(bus, &mut cycles) as i8;
                if result != 0 {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // CBNE dp+X,rel — compare A with direct-page byte indexed by X and branch if not equal.
            0xDE => {
                let dp = self.fetch(bus, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let addr = self.direct_page_base() | u16::from(dp.wrapping_add(self.x));
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let offset = self.fetch(bus, &mut cycles) as i8;
                if self.a != value {
                    self.branch(offset);
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // DBNZ Y,rel — decrement Y and branch if result is non-zero.
            0xFE => {
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
                let offset = self.fetch(bus, &mut cycles) as i8;
                self.y = self.y.wrapping_sub(1);
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
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value | self.a, &mut cycles);
            }
            // TCLR1 !abs — clear bits in memory by A; N/Z from A - M.
            0x4E => {
                let addr = self.fetch_u16(bus, &mut cycles);
                let value = self.read_cycle(bus, addr, &mut cycles);
                self.update_nz8(self.a.wrapping_sub(value));
                self.read_cycle(bus, addr, &mut cycles);
                self.write_cycle(bus, addr, value & !self.a, &mut cycles);
            }
            // INCW dp — increment 16-bit direct-page word, update N/Z from 16-bit result.
            0x3A => {
                let dp = self.fetch(bus, &mut cycles);
                let base = self.direct_page_base();
                let lo_addr = base | u16::from(dp);
                let hi_addr = base | u16::from(dp.wrapping_add(1));
                let lo = self.read_cycle(bus, lo_addr, &mut cycles);
                let lo_result = lo.wrapping_add(1);
                self.write_cycle(bus, lo_addr, lo_result, &mut cycles);
                let hi = self.read_cycle(bus, hi_addr, &mut cycles);
                let hi_result = hi.wrapping_add(u8::from(lo == 0xFF));
                self.write_cycle(bus, hi_addr, hi_result, &mut cycles);
                let result = u16::from(lo_result) | (u16::from(hi_result) << 8);
                self.update_nz16(result);
            }
            // DECW dp — decrement 16-bit direct-page word, update N/Z from 16-bit result.
            0x1A => {
                let dp = self.fetch(bus, &mut cycles);
                let base = self.direct_page_base();
                let lo_addr = base | u16::from(dp);
                let hi_addr = base | u16::from(dp.wrapping_add(1));
                let lo = self.read_cycle(bus, lo_addr, &mut cycles);
                let lo_result = lo.wrapping_sub(1);
                self.write_cycle(bus, lo_addr, lo_result, &mut cycles);
                let hi = self.read_cycle(bus, hi_addr, &mut cycles);
                let hi_result = hi.wrapping_sub(u8::from(lo == 0x00));
                self.write_cycle(bus, hi_addr, hi_result, &mut cycles);
                let result = u16::from(lo_result) | (u16::from(hi_result) << 8);
                self.update_nz16(result);
            }
            // ADDW YA,dp — add 16-bit direct-page word to YA (5 cycles).
            0x7A => {
                let dp = self.fetch(bus, &mut cycles);
                let value = self.read_word_direct_page_with_idle(bus, dp, &mut cycles);
                self.add_to_ya(value);
            }
            // SUBW YA,dp — subtract 16-bit direct-page word from YA (5 cycles).
            0x9A => {
                let dp = self.fetch(bus, &mut cycles);
                let value = self.read_word_direct_page_with_idle(bus, dp, &mut cycles);
                self.subtract_from_ya(value);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                for _ in 0..7 {
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // DIV YA,X — divide YA / X, quotient in A, remainder in Y (12 cycles).
            0x9E => {
                self.div_ya();
                // DIV takes 12 cycles: 1 fetch + 11 operation/idle
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                for _ in 0..10 {
                    self.idle_cycle(bus, &mut cycles);
                }
            }
            // XCN A — exchange high/low nibbles in A, update N/Z.
            0x9F => {
                self.a = self.a.rotate_left(4);
                self.update_nz8(self.a);
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                for _ in 0..3 {
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
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
                self.dummy_read_cycle(bus, self.pc, &mut cycles);
                self.idle_cycle(bus, &mut cycles);
            }
            // SLEEP — halt CPU until external wakeup source.
            0xEF => {
                trace_apu!(1; "SPC entered SLEEP at ${:04X}", opcode_pc);
                for _ in 0..3 {
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
                self.halted = true;
            }
            // STOP — halt CPU clock until reset.
            0xFF => {
                trace_apu!(1; "SPC entered STOP at ${:04X}", opcode_pc);
                for _ in 0..3 {
                    self.idle_cycle(bus, &mut cycles);
                    self.idle_cycle(bus, &mut cycles);
                }
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
        self.in_progress = None;
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

    struct VariableReadBus {
        ram: Box<[u8; 0x1_0000]>,
        read_cost: u8,
        idle_cost: u8,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum BusOp {
        Read(u16),
        Write(u16, u8),
        Idle,
    }

    struct RecordingBus {
        ram: Box<[u8; 0x1_0000]>,
        ops: Vec<BusOp>,
    }

    impl RecordingBus {
        fn new() -> Self {
            Self {
                ram: Box::new([0; 0x1_0000]),
                ops: Vec::new(),
            }
        }

        fn load(&mut self, addr: u16, data: &[u8]) {
            for (i, &byte) in data.iter().enumerate() {
                self.ram[addr.wrapping_add(i as u16) as usize] = byte;
            }
        }
    }

    impl Spc700Bus for RecordingBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.ops.push(BusOp::Read(addr));
            self.ram[addr as usize]
        }

        fn write(&mut self, addr: u16, value: u8) {
            self.ops.push(BusOp::Write(addr, value));
            self.ram[addr as usize] = value;
        }

        fn idle(&mut self) {
            self.ops.push(BusOp::Idle);
        }

        fn dummy_read(&mut self, addr: u16) {
            self.ops.push(BusOp::Read(addr));
        }
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

    impl VariableReadBus {
        fn new(read_cost: u8, idle_cost: u8) -> Self {
            Self {
                ram: Box::new([0; 0x1_0000]),
                read_cost,
                idle_cost,
            }
        }

        fn load(&mut self, addr: u16, data: &[u8]) {
            for (i, &byte) in data.iter().enumerate() {
                self.ram[addr.wrapping_add(i as u16) as usize] = byte;
            }
        }
    }

    impl Spc700Bus for VariableReadBus {
        fn read_cycles(&self, _addr: u16) -> u8 {
            self.read_cost
        }

        fn read(&mut self, addr: u16) -> u8 {
            self.ram[addr as usize]
        }

        fn write(&mut self, addr: u16, value: u8) {
            self.ram[addr as usize] = value;
        }

        fn idle_cycles(&self) -> u8 {
            self.idle_cost
        }

        fn idle(&mut self) {}

        fn dummy_read(&mut self, _addr: u16) {}
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

        fn dummy_read(&mut self, _addr: u16) {
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
    fn nop_dummy_read_uses_displayed_address_read_cycle_cost() {
        let mut cpu = Spc700::new();
        let mut bus = VariableReadBus::new(2, 1);
        bus.load(0x0200, &[0x00]); // NOP
        cpu.load_state_for_processor_test(0, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(
            cycles, 4,
            "Mesen charges dummy reads using the displayed address wait-state class"
        );
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
    fn mov_indirect_x_a_reads_next_pc_before_target_dummy_read_and_write() {
        let mut cpu = Spc700::new();
        let mut bus = RecordingBus::new();
        bus.load(0x0200, &[0xC6]); // MOV (X),A
        cpu.load_state_for_processor_test(0x5A, 0x10, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(
            bus.ops,
            vec![
                BusOp::Read(0x0200),
                BusOp::Read(0x0201),
                BusOp::Read(0x0010),
                BusOp::Write(0x0010, 0x5A),
            ]
        );
    }

    #[test]
    fn mov_indirect_x_increment_a_reads_next_pc_and_idles_before_target_write() {
        let mut cpu = Spc700::new();
        let mut bus = RecordingBus::new();
        bus.load(0x0200, &[0xAF]); // MOV (X)+,A
        cpu.load_state_for_processor_test(0x5A, 0x10, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(
            bus.ops,
            vec![
                BusOp::Read(0x0200),
                BusOp::Read(0x0201),
                BusOp::Idle,
                BusOp::Write(0x0010, 0x5A),
            ]
        );
        assert_eq!(cpu.x(), 0x11);
    }

    #[test]
    fn mov_a_indirect_x_increment_reads_next_pc_before_target_read_and_idle() {
        let mut cpu = Spc700::new();
        let mut bus = RecordingBus::new();
        bus.load(0x0200, &[0xBF]); // MOV A,(X)+
        bus.load(0x0010, &[0x5A]);
        cpu.load_state_for_processor_test(0, 0x10, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(
            bus.ops,
            vec![
                BusOp::Read(0x0200),
                BusOp::Read(0x0201),
                BusOp::Read(0x0010),
                BusOp::Idle,
            ]
        );
        assert_eq!(cpu.a(), 0x5A);
        assert_eq!(cpu.x(), 0x11);
    }

    #[test]
    fn mov_direct_x_a_idles_before_indexed_dummy_read_and_write() {
        let mut cpu = Spc700::new();
        let mut bus = RecordingBus::new();
        bus.load(0x0200, &[0xD4, 0x20]); // MOV $20+X,A
        cpu.load_state_for_processor_test(0x5A, 0x05, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(
            bus.ops,
            vec![
                BusOp::Read(0x0200),
                BusOp::Read(0x0201),
                BusOp::Idle,
                BusOp::Read(0x0025),
                BusOp::Write(0x0025, 0x5A),
            ]
        );
    }

    #[test]
    fn movw_ya_direct_idles_between_low_and_high_byte_reads() {
        let mut cpu = Spc700::new();
        let mut bus = RecordingBus::new();
        bus.load(0x0200, &[0xBA, 0x30]); // MOVW YA,$30
        bus.load(0x0030, &[0x34, 0x12]);
        cpu.load_state_for_processor_test(0, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(
            bus.ops,
            vec![
                BusOp::Read(0x0200),
                BusOp::Read(0x0201),
                BusOp::Read(0x0030),
                BusOp::Idle,
                BusOp::Read(0x0031),
            ]
        );
        assert_eq!(cpu.ya(), 0x1234);
    }

    #[test]
    fn incw_direct_writes_low_byte_before_reading_high_byte() {
        let mut cpu = Spc700::new();
        let mut bus = RecordingBus::new();
        bus.load(0x0200, &[0x3A, 0x30]); // INCW $30
        bus.load(0x0030, &[0xFF, 0x12]);
        cpu.load_state_for_processor_test(0, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(
            bus.ops,
            vec![
                BusOp::Read(0x0200),
                BusOp::Read(0x0201),
                BusOp::Read(0x0030),
                BusOp::Write(0x0030, 0x00),
                BusOp::Read(0x0031),
                BusOp::Write(0x0031, 0x13),
            ]
        );
    }

    #[test]
    fn tset_absolute_reads_target_twice_before_write() {
        let mut cpu = Spc700::new();
        let mut bus = RecordingBus::new();
        bus.load(0x0200, &[0x0E, 0x34, 0x12]); // TSET1 $1234
        bus.load(0x1234, &[0x10]);
        cpu.load_state_for_processor_test(0x03, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(
            bus.ops,
            vec![
                BusOp::Read(0x0200),
                BusOp::Read(0x0201),
                BusOp::Read(0x0202),
                BusOp::Read(0x1234),
                BusOp::Read(0x1234),
                BusOp::Write(0x1234, 0x13),
            ]
        );
    }

    #[test]
    fn cbne_direct_reads_target_before_relative_operand() {
        let mut cpu = Spc700::new();
        let mut bus = RecordingBus::new();
        bus.load(0x0200, &[0x2E, 0x30, 0x04]); // CBNE $30,+4
        bus.load(0x0030, &[0x10]);
        cpu.load_state_for_processor_test(0x20, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 7);
        assert_eq!(
            bus.ops,
            vec![
                BusOp::Read(0x0200),
                BusOp::Read(0x0201),
                BusOp::Read(0x0030),
                BusOp::Idle,
                BusOp::Read(0x0202),
                BusOp::Idle,
                BusOp::Idle,
            ]
        );
        assert_eq!(cpu.pc(), 0x0207);
    }

    #[test]
    fn dbnz_direct_writes_target_before_relative_operand() {
        let mut cpu = Spc700::new();
        let mut bus = RecordingBus::new();
        bus.load(0x0200, &[0x6E, 0x30, 0x04]); // DBNZ $30,+4
        bus.load(0x0030, &[0x02]);
        cpu.load_state_for_processor_test(0, 0, 0, 0xEF, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 7);
        assert_eq!(
            bus.ops,
            vec![
                BusOp::Read(0x0200),
                BusOp::Read(0x0201),
                BusOp::Read(0x0030),
                BusOp::Write(0x0030, 0x01),
                BusOp::Read(0x0202),
                BusOp::Idle,
                BusOp::Idle,
            ]
        );
        assert_eq!(cpu.pc(), 0x0207);
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
        bus.load(0x0200, &[0x28, 0x0F]); // AND A,#$0F
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
        bus.load(0x0200, &[0x28, 0x00]); // AND A,#$00
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
        bus.load(0x0200, &[0x08, 0x0F]); // OR A,#$0F
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
        bus.load(0x0200, &[0x08, 0x00]); // OR A,#$00
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
        bus.load(0x0200, &[0x48, 0xFF]); // EOR A,#$FF
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
        bus.load(0x0200, &[0x48, 0x55]); // EOR A,#$55
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
    fn adc_a_dp_sets_half_carry_on_low_nibble_overflow() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x84, 0x10]); // ADC A,$10
        bus.set(0x0010, 0x01);
        cpu.load_state_for_processor_test(0x0F, 0, 0, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0x10);
        assert!(cpu.flag(FLAG_HALF_CARRY));
        assert!(!cpu.flag(FLAG_CARRY));
    }

    #[test]
    fn sbc_a_imm_simple_no_borrow_when_carry_set() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xA8, 0x10]); // SBC A,#$10
        cpu.load_state_for_processor_test(0x30, 0, 0, 0xEF, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 2);
        assert_eq!(cpu.pc(), 0x0202);
        assert_eq!(cpu.a(), 0x20);
        assert!(cpu.flag(FLAG_CARRY)); // Carry is set on no borrow
        assert!(!cpu.flag(FLAG_OVERFLOW));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn sbc_a_imm_with_borrow_when_carry_clear() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xA8, 0x10]); // SBC A,#$10
        cpu.load_state_for_processor_test(0x05, 0, 0, 0xEF, 0x0200, 0);

        let _cycles = cpu.step(&mut bus);

        assert_eq!(cpu.a(), 0xF4);
        assert!(!cpu.flag(FLAG_CARRY)); // Carry is clear on borrow
        assert!(!cpu.flag(FLAG_OVERFLOW));
        assert!(cpu.flag(FLAG_NEGATIVE));
    }

    #[test]
    fn sbc_a_dp_opcode_a4_reads_direct_page_and_takes_three_cycles() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xA4, 0x10]); // SBC A,$10
        bus.set(0x0010, 0x01);
        cpu.load_state_for_processor_test(0x10, 0, 0, 0xEF, 0x0200, FLAG_CARRY);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.a(), 0x0F);
        assert!(cpu.flag(FLAG_CARRY));
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
        cpu.load_state_for_processor_test(0x00, 0xBB, 0xCC, 0xEF, 0x0200, FLAG_ZERO);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xF0); // SP incremented
        assert_eq!(cpu.a(), 0x88); // A loaded from stack
        assert!(cpu.flag(FLAG_ZERO)); // POP A must not alter flags
    }

    #[test]
    fn pop_x_pops_x_from_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xCE]); // POP X
        bus.write(0x01F0, 0x44); // Stack contains value
        cpu.load_state_for_processor_test(0xAA, 0x00, 0xCC, 0xEF, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xF0); // SP incremented
        assert_eq!(cpu.x(), 0x44); // X loaded from stack
        assert!(cpu.flag(FLAG_NEGATIVE)); // POP X must not alter flags
    }

    #[test]
    fn pop_y_pops_y_from_stack() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xEE]); // POP Y
        bus.write(0x01F0, 0x99); // Stack contains value
        cpu.load_state_for_processor_test(0xAA, 0xBB, 0x00, 0xEF, 0x0200, FLAG_ZERO);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert_eq!(cpu.sp(), 0xF0); // SP incremented
        assert_eq!(cpu.y(), 0x99); // Y loaded from stack
        assert!(cpu.flag(FLAG_ZERO)); // POP Y must not alter flags
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
    fn decw_dp_keeps_zero_clear_when_16bit_result_is_nonzero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x1A, 0x20]); // DECW $20
        bus.set(0x0020, 0x02);
        bus.set(0x0021, 0x00);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 6);
        assert_eq!(bus.read(0x0020), 0x01);
        assert_eq!(bus.read(0x0021), 0x00);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
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
    fn movw_ya_dp_keeps_zero_clear_when_low_byte_is_nonzero() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0xBA, 0x30]); // MOVW YA,$30
        bus.set(0x0030, 0xDB);
        bus.set(0x0031, 0x00);
        cpu.load_state_for_processor_test(0x00, 0x00, 0x00, 0xF0, 0x0200, FLAG_ZERO);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 5);
        assert_eq!(cpu.a(), 0xDB);
        assert_eq!(cpu.y(), 0x00);
        assert!(!cpu.flag(FLAG_ZERO));
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
    fn cmp_x_abs_preserves_overflow_flag() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x1E, 0x34, 0x12]); // CMP X,$1234
        bus.set(0x1234, 0x53);
        cpu.load_state_for_processor_test(0x00, 0x53, 0x00, 0xF0, 0x0200, FLAG_OVERFLOW);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 4);
        assert!(cpu.flag(FLAG_OVERFLOW));
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
    fn or_a_dp_opcode_04_reads_direct_page_and_takes_three_cycles() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x04, 0x10]); // OR A,$10
        bus.set(0x0010, 0x0F);
        cpu.load_state_for_processor_test(0x30, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.a(), 0x3F);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn and_a_dp_opcode_24_reads_direct_page_and_takes_three_cycles() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x24, 0x10]); // AND A,$10
        bus.set(0x0010, 0x0F);
        cpu.load_state_for_processor_test(0x3F, 0, 0, 0xF0, 0x0200, FLAG_NEGATIVE);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.a(), 0x0F);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
    }

    #[test]
    fn eor_a_dp_opcode_44_reads_direct_page_and_takes_three_cycles() {
        let mut cpu = Spc700::new();
        let mut bus = FlatRamBus::new();
        bus.load(0x0200, &[0x44, 0x10]); // EOR A,$10
        bus.set(0x0010, 0x0F);
        cpu.load_state_for_processor_test(0x55, 0, 0, 0xF0, 0x0200, 0);

        let cycles = cpu.step(&mut bus);

        assert_eq!(cycles, 3);
        assert_eq!(cpu.a(), 0x5A);
        assert!(!cpu.flag(FLAG_NEGATIVE));
        assert!(!cpu.flag(FLAG_ZERO));
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

        assert_eq!(cycles, 5);
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

        assert_eq!(cycles, 5);
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

        assert_eq!(first_cycles, 7);
        assert_eq!(second_cycles, 1);
        assert_eq!(bus.cycles(), 8);
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

        assert_eq!(first_cycles, 7);
        assert_eq!(second_cycles, 1);
        assert_eq!(bus.cycles(), 8);
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

        assert_eq!(first_cycles, 31);
        assert_eq!(second_cycles, 5);
        assert_eq!(bus.cycles(), 36);
    }

    // -----------------------------------------------------------------------
    // #2914: sub-instruction port visibility requires cycle scripts for every
    // opcode the blargg shell uses to poll or post through $F4-$F7. Each
    // scripted opcode must execute identically to the atomic step():
    // same registers/flags/PC and the same bus-op sequence, one bus op per
    // step_one_cycle call.
    // -----------------------------------------------------------------------

    fn assert_cycle_script_matches_atomic(setup: &[u8], insn: &[u8], pokes: &[(u16, u8)]) {
        let opcode = insn[0];
        assert!(
            Spc700::opcode_is_cycle_scripted(opcode),
            "opcode ${opcode:02X} must be cycle-scripted"
        );

        let build = |ops_recorded: bool| {
            let mut bus = RecordingBus::new();
            let mut program = setup.to_vec();
            program.extend_from_slice(insn);
            bus.load(0x0200, &program);
            for &(addr, value) in pokes {
                bus.ram[addr as usize] = value;
            }
            let mut cpu = Spc700::new();
            cpu.pc = 0x0200;
            // Execute the setup instructions atomically on both instances.
            let setup_end = 0x0200 + setup.len() as u16;
            while cpu.pc() < setup_end {
                cpu.step(&mut bus);
            }
            if !ops_recorded {
                bus.ops.clear();
            }
            (cpu, bus)
        };

        let (mut atomic_cpu, mut atomic_bus) = build(false);
        let (mut scripted_cpu, mut scripted_bus) = build(false);
        atomic_bus.ops.clear();
        scripted_bus.ops.clear();

        let cycles = atomic_cpu.step(&mut atomic_bus);

        let mut scripted_cycles = 0u8;
        loop {
            scripted_cpu.step_one_cycle(&mut scripted_bus);
            scripted_cycles += 1;
            if !scripted_cpu.has_in_progress_op() {
                break;
            }
        }

        assert_eq!(
            scripted_cycles, cycles,
            "op ${opcode:02X}: cycle-stepped op count must equal atomic cycles"
        );
        assert_eq!(
            scripted_bus.ops, atomic_bus.ops,
            "op ${opcode:02X}: bus-op sequences must match"
        );
        assert_eq!(scripted_cpu.a(), atomic_cpu.a(), "op ${opcode:02X}: A");
        assert_eq!(scripted_cpu.x(), atomic_cpu.x(), "op ${opcode:02X}: X");
        assert_eq!(scripted_cpu.y(), atomic_cpu.y(), "op ${opcode:02X}: Y");
        assert_eq!(
            scripted_cpu.psw(),
            atomic_cpu.psw(),
            "op ${opcode:02X}: PSW"
        );
        assert_eq!(scripted_cpu.pc(), atomic_cpu.pc(), "op ${opcode:02X}: PC");
        assert_eq!(
            &scripted_bus.ram[..],
            &atomic_bus.ram[..],
            "op ${opcode:02X}: RAM"
        );
    }

    #[test]
    fn cycle_scripts_match_atomic_execution_for_port_poll_and_post_opcodes() {
        // Register setup prefixes executed atomically before the instruction
        // under test: A=$42, X=$02, Y=$01.
        let regs: &[u8] = &[0xE8, 0x42, 0xCD, 0x02, 0x8D, 0x01];
        let pokes: &[(u16, u8)] = &[
            (0x00F4, 0x80),
            (0x00F5, 0x01),
            (0x00F6, 0x42),
            (0x0030, 0xF3),
            (0x0031, 0x00),
            (0x0032, 0xF4),
            (0x0033, 0x00),
        ];

        // Existing trampoline scripts must stay covered.
        assert_cycle_script_matches_atomic(regs, &[0x2F, 0x02], pokes); // BRA rel
        assert_cycle_script_matches_atomic(regs, &[0xE8, 0x99], pokes); // MOV A,#imm

        // Direct-page reads.
        assert_cycle_script_matches_atomic(regs, &[0xE4, 0xF4], pokes); // MOV A,dp
        assert_cycle_script_matches_atomic(regs, &[0xF8, 0xF5], pokes); // MOV X,dp
        assert_cycle_script_matches_atomic(regs, &[0xEB, 0xF6], pokes); // MOV Y,dp
        assert_cycle_script_matches_atomic(regs, &[0x64, 0xF4], pokes); // CMP A,dp
        assert_cycle_script_matches_atomic(regs, &[0x3E, 0xF4], pokes); // CMP X,dp
        assert_cycle_script_matches_atomic(regs, &[0x7E, 0xF4], pokes); // CMP Y,dp

        // Direct-page indexed reads.
        assert_cycle_script_matches_atomic(regs, &[0xF4, 0xF2], pokes); // MOV A,dp+X
        assert_cycle_script_matches_atomic(regs, &[0x74, 0xF2], pokes); // CMP A,dp+X

        // Absolute reads.
        assert_cycle_script_matches_atomic(regs, &[0xE5, 0xF4, 0x00], pokes); // MOV A,!abs
        assert_cycle_script_matches_atomic(regs, &[0x8F, 0x77, 0xF4], pokes); // MOV dp,#imm
        assert_cycle_script_matches_atomic(regs, &[0x78, 0xCC, 0xF4], pokes); // CMP dp,#imm
        assert_cycle_script_matches_atomic(regs, &[0xBA, 0xF4], pokes); // MOVW YA,dp

        // ALU A,mem/imm family (#2938 universal stepping, batch 1).
        for (insn, name) in [
            (&[0x04u8, 0xF4][..], "OR A,dp"),
            (&[0x24, 0xF4], "AND A,dp"),
            (&[0x44, 0xF4], "EOR A,dp"),
            (&[0x84, 0xF4], "ADC A,dp"),
            (&[0xA4, 0xF4], "SBC A,dp"),
            (&[0x14, 0xF2], "OR A,dp+X"),
            (&[0x34, 0xF2], "AND A,dp+X"),
            (&[0x54, 0xF2], "EOR A,dp+X"),
            (&[0x94, 0xF2], "ADC A,dp+X"),
            (&[0xB4, 0xF2], "SBC A,dp+X"),
            (&[0x05, 0xF4, 0x00], "OR A,!abs"),
            (&[0x25, 0xF4, 0x00], "AND A,!abs"),
            (&[0x45, 0xF4, 0x00], "EOR A,!abs"),
            (&[0x85, 0xF4, 0x00], "ADC A,!abs"),
            (&[0xA5, 0xF4, 0x00], "SBC A,!abs"),
            (&[0x08, 0x5A], "OR A,#imm"),
            (&[0x28, 0x5A], "AND A,#imm"),
            (&[0x48, 0x5A], "EOR A,#imm"),
            (&[0x88, 0x5A], "ADC A,#imm"),
            (&[0xA8, 0x5A], "SBC A,#imm"),
            (&[0x68, 0x5A], "CMP A,#imm"),
        ] {
            let _ = name;
            assert_cycle_script_matches_atomic(regs, insn, pokes);
        }

        // Implied / register / flag ops (#2938 batch 2).
        for op in [
            0x00u8, 0xBC, 0x9C, 0x3D, 0x1D, 0xFC, 0xDC, 0x7D, 0x5D, 0xDD, 0xFD, 0x9D, 0xBD, 0x60,
            0x80, 0x20, 0x40, 0xE0, 0xA0, 0xC0, 0xED,
        ] {
            assert_cycle_script_matches_atomic(regs, &[op], pokes);
        }

        // Conditional branches, taken and not-taken (#2938 batch 3). The
        // shared register file has Z clear / C clear / N clear / V clear, so
        // pick both polarities per flag.
        for op in [0xF0u8, 0xD0, 0xB0, 0x90, 0x30, 0x10, 0x70, 0x50] {
            assert_cycle_script_matches_atomic(regs, &[op, 0x02], pokes);
        }

        // Read-modify-write and dp,#imm families (#2938 batch 4).
        for insn in [
            &[0xABu8, 0x30][..],
            &[0x8B, 0x30],
            &[0x0B, 0x30],
            &[0x4B, 0x30],
            &[0x2B, 0x30],
            &[0x6B, 0x30],
            &[0xBB, 0x2E],
            &[0x9B, 0x2E],
            &[0x1B, 0x2E],
            &[0x5B, 0x2E],
            &[0x3B, 0x2E],
            &[0x7B, 0x2E],
            &[0xAC, 0x30, 0x00],
            &[0x8C, 0x30, 0x00],
            &[0x0C, 0x30, 0x00],
            &[0x4C, 0x30, 0x00],
            &[0x2C, 0x30, 0x00],
            &[0x6C, 0x30, 0x00],
            &[0x18, 0x5A, 0x30],
            &[0x38, 0x5A, 0x30],
            &[0x58, 0x5A, 0x30],
            &[0x98, 0x5A, 0x30],
            &[0xB8, 0x5A, 0x30],
        ] {
            assert_cycle_script_matches_atomic(regs, insn, pokes);
        }

        // (X) indirect, indexed loads/stores, ALU abs+X/Y (#2938 batch 5).
        for insn in [
            &[0xE6u8][..],
            &[0xBF],
            &[0xC6],
            &[0xAF],
            &[0x06],
            &[0x26],
            &[0x46],
            &[0x66],
            &[0x86],
            &[0xA6],
            &[0xC9, 0x30, 0x00],
            &[0xCC, 0x30, 0x00],
            &[0xD5, 0x30, 0x00],
            &[0xD6, 0x30, 0x00],
            &[0xD9, 0x2E],
            &[0xDB, 0x2E],
            &[0xF5, 0x2E, 0x00],
            &[0xF6, 0x2E, 0x00],
            &[0xF9, 0x2E],
            &[0xFB, 0x2E],
            &[0x15, 0x2E, 0x00],
            &[0x16, 0x2E, 0x00],
            &[0x35, 0x2E, 0x00],
            &[0x36, 0x2E, 0x00],
            &[0x55, 0x2E, 0x00],
            &[0x56, 0x2E, 0x00],
            &[0x75, 0x2E, 0x00],
            &[0x76, 0x2E, 0x00],
            &[0x95, 0x2E, 0x00],
            &[0x96, 0x2E, 0x00],
            &[0xB5, 0x2E, 0x00],
            &[0xB6, 0x2E, 0x00],
        ] {
            assert_cycle_script_matches_atomic(regs, insn, pokes);
        }

        // Bit ops, dp<-dp, (X)<-(Y) families (#2938 batch 6).
        for insn in [
            &[0x02u8, 0x30][..],
            &[0x22, 0x30],
            &[0x42, 0x30],
            &[0x62, 0x30],
            &[0x82, 0x30],
            &[0xA2, 0x30],
            &[0xC2, 0x30],
            &[0xE2, 0x30],
            &[0x12, 0x30],
            &[0x32, 0x30],
            &[0x52, 0x30],
            &[0x72, 0x30],
            &[0x92, 0x30],
            &[0xB2, 0x30],
            &[0xD2, 0x30],
            &[0xF2, 0x30],
            &[0x03, 0x30, 0x02],
            &[0x23, 0x30, 0x02],
            &[0x43, 0x30, 0x02],
            &[0x63, 0x30, 0x02],
            &[0x83, 0x30, 0x02],
            &[0xA3, 0x30, 0x02],
            &[0xC3, 0x30, 0x02],
            &[0xE3, 0x30, 0x02],
            &[0x13, 0x30, 0x02],
            &[0x33, 0x30, 0x02],
            &[0x53, 0x30, 0x02],
            &[0x73, 0x30, 0x02],
            &[0x93, 0x30, 0x02],
            &[0xB3, 0x30, 0x02],
            &[0xD3, 0x30, 0x02],
            &[0xF3, 0x30, 0x02],
            &[0x09, 0x31, 0x30],
            &[0x29, 0x31, 0x30],
            &[0x49, 0x31, 0x30],
            &[0x69, 0x31, 0x30],
            &[0x89, 0x31, 0x30],
            &[0xA9, 0x31, 0x30],
            &[0xFA, 0x31, 0x30],
            &[0x19],
            &[0x39],
            &[0x59],
            &[0x79],
            &[0x99],
            &[0xB9],
        ] {
            assert_cycle_script_matches_atomic(regs, insn, pokes);
        }

        // Stack, word ops, indirect ALU, CBNE/DBNZ (#2938 batch 7).
        for insn in [
            &[0x2Du8][..],
            &[0x4D],
            &[0x6D],
            &[0x0D],
            &[0xAE],
            &[0xCE],
            &[0xEE],
            &[0x8E],
            &[0xDA, 0x30],
            &[0x3A, 0x30],
            &[0x1A, 0x30],
            &[0x7A, 0x30],
            &[0x9A, 0x30],
            &[0x5A, 0x30],
            &[0x07, 0x30],
            &[0x27, 0x30],
            &[0x47, 0x30],
            &[0x87, 0x30],
            &[0xA7, 0x30],
            &[0x17, 0x30],
            &[0x37, 0x30],
            &[0x57, 0x30],
            &[0x97, 0x30],
            &[0xB7, 0x30],
            &[0x2E, 0x30, 0x02],
            &[0xDE, 0x2E, 0x02],
            &[0x6E, 0x30, 0x02],
            &[0xFE, 0x02],
        ] {
            assert_cycle_script_matches_atomic(regs, insn, pokes);
        }

        // Coverage pin for #2938: bump as families are scripted; the goal
        // is 256, at which point the atomic path is deleted.
        let scripted = (0u16..=0xFF)
            .filter(|&op| Spc700::opcode_is_cycle_scripted(op as u8))
            .count();
        assert_eq!(
            scripted, 204,
            "cycle-script coverage changed; update the pin"
        );
        assert_cycle_script_matches_atomic(regs, &[0xE9, 0xF5, 0x00], pokes); // MOV X,!abs
        assert_cycle_script_matches_atomic(regs, &[0xEC, 0xF6, 0x00], pokes); // MOV Y,!abs
        assert_cycle_script_matches_atomic(regs, &[0x65, 0xF4, 0x00], pokes); // CMP A,!abs

        // Indirect reads through a direct-page pointer.
        assert_cycle_script_matches_atomic(regs, &[0xE7, 0x30], pokes); // MOV A,[dp+X]
        assert_cycle_script_matches_atomic(regs, &[0x67, 0x30], pokes); // CMP A,[dp+X]
        assert_cycle_script_matches_atomic(regs, &[0xF7, 0x30], pokes); // MOV A,[dp]+Y
        assert_cycle_script_matches_atomic(regs, &[0x77, 0x30], pokes); // CMP A,[dp]+Y

        // Port posts (stores).
        assert_cycle_script_matches_atomic(regs, &[0xC4, 0xF4], pokes); // MOV dp,A
        assert_cycle_script_matches_atomic(regs, &[0xD8, 0xF5], pokes); // MOV dp,X
        assert_cycle_script_matches_atomic(regs, &[0xCB, 0xF6], pokes); // MOV dp,Y
        assert_cycle_script_matches_atomic(regs, &[0xD4, 0xF2], pokes); // MOV dp+X,A
        assert_cycle_script_matches_atomic(regs, &[0xC5, 0xF4, 0x00], pokes); // MOV !abs,A
    }
}
