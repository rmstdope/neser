//! ARM7TDMI CPU core wrapper.
//!
//! This struct ties the register file, the bus and the ARM/Thumb instruction
//! executors together. It implements the fetch / decode / execute loop, a
//! simple S/N cycle accounting model, exception entry vectors and IRQ/FIQ
//! interrupt dispatch.
//!
//! The implementation mirrors the structure of the existing NES (`Cpu` in
//! `src/nes/cpu/cpu.rs`) and Game Boy (`Sm83` in `src/gb/cpu/sm83.rs`) cores
//! to keep the module layout consistent across emulated systems.

// Many literals in tests below are deliberately written using bit fields that
// mirror the ARM/Thumb instruction encoding for readability.
#![allow(clippy::unusual_byte_groupings)]
#![allow(clippy::identity_op)]

use super::arm;
use super::bus::Bus;
use super::registers::{CpuMode, FLAG_F, FLAG_I, FLAG_T, Registers};
use super::thumb;

/// Address of each ARM exception vector in the BIOS region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionVector {
    Reset = 0x00,
    Undefined = 0x04,
    SoftwareInterrupt = 0x08,
    PrefetchAbort = 0x0C,
    DataAbort = 0x10,
    Irq = 0x18,
    Fiq = 0x1C,
}

/// Result of a high-level SWI emulation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HleSwiResult {
    /// SWI not handled — fall through to normal BIOS dispatch.
    NotHandled,
    /// SWI handled — advance PC past the SWI instruction.
    Handled,
    /// SWI handled but PC should stay at the SWI instruction so it
    /// re-executes after the next IRQ handler returns (used for
    /// IntrWait/VBlankIntrWait looping).
    HandledRepeatSwi,
}

/// ARM7TDMI CPU core.
pub struct Arm7tdmi {
    /// Register file.
    pub regs: Registers,
    /// Cumulative cycle counter.
    pub cycles: u64,
    /// Pending IRQ line (rising-edge triggered).
    irq_pending: bool,
    /// Pending FIQ line (rising-edge triggered).
    fiq_pending: bool,
    /// Whether the prefetch buffers contain valid instructions for the
    /// current PC/state.
    prefetch_valid: bool,
    /// Three-entry ARM prefetch buffer: [current, next, next-next].
    prefetch_arm: [u32; 3],
    /// Three-entry Thumb prefetch buffer: [current, next, next-next].
    prefetch_thumb: [u16; 3],
    /// CPU is halted (e.g. via SWI Halt/VBlankIntrWait). While halted the
    /// CPU consumes idle cycles until an enabled interrupt wakes it.
    halted: bool,
    /// When true, known SWI calls are handled via high-level emulation
    /// instead of dispatching to the BIOS vector.
    hle_swi: bool,
    /// IntrWait/VBlankIntrWait interrupt mask. `Some(mask)` while the CPU
    /// is looping, waiting for specific interrupt flags in BIOS_IF
    /// (0x03007FF8). `None` when not inside an IntrWait loop.
    /// The SWI re-executes after each IRQ handler returns; when the
    /// requested flags appear in BIOS_IF, they are cleared and this is
    /// reset to `None`, allowing PC to advance past the SWI.
    intr_wait_mask: Option<u16>,
    /// Debug counter: number of times dispatch_irq has been called.
    #[cfg(test)]
    irq_dispatch_count: u64,
    /// Debug: tracks unhandled SWI numbers seen when HLE is active.
    #[cfg(test)]
    pub unhandled_swis: Vec<u8>,
}

impl Default for Arm7tdmi {
    fn default() -> Self {
        Self::new()
    }
}

impl Arm7tdmi {
    /// Create a new CPU core. Enters Supervisor mode at the reset vector with
    /// IRQ and FIQ both masked, mirroring the real hardware reset behaviour.
    pub fn new() -> Self {
        let mut regs = Registers::new();
        regs.switch_mode(CpuMode::Supervisor);
        regs.cpsr |= FLAG_I | FLAG_F;
        regs.set_thumb(false);
        regs.r[15] = ExceptionVector::Reset as u32;
        Self {
            regs,
            cycles: 0,
            irq_pending: false,
            fiq_pending: false,
            prefetch_valid: false,
            prefetch_arm: [0; 3],
            prefetch_thumb: [0; 3],
            halted: false,
            hle_swi: false,
            intr_wait_mask: None,
            #[cfg(test)]
            irq_dispatch_count: 0,
            #[cfg(test)]
            unhandled_swis: Vec::new(),
        }
    }

    fn refill_prefetch<B: Bus>(&mut self, bus: &mut B) {
        let pc = self.regs.r[15];
        if self.thumb() {
            self.prefetch_thumb[0] = bus.read16(pc);
            self.prefetch_thumb[1] = bus.read16(pc.wrapping_add(2));
            self.prefetch_thumb[2] = bus.read16(pc.wrapping_add(4));
        } else {
            self.prefetch_arm[0] = bus.read32(pc);
            self.prefetch_arm[1] = bus.read32(pc.wrapping_add(4));
            self.prefetch_arm[2] = bus.read32(pc.wrapping_add(8));
        }
        self.prefetch_valid = true;
    }

    /// Whether the CPU is currently executing in Thumb state.
    pub fn thumb(&self) -> bool {
        self.regs.thumb()
    }

    /// Raise an external IRQ. Will be dispatched on the next `step` if not
    /// masked by the I flag.
    pub fn raise_irq(&mut self) {
        self.irq_pending = true;
    }

    /// Clear a previously raised IRQ.
    pub fn clear_irq(&mut self) {
        self.irq_pending = false;
    }

    /// Raise an external FIQ. Will be dispatched on the next `step` if not
    /// masked by the F flag.
    pub fn raise_fiq(&mut self) {
        self.fiq_pending = true;
    }

    /// Clear a previously raised FIQ.
    pub fn clear_fiq(&mut self) {
        self.fiq_pending = false;
    }

    /// Put the CPU into halt state. While halted the CPU idles (consuming
    /// 1 cycle per step) until an enabled interrupt wakes it.
    pub fn halt(&mut self) {
        self.halted = true;
    }

    /// Returns true if the CPU is currently in halt state.
    #[cfg(test)]
    pub fn is_halted(&self) -> bool {
        self.halted
    }

    /// Enable or disable high-level emulation of known SWI calls.
    /// When enabled, SWI 0x02 (Halt) and SWI 0x05 (VBlankIntrWait)
    /// are handled in Rust rather than dispatching to the BIOS vector.
    pub fn set_hle_swi(&mut self, enabled: bool) {
        self.hle_swi = enabled;
    }

    /// Debug: how many times dispatch_irq has been called.
    #[cfg(test)]
    pub fn irq_dispatch_count(&self) -> u64 {
        self.irq_dispatch_count
    }

    /// Return the size of the instruction currently pointed to by R15.
    fn instr_size(&self) -> u32 {
        if self.thumb() { 2 } else { 4 }
    }

    /// Run a single instruction. Returns the number of cycles consumed.
    ///
    /// The PC value visible to instructions during execution is two
    /// instructions ahead of the address being executed, matching the
    /// ARM7TDMI three-stage pipeline.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        // Service pending interrupts first (FIQ has higher priority).
        // An interrupt also wakes the CPU from halt state.
        if self.fiq_pending && !self.regs.f_flag() {
            self.halted = false;
            self.dispatch_fiq();
            return 3;
        }
        if self.irq_pending && !self.regs.i_flag() {
            self.halted = false;
            self.dispatch_irq();
            return 3;
        }

        // While halted, idle for 1 cycle without executing instructions.
        if self.halted {
            self.cycles = self.cycles.wrapping_add(1);
            return 1;
        }

        if !self.prefetch_valid {
            self.refill_prefetch(bus);
        }

        let exec_pc = self.regs.r[15];
        let cycles = if self.thumb() {
            // PC during Thumb execution should read as exec_pc + 4.
            self.regs.r[15] = exec_pc.wrapping_add(4);
            let raw = self.prefetch_thumb[0];
            let mut outcome = thumb::execute(&mut self.regs, bus, raw);
            // Try HLE before normal SWI dispatch.
            if outcome.swi {
                match self.try_hle_swi(bus) {
                    HleSwiResult::Handled => {
                        outcome.swi = false;
                        outcome.branched = false;
                    }
                    HleSwiResult::HandledRepeatSwi => {
                        outcome.swi = false;
                        outcome.branched = true;
                        self.regs.r[15] = exec_pc;
                        self.prefetch_valid = false;
                    }
                    HleSwiResult::NotHandled => {}
                }
            }
            if outcome.undefined {
                self.dispatch_undefined(exec_pc);
            } else if outcome.swi {
                self.dispatch_swi(exec_pc);
            } else if outcome.branched {
                self.prefetch_valid = false;
            } else if !outcome.branched {
                self.regs.r[15] = exec_pc.wrapping_add(2);
                self.prefetch_thumb[0] = self.prefetch_thumb[1];
                self.prefetch_thumb[1] = self.prefetch_thumb[2];
                self.prefetch_thumb[2] = bus.read16(exec_pc.wrapping_add(6));
            }
            outcome.cycles as u32
        } else {
            // PC during ARM execution should read as exec_pc + 8.
            self.regs.r[15] = exec_pc.wrapping_add(8);
            let raw = self.prefetch_arm[0];
            let mut outcome = arm::execute(&mut self.regs, bus, raw);
            // Try HLE before normal SWI dispatch.
            if outcome.swi {
                match self.try_hle_swi(bus) {
                    HleSwiResult::Handled => {
                        outcome.swi = false;
                        outcome.branched = false;
                    }
                    HleSwiResult::HandledRepeatSwi => {
                        outcome.swi = false;
                        outcome.branched = true;
                        self.regs.r[15] = exec_pc;
                        self.prefetch_valid = false;
                    }
                    HleSwiResult::NotHandled => {}
                }
            }
            if outcome.undefined {
                self.dispatch_undefined(exec_pc);
            } else if outcome.swi {
                self.dispatch_swi(exec_pc);
            } else if outcome.branched {
                self.prefetch_valid = false;
            } else if !outcome.branched {
                self.regs.r[15] = exec_pc.wrapping_add(4);
                self.prefetch_arm[0] = self.prefetch_arm[1];
                self.prefetch_arm[1] = self.prefetch_arm[2];
                self.prefetch_arm[2] = bus.read32(exec_pc.wrapping_add(12));
            }
            outcome.cycles as u32
        };

        self.cycles = self.cycles.wrapping_add(cycles as u64);
        cycles
    }

    /// Dispatch a software interrupt: switch to Supervisor mode, save state
    /// and jump to the SWI vector.
    fn dispatch_swi(&mut self, instr_pc: u32) {
        let return_addr = instr_pc.wrapping_add(self.instr_size());
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Supervisor);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = return_addr;
        self.regs.cpsr |= FLAG_I;
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::SoftwareInterrupt as u32;
        self.prefetch_valid = false;
    }

    /// Dispatch an undefined instruction exception: switch to Undefined mode,
    /// save state and jump to the undefined vector (0x04).
    fn dispatch_undefined(&mut self, instr_pc: u32) {
        let return_addr = instr_pc.wrapping_add(self.instr_size());
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Undefined);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = return_addr;
        self.regs.cpsr |= FLAG_I;
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::Undefined as u32;
        self.prefetch_valid = false;
    }

    /// Dispatch an IRQ: switch to IRQ mode, save state and jump to 0x18.
    fn dispatch_irq(&mut self) {
        #[cfg(test)]
        {
            self.irq_dispatch_count += 1;
        }
        // Return address per ARM7TDMI spec and GBATek:
        //   LR_irq = address_of_next_instruction + 4
        // In our pipeline model R15 holds the address of the next instruction
        // to execute, so LR_irq = R15 + 4 regardless of ARM/THUMB state.
        // (Return via `SUBS PC, LR, #4` restores PC to the interrupted
        // instruction.)
        let next_pc = self.regs.r[15].wrapping_add(4);
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Irq);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = next_pc;
        self.regs.cpsr |= FLAG_I;
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::Irq as u32;
        self.prefetch_valid = false;
        self.cycles = self.cycles.wrapping_add(3);
        // The pending line is treated as a latched edge: clear on dispatch so
        // unmasking I later does not spuriously re-enter the handler.
        self.irq_pending = false;
    }

    /// Dispatch an FIQ: switch to FIQ mode, mask both IRQ and FIQ, jump to 0x1C.
    fn dispatch_fiq(&mut self) {
        // Same return-address rule as IRQ: LR_fiq = R15 + 4 regardless of
        // ARM/THUMB state. Return via `SUBS PC, LR, #4`.
        let next_pc = self.regs.r[15].wrapping_add(4);
        let cpsr = self.regs.cpsr;
        self.regs.switch_mode(CpuMode::Fiq);
        self.regs.set_spsr(cpsr);
        self.regs.r[14] = next_pc;
        self.regs.cpsr |= FLAG_I | FLAG_F;
        self.regs.cpsr &= !FLAG_T;
        self.regs.r[15] = ExceptionVector::Fiq as u32;
        self.prefetch_valid = false;
        self.cycles = self.cycles.wrapping_add(3);
        // Latched-edge semantics: clear pending on dispatch.
        self.fiq_pending = false;
    }

    /// BIOS_IF address in IWRAM where the BIOS IRQ dispatcher records
    /// which interrupts have fired (used by IntrWait/VBlankIntrWait).
    const BIOS_IF_ADDR: u32 = 0x0300_7FF8;

    /// Try to handle a SWI via high-level emulation.
    ///
    /// Returns [`HleSwiResult`] to tell the caller whether and how the
    /// SWI was handled:
    /// - `Handled`: advance PC past the SWI (normal one-shot HLE).
    /// - `HandledRepeatSwi`: keep PC at the SWI so it re-executes after
    ///   the next IRQ handler returns (IntrWait/VBlankIntrWait loop).
    /// - `NotHandled`: fall through to normal BIOS dispatch.
    ///
    /// Currently handles Halt (0x02), IntrWait (0x04),
    /// VBlankIntrWait (0x05), Div (0x06), DivArm (0x07), Sqrt (0x08),
    /// ArcTan (0x09), ArcTan2 (0x0A), CpuSet (0x0B), and
    /// CpuFastSet (0x0C).
    fn try_hle_swi<B: Bus>(&mut self, bus: &mut B) -> HleSwiResult {
        if !self.hle_swi {
            return HleSwiResult::NotHandled;
        }

        // Extract SWI number from the prefetched instruction (still in slot 0).
        let swi_number = if self.thumb() {
            // THUMB SWI: comment byte in bits 7:0
            (self.prefetch_thumb[0] & 0xFF) as u8
        } else {
            // ARM SWI: GBA convention uses bits 23:16 for the SWI number
            ((self.prefetch_arm[0] >> 16) & 0xFF) as u8
        };

        match swi_number {
            0x02 => {
                // Halt: halt CPU until any enabled interrupt fires.
                self.halted = true;
                HleSwiResult::Handled
            }
            0x04 => {
                // IntrWait(r0=discard_old, r1=flag_mask):
                self.hle_intr_wait(bus, self.regs.r[0], self.regs.r[1] as u16)
            }
            0x05 => {
                // VBlankIntrWait ≡ IntrWait(1, 0x0001):
                self.hle_intr_wait(bus, 1, 0x0001)
            }
            0x0B => {
                // CpuSet: memory copy/fill using 16-bit or 32-bit units.
                self.hle_cpu_set(bus);
                HleSwiResult::Handled
            }
            0x0C => {
                // CpuFastSet: memory copy/fill using 32-bit units, count
                // rounded up to a multiple of 8.
                self.hle_cpu_fast_set(bus);
                HleSwiResult::Handled
            }
            0x06 => {
                // Div: signed integer division.
                self.hle_div();
                HleSwiResult::Handled
            }
            0x07 => {
                // DivArm: same as Div but with swapped arguments.
                self.hle_div_arm();
                HleSwiResult::Handled
            }
            0x08 => {
                // Sqrt: integer square root.
                self.hle_sqrt();
                HleSwiResult::Handled
            }
            0x09 => {
                // ArcTan: fixed-point arctangent.
                self.hle_arctan();
                HleSwiResult::Handled
            }
            0x0A => {
                // ArcTan2: fixed-point atan2.
                self.hle_arctan2();
                HleSwiResult::Handled
            }
            _ => {
                #[cfg(test)]
                {
                    if !self.unhandled_swis.contains(&swi_number) {
                        self.unhandled_swis.push(swi_number);
                    }
                }
                HleSwiResult::NotHandled
            }
        }
    }

    /// HLE implementation of IntrWait / VBlankIntrWait.
    ///
    /// On first entry (`intr_wait_mask` is `None`):
    /// 1. If `discard_old != 0`: clear requested flags from BIOS_IF.
    /// 2. Set IME = 1 and halt the CPU.
    /// 3. Return `HandledRepeatSwi` so PC stays at the SWI instruction.
    ///
    /// On re-entry after an IRQ handler returns (`intr_wait_mask` is `Some`):
    /// - If any requested flags are set in BIOS_IF: clear them, reset
    ///   the mask to `None`, and return `Handled` to advance PC.
    /// - Otherwise: re-halt and return `HandledRepeatSwi`.
    fn hle_intr_wait<B: Bus>(
        &mut self,
        bus: &mut B,
        discard_old: u32,
        flag_mask: u16,
    ) -> HleSwiResult {
        if let Some(mask) = self.intr_wait_mask {
            // Re-entry: check if the desired interrupt(s) fired.
            let bios_if = bus.read16(Self::BIOS_IF_ADDR);
            if bios_if & mask != 0 {
                // Acknowledged — clear the matched flags and resume.
                bus.write16(Self::BIOS_IF_ADDR, bios_if & !mask);
                self.intr_wait_mask = None;
                HleSwiResult::Handled
            } else {
                // Not yet — re-halt and wait for the next IRQ.
                self.halted = true;
                HleSwiResult::HandledRepeatSwi
            }
        } else {
            // First entry: set up the wait.
            self.intr_wait_mask = Some(flag_mask);
            if discard_old != 0 {
                let bios_if = bus.read16(Self::BIOS_IF_ADDR);
                bus.write16(Self::BIOS_IF_ADDR, bios_if & !flag_mask);
            }
            bus.write16(0x0400_0208, 1); // IME = 1
            self.halted = true;
            HleSwiResult::HandledRepeatSwi
        }
    }

    /// HLE implementation of SWI 0x0B — CpuSet.
    ///
    /// r0 = source address, r1 = destination address,
    /// r2 = count + flags (bit 26: 0=16-bit, 1=32-bit; bit 24: 0=copy, 1=fill).
    fn hle_cpu_set<B: Bus>(&mut self, bus: &mut B) {
        let src = self.regs.r[0];
        let dst = self.regs.r[1];
        let ctrl = self.regs.r[2];
        let count = ctrl & 0x001F_FFFF;
        let word_mode = ctrl & (1 << 26) != 0;
        let fill_mode = ctrl & (1 << 24) != 0;

        if word_mode {
            let src_aligned = src & !3;
            let dst_aligned = dst & !3;
            let fill_val = if fill_mode {
                bus.read32(src_aligned)
            } else {
                0
            };
            for i in 0..count {
                let val = if fill_mode {
                    fill_val
                } else {
                    bus.read32(src_aligned.wrapping_add(i * 4))
                };
                bus.write32(dst_aligned.wrapping_add(i * 4), val);
            }
        } else {
            let src_aligned = src & !1;
            let dst_aligned = dst & !1;
            let fill_val = if fill_mode {
                bus.read16(src_aligned)
            } else {
                0
            };
            for i in 0..count {
                let val = if fill_mode {
                    fill_val
                } else {
                    bus.read16(src_aligned.wrapping_add(i * 2))
                };
                bus.write16(dst_aligned.wrapping_add(i * 2), val);
            }
        }
    }

    /// HLE implementation of SWI 0x0C — CpuFastSet.
    ///
    /// r0 = source address, r1 = destination address,
    /// r2 = count + flags (bit 24: 0=copy, 1=fill).
    /// Always 32-bit transfers; count is rounded up to a multiple of 8.
    fn hle_cpu_fast_set<B: Bus>(&mut self, bus: &mut B) {
        let src = self.regs.r[0] & !3;
        let dst = self.regs.r[1] & !3;
        let ctrl = self.regs.r[2];
        let raw_count = ctrl & 0x001F_FFFF;
        let count = (raw_count + 7) & !7; // round up to multiple of 8
        let fill_mode = ctrl & (1 << 24) != 0;

        let fill_val = if fill_mode { bus.read32(src) } else { 0 };
        for i in 0..count {
            let val = if fill_mode {
                fill_val
            } else {
                bus.read32(src.wrapping_add(i * 4))
            };
            bus.write32(dst.wrapping_add(i * 4), val);
        }
    }

    /// HLE implementation of SWI 0x06 — Div.
    ///
    /// r0 = numerator, r1 = denominator →
    /// r0 = quotient, r1 = remainder, r3 = abs(quotient).
    fn hle_div(&mut self) {
        let num = self.regs.r[0] as i32;
        let denom = self.regs.r[1] as i32;

        if denom == 0 {
            // GBA BIOS div-by-zero behavior: return sign(num) as quotient.
            self.regs.r[0] = if num < 0 { (-1i32) as u32 } else { 1 };
            self.regs.r[1] = num as u32;
            self.regs.r[3] = 1;
        } else if denom == -1 && num == i32::MIN {
            // Overflow: INT_MIN / -1 can't be represented.
            self.regs.r[0] = i32::MIN as u32;
            self.regs.r[1] = 0;
            self.regs.r[3] = i32::MIN as u32;
        } else {
            let quot = num / denom;
            let rem = num % denom;
            self.regs.r[0] = quot as u32;
            self.regs.r[1] = rem as u32;
            self.regs.r[3] = quot.unsigned_abs();
        }
    }

    /// HLE implementation of SWI 0x07 — DivArm.
    ///
    /// Same as Div but r0 = denominator, r1 = numerator (swapped).
    fn hle_div_arm(&mut self) {
        self.regs.r.swap(0, 1);
        self.hle_div();
    }

    /// HLE implementation of SWI 0x08 — Sqrt.
    ///
    /// r0 = unsigned 32-bit value → r0 = integer square root.
    fn hle_sqrt(&mut self) {
        let x = self.regs.r[0];
        self.regs.r[0] = (x as f64).sqrt() as u32;
    }

    /// HLE implementation of SWI 0x09 — ArcTan.
    ///
    /// r0 = tan (signed fixed-point 1.14) → r0 = angle, r1 = intermediate a,
    /// r3 = accumulated coefficient b.
    ///
    /// Uses the same Taylor-series coefficients as the real GBA BIOS.
    fn hle_arctan(&mut self) {
        let i = self.regs.r[0] as i32;
        let a = (i.wrapping_mul(i) >> 14).wrapping_neg();
        let mut b = ((0xA9_i32).wrapping_mul(a) >> 14).wrapping_add(0x390);
        b = (b.wrapping_mul(a) >> 14).wrapping_add(0x91C);
        b = (b.wrapping_mul(a) >> 14).wrapping_add(0xFB6);
        b = (b.wrapping_mul(a) >> 14).wrapping_add(0x16AA);
        b = (b.wrapping_mul(a) >> 14).wrapping_add(0x2081);
        b = (b.wrapping_mul(a) >> 14).wrapping_add(0x3651);
        b = (b.wrapping_mul(a) >> 14).wrapping_add(0xA2F9);
        self.regs.r[0] = (i.wrapping_mul(b) >> 16) as u32;
        self.regs.r[1] = a as u32;
        self.regs.r[3] = b as u32;
    }

    /// HLE implementation of SWI 0x0A — ArcTan2.
    ///
    /// r0 = x, r1 = y → r0 = angle in [0, 0xFFFF] (full circle).
    /// Sets r3 = 0x170 to match the real BIOS's register clobber.
    fn hle_arctan2(&mut self) {
        let x = self.regs.r[0] as i32;
        let y = self.regs.r[1] as i32;

        if y == 0 {
            self.regs.r[0] = if x >= 0 { 0 } else { 0x8000 };
            self.regs.r[3] = 0x170;
            return;
        }
        if x == 0 {
            self.regs.r[0] = if y >= 0 { 0x4000 } else { 0xC000 };
            self.regs.r[3] = 0x170;
            return;
        }

        if y >= 0 {
            if x >= 0 {
                if x >= y {
                    self.regs.r[0] = ((y as i64) << 14).wrapping_div(x as i64) as i32 as u32;
                    self.hle_arctan();
                } else {
                    self.regs.r[0] = ((x as i64) << 14).wrapping_div(y as i64) as i32 as u32;
                    self.hle_arctan();
                    self.regs.r[0] = 0x4000_u32.wrapping_sub(self.regs.r[0]);
                }
            } else if (-x) >= y {
                self.regs.r[0] = ((y as i64) << 14).wrapping_div(x as i64) as i32 as u32;
                self.hle_arctan();
                self.regs.r[0] = self.regs.r[0].wrapping_add(0x8000);
            } else {
                self.regs.r[0] = ((x as i64) << 14).wrapping_div(y as i64) as i32 as u32;
                self.hle_arctan();
                self.regs.r[0] = 0x4000_u32.wrapping_sub(self.regs.r[0]);
            }
        } else if x <= 0 {
            if (-x) > (-y) {
                self.regs.r[0] = ((y as i64) << 14).wrapping_div(x as i64) as i32 as u32;
                self.hle_arctan();
                self.regs.r[0] = self.regs.r[0].wrapping_add(0x8000);
            } else {
                self.regs.r[0] = ((x as i64) << 14).wrapping_div(y as i64) as i32 as u32;
                self.hle_arctan();
                self.regs.r[0] = 0xC000_u32.wrapping_sub(self.regs.r[0]);
            }
        } else if x >= (-y) {
            self.regs.r[0] = ((y as i64) << 14).wrapping_div(x as i64) as i32 as u32;
            self.hle_arctan();
            self.regs.r[0] = self.regs.r[0].wrapping_add(0x10000);
        } else {
            self.regs.r[0] = ((x as i64) << 14).wrapping_div(y as i64) as i32 as u32;
            self.hle_arctan();
            self.regs.r[0] = 0xC000_u32.wrapping_sub(self.regs.r[0]);
        }

        self.regs.r[0] &= 0xFFFF;
        self.regs.r[3] = 0x170;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gba::cpu::bus::RamBus;
    use crate::gba::cpu::registers::FLAG_Z;

    fn write_arm_word(bus: &mut RamBus, addr: u32, word: u32) {
        bus.write_word(addr, word);
    }

    #[test]
    fn arm_add_test_vector_executes() {
        // Test vector: R0=1, R1=2; ADD R2,R0,R1 -> R2=3, flags unchanged.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.r[15] = 0x0;
        cpu.regs.r[0] = 1;
        cpu.regs.r[1] = 2;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = RamBus::new(0x100);
        // ADD R2, R0, R1 (cond=AL, opcode=4, S=0, Rn=R0, Rd=R2, Rm=R1).
        // Encoding: cond(0xE) | 000 | opcode(0100) | S(0) | Rn(0) | Rd(2)
        //           | shift_imm(0) | shift_type(00) | 0 | Rm(1)
        let instr = (0xE_u32 << 28)
            | (0x4_u32 << 21)
            | (0_u32 << 20)
            | (0_u32 << 16)
            | (2_u32 << 12)
            | 1_u32;
        write_arm_word(&mut bus, 0x0, instr);

        cpu.step(&mut bus);
        assert_eq!(cpu.regs.r[2], 3);
        assert_eq!(cpu.regs.cpsr, cpsr_before);
        // PC advanced past the executed instruction.
        assert_eq!(cpu.regs.r[15], 0x4);
    }

    #[test]
    fn irq_dispatch_test_vector() {
        // Test vector: CPU in USR mode, IRQ raised
        // -> Mode → IRQ, SPSR_irq = old CPSR, LR_irq = PC+4, IRQ masked.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I; // unmask IRQ
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        cpu.raise_irq();
        let mut bus = RamBus::new(0x100);
        cpu.step(&mut bus);

        assert_eq!(cpu.regs.mode(), CpuMode::Irq);
        assert_eq!(cpu.regs.spsr(), cpsr_before);
        assert_eq!(cpu.regs.r[14], 0x100 + 4); // ARM mode -> +4
        assert!(cpu.regs.i_flag(), "IRQ must be masked after dispatch");
        assert_eq!(cpu.regs.r[15], 0x18);
    }

    #[test]
    fn fiq_dispatch_test_vector() {
        // Test vector: CPU in USR mode, FIQ raised
        // -> Mode → FIQ, banked R8–R12 active, FIQ+IRQ masked.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !(FLAG_I | FLAG_F);
        for i in 8..=12 {
            cpu.regs.r[i] = 0x1000 + i as u32; // USR R8..R12
        }
        cpu.regs.r[15] = 0x200;

        cpu.raise_fiq();
        let mut bus = RamBus::new(0x100);
        cpu.step(&mut bus);

        assert_eq!(cpu.regs.mode(), CpuMode::Fiq);
        // Banked R8..R12 are now the FIQ bank (initial zeros).
        for i in 8..=12 {
            assert_eq!(cpu.regs.r[i], 0);
        }
        assert!(cpu.regs.f_flag(), "FIQ must be masked after dispatch");
        assert!(
            cpu.regs.i_flag(),
            "IRQ must also be masked after FIQ dispatch"
        );
        assert_eq!(cpu.regs.r[15], 0x1C);

        // Returning to USR restores the original USR R8..R12.
        cpu.regs.switch_mode(CpuMode::User);
        for i in 8..=12 {
            assert_eq!(cpu.regs.r[i], 0x1000 + i as u32);
        }
    }

    #[test]
    fn irq_does_not_dispatch_when_masked() {
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr |= FLAG_I; // mask IRQ
        cpu.regs.r[15] = 0x0;
        cpu.raise_irq();
        let mut bus = RamBus::new(0x100);
        // Code: NOP-equivalent (MOV R0,R0)
        let instr = (0xE_u32 << 28) | (0xD_u32 << 21); // MOV R0,R0
        bus.write_word(0x0, instr);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.mode(), CpuMode::User);
    }

    #[test]
    fn dispatched_irq_pending_is_cleared() {
        // Latched-edge semantics: once dispatched, IRQ pending must clear so
        // that unmasking I after the handler doesn't immediately re-enter.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I;
        cpu.regs.r[15] = 0x100;
        cpu.raise_irq();
        let mut bus = RamBus::new(0x100);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.mode(), CpuMode::Irq);

        // Simulate "return from handler": switch back to USR and unmask I.
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I;
        // Bus must contain a valid ARM word so the next step can decode it.
        bus.write_word(0x0, (0xE_u32 << 28) | (0xD_u32 << 21)); // MOV R0,R0
        cpu.regs.r[15] = 0x0;
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.mode(),
            CpuMode::User,
            "IRQ pending must be cleared on dispatch"
        );
    }

    #[test]
    fn dispatched_fiq_pending_is_cleared() {
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !(FLAG_I | FLAG_F);
        cpu.regs.r[15] = 0x100;
        cpu.raise_fiq();
        let mut bus = RamBus::new(0x100);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.mode(), CpuMode::Fiq);

        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !(FLAG_I | FLAG_F);
        bus.write_word(0x0, (0xE_u32 << 28) | (0xD_u32 << 21));
        cpu.regs.r[15] = 0x0;
        cpu.step(&mut bus);
        assert_eq!(
            cpu.regs.mode(),
            CpuMode::User,
            "FIQ pending must be cleared on dispatch"
        );
    }

    #[test]
    fn thumb_arm_switch_via_bx() {
        // Test vector: BX with bit 0 clear -> CPSR T cleared, ARM mode resumes.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.set_thumb(true);
        cpu.regs.r[15] = 0x0;
        cpu.regs.r[1] = 0x40; // bit 0 clear
        let mut bus = RamBus::new(0x200);
        // Thumb BX R1: 0100_0111_0_0001_000
        let bx = 0b0100_0111_0_0001_000u16;
        bus.write_halfword(0x0, bx);
        cpu.step(&mut bus);
        assert!(!cpu.thumb());
        assert_eq!(cpu.regs.r[15], 0x40);
    }

    #[test]
    fn conditional_skip_test_vector() {
        // Test vector: ADDNE R0,R0,#1 with Z set -> R0 unchanged.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr |= FLAG_Z;
        cpu.regs.r[15] = 0x0;
        cpu.regs.r[0] = 5;
        let mut bus = RamBus::new(0x100);
        // ADDNE R0,R0,#1
        let instr = (0x1_u32 << 28) | (0b001 << 25) | (0x4_u32 << 21) | 1u32;
        bus.write_word(0x0, instr);
        cpu.step(&mut bus);
        assert_eq!(cpu.regs.r[0], 5);
        assert_eq!(cpu.regs.r[15], 0x4); // PC still advanced (prefetch).
    }

    #[test]
    fn boot_stub_smoke_test() {
        // Run a tiny boot sequence: ADD, BX-to-Thumb, MOV-imm-thumb, B.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.r[15] = 0x0;
        let mut bus = RamBus::new(0x200);

        // 0x00: MOV R0,#5      (ARM, immediate)
        let mov_imm = (0xE_u32 << 28) | (0b001 << 25) | (0xD_u32 << 21) | 5u32;
        bus.write_word(0x00, mov_imm);
        // 0x04: MOV R1,#0x11   ; target Thumb addr (0x10 | 1)
        let mov_imm2 = (0xE_u32 << 28) | (0b001 << 25) | (0xD_u32 << 21) | (1u32 << 12) | 0x11u32;
        bus.write_word(0x04, mov_imm2);
        // 0x08: BX R1
        let bx = 0xE12F_FF11u32;
        bus.write_word(0x08, bx);
        // 0x10 (Thumb): MOV R2,#42
        let thumb_mov = 0b00100_010_00101010u16;
        bus.write_halfword(0x10, thumb_mov);
        // 0x12: B #0  -> infinite loop (PC won't change in our test, we only step a few times)
        let thumb_b = 0b11100_111_1111_1110u16; // offset -2 -> targets self minus 2 effectively
        bus.write_halfword(0x12, thumb_b);

        // Execute up to a fixed number of cycles, ensuring the CPU never
        // enters an undefined state.
        for _ in 0..16 {
            cpu.step(&mut bus);
        }
        assert_eq!(cpu.regs.r[0], 5);
        assert_eq!(cpu.regs.r[2], 42);
        assert!(
            cpu.thumb(),
            "execution should have switched into Thumb state"
        );
    }

    #[test]
    fn arm_prefetch_keeps_two_instructions_after_self_modify() {
        // Mirrors gba-tests/nes t001: store over the next two ARM opcodes and
        // ensure they still execute from the prefetch queue.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.r[15] = 0x0;
        let mut bus = RamBus::new(0x100);

        bus.write_word(0x00, 0xE3A02000); // mov r2, #0
        bus.write_word(0x04, 0xE3A00014); // mov r0, #0x14 (.pipe1)
        bus.write_word(0x08, 0xE3A01018); // mov r1, #0x18 (.pipe2)
        bus.write_word(0x0C, 0xE5802000); // str r2, [r0]
        bus.write_word(0x10, 0xE5812000); // str r2, [r1]
        bus.write_word(0x14, 0xE3520000); // .pipe1: cmp r2, #0
        bus.write_word(0x18, 0x0A000000); // .pipe2: beq pass
        bus.write_word(0x1C, 0xE3A04001); // fail: mov r4, #1
        bus.write_word(0x20, 0xE3A04002); // pass: mov r4, #2

        for _ in 0..8 {
            cpu.step(&mut bus);
        }

        assert_eq!(cpu.regs.r[4], 2);
    }

    #[test]
    fn undefined_instruction_dispatch_arm() {
        // Test that executing a CLZ instruction (ARMv5TE) causes the CPU to
        // enter Undefined mode with correct state.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.cpsr &= !FLAG_I; // unmask IRQ
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = RamBus::new(0x200);
        // CLZ R0, R1: cond(0xE) 0001 0110 1111 0000 1111 0001 0001
        let clz: u32 = 0xE16F_0F11;
        write_arm_word(&mut bus, 0x100, clz);

        cpu.step(&mut bus);

        assert_eq!(
            cpu.regs.mode(),
            CpuMode::Undefined,
            "CPU should be in Undefined mode"
        );
        assert_eq!(cpu.regs.spsr(), cpsr_before, "SPSR_und should be old CPSR");
        assert_eq!(
            cpu.regs.r[14],
            0x100 + 4,
            "LR_und should be address after undefined instr"
        );
        assert!(
            cpu.regs.i_flag(),
            "IRQ must be masked after undefined dispatch"
        );
        assert_eq!(
            cpu.regs.r[15],
            ExceptionVector::Undefined as u32,
            "PC should be at undefined vector (0x04)"
        );
    }

    #[test]
    fn undefined_instruction_dispatch_thumb() {
        // Test that executing Thumb BLX (ARMv5TE) causes the CPU to enter
        // Undefined mode with correct state.
        let mut cpu = Arm7tdmi::new();
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.set_thumb(true);
        cpu.regs.cpsr &= !FLAG_I;
        cpu.regs.r[15] = 0x100;
        let cpsr_before = cpu.regs.cpsr;

        let mut bus = RamBus::new(0x200);
        // Thumb BLX: 11101_00000000000 = 0xE800
        let blx_thumb: u16 = 0xE800;
        bus.write16(0x100, blx_thumb);
        bus.write16(0x102, 0); // padding for prefetch
        bus.write16(0x104, 0);

        cpu.step(&mut bus);

        assert_eq!(
            cpu.regs.mode(),
            CpuMode::Undefined,
            "CPU should be in Undefined mode"
        );
        assert_eq!(cpu.regs.spsr(), cpsr_before, "SPSR_und should be old CPSR");
        assert_eq!(
            cpu.regs.r[14],
            0x100 + 2,
            "LR_und should be address after undefined Thumb instr"
        );
        assert!(
            cpu.regs.i_flag(),
            "IRQ must be masked after undefined dispatch"
        );
        assert!(!cpu.regs.thumb(), "T bit should be clear (ARM state)");
        assert_eq!(cpu.regs.r[15], ExceptionVector::Undefined as u32);
    }

    // ---------------------------------------------------------------
    // CpuSet (SWI 0x0B) and CpuFastSet (SWI 0x0C) HLE tests
    // ---------------------------------------------------------------

    /// Helper: create a CPU + bus for HLE SWI tests.
    /// Returns CPU at PC=0 in ARM mode with hle_swi=true and a 4KB RamBus.
    fn hle_setup() -> (Arm7tdmi, RamBus) {
        let mut cpu = Arm7tdmi::new();
        cpu.hle_swi = true;
        cpu.regs.switch_mode(CpuMode::User);
        cpu.regs.r[15] = 0x0;
        let bus = RamBus::new(0x1000);
        (cpu, bus)
    }

    #[test]
    fn hle_cpu_set_copies_halfwords() {
        // CpuSet(src=0x100, dst=0x200, count=4 | size=16bit | copy mode)
        // Should copy 4 halfwords (8 bytes) from 0x100 to 0x200.
        let (mut cpu, mut bus) = hle_setup();

        // Write source data at 0x100.
        bus.write16(0x100, 0xCAFE);
        bus.write16(0x102, 0xBABE);
        bus.write16(0x104, 0xDEAD);
        bus.write16(0x106, 0xBEEF);

        // SWI 0x0B (CpuSet): ARM encoding 0xEF0B0000
        write_arm_word(&mut bus, 0x0, 0xEF0B_0000);

        cpu.regs.r[0] = 0x100; // source
        cpu.regs.r[1] = 0x200; // destination
        cpu.regs.r[2] = 4; // count=4, bit24=0 (16-bit), bit25=0 (copy)
        cpu.step(&mut bus);

        assert_eq!(bus.read16(0x200), 0xCAFE);
        assert_eq!(bus.read16(0x202), 0xBABE);
        assert_eq!(bus.read16(0x204), 0xDEAD);
        assert_eq!(bus.read16(0x206), 0xBEEF);
    }

    #[test]
    fn hle_cpu_set_copies_words() {
        // CpuSet with bit26=1 (32-bit copy)
        let (mut cpu, mut bus) = hle_setup();

        bus.write32(0x100, 0xDEAD_BEEF);
        bus.write32(0x104, 0xCAFE_BABE);

        write_arm_word(&mut bus, 0x0, 0xEF0B_0000);

        cpu.regs.r[0] = 0x100;
        cpu.regs.r[1] = 0x200;
        cpu.regs.r[2] = 2 | (1 << 26); // count=2, bit26=1 (32-bit), copy
        cpu.step(&mut bus);

        assert_eq!(bus.read32(0x200), 0xDEAD_BEEF);
        assert_eq!(bus.read32(0x204), 0xCAFE_BABE);
    }

    #[test]
    fn hle_cpu_set_fill_mode_replicates_first_value() {
        // CpuSet with bit24=1 (fill), bit26=1 (32-bit): copies first source value repeatedly.
        let (mut cpu, mut bus) = hle_setup();

        bus.write32(0x100, 0xA5A5_A5A5);

        write_arm_word(&mut bus, 0x0, 0xEF0B_0000);

        cpu.regs.r[0] = 0x100;
        cpu.regs.r[1] = 0x200;
        // count=4, bit26=1 (32-bit), bit24=1 (fill)
        cpu.regs.r[2] = 4 | (1 << 26) | (1 << 24);
        cpu.step(&mut bus);

        assert_eq!(bus.read32(0x200), 0xA5A5_A5A5);
        assert_eq!(bus.read32(0x204), 0xA5A5_A5A5);
        assert_eq!(bus.read32(0x208), 0xA5A5_A5A5);
        assert_eq!(bus.read32(0x20C), 0xA5A5_A5A5);
    }

    #[test]
    fn hle_cpu_fast_set_copies_words() {
        // CpuFastSet (SWI 0x0C): always 32-bit, count rounded to multiple of 8.
        let (mut cpu, mut bus) = hle_setup();

        for i in 0u32..8 {
            bus.write32(0x100 + i * 4, 0x1000_0000 + i);
        }

        write_arm_word(&mut bus, 0x0, 0xEF0C_0000);

        cpu.regs.r[0] = 0x100;
        cpu.regs.r[1] = 0x200;
        cpu.regs.r[2] = 8; // count=8, bit24=0 (copy)
        cpu.step(&mut bus);

        for i in 0u32..8 {
            assert_eq!(
                bus.read32(0x200 + i * 4),
                0x1000_0000 + i,
                "CpuFastSet word {i} mismatch"
            );
        }
    }

    #[test]
    fn hle_cpu_fast_set_fill_mode() {
        // CpuFastSet fill: replicate first source word, count rounded to 8.
        let (mut cpu, mut bus) = hle_setup();

        bus.write32(0x100, 0xBEEF_CAFE);

        write_arm_word(&mut bus, 0x0, 0xEF0C_0000);

        cpu.regs.r[0] = 0x100;
        cpu.regs.r[1] = 0x200;
        cpu.regs.r[2] = 3 | (1 << 24); // count=3, fill. Rounds up to 8.
        cpu.step(&mut bus);

        for i in 0u32..8 {
            assert_eq!(
                bus.read32(0x200 + i * 4),
                0xBEEF_CAFE,
                "CpuFastSet fill word {i} mismatch"
            );
        }
    }

    // ── BIOS Math HLE tests ──────────────────────────────────────────

    /// Helper: execute an ARM SWI instruction with input registers and return
    /// the CPU state. SWI number is embedded in the instruction encoding.
    fn run_hle_swi(swi_num: u8, r0: u32, r1: u32) -> Arm7tdmi {
        let (mut cpu, mut bus) = hle_setup();
        // ARM SWI encoding: 0xEF000000 | (swi_num << 16)
        let instr = 0xEF00_0000 | ((swi_num as u32) << 16);
        write_arm_word(&mut bus, 0x0, instr);
        cpu.regs.r[0] = r0;
        cpu.regs.r[1] = r1;
        cpu.regs.r[3] = 0; // clear r3 to detect if it's set
        cpu.step(&mut bus);
        cpu
    }

    #[test]
    fn hle_div_basic() {
        // Div(7, 3) → r0=2, r1=1, r3=2
        let cpu = run_hle_swi(0x06, 7, 3);
        assert_eq!(cpu.regs.r[0], 2, "quotient");
        assert_eq!(cpu.regs.r[1], 1, "remainder");
        assert_eq!(cpu.regs.r[3], 2, "abs(quotient)");
    }

    #[test]
    fn hle_div_negative_numerator() {
        // Div(-7, 3) → r0=-2 (0xFFFFFFFE), r1=-1 (0xFFFFFFFF), r3=2
        let cpu = run_hle_swi(0x06, (-7i32) as u32, 3);
        assert_eq!(cpu.regs.r[0] as i32, -2, "quotient");
        assert_eq!(cpu.regs.r[1] as i32, -1, "remainder");
        assert_eq!(cpu.regs.r[3], 2, "abs(quotient)");
    }

    #[test]
    fn hle_div_by_zero() {
        // Div(1, 0) → r0=1, r1=1, r3=1 (per GBA BIOS behavior)
        let cpu = run_hle_swi(0x06, 1, 0);
        assert_eq!(cpu.regs.r[0], 1, "quotient for div-by-zero");
        assert_eq!(cpu.regs.r[1], 1, "remainder for div-by-zero");
        assert_eq!(cpu.regs.r[3], 1, "abs for div-by-zero");
    }

    #[test]
    fn hle_div_zero_by_zero() {
        // Div(0, 0) → r0=1, r1=0, r3=1
        let cpu = run_hle_swi(0x06, 0, 0);
        assert_eq!(cpu.regs.r[0], 1, "quotient 0/0");
        assert_eq!(cpu.regs.r[1], 0, "remainder 0/0");
        assert_eq!(cpu.regs.r[3], 1, "abs 0/0");
    }

    #[test]
    fn hle_div_int_min_by_neg_one() {
        // Div(INT_MIN, -1) → r0=INT_MIN, r1=0, r3=INT_MIN (overflow)
        let cpu = run_hle_swi(0x06, 0x8000_0000, 0xFFFF_FFFF);
        assert_eq!(cpu.regs.r[0], 0x8000_0000, "quotient INT_MIN/-1");
        assert_eq!(cpu.regs.r[1], 0, "remainder INT_MIN/-1");
        assert_eq!(cpu.regs.r[3], 0x8000_0000, "abs INT_MIN/-1 (overflow)");
    }

    #[test]
    fn hle_arctan_zero() {
        // ArcTan(0) → r0=0, r1=0, r3=0xA2F9
        // (from mgba-emu/suite: i=0, a=0, b=0xA2F9, result=0)
        let cpu = run_hle_swi(0x09, 0, 0);
        assert_eq!(cpu.regs.r[0], 0, "ArcTan(0) result");
        assert_eq!(cpu.regs.r[1] as i32, 0, "ArcTan(0) intermediate a");
        assert_eq!(cpu.regs.r[3], 0xA2F9, "ArcTan(0) coefficient b");
    }

    #[test]
    fn hle_arctan_quarter() {
        // ArcTan(0x4000) → r0=0x2000, r1=0xFFFFC000, r3=0x8000
        // (from mgba-emu/suite expected values)
        let cpu = run_hle_swi(0x09, 0x4000, 0);
        assert_eq!(cpu.regs.r[0], 0x2000, "ArcTan(0x4000) result");
        assert_eq!(cpu.regs.r[1], 0xFFFFC000, "ArcTan(0x4000) intermediate a");
        assert_eq!(cpu.regs.r[3], 0x8000, "ArcTan(0x4000) coefficient b");
    }

    #[test]
    fn hle_arctan2_zero_zero() {
        // ArcTan2(0, 0) → r0=0, r1=0, r3=0x170
        let cpu = run_hle_swi(0x0A, 0, 0);
        assert_eq!(cpu.regs.r[0], 0, "ArcTan2(0,0) angle");
        assert_eq!(cpu.regs.r[1], 0, "ArcTan2(0,0) r1");
        assert_eq!(cpu.regs.r[3], 0x170, "ArcTan2(0,0) r3 clobber");
    }

    #[test]
    fn hle_arctan2_equal_positive() {
        // ArcTan2(0x4000, 0x4000) → r0=0x2000, r1=0xFFFFC000, r3=0x170
        let cpu = run_hle_swi(0x0A, 0x4000, 0x4000);
        assert_eq!(cpu.regs.r[0], 0x2000, "ArcTan2(0x4000,0x4000) angle");
        assert_eq!(cpu.regs.r[1], 0xFFFFC000, "ArcTan2(0x4000,0x4000) r1");
        assert_eq!(cpu.regs.r[3], 0x170, "ArcTan2(0x4000,0x4000) r3");
    }

    #[test]
    fn hle_arctan2_negative_x_zero_y() {
        // ArcTan2(0xFFFF0000, 0) → r0=0x8000, r1=0, r3=0x170
        let cpu = run_hle_swi(0x0A, 0xFFFF0000, 0);
        assert_eq!(cpu.regs.r[0], 0x8000, "ArcTan2(neg,0) angle");
        assert_eq!(cpu.regs.r[1], 0, "ArcTan2(neg,0) r1");
        assert_eq!(cpu.regs.r[3], 0x170, "ArcTan2(neg,0) r3");
    }

    // --- VBlankIntrWait / IntrWait HLE looping tests ---
    //
    // BIOS_IF at 0x03007FF8 wraps to offset 0xFF8 in a 0x1000-byte RamBus.
    // IME at 0x04000208 wraps to offset 0x208.

    const BIOS_IF_ADDR: u32 = 0x0300_7FF8;

    /// After VBlankIntrWait HLE, the CPU should be halted and PC should
    /// remain at the SWI instruction (not advance past it), so the SWI
    /// will re-execute after an IRQ handler returns.
    #[test]
    fn hle_vblank_intr_wait_halts_and_keeps_pc_at_swi() {
        let (mut cpu, mut bus) = hle_setup();
        // Place ARM SWI 0x05 (VBlankIntrWait) at address 0x00.
        write_arm_word(&mut bus, 0x0, 0xEF05_0000);
        cpu.step(&mut bus);
        assert!(cpu.is_halted(), "CPU should be halted after VBlankIntrWait");
        assert_eq!(
            cpu.regs.r[15], 0x00,
            "PC should stay at SWI instruction, not advance"
        );
    }

    /// When VBlankIntrWait is re-entered after a non-VBlank IRQ,
    /// the CPU should re-halt because VBlank hasn't fired yet.
    #[test]
    fn hle_vblank_intr_wait_rehalts_on_non_vblank_irq() {
        let (mut cpu, mut bus) = hle_setup();
        write_arm_word(&mut bus, 0x0, 0xEF05_0000);

        // First call: enters wait state.
        cpu.step(&mut bus);
        assert!(cpu.is_halted());

        // Simulate IRQ handler return: set a non-VBlank flag in BIOS_IF
        // (bit 3 = Timer 0), un-halt, and set PC back to SWI address.
        bus.write16(BIOS_IF_ADDR, 0x0008); // Timer 0 flag
        cpu.halted = false;
        cpu.regs.r[15] = 0x00;
        cpu.prefetch_valid = false;

        // Re-execute the SWI — should see no VBlank bit and re-halt.
        cpu.step(&mut bus);
        assert!(
            cpu.is_halted(),
            "CPU should re-halt — VBlank bit is not in BIOS_IF"
        );
        assert_eq!(
            cpu.regs.r[15], 0x00,
            "PC should remain at SWI for next re-entry"
        );
    }

    /// When VBlankIntrWait is re-entered after VBlank fires (bit 0 set
    /// in BIOS_IF), the CPU should clear the VBlank bit and advance PC.
    #[test]
    fn hle_vblank_intr_wait_advances_on_vblank() {
        let (mut cpu, mut bus) = hle_setup();
        write_arm_word(&mut bus, 0x0, 0xEF05_0000);

        // First call: enters wait state.
        cpu.step(&mut bus);
        assert!(cpu.is_halted());

        // Simulate IRQ handler return: set VBlank flag in BIOS_IF.
        bus.write16(BIOS_IF_ADDR, 0x0001); // VBlank
        cpu.halted = false;
        cpu.regs.r[15] = 0x00;
        cpu.prefetch_valid = false;

        // Re-execute SWI — should see VBlank, clear it, and advance.
        cpu.step(&mut bus);
        assert!(
            !cpu.is_halted(),
            "CPU should NOT be halted after VBlank acknowledged"
        );
        assert_eq!(
            cpu.regs.r[15], 0x04,
            "PC should have advanced past the SWI instruction"
        );
        assert_eq!(
            bus.read16(BIOS_IF_ADDR),
            0,
            "VBlank bit should be cleared from BIOS_IF"
        );
    }

    /// IntrWait (SWI 0x04) with r0=1, r1=0x08 (Timer 0) should loop until
    /// Timer 0 fires. A VBlank interrupt should not satisfy the wait.
    #[test]
    fn hle_intr_wait_loops_until_requested_irq() {
        let (mut cpu, mut bus) = hle_setup();
        write_arm_word(&mut bus, 0x0, 0xEF04_0000); // SWI 0x04 (IntrWait)
        cpu.regs.r[0] = 1; // discard_old = true
        cpu.regs.r[1] = 0x0008; // wait for Timer 0 (bit 3)

        // First call: enters wait state.
        cpu.step(&mut bus);
        assert!(cpu.is_halted(), "CPU should be halted after IntrWait");

        // Simulate VBlank IRQ (wrong interrupt for this wait).
        bus.write16(BIOS_IF_ADDR, 0x0001); // VBlank
        cpu.halted = false;
        cpu.regs.r[15] = 0x00;
        cpu.prefetch_valid = false;

        cpu.step(&mut bus);
        assert!(
            cpu.is_halted(),
            "CPU should re-halt — VBlank is not the requested interrupt"
        );

        // Now simulate Timer 0 IRQ (correct interrupt).
        bus.write16(BIOS_IF_ADDR, 0x0009); // VBlank + Timer 0
        cpu.halted = false;
        cpu.regs.r[15] = 0x00;
        cpu.prefetch_valid = false;

        cpu.step(&mut bus);
        assert!(
            !cpu.is_halted(),
            "CPU should advance — Timer 0 was in BIOS_IF"
        );
        assert_eq!(
            bus.read16(BIOS_IF_ADDR) & 0x0008,
            0,
            "Timer 0 bit should be cleared from BIOS_IF"
        );
    }

    /// IntrWait with r0=0 (don't discard) should NOT clear existing BIOS_IF
    /// flags on entry. If the requested flag is already set, it should
    /// still wait (halt first), then return on re-entry.
    #[test]
    fn hle_intr_wait_no_discard_preserves_existing_flags() {
        let (mut cpu, mut bus) = hle_setup();
        write_arm_word(&mut bus, 0x0, 0xEF04_0000); // SWI 0x04
        cpu.regs.r[0] = 0; // discard_old = false
        cpu.regs.r[1] = 0x0001; // wait for VBlank

        // Pre-set VBlank in BIOS_IF.
        bus.write16(BIOS_IF_ADDR, 0x0001);

        // First call: should NOT clear the pre-existing VBlank flag.
        cpu.step(&mut bus);
        assert!(cpu.is_halted(), "CPU should halt on first IntrWait call");

        // Simulate handler return with VBlank still in BIOS_IF.
        cpu.halted = false;
        cpu.regs.r[15] = 0x00;
        cpu.prefetch_valid = false;

        // Re-entry: VBlank is set → should advance.
        cpu.step(&mut bus);
        assert!(
            !cpu.is_halted(),
            "CPU should advance — VBlank was already in BIOS_IF"
        );
    }
}
