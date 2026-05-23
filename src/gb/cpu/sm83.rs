use serde::{Deserialize, Serialize};

use crate::gb::bus::GbBus;
use crate::gb::model::CgbModel;
use crate::trace_cpu;

// ---------------------------------------------------------------------------
// Flag bit positions in register F (upper nibble only; bits 0–3 always 0)
// ---------------------------------------------------------------------------
const FLAG_Z: u8 = 1 << 7; // Zero
const FLAG_N: u8 = 1 << 6; // Subtract
const FLAG_H: u8 = 1 << 5; // Half-carry
const FLAG_C: u8 = 1 << 4; // Carry

/// SM83 register file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registers {
    pub a: u8,
    pub f: u8, // bits 7-4: Z N H C; bits 3-0: always 0
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    pub sp: u16,
    pub pc: u16,
}

impl Registers {
    pub fn new() -> Self {
        Self {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0,
            pc: 0,
        }
    }

    // --- 16-bit pair helpers ----------------------------------------------

    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f & 0xF0])
    }
    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }
    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }
    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }

    pub fn set_af(&mut self, val: u16) {
        let [hi, lo] = val.to_be_bytes();
        self.a = hi;
        self.f = lo & 0xF0;
    }
    pub fn set_bc(&mut self, val: u16) {
        let [hi, lo] = val.to_be_bytes();
        self.b = hi;
        self.c = lo;
    }
    pub fn set_de(&mut self, val: u16) {
        let [hi, lo] = val.to_be_bytes();
        self.d = hi;
        self.e = lo;
    }
    pub fn set_hl(&mut self, val: u16) {
        let [hi, lo] = val.to_be_bytes();
        self.h = hi;
        self.l = lo;
    }

    // --- Flag helpers -----------------------------------------------------

    pub fn z_flag(&self) -> bool {
        self.f & FLAG_Z != 0
    }
    pub fn n_flag(&self) -> bool {
        self.f & FLAG_N != 0
    }
    pub fn h_flag(&self) -> bool {
        self.f & FLAG_H != 0
    }
    pub fn c_flag(&self) -> bool {
        self.f & FLAG_C != 0
    }

    pub fn set_flags(&mut self, z: bool, n: bool, h: bool, c: bool) {
        self.f = 0;
        if z {
            self.f |= FLAG_Z;
        }
        if n {
            self.f |= FLAG_N;
        }
        if h {
            self.f |= FLAG_H;
        }
        if c {
            self.f |= FLAG_C;
        }
    }
}

impl Default for Registers {
    fn default() -> Self {
        Self::new()
    }
}

/// SM83 (LR35902) CPU core.
///
/// `B` must implement [`GbBus`] — use [`StubBus`] for unit tests or a full
/// bus implementation for integration tests.
pub struct Sm83<B: GbBus> {
    pub regs: Registers,
    pub ime: bool,
    /// Set by HALT; cleared when an interrupt fires or IME is re-enabled.
    pub halted: bool,
    /// Set by STOP when it enters low-power standby instead of switching CGB speed.
    pub stopped: bool,
    /// Set when HALT executes with IME=false and a pending interrupt (HALT bug).
    /// The next opcode fetch reads without advancing PC.
    pub halt_bug: bool,
    /// Set by EI; IME is enabled _after_ the instruction following EI.
    ime_pending: bool,
    pub bus: B,
    cycles: u64,
    /// Last memory address written by the CPU (for write-address breakpoints).
    last_write_addr: Option<u16>,
}

impl<B: GbBus> Sm83<B> {
    pub fn new(bus: B) -> Self {
        Self {
            regs: Registers::new(),
            ime: false,
            halted: false,
            stopped: false,
            halt_bug: false,
            ime_pending: false,
            bus,
            cycles: 0,
            last_write_addr: None,
        }
    }

    /// Total M-cycles elapsed since construction.
    pub fn cycles(&self) -> u64 {
        self.cycles
    }

    /// Set the M-cycle counter (used by save-state restore).
    pub fn set_cycles(&mut self, cycles: u64) {
        self.cycles = cycles;
    }

    /// Returns `true` if IME will be enabled after the next instruction (EI pending).
    pub fn ime_pending(&self) -> bool {
        self.ime_pending
    }

    /// Set the IME-pending flag (used by save-state restore).
    pub fn set_ime_pending(&mut self, pending: bool) {
        self.ime_pending = pending;
    }

    /// Get the last memory address written by the CPU.
    /// Returns `None` if no write has occurred since reset or last clear.
    pub fn last_cpu_write_addr(&self) -> Option<u16> {
        self.last_write_addr
    }

    /// Clear the last write address (for debugger use).
    pub fn clear_last_write_addr(&mut self) {
        self.last_write_addr = None;
    }

    /// Reset the CPU to power-on state.
    ///
    /// All registers zeroed, IME/halted/stopped/halt_bug/ime_pending cleared.
    /// PC starts at $0000 (boot ROM entry point).
    /// The cycle counter is intentionally NOT reset.
    pub fn reset_to_power_on(&mut self) {
        self.regs = Registers::new();
        self.ime = false;
        self.halted = false;
        self.stopped = false;
        self.halt_bug = false;
        self.last_write_addr = None;
        self.ime_pending = false;
    }

    /// Reset the CPU registers to the post-boot-ROM CGB state.
    ///
    /// A CGB exits its boot ROM with A=$11 (hardware identifier), which allows
    /// cartridges to detect CGB hardware at runtime. Uses CGB-E (most common)
    /// register values by default.
    pub fn reset_registers_cgb(&mut self) {
        self.reset_registers_cgb_for_model(CgbModel::CgbE);
    }

    /// Reset the CPU registers to the post-boot-ROM CGB state for a specific model.
    ///
    /// Post-boot register values (same for all CGB models currently):
    /// A=$11 F=$80 B=$00 C=$00 D=$00 E=$08 H=$00 L=$7C SP=$FFFE
    ///
    /// Reference: Mooneye test suite boot_regs-cgb.s (verified against real hardware).
    /// The model parameter is retained for future model-specific differences.
    pub fn reset_registers_cgb_for_model(&mut self, _model: CgbModel) {
        self.regs.set_af(0x1180); // A=$11, F=$80
        self.regs.set_bc(0x0000); // B=$00, C=$00
        self.regs.set_de(0x0008); // D=$00, E=$08
        self.regs.set_hl(0x007C); // H=$00, L=$7C

        self.regs.sp = 0xFFFE;
        self.regs.pc = 0x0100;
        self.ime = false;
        self.halted = false;
        self.stopped = false;
        self.halt_bug = false;
        self.ime_pending = false;
    }

    /// Advance the cycle counter by 1 M-cycle and tick peripherals.
    ///
    /// Used for internal CPU cycles that do not correspond to a memory access
    /// (e.g. the extra cycle consumed by taken conditional branches, PUSH, etc.).
    fn internal_cycle(&mut self) {
        trace_cpu!(2; "      internal");
        self.cycles += 1;
        self.bus.tick(1);
    }

    /// Read a byte from the bus and advance the cycle counter by 1 M-cycle.
    ///
    /// Peripherals are ticked **before** the memory read, matching real SM83
    /// hardware where the timer increments at T-cycle 0 of the M-cycle and
    /// the memory bus is sampled at T-cycle 3.  This ensures that reads from
    /// memory-mapped timer registers (e.g. TIMA) see the post-increment value
    /// when the timer fires on the same M-cycle as the access.
    fn read(&mut self, addr: u16) -> u8 {
        self.cycles += 1;
        self.bus.tick(1);
        let val = self.bus.read(addr);
        trace_cpu!(2; "      read  ${:04X} = ${:02X}", addr, val);
        val
    }

    /// Write a byte to the bus and advance the cycle counter by 1 M-cycle.
    ///
    /// Peripherals are ticked **before** the memory write for the same reason
    /// as [`read`]: the timer advances at T-cycle 0 before the write at T-cycle 3.
    fn write(&mut self, addr: u16, val: u8) {
        self.last_write_addr = Some(addr);
        self.cycles += 1;
        self.bus.write_cpu_m_cycle(addr, val);
        trace_cpu!(2; "      write ${:04X} = ${:02X}", addr, val);
    }

    /// Fetch the next byte at PC and increment PC.
    ///
    /// If the HALT bug flag is set, PC is not advanced (the byte is read
    /// a second time from the same address on the following fetch).
    fn fetch_byte(&mut self) -> u8 {
        let val = self.read(self.regs.pc);
        if self.halt_bug {
            // HALT bug: omit the PC increment so the same byte is fetched again
            // on the next call to fetch_byte().
            self.halt_bug = false;
        } else {
            self.regs.pc = self.regs.pc.wrapping_add(1);
        }
        val
    }

    /// Fetch the opcode byte at PC without ticking peripherals.
    ///
    /// Used by [`execute()`] after the pre-tick has already advanced
    /// timer/serial for the M1 cycle.  Subsequent bytes in multi-byte
    /// instructions still use [`fetch_byte()`] which ticks normally.
    ///
    /// Honors the HALT bug flag identically to [`fetch_byte()`].
    fn fetch_byte_no_tick(&mut self) -> u8 {
        let val = self.bus.read(self.regs.pc);
        if self.halt_bug {
            // HALT bug: omit the PC increment so the same byte is fetched again
            // on the next call to fetch_byte().
            self.halt_bug = false;
        } else {
            self.regs.pc = self.regs.pc.wrapping_add(1);
        }
        val
    }

    /// Fetch a 16-bit little-endian immediate at PC and advance PC by 2.
    fn fetch_u16(&mut self) -> u16 {
        let lo = self.fetch_byte() as u16;
        let hi = self.fetch_byte() as u16;
        (hi << 8) | lo
    }

    /// Push a 16-bit value onto the stack (SP decremented twice).
    fn push_u16(&mut self, val: u16) {
        let [hi, lo] = val.to_be_bytes();
        let sp0 = self.regs.sp;
        self.regs.sp = sp0.wrapping_sub(1);
        self.bus.notify_idu_glitch(sp0); // M2: IDU-only DEC SP
        self.write(self.regs.sp, hi); // M3: write hi to [SP]
        let sp1 = self.regs.sp;
        self.regs.sp = sp1.wrapping_sub(1);
        self.bus.notify_idu_glitch(sp1); // M3: DEC SP ("Write During Decrease" = single write corruption)
        self.write(self.regs.sp, lo); // M4: write lo to [SP]
        self.bus.notify_oam_write(self.regs.sp); // M4: plain write, no IDU
    }

    /// Pop a 16-bit value from the stack (SP incremented twice).
    ///
    /// Per Pan Docs "OAM Corruption Bug": POP triggers only 3 corruption events:
    ///   M2: read [SP] + IDU INC SP → notify_idu_with_prior_read (Read During IDU)
    ///   M3: read [SP+1], no IDU    → notify_oam_read            (plain read corruption)
    fn pop_u16(&mut self) -> u16 {
        let lo = self.read(self.regs.sp) as u16;
        let sp0 = self.regs.sp;
        self.regs.sp = sp0.wrapping_add(1);
        self.bus.notify_idu_with_prior_read(sp0); // M2: read + IDU INC SP
        let hi = self.read(self.regs.sp) as u16;
        let sp1 = self.regs.sp;
        self.regs.sp = sp1.wrapping_add(1);
        self.bus.notify_oam_read(sp1); // M3: read only, no IDU glitch
        (hi << 8) | lo
    }

    // --- Register file accessors by r-field (bits 2–0 in opcode) ----------

    fn read_r8(&mut self, r: u8) -> u8 {
        match r & 0x07 {
            0 => self.regs.b,
            1 => self.regs.c,
            2 => self.regs.d,
            3 => self.regs.e,
            4 => self.regs.h,
            5 => self.regs.l,
            6 => {
                let hl = self.regs.hl();
                self.read(hl)
            }
            7 => self.regs.a,
            _ => unreachable!(),
        }
    }

    fn write_r8(&mut self, r: u8, val: u8) {
        match r & 0x07 {
            0 => self.regs.b = val,
            1 => self.regs.c = val,
            2 => self.regs.d = val,
            3 => self.regs.e = val,
            4 => self.regs.h = val,
            5 => self.regs.l = val,
            6 => {
                let hl = self.regs.hl();
                self.write(hl, val);
            }
            7 => self.regs.a = val,
            _ => unreachable!(),
        }
    }

    // --- ALU helpers -------------------------------------------------------

    fn alu_add(&mut self, operand: u8, carry_in: u8) {
        let a = self.regs.a;
        let result16 = (a as u16)
            .wrapping_add(operand as u16)
            .wrapping_add(carry_in as u16);
        let result = result16 as u8;
        let h = (a & 0x0F) + (operand & 0x0F) + carry_in > 0x0F;
        let c = result16 > 0xFF;
        self.regs.set_flags(result == 0, false, h, c);
        self.regs.a = result;
    }

    fn alu_sub(&mut self, operand: u8, carry_in: u8) {
        let a = self.regs.a;
        let result16 = (a as i16) - (operand as i16) - (carry_in as i16);
        let result = result16 as u8;
        let h = (a & 0x0F) < (operand & 0x0F) + carry_in;
        let c = (a as u16) < (operand as u16) + (carry_in as u16);
        self.regs.set_flags(result == 0, true, h, c);
        self.regs.a = result;
    }

    fn alu_and(&mut self, operand: u8) {
        self.regs.a &= operand;
        let z = self.regs.a == 0;
        self.regs.set_flags(z, false, true, false);
    }

    fn alu_or(&mut self, operand: u8) {
        self.regs.a |= operand;
        let z = self.regs.a == 0;
        self.regs.set_flags(z, false, false, false);
    }

    fn alu_xor(&mut self, operand: u8) {
        self.regs.a ^= operand;
        let z = self.regs.a == 0;
        self.regs.set_flags(z, false, false, false);
    }

    fn alu_cp(&mut self, operand: u8) {
        let saved = self.regs.a;
        self.alu_sub(operand, 0);
        self.regs.a = saved;
    }

    fn alu_inc(&mut self, val: u8) -> u8 {
        let result = val.wrapping_add(1);
        let h = (val & 0x0F) == 0x0F;
        // C flag is unchanged; preserve it
        let c = self.regs.c_flag();
        self.regs.set_flags(result == 0, false, h, c);
        result
    }

    fn alu_dec(&mut self, val: u8) -> u8 {
        let result = val.wrapping_sub(1);
        let h = (val & 0x0F) == 0x00;
        let c = self.regs.c_flag();
        self.regs.set_flags(result == 0, true, h, c);
        result
    }

    fn alu_add_hl(&mut self, val: u16) {
        let hl = self.regs.hl();
        let result = hl.wrapping_add(val);
        let h = (hl & 0x0FFF) + (val & 0x0FFF) > 0x0FFF;
        let c = (hl as u32) + (val as u32) > 0xFFFF;
        let z = self.regs.z_flag(); // Z unchanged
        self.regs.set_flags(z, false, h, c);
        self.regs.set_hl(result);
    }

    /// Compute `SP + e` for ADD SP,e (0xE8) and LD HL,SP+e (0xF8).
    /// Sets flags: Z=0 N=0 H C based on lower-byte arithmetic.
    fn alu_sp_offset(&mut self, e: i8) -> u16 {
        let sp = self.regs.sp;
        let offset = e as u16;
        let result = sp.wrapping_add(offset);
        let h = (sp & 0x0F) + (offset & 0x0F) > 0x0F;
        let c = (sp & 0xFF) + (offset & 0xFF) > 0xFF;
        self.regs.set_flags(false, false, h, c);
        result
    }

    // --- Accumulator rotate helpers (Z always cleared) -------------------

    /// Rotate A left; carry = old bit 7; Z always cleared.
    fn rlca(&mut self) {
        let c = self.regs.a >> 7;
        self.regs.a = (self.regs.a << 1) | c;
        self.regs.set_flags(false, false, false, c != 0);
    }

    /// Rotate A right; carry = old bit 0; Z always cleared.
    fn rrca(&mut self) {
        let c = self.regs.a & 1;
        self.regs.a = (self.regs.a >> 1) | (c << 7);
        self.regs.set_flags(false, false, false, c != 0);
    }

    /// Rotate A left through carry; Z always cleared.
    fn rla(&mut self) {
        let old_c = self.regs.c_flag() as u8;
        let new_c = self.regs.a >> 7;
        self.regs.a = (self.regs.a << 1) | old_c;
        self.regs.set_flags(false, false, false, new_c != 0);
    }

    /// Rotate A right through carry; Z always cleared.
    fn rra(&mut self) {
        let old_c = self.regs.c_flag() as u8;
        let new_c = self.regs.a & 1;
        self.regs.a = (self.regs.a >> 1) | (old_c << 7);
        self.regs.set_flags(false, false, false, new_c != 0);
    }

    // --- CB rotate / shift helpers (Z reflects result) -------------------

    fn rlc(&mut self, val: u8) -> u8 {
        let c = val >> 7;
        let result = (val << 1) | c;
        self.regs.set_flags(result == 0, false, false, c != 0);
        result
    }

    fn rrc(&mut self, val: u8) -> u8 {
        let c = val & 1;
        let result = (val >> 1) | (c << 7);
        self.regs.set_flags(result == 0, false, false, c != 0);
        result
    }

    fn rl(&mut self, val: u8) -> u8 {
        let old_c = self.regs.c_flag() as u8;
        let new_c = val >> 7;
        let result = (val << 1) | old_c;
        self.regs.set_flags(result == 0, false, false, new_c != 0);
        result
    }

    fn rr(&mut self, val: u8) -> u8 {
        let old_c = self.regs.c_flag() as u8;
        let new_c = val & 1;
        let result = (val >> 1) | (old_c << 7);
        self.regs.set_flags(result == 0, false, false, new_c != 0);
        result
    }

    fn sla(&mut self, val: u8) -> u8 {
        let c = val >> 7;
        let result = val << 1;
        self.regs.set_flags(result == 0, false, false, c != 0);
        result
    }

    fn sra(&mut self, val: u8) -> u8 {
        let c = val & 1;
        let result = ((val as i8) >> 1) as u8;
        self.regs.set_flags(result == 0, false, false, c != 0);
        result
    }

    fn swap(&mut self, val: u8) -> u8 {
        let result = val.rotate_right(4);
        self.regs.set_flags(result == 0, false, false, false);
        result
    }

    fn srl(&mut self, val: u8) -> u8 {
        let c = val & 1;
        let result = val >> 1;
        self.regs.set_flags(result == 0, false, false, c != 0);
        result
    }

    // --- Interrupt dispatch -----------------------------------------------

    /// Check for pending interrupts and service the highest-priority one.
    ///
    /// Returns `true` when a full interrupt dispatch was performed (the
    /// execute() slot is consumed and the caller must return immediately).
    /// Returns `false` in all other cases, including:
    /// - no pending interrupt,
    /// - IME=false with CPU not halted (interrupt ignored),
    /// - IME=false HALT wake-up (halted is cleared; execute() proceeds
    ///   directly to the next instruction fetch with no extra cycle).
    fn service_interrupts(&mut self) -> bool {
        let ie = self.bus.read(0xFFFF);
        let if_ = self.bus.read(0xFF0F);
        let pending = ie & if_ & 0x1F;
        if pending == 0 {
            return false;
        }

        // Wake from HALT regardless of IME.
        if self.halted {
            self.halted = false;
            if !self.ime {
                // IME=0 wake-up: clear halted and return false so execute()
                // immediately proceeds to the next instruction fetch.  This
                // gives HALT-with-IME=0 exactly the same M-cycle cost as a
                // series of NOPs (no extra wake-up cycle).
                return false;
            }
            // IME=true: fall through to full interrupt dispatch below.
        }

        if !self.ime {
            return false;
        }

        self.ime = false;
        self.ime_pending = false;

        // 1 NOP cycle — M2 of the 5-cycle dispatch.
        // M1 was consumed by execute()'s pre-tick.
        self.internal_cycle();

        // Push PC high byte onto the stack (M3).
        // Uses the same SP-decrement / OAM-notification pattern as push_u16.
        let [pc_hi, pc_lo] = self.regs.pc.to_be_bytes();
        let sp0 = self.regs.sp;
        self.regs.sp = sp0.wrapping_sub(1);
        self.bus.notify_idu_glitch(sp0);
        self.write(self.regs.sp, pc_hi);

        // Re-read IE and IF after the high-byte push: if SP landed on $FFFF
        // (IE), or a higher-priority interrupt became pending during dispatch,
        // the vector selection uses the live queue rather than the queue that
        // originally accepted the interrupt.
        let mut interrupt_queue = self.bus.read(0xFFFF) & self.bus.read(0xFF0F) & 0x1F;

        // Push PC low byte onto the stack (M4).
        let sp1 = self.regs.sp;
        self.regs.sp = sp1.wrapping_sub(1);
        self.bus.notify_idu_glitch(sp1);
        self.write(self.regs.sp, pc_lo);
        self.bus.notify_oam_write(self.regs.sp);

        // AND with current IF: handles the lo-byte-to-$FF0F (IF register) edge
        // case naturally — if the lo push overwrote IF, we see the new value.
        interrupt_queue &= self.bus.read(0xFF0F) & 0x1F;

        // 1 final internal cycle for the vector jump (M5).
        self.internal_cycle();

        if interrupt_queue == 0 {
            // Dispatch cancelled: the IE overwrite (via hi-byte push to $FFFF)
            // cleared all pending interrupts.  Jump to $0000; IF is NOT cleared.
            self.regs.pc = 0x0000;
            trace_cpu!(1; "INT cancelled -> PC=$0000");
        } else {
            let bit = interrupt_queue.trailing_zeros() as u8;
            // Clear the IF bit for the dispatched interrupt (may differ from the
            // originally-selected bit if IE was modified during the push).
            let new_if = self.bus.read(0xFF0F) & !(1 << bit);
            self.bus.write(0xFF0F, new_if);
            self.regs.pc = 0x0040 + (bit as u16) * 8;
            let interrupt_name = match bit {
                0 => "VBlank",
                1 => "LCD STAT",
                2 => "Timer",
                3 => "Serial",
                4 => "Joypad",
                _ => "Unknown",
            };
            trace_cpu!(1; "INT {} -> PC=${:04X}", interrupt_name, self.regs.pc);
        }

        true
    }

    // --- Conditional helpers -----------------------------------------------

    fn check_condition(&self, cond: u8) -> bool {
        match cond & 0x03 {
            0 => !self.regs.z_flag(), // NZ
            1 => self.regs.z_flag(),  // Z
            2 => !self.regs.c_flag(), // NC
            3 => self.regs.c_flag(),  // C
            _ => unreachable!(),
        }
    }

    // -----------------------------------------------------------------------
    // Fetch-decode-execute
    // -----------------------------------------------------------------------

    /// Execute one instruction (or service a pending interrupt) and return.
    ///
    /// If halted and no interrupt is pending, consumes 1 M-cycle doing nothing.
    ///
    /// ## M1 cycle ordering
    ///
    /// On real SM83 hardware the DIV counter increments at T0 of the M1
    /// cycle and the interrupt controller samples IE & IF at T1–T2 of the
    /// *same* cycle.  To match this, we tick peripherals **before** checking
    /// interrupts so that timer/serial side-effects (e.g. serial-transfer
    /// completion setting IF.3) are visible to the interrupt check.
    ///
    /// The opcode read then uses [`fetch_byte_no_tick()`] since the M1 tick
    /// has already been consumed by the pre-tick.  Subsequent bytes in
    /// multi-byte instructions continue to use the regular [`fetch_byte()`]
    /// which ticks normally.
    pub fn execute(&mut self) {
        // Clear last_write_addr at instruction boundary to prevent spurious
        // write-address breakpoint triggers on later instructions that don't
        // perform writes (matches NES CPU behavior).
        self.last_write_addr = None;

        // Snapshot IME-pending state *before* this instruction runs.
        // IME becomes active at the *end* of the instruction following EI —
        // not at the start — so interrupts can only fire from the third
        // instruction onwards (EI, following-instr, *here*).
        let pending = self.ime_pending;

        // Per-instruction bus/PPU setup (must precede the M1 tick).
        self.bus.begin_instruction();

        // T0 of M1: advance timer/serial before the interrupt check.
        self.cycles += 1;
        self.bus.tick(1);

        // Check if CPU should be halted for HDMA/GDMA transfer.
        // When HDMA is active, the CPU is halted for 8 M-cycles per block.
        // The bus sets hdma_halt_cycles when a transfer begins, and we consume
        // one cycle here, skipping instruction execution for this M-cycle.
        if self.bus.consume_hdma_halt_cycle() {
            // eprintln!("[CPU] Halted for HDMA/GDMA (remaining cycles will follow)");
            return;
        }

        if self.stopped {
            if self.bus.read(0xFF0F) & 0x10 != 0 {
                self.stopped = false;
            } else {
                return;
            }
        }

        // T1–T2 of M1: check & potentially dispatch interrupts.
        // service_interrupts() consumes 4 more M-cycles internally when
        // dispatching (NOP + push_hi + push_lo + vector), giving the correct
        // total of 5 M-cycles for an interrupt dispatch.
        if self.service_interrupts() {
            return;
        }

        if self.halted {
            // The pre-tick above IS the halt stall cycle.  (1M total)
        } else {
            // T3 of M1: read opcode (no additional tick).
            let pc = self.regs.pc; // Capture PC before fetch for tracing
            let opcode = self.fetch_byte_no_tick();

            // Level 1 CPU tracing: emit instruction execution trace
            // NOTE: We only trace the opcode byte to avoid speculative bus reads
            // that could have side effects (e.g., OAM corruption during Mode 2).
            // Operands are not resolved to keep the trace simple and side-effect-free.
            use crate::platform::debugging::cpu_trace_level;
            if cpu_trace_level() >= 1 {
                let hex = format!("{:02X}", opcode);
                let asm = if opcode == 0xCB {
                    // For CB prefix, we can't safely read the operand without side effects,
                    // so just show the prefix mnemonic
                    "CB prefix".to_string()
                } else {
                    // For regular instructions, show the mnemonic template (e.g., "LD A,n8")
                    crate::gb::cpu::opcode::lookup(opcode).mnemonic.to_string()
                };

                trace_cpu!(1;
                    "exec PC={:04X} {:<8} {:<14} AF={:04X} BC={:04X} DE={:04X} HL={:04X} SP={:04X} cyc={:<3}",
                    pc, hex.as_str(), asm.as_str(),
                    self.regs.af(), self.regs.bc(), self.regs.de(), self.regs.hl(), self.regs.sp,
                    self.cycles
                );
            }

            self.decode_execute(opcode);
        }

        // Activate delayed IME after the instruction/stall that follows EI.
        // If DI was executed during this instruction, self.ime_pending is now
        // false, so the activation is naturally cancelled (EI→DI semantics).
        if pending && self.ime_pending {
            self.ime = true;
            self.ime_pending = false;
        }
    }

    fn decode_execute(&mut self, opcode: u8) {
        match opcode {
            // --- NOP ------------------------------------------------------
            0x00 => {}

            // --- LD r16, n16 ----------------------------------------------
            0x01 => {
                let v = self.fetch_u16();
                self.regs.set_bc(v);
            }
            0x11 => {
                let v = self.fetch_u16();
                self.regs.set_de(v);
            }
            0x21 => {
                let v = self.fetch_u16();
                self.regs.set_hl(v);
            }
            0x31 => {
                let v = self.fetch_u16();
                self.regs.sp = v;
            }

            // --- LD (BC/DE), A -------------------------------------------
            0x02 => {
                let addr = self.regs.bc();
                let a = self.regs.a;
                self.write(addr, a);
            }
            0x12 => {
                let addr = self.regs.de();
                let a = self.regs.a;
                self.write(addr, a);
            }

            // --- LD A, (BC/DE) -------------------------------------------
            0x0A => {
                let addr = self.regs.bc();
                self.regs.a = self.read(addr);
            }
            0x1A => {
                let addr = self.regs.de();
                self.regs.a = self.read(addr);
            }

            // --- INC r16 --------------------------------------------------
            0x03 => {
                let old = self.regs.bc();
                self.regs.set_bc(old.wrapping_add(1));
                self.internal_cycle();
                self.bus.notify_idu_glitch(old);
            }
            0x13 => {
                let old = self.regs.de();
                self.regs.set_de(old.wrapping_add(1));
                self.internal_cycle();
                self.bus.notify_idu_glitch(old);
            }
            0x23 => {
                let old = self.regs.hl();
                self.regs.set_hl(old.wrapping_add(1));
                self.internal_cycle();
                self.bus.notify_idu_glitch(old);
            }
            0x33 => {
                let old = self.regs.sp;
                self.regs.sp = old.wrapping_add(1);
                self.internal_cycle();
                self.bus.notify_idu_glitch(old);
            }

            // --- DEC r16 --------------------------------------------------
            0x0B => {
                let old = self.regs.bc();
                self.regs.set_bc(old.wrapping_sub(1));
                self.internal_cycle();
                self.bus.notify_idu_glitch(old);
            }
            0x1B => {
                let old = self.regs.de();
                self.regs.set_de(old.wrapping_sub(1));
                self.internal_cycle();
                self.bus.notify_idu_glitch(old);
            }
            0x2B => {
                let old = self.regs.hl();
                self.regs.set_hl(old.wrapping_sub(1));
                self.internal_cycle();
                self.bus.notify_idu_glitch(old);
            }
            0x3B => {
                let old = self.regs.sp;
                self.regs.sp = old.wrapping_sub(1);
                self.internal_cycle();
                self.bus.notify_idu_glitch(old);
            }

            // --- INC r8 (B/C/D/E/H/L/(HL)/A) ---------------------------
            0x04 | 0x0C | 0x14 | 0x1C | 0x24 | 0x2C | 0x34 | 0x3C => {
                let r = (opcode >> 3) & 0x07;
                let val = self.read_r8(r);
                let result = self.alu_inc(val);
                self.write_r8(r, result);
            }

            // --- DEC r8 (B/C/D/E/H/L/(HL)/A) ---------------------------
            0x05 | 0x0D | 0x15 | 0x1D | 0x25 | 0x2D | 0x35 | 0x3D => {
                let r = (opcode >> 3) & 0x07;
                let val = self.read_r8(r);
                let result = self.alu_dec(val);
                self.write_r8(r, result);
            }

            // --- LD r8, n8 (B/C/D/E/H/L/(HL)/A) -------------------------
            0x06 | 0x0E | 0x16 | 0x1E | 0x26 | 0x2E | 0x36 | 0x3E => {
                let r = (opcode >> 3) & 0x07;
                let n = self.fetch_byte();
                self.write_r8(r, n);
            }

            // --- RLCA / RRCA / RLA / RRA ----------------------------------
            0x07 => self.rlca(),
            0x0F => self.rrca(),
            0x17 => self.rla(),
            0x1F => self.rra(),

            // --- LD (n16), SP -------------------------------------------
            0x08 => {
                let addr = self.fetch_u16();
                let [lo, hi] = self.regs.sp.to_le_bytes();
                self.write(addr, lo);
                self.write(addr.wrapping_add(1), hi);
            }

            // --- ADD HL, r16 --------------------------------------------
            0x09 => {
                let v = self.regs.bc();
                self.alu_add_hl(v);
                self.internal_cycle();
            }
            0x19 => {
                let v = self.regs.de();
                self.alu_add_hl(v);
                self.internal_cycle();
            }
            0x29 => {
                let v = self.regs.hl();
                self.alu_add_hl(v);
                self.internal_cycle();
            }
            0x39 => {
                let v = self.regs.sp;
                self.alu_add_hl(v);
                self.internal_cycle();
            }

            // --- LD (HL+/-), A  and  LD A, (HL+/-) ----------------------
            0x22 => {
                let addr = self.regs.hl();
                let a = self.regs.a;
                self.write(addr, a);
                self.regs.set_hl(addr.wrapping_add(1));
            }
            0x32 => {
                let addr = self.regs.hl();
                let a = self.regs.a;
                self.write(addr, a);
                self.regs.set_hl(addr.wrapping_sub(1));
            }
            0x2A => {
                let addr = self.regs.hl();
                self.regs.a = self.read(addr);
                self.regs.set_hl(addr.wrapping_add(1));
                self.bus.notify_idu_with_prior_read(addr); // read + IDU INC HL
            }
            0x3A => {
                let addr = self.regs.hl();
                self.regs.a = self.read(addr);
                self.regs.set_hl(addr.wrapping_sub(1));
                self.bus.notify_idu_with_prior_read(addr); // read + IDU DEC HL
            }

            // --- DAA ----------------------------------------------------
            0x27 => {
                let mut a = self.regs.a;
                let n = self.regs.n_flag();
                let h = self.regs.h_flag();
                let c = self.regs.c_flag();
                let mut new_c = false;
                if !n {
                    if c || a > 0x99 {
                        a = a.wrapping_add(0x60);
                        new_c = true;
                    }
                    if h || (a & 0x0F) > 0x09 {
                        a = a.wrapping_add(0x06);
                    }
                } else {
                    if c {
                        a = a.wrapping_sub(0x60);
                        new_c = true;
                    }
                    if h {
                        a = a.wrapping_sub(0x06);
                    }
                }
                self.regs.a = a;
                let z = a == 0;
                self.regs.set_flags(z, n, false, new_c);
            }

            // --- CPL -------------------------------------------------------
            0x2F => {
                self.regs.a = !self.regs.a;
                let z = self.regs.z_flag();
                let c = self.regs.c_flag();
                self.regs.set_flags(z, true, true, c);
            }

            // --- SCF / CCF -------------------------------------------------
            0x37 => {
                let z = self.regs.z_flag();
                self.regs.set_flags(z, false, false, true);
            }
            0x3F => {
                let z = self.regs.z_flag();
                let c = !self.regs.c_flag();
                self.regs.set_flags(z, false, false, c);
            }

            // --- HALT / STOP ---------------------------------------------
            0x76 => {
                // HALT bug: when IME=false (and not about to become true via EI delay)
                // and there is a pending interrupt, the CPU does NOT enter HALT mode.
                // Instead execution continues but the next opcode fetch reads PC without advancing it.
                // If EI was just executed (ime_pending=true), IME will become true after
                // HALT executes, so the HALT bug does NOT trigger.
                let ie = self.bus.read(0xFFFF);
                let if_ = self.bus.read(0xFF0F);
                if !self.ime && !self.ime_pending && (ie & if_ & 0x1F) != 0 {
                    self.halt_bug = true;
                } else {
                    self.halted = true;
                    // Special case: when HALT is executed with ime_pending=true and
                    // a buffered interrupt, the PC should point back to the HALT instruction
                    // so that when the interrupt fires, it pushes the HALT address as the
                    // return address. This allows multiple buffered interrupts to all return
                    // to HALT, re-executing it until all interrupts are serviced.
                    if self.ime_pending && (ie & if_ & 0x1F) != 0 {
                        self.regs.pc = self.regs.pc.wrapping_sub(1);
                    }
                }
            }
            0x10 => {
                // STOP — consume the next byte (it should be 0x00)
                self.fetch_byte();
                // If the bus supports CGB speed switching and KEY1 is armed,
                // perform the speed switch instead of halting.
                if !self.bus.try_speed_switch() {
                    self.stopped = true;
                }
            }

            // --- LD r8, r8 block (0x40-0x7F, excluding 0x76 = HALT) ------
            0x40..=0x7F => {
                let dst = (opcode >> 3) & 0x07;
                let src = opcode & 0x07;
                let val = self.read_r8(src);
                self.write_r8(dst, val);
            }

            // --- ALU A, r block (0x80–0xBF) --------------------------------
            0x80..=0xBF => {
                let op = (opcode >> 3) & 0x07;
                let src = opcode & 0x07;
                let val = self.read_r8(src);
                self.alu_dispatch(op, val);
            }

            // --- RET NZ / RET Z / RET NC / RET C / RET / RETI -----------
            0xC0 | 0xC8 | 0xD0 | 0xD8 => {
                self.internal_cycle(); // condition evaluation
                let cond = (opcode >> 3) & 0x03;
                if self.check_condition(cond) {
                    let addr = self.pop_u16();
                    self.regs.pc = addr;
                    self.internal_cycle();
                }
            }
            0xC9 => {
                let addr = self.pop_u16();
                self.regs.pc = addr;
                self.internal_cycle();
            }
            0xD9 => {
                // RETI
                let addr = self.pop_u16();
                self.regs.pc = addr;
                self.ime = true;
                self.internal_cycle();
            }

            // --- POP r16 ------------------------------------------------
            0xC1 => {
                let v = self.pop_u16();
                self.regs.set_bc(v);
            }
            0xD1 => {
                let v = self.pop_u16();
                self.regs.set_de(v);
            }
            0xE1 => {
                let v = self.pop_u16();
                self.regs.set_hl(v);
            }
            0xF1 => {
                let v = self.pop_u16();
                self.regs.set_af(v);
            }

            // --- PUSH r16 -----------------------------------------------
            0xC5 => {
                let v = self.regs.bc();
                self.internal_cycle();
                self.push_u16(v);
            }
            0xD5 => {
                let v = self.regs.de();
                self.internal_cycle();
                self.push_u16(v);
            }
            0xE5 => {
                let v = self.regs.hl();
                self.internal_cycle();
                self.push_u16(v);
            }
            0xF5 => {
                let v = self.regs.af();
                self.internal_cycle();
                self.push_u16(v);
            }

            // --- JP n16, JP HL, JP cc -----------------------------------
            0xC3 => {
                let addr = self.fetch_u16();
                self.regs.pc = addr;
                self.internal_cycle();
            }
            0xE9 => {
                self.regs.pc = self.regs.hl();
            }
            0xC2 | 0xCA | 0xD2 | 0xDA => {
                let addr = self.fetch_u16();
                let cond = (opcode >> 3) & 0x03;
                if self.check_condition(cond) {
                    self.regs.pc = addr;
                    self.internal_cycle();
                }
            }

            // --- JR e8, JR cc, e8 ----------------------------------------
            0x18 => {
                let e = self.fetch_byte() as i8;
                self.regs.pc = self.regs.pc.wrapping_add(e as u16);
                self.internal_cycle();
            }
            0x20 | 0x28 | 0x30 | 0x38 => {
                let e = self.fetch_byte() as i8;
                let cond = (opcode >> 3) & 0x03;
                if self.check_condition(cond) {
                    self.regs.pc = self.regs.pc.wrapping_add(e as u16);
                    self.internal_cycle();
                }
            }

            // --- CALL n16, CALL cc ---------------------------------------
            0xCD => {
                let addr = self.fetch_u16();
                self.internal_cycle();
                let pc = self.regs.pc;
                self.push_u16(pc);
                self.regs.pc = addr;
            }
            0xC4 | 0xCC | 0xD4 | 0xDC => {
                let addr = self.fetch_u16();
                let cond = (opcode >> 3) & 0x03;
                if self.check_condition(cond) {
                    self.internal_cycle();
                    let pc = self.regs.pc;
                    self.push_u16(pc);
                    self.regs.pc = addr;
                }
            }

            // --- RST vectors ---------------------------------------------
            0xC7 | 0xCF | 0xD7 | 0xDF | 0xE7 | 0xEF | 0xF7 | 0xFF => {
                let vec = (opcode & 0x38) as u16;
                self.internal_cycle();
                let pc = self.regs.pc;
                self.push_u16(pc);
                self.regs.pc = vec;
            }

            // --- ALU A, n8 (ADD/ADC/SUB/SBC/AND/XOR/OR/CP immediate) ----
            0xC6 | 0xCE | 0xD6 | 0xDE | 0xE6 | 0xEE | 0xF6 | 0xFE => {
                let n = self.fetch_byte();
                let op = (opcode >> 3) & 0x07;
                self.alu_dispatch(op, n);
            }

            // --- LDH / LD wide ------------------------------------------
            0xE0 => {
                let n = self.fetch_byte();
                let a = self.regs.a;
                self.write(0xFF00 | (n as u16), a);
            }
            0xF0 => {
                let n = self.fetch_byte();
                self.regs.a = self.read(0xFF00 | (n as u16));
            }
            0xE2 => {
                let c = self.regs.c;
                let a = self.regs.a;
                self.write(0xFF00 | (c as u16), a);
            }
            0xF2 => {
                let c = self.regs.c;
                self.regs.a = self.read(0xFF00 | (c as u16));
            }
            0xEA => {
                let addr = self.fetch_u16();
                let a = self.regs.a;
                self.write(addr, a);
            }
            0xFA => {
                let addr = self.fetch_u16();
                self.regs.a = self.read(addr);
            }

            // --- SP offsets ----------------------------------------------
            0xE8 => {
                let e = self.fetch_byte() as i8;
                let result = self.alu_sp_offset(e);
                self.regs.sp = result;
                self.internal_cycle();
                self.internal_cycle();
            }
            0xF8 => {
                let e = self.fetch_byte() as i8;
                let result = self.alu_sp_offset(e);
                self.regs.set_hl(result);
                self.internal_cycle();
            }
            0xF9 => {
                self.regs.sp = self.regs.hl();
                self.internal_cycle();
            }

            // --- DI / EI -----------------------------------------------
            0xF3 => {
                self.ime = false;
                self.ime_pending = false;
            }
            0xFB => {
                self.ime_pending = true;
            }

            // --- PREFIX CB -----------------------------------------------
            0xCB => {
                let cb_opcode = self.fetch_byte();
                self.execute_cb(cb_opcode);
            }

            // --- Illegal opcodes (lock up / treat as NOP) ----------------
            0xD3 | 0xDB | 0xDD | 0xE3 | 0xE4 | 0xEB | 0xEC | 0xED | 0xF4 | 0xFC | 0xFD => {
                // Real hardware locks up; we treat it as NOP for test purposes.
            }
        }
    }

    fn alu_dispatch(&mut self, op: u8, val: u8) {
        match op {
            0 => self.alu_add(val, 0),
            1 => {
                let c = self.regs.c_flag() as u8;
                self.alu_add(val, c);
            }
            2 => self.alu_sub(val, 0),
            3 => {
                let c = self.regs.c_flag() as u8;
                self.alu_sub(val, c);
            }
            4 => self.alu_and(val),
            5 => self.alu_xor(val),
            6 => self.alu_or(val),
            7 => self.alu_cp(val),
            _ => unreachable!(),
        }
    }

    fn execute_cb(&mut self, cb: u8) {
        let r = cb & 0x07;
        let op = (cb >> 3) & 0x07;
        let kind = cb >> 6;
        let bit = op;

        let val = self.read_r8(r);
        let result = match kind {
            0 => match op {
                0 => self.rlc(val),
                1 => self.rrc(val),
                2 => self.rl(val),
                3 => self.rr(val),
                4 => self.sla(val),
                5 => self.sra(val),
                6 => self.swap(val),
                7 => self.srl(val),
                _ => unreachable!(),
            },
            1 => {
                // BIT — does not write back
                let z = val & (1 << bit) == 0;
                let c = self.regs.c_flag();
                self.regs.set_flags(z, false, true, c);
                return;
            }
            2 => val & !(1 << bit), // RES
            3 => val | (1 << bit),  // SET
            _ => unreachable!(),
        };
        self.write_r8(r, result);
    }
}

// ---------------------------------------------------------------------------
// Unit tests — RED phase
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    /// Helper: build a CPU pre-loaded with a byte sequence at address 0x0000.
    /// PC starts at 0x0000.
    struct TestBus {
        mem: [u8; 0x10000],
    }

    impl TestBus {
        fn new(program: &[u8]) -> Self {
            let mut mem = [0u8; 0x10000];
            mem[..program.len()].copy_from_slice(program);
            Self { mem }
        }
    }

    impl GbBus for TestBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[addr as usize]
        }
        fn write(&mut self, addr: u16, val: u8) {
            self.mem[addr as usize] = val;
        }
    }

    fn cpu_with(program: &[u8]) -> Sm83<TestBus> {
        Sm83::new(TestBus::new(program))
    }

    struct WritePhaseSpyBus {
        mem: [u8; 0x10000],
        dot: u64,
        write_dot_mod4: Option<u8>,
    }

    impl WritePhaseSpyBus {
        fn new(program: &[u8]) -> Self {
            let mut mem = [0u8; 0x10000];
            mem[..program.len()].copy_from_slice(program);
            Self {
                mem,
                dot: 0,
                write_dot_mod4: None,
            }
        }
    }

    struct LateInterruptBus {
        mem: [u8; 0x10000],
        ticks: u8,
    }

    impl LateInterruptBus {
        fn new() -> Self {
            let mut mem = [0u8; 0x10000];
            mem[0xFFFF] = 0x03; // IE: VBlank + STAT enabled
            mem[0xFF0F] = 0x02; // IF: STAT pending at dispatch start
            Self { mem, ticks: 0 }
        }
    }

    impl GbBus for LateInterruptBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[addr as usize]
        }

        fn write(&mut self, addr: u16, val: u8) {
            self.mem[addr as usize] = val;
        }

        fn tick(&mut self, m_cycles: u8) {
            for _ in 0..m_cycles {
                self.ticks = self.ticks.saturating_add(1);
                if self.ticks == 3 {
                    self.mem[0xFF0F] |= 0x01; // VBlank becomes pending during dispatch
                }
            }
        }
    }

    impl GbBus for WritePhaseSpyBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[addr as usize]
        }

        fn write(&mut self, addr: u16, val: u8) {
            self.write_dot_mod4 = Some((self.dot % 4) as u8);
            self.mem[addr as usize] = val;
        }

        fn tick(&mut self, m_cycles: u8) {
            self.dot += u64::from(m_cycles) * 4;
        }

        fn write_cpu_m_cycle(&mut self, addr: u16, val: u8) {
            self.dot += 3;
            self.write(addr, val);
            self.dot += 1;
        }
    }

    #[test]
    fn test_cpu_writes_reach_bus_at_t3_of_m_cycle() {
        let mut cpu = Sm83::new(WritePhaseSpyBus::new(&[0xE0, 0x40]));
        cpu.regs.a = 0x91;

        cpu.execute();

        assert_eq!(
            cpu.bus.write_dot_mod4,
            Some(3),
            "SM83 writes should reach the bus at T3 of the write M-cycle"
        );
    }

    // -----------------------------------------------------------------------
    // NOP
    // -----------------------------------------------------------------------

    #[test]
    fn test_nop_advances_pc_by_1_and_takes_1_m_cycle() {
        let mut cpu = cpu_with(&[0x00]);
        cpu.execute();
        assert_eq!(cpu.regs.pc, 1, "PC should advance by 1 after NOP");
        assert_eq!(cpu.cycles(), 1, "NOP should take 1 M-cycle");
    }

    // -----------------------------------------------------------------------
    // LD r8, n8 (immediate loads)
    // -----------------------------------------------------------------------

    #[test]
    fn test_ld_b_n8_loads_immediate_into_b() {
        let mut cpu = cpu_with(&[0x06, 0x42]);
        cpu.execute();
        assert_eq!(cpu.regs.b, 0x42, "LD B,n8 should load 0x42 into B");
        assert_eq!(cpu.regs.pc, 2);
        assert_eq!(cpu.cycles(), 2);
    }

    #[test]
    fn test_ld_c_n8_loads_immediate_into_c() {
        let mut cpu = cpu_with(&[0x0E, 0x55]);
        cpu.execute();
        assert_eq!(cpu.regs.c, 0x55);
    }

    #[test]
    fn test_ld_d_n8_loads_immediate_into_d() {
        let mut cpu = cpu_with(&[0x16, 0x10]);
        cpu.execute();
        assert_eq!(cpu.regs.d, 0x10);
    }

    #[test]
    fn test_ld_e_n8_loads_immediate_into_e() {
        let mut cpu = cpu_with(&[0x1E, 0xAB]);
        cpu.execute();
        assert_eq!(cpu.regs.e, 0xAB);
    }

    #[test]
    fn test_ld_h_n8_loads_immediate_into_h() {
        let mut cpu = cpu_with(&[0x26, 0xCC]);
        cpu.execute();
        assert_eq!(cpu.regs.h, 0xCC);
    }

    #[test]
    fn test_ld_l_n8_loads_immediate_into_l() {
        let mut cpu = cpu_with(&[0x2E, 0x0F]);
        cpu.execute();
        assert_eq!(cpu.regs.l, 0x0F);
    }

    #[test]
    fn test_ld_a_n8_loads_immediate_into_a() {
        let mut cpu = cpu_with(&[0x3E, 0xFF]);
        cpu.execute();
        assert_eq!(cpu.regs.a, 0xFF);
    }

    // -----------------------------------------------------------------------
    // LD r8, r8
    // -----------------------------------------------------------------------

    #[test]
    fn test_ld_b_c_copies_c_into_b() {
        // LD C,n8; LD B,C
        let mut cpu = cpu_with(&[0x0E, 0x77, 0x41]);
        cpu.execute(); // LD C, 0x77
        cpu.execute(); // LD B, C
        assert_eq!(cpu.regs.b, 0x77, "LD B,C should copy C into B");
        assert_eq!(cpu.regs.c, 0x77, "C should be unchanged");
    }

    #[test]
    fn test_ld_a_b_copies_b_into_a() {
        let mut cpu = cpu_with(&[0x06, 0x11, 0x78]);
        cpu.execute(); // LD B, 0x11
        cpu.execute(); // LD A, B
        assert_eq!(cpu.regs.a, 0x11);
    }

    // -----------------------------------------------------------------------
    // LD r16, n16
    // -----------------------------------------------------------------------

    #[test]
    fn test_ld_bc_n16_loads_immediate_into_bc() {
        let mut cpu = cpu_with(&[0x01, 0x34, 0x12]); // LD BC, 0x1234 (little-endian)
        cpu.execute();
        assert_eq!(cpu.regs.bc(), 0x1234);
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn test_ld_hl_n16_loads_immediate_into_hl() {
        let mut cpu = cpu_with(&[0x21, 0xAD, 0xDE]); // LD HL, 0xDEAD
        cpu.execute();
        assert_eq!(cpu.regs.hl(), 0xDEAD);
    }

    // -----------------------------------------------------------------------
    // ADD A, r — flags
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_a_b_result_and_no_flags_on_nonzero() {
        // A=0x01, B=0x02 → A=0x03, Z=0, N=0, H=0, C=0
        let mut cpu = cpu_with(&[0x3E, 0x01, 0x06, 0x02, 0x80]);
        cpu.execute(); // LD A, 0x01
        cpu.execute(); // LD B, 0x02
        cpu.execute(); // ADD A, B
        assert_eq!(cpu.regs.a, 0x03);
        assert!(!cpu.regs.z_flag(), "Z should be clear");
        assert!(!cpu.regs.n_flag(), "N should be clear");
        assert!(!cpu.regs.h_flag(), "H should be clear");
        assert!(!cpu.regs.c_flag(), "C should be clear");
    }

    #[test]
    fn test_add_a_a_zero_sets_z_flag() {
        // A=0, B=0 → A=0, Z=1
        let mut cpu = cpu_with(&[0x87]); // ADD A, A  (A starts at 0)
        cpu.execute();
        assert_eq!(cpu.regs.a, 0);
        assert!(cpu.regs.z_flag(), "Z should be set when result is 0");
        assert!(!cpu.regs.n_flag());
    }

    #[test]
    fn test_add_a_b_sets_half_carry_flag() {
        // A=0x0F, B=0x01 → A=0x10, H=1
        let mut cpu = cpu_with(&[0x3E, 0x0F, 0x06, 0x01, 0x80]);
        cpu.execute();
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.a, 0x10);
        assert!(
            cpu.regs.h_flag(),
            "H should be set (0x0F + 0x01 half-carry)"
        );
        assert!(!cpu.regs.c_flag());
    }

    #[test]
    fn test_add_a_b_sets_carry_flag_on_overflow() {
        // A=0xFF, B=0x01 → A=0x00, C=1, Z=1
        let mut cpu = cpu_with(&[0x3E, 0xFF, 0x06, 0x01, 0x80]);
        cpu.execute();
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.a, 0x00);
        assert!(cpu.regs.c_flag(), "C should be set on byte overflow");
        assert!(cpu.regs.z_flag(), "Z should be set when result is 0");
    }

    // -----------------------------------------------------------------------
    // SUB A, r — N flag
    // -----------------------------------------------------------------------

    #[test]
    fn test_sub_a_a_results_zero_n_and_z_set() {
        // A=0x05; SUB A,A → A=0, Z=1, N=1
        let mut cpu = cpu_with(&[0x3E, 0x05, 0x97]);
        cpu.execute(); // LD A, 0x05
        cpu.execute(); // SUB A, A
        assert_eq!(cpu.regs.a, 0);
        assert!(cpu.regs.z_flag(), "Z should be set");
        assert!(cpu.regs.n_flag(), "N should be set after subtraction");
        assert!(!cpu.regs.h_flag());
        assert!(!cpu.regs.c_flag());
    }

    #[test]
    fn test_sub_a_sets_carry_when_underflow() {
        // A=0x00, B=0x01 → A=0xFF, C=1, N=1
        let mut cpu = cpu_with(&[0x06, 0x01, 0x90]); // LD B,1; SUB A,B
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.a, 0xFF);
        assert!(cpu.regs.c_flag());
        assert!(cpu.regs.n_flag());
    }

    // -----------------------------------------------------------------------
    // XOR A, A
    // -----------------------------------------------------------------------

    #[test]
    fn test_xor_a_a_clears_a_and_sets_z() {
        let mut cpu = cpu_with(&[0x3E, 0x55, 0xAF]); // LD A,0x55; XOR A,A
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.a, 0);
        assert!(cpu.regs.z_flag(), "Z should be set after XOR A,A");
        assert!(!cpu.regs.n_flag());
        assert!(!cpu.regs.h_flag());
        assert!(!cpu.regs.c_flag());
    }

    // -----------------------------------------------------------------------
    // AND A, r — H flag always set
    // -----------------------------------------------------------------------

    #[test]
    fn test_and_a_b_sets_h_flag() {
        let mut cpu = cpu_with(&[0x3E, 0xFF, 0x06, 0x0F, 0xA0]); // LD A,0xFF; LD B,0x0F; AND A,B
        cpu.execute();
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.a, 0x0F);
        assert!(cpu.regs.h_flag(), "AND always sets H");
        assert!(!cpu.regs.n_flag());
        assert!(!cpu.regs.c_flag());
    }

    // -----------------------------------------------------------------------
    // INC / DEC r8 — flag behavior
    // -----------------------------------------------------------------------

    #[test]
    fn test_inc_b_sets_h_flag_on_nibble_overflow() {
        // B=0x0F; INC B → B=0x10, H=1, Z=0, N=0
        let mut cpu = cpu_with(&[0x06, 0x0F, 0x04]);
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.b, 0x10);
        assert!(cpu.regs.h_flag());
        assert!(!cpu.regs.z_flag());
        assert!(!cpu.regs.n_flag());
    }

    #[test]
    fn test_inc_b_sets_z_flag_on_wrap() {
        // B=0xFF; INC B → B=0x00, Z=1
        let mut cpu = cpu_with(&[0x06, 0xFF, 0x04]);
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.b, 0x00);
        assert!(cpu.regs.z_flag());
    }

    #[test]
    fn test_dec_b_sets_n_flag() {
        let mut cpu = cpu_with(&[0x06, 0x02, 0x05]);
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.b, 0x01);
        assert!(cpu.regs.n_flag(), "DEC always sets N");
    }

    #[test]
    fn test_dec_b_sets_z_when_result_is_zero() {
        let mut cpu = cpu_with(&[0x06, 0x01, 0x05]);
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.b, 0);
        assert!(cpu.regs.z_flag());
        assert!(cpu.regs.n_flag());
    }

    // -----------------------------------------------------------------------
    // PUSH / POP round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_push_bc_pop_de_transfers_value() {
        // Set BC=0xBEEF, SP=0xFF00; PUSH BC; POP DE
        // program: LD BC,n16; LD SP,n16; PUSH BC; POP DE
        let mut cpu = cpu_with(&[
            0x01, 0xEF, 0xBE, // LD BC, 0xBEEF
            0x31, 0x00, 0xFF, // LD SP, 0xFF00
            0xC5, // PUSH BC
            0xD1, // POP DE
        ]);
        cpu.execute(); // LD BC
        cpu.execute(); // LD SP
        cpu.execute(); // PUSH BC
        cpu.execute(); // POP DE
        assert_eq!(
            cpu.regs.de(),
            0xBEEF,
            "POP DE should restore pushed BC value"
        );
    }

    // -----------------------------------------------------------------------
    // JR e8 — relative jump
    // -----------------------------------------------------------------------

    #[test]
    fn test_jr_unconditional_takes_3_m_cycles_and_jumps() {
        // JR +2: PC starts at 0, fetch opcode (1 M-cycle), fetch offset (1), jump (1) = 3
        // After the jump: new PC = 0x0002 + 0x0002 = 0x0004
        let mut cpu = cpu_with(&[0x18, 0x02]);
        cpu.execute();
        assert_eq!(cpu.regs.pc, 0x04, "JR +2 from PC=2 → PC=4");
        assert_eq!(cpu.cycles(), 3);
    }

    #[test]
    fn test_jr_nz_not_taken_no_extra_cycle() {
        // Z=0 so NZ condition is true → branch taken
        // A=0x01, then set Z via XOR then restore...
        // Instead: start with Z=1 (set manually via XOR A,A), then JR NZ should NOT jump
        let mut cpu = cpu_with(&[
            0xAF, // XOR A,A → Z=1
            0x20, 0x05, // JR NZ, +5 — not taken because Z=1
            0x00, // NOP (executed next)
        ]);
        cpu.execute(); // XOR A,A
        let pc_before = cpu.regs.pc;
        cpu.execute(); // JR NZ, +5
        // Not taken → PC = pc_before + 2 (opcode + offset)
        assert_eq!(
            cpu.regs.pc,
            pc_before + 2,
            "JR NZ not taken should advance PC by 2"
        );
        assert_eq!(
            cpu.cycles(),
            1 + 2, // XOR(1) + JR-not-taken(2)
            "Not-taken JR NZ costs 2 M-cycles"
        );
    }

    #[test]
    fn test_jr_nz_taken_adds_extra_cycle() {
        // Z=0 (default), JR NZ should jump
        let mut cpu = cpu_with(&[0x20, 0x03]); // JR NZ, +3; → PC = 2 + 3 = 5
        cpu.execute();
        assert_eq!(cpu.regs.pc, 5);
        assert_eq!(cpu.cycles(), 3, "Taken JR NZ costs 3 M-cycles");
    }

    // -----------------------------------------------------------------------
    // CALL / RET
    // -----------------------------------------------------------------------

    #[test]
    fn test_call_pushes_pc_and_jumps() {
        // CALL 0x0100
        // Program at 0x0000: CD 00 01
        // SP starts at 0xFFFE
        let mut program = [0u8; 0x200];
        program[0] = 0xCD;
        program[1] = 0x00;
        program[2] = 0x01;
        let mut cpu = Sm83::new(TestBus::new(&program));
        cpu.regs.sp = 0xFFFE;
        cpu.execute();
        assert_eq!(cpu.regs.pc, 0x0100, "CALL should jump to target");
        assert_eq!(
            cpu.regs.sp, 0xFFFC,
            "CALL should push return addr, decrementing SP by 2"
        );
    }

    #[test]
    fn test_ret_pops_and_jumps_back() {
        // Manually push 0x0200 onto the stack then RET
        let mut program = [0u8; 0x300];
        program[0] = 0xC9; // RET at 0x0000
        let mut cpu = Sm83::new(TestBus::new(&program));
        cpu.regs.sp = 0xFFFE;
        // Push 0x0200 manually
        cpu.regs.sp = cpu.regs.sp.wrapping_sub(2);
        cpu.bus.write(cpu.regs.sp, 0x00);
        cpu.bus.write(cpu.regs.sp.wrapping_add(1), 0x02);
        cpu.execute(); // RET
        assert_eq!(
            cpu.regs.pc, 0x0200,
            "RET should load popped address into PC"
        );
    }

    // -----------------------------------------------------------------------
    // CB-prefix — BIT, SET, RES
    // -----------------------------------------------------------------------

    #[test]
    fn test_cb_bit_0_b_clears_z_when_bit_set() {
        // B=0x01; CB BIT 0,B → Z=0 (bit 0 is set)
        let mut cpu = cpu_with(&[0x06, 0x01, 0xCB, 0x40]); // LD B,1; BIT 0,B
        cpu.execute();
        cpu.execute();
        assert!(
            !cpu.regs.z_flag(),
            "BIT 0,B: Z should be clear if bit is set"
        );
        assert!(cpu.regs.h_flag(), "BIT always sets H");
        assert!(!cpu.regs.n_flag(), "BIT always clears N");
    }

    #[test]
    fn test_cb_bit_0_b_sets_z_when_bit_clear() {
        // B=0x02; CB BIT 0,B → Z=1 (bit 0 is clear)
        let mut cpu = cpu_with(&[0x06, 0x02, 0xCB, 0x40]);
        cpu.execute();
        cpu.execute();
        assert!(
            cpu.regs.z_flag(),
            "BIT 0,B: Z should be set if bit is clear"
        );
    }

    #[test]
    fn test_cb_set_0_b_sets_bit_0() {
        let mut cpu = cpu_with(&[0x06, 0x00, 0xCB, 0xC0]); // LD B,0; SET 0,B
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.b, 0x01, "SET 0,B should set bit 0");
    }

    #[test]
    fn test_cb_res_0_b_clears_bit_0() {
        let mut cpu = cpu_with(&[0x06, 0xFF, 0xCB, 0x80]); // LD B,0xFF; RES 0,B
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.b, 0xFE, "RES 0,B should clear bit 0");
    }

    #[test]
    fn test_cb_rl_b_rotates_through_carry() {
        // B=0x80, C_flag=0 → RL B → B=0x00, C=1, Z=1
        let mut cpu = cpu_with(&[0x06, 0x80, 0xCB, 0x10]); // LD B,0x80; RL B
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.b, 0x00);
        assert!(cpu.regs.c_flag());
        assert!(cpu.regs.z_flag());
    }

    // -----------------------------------------------------------------------
    // HALT
    // -----------------------------------------------------------------------

    #[test]
    fn test_halt_sets_halted_flag() {
        let mut cpu = cpu_with(&[0x76]); // HALT
        cpu.execute();
        assert!(cpu.halted, "HALT should set the halted flag");
    }

    #[test]
    fn test_halted_cpu_consumes_1_m_cycle_per_execute_without_advancing_pc() {
        let mut cpu = cpu_with(&[0x76, 0x00]);
        cpu.execute(); // HALT — PC=1
        let pc = cpu.regs.pc;
        let cycles_before = cpu.cycles();
        cpu.execute(); // stall (halted)
        assert_eq!(cpu.regs.pc, pc, "PC should not advance while halted");
        assert_eq!(
            cpu.cycles(),
            cycles_before + 1,
            "Each halted tick costs 1 M-cycle"
        );
    }

    #[test]
    fn test_halt_bug_instruction_after_halt_executes_twice() {
        // HALT bug: when HALT executes with IME=false and a pending interrupt,
        // the CPU does NOT halt. The next opcode byte is fetched without
        // advancing PC, so the instruction after HALT executes twice.
        //
        // Memory layout:
        //   0x0000: 0x76  HALT
        //   0x0001: 0x04  INC B
        //   0xFFFF: 0x01  IE = VBlank enabled
        //   0xFF0F: 0x01  IF = VBlank pending
        let mut program = [0u8; 0x10000];
        program[0x0000] = 0x76; // HALT
        program[0x0001] = 0x04; // INC B
        program[0xFFFF] = 0x01; // IE: VBlank
        program[0xFF0F] = 0x01; // IF: VBlank pending
        let mut cpu = Sm83::new(TestBus::new(&program));
        cpu.ime = false;

        cpu.execute(); // HALT — halt_bug fires; CPU does not halt
        assert!(!cpu.halted, "CPU should not halt when halt bug fires");
        assert!(cpu.halt_bug, "halt_bug flag should be set");

        // First execute after HALT bug: reads 0x04 at PC=1, PC stays at 1
        cpu.execute();
        assert_eq!(cpu.regs.b, 1, "INC B executes first time");
        assert_eq!(cpu.regs.pc, 1, "PC should not advance after bugged fetch");

        // Second execute: reads 0x04 at PC=1 again (normally this time), PC → 2
        cpu.execute();
        assert_eq!(
            cpu.regs.b, 2,
            "INC B executes second time (HALT bug: same byte fetched again)"
        );
        assert_eq!(cpu.regs.pc, 2, "PC advances normally on second fetch");
    }

    // -----------------------------------------------------------------------
    // CPL
    // -----------------------------------------------------------------------

    #[test]
    fn test_cpl_complements_a_and_sets_n_h() {
        let mut cpu = cpu_with(&[0x3E, 0b10110101, 0x2F]); // LD A,0xB5; CPL
        cpu.execute();
        cpu.execute();
        assert_eq!(cpu.regs.a, !0b10110101u8);
        assert!(cpu.regs.n_flag(), "CPL sets N");
        assert!(cpu.regs.h_flag(), "CPL sets H");
    }

    // -----------------------------------------------------------------------
    // SCF / CCF
    // -----------------------------------------------------------------------

    #[test]
    fn test_scf_sets_carry_and_clears_n_h() {
        let mut cpu = cpu_with(&[0x37]); // SCF
        cpu.execute();
        assert!(cpu.regs.c_flag());
        assert!(!cpu.regs.n_flag());
        assert!(!cpu.regs.h_flag());
    }

    #[test]
    fn test_ccf_flips_carry() {
        let mut cpu = cpu_with(&[0x37, 0x3F]); // SCF; CCF
        cpu.execute();
        cpu.execute();
        assert!(
            !cpu.regs.c_flag(),
            "CCF should flip carry from set to clear"
        );
    }

    // -----------------------------------------------------------------------
    // Interrupt dispatch
    // -----------------------------------------------------------------------

    #[test]
    fn test_interrupt_dispatch_jumps_to_vblank_vector_when_ime_set() {
        // Sets up: IME=true, IE=0x01 (VBlank enabled), IF=0x01 (VBlank pending)
        // CPU should service the interrupt on next execute():
        //   - 2 internal cycles + push_pc (2 cycles) + jump (1 cycle) = 5 M-cycles
        let mut program = [0u8; 0x10000];
        program[0xFFFF] = 0x01; // IE: VBlank bit
        program[0xFF0F] = 0x01; // IF: VBlank pending
        let mut cpu = Sm83::new(TestBus::new(&program));
        cpu.ime = true;
        cpu.regs.pc = 0x1000;
        cpu.regs.sp = 0xFFFE;
        let cycles_before = cpu.cycles();
        cpu.execute();
        assert_eq!(cpu.regs.pc, 0x0040, "Should jump to VBlank vector 0x0040");
        assert_eq!(
            cpu.cycles() - cycles_before,
            5,
            "Interrupt dispatch takes 5 M-cycles"
        );
        assert!(!cpu.ime, "IME should be cleared after interrupt");
    }

    #[test]
    fn test_interrupt_clears_if_bit_for_serviced_source() {
        let mut program = [0u8; 0x10000];
        program[0xFFFF] = 0x01; // IE: VBlank
        program[0xFF0F] = 0x01; // IF: VBlank
        let mut cpu = Sm83::new(TestBus::new(&program));
        cpu.ime = true;
        cpu.regs.sp = 0xFFFE;
        cpu.execute();
        let new_if = cpu.bus.mem[0xFF0F];
        assert_eq!(
            new_if & 0x01,
            0,
            "VBlank IF bit should be cleared after dispatch"
        );
    }

    #[test]
    fn test_interrupt_dispatch_uses_late_higher_priority_request() {
        let mut cpu = Sm83::new(LateInterruptBus::new());
        cpu.ime = true;
        cpu.regs.pc = 0x1000;
        cpu.regs.sp = 0xFFFE;

        cpu.execute();

        assert_eq!(
            cpu.regs.pc, 0x0040,
            "A higher-priority VBlank request that appears during dispatch should win over the initially pending STAT request"
        );
        assert_eq!(
            cpu.bus.mem[0xFF0F] & 0x03,
            0x02,
            "Dispatch should clear only VBlank IF and leave the original STAT request pending"
        );
    }

    #[test]
    fn test_interrupt_with_ime_false_only_wakes_from_halt() {
        let mut program = [0u8; 0x10000];
        program[0xFFFF] = 0x01;
        program[0xFF0F] = 0x01;
        let mut cpu = Sm83::new(TestBus::new(&program));
        cpu.ime = false;
        cpu.halted = true;
        cpu.regs.pc = 0x1000;
        cpu.execute();
        assert!(!cpu.halted, "Pending interrupt should wake CPU from HALT");
        // With IME=false the CPU wakes from HALT and immediately executes the
        // next instruction (NOP at $1000) — no interrupt dispatch, so PC
        // advances normally rather than jumping to an ISR vector.
        assert_eq!(
            cpu.regs.pc, 0x1001,
            "PC should advance (next instr), not jump to ISR vector"
        );
    }

    // -----------------------------------------------------------------------
    // LD (n16), SP  (0x08)
    // -----------------------------------------------------------------------

    /// LD (n16),SP must store SP in little-endian order:
    /// low byte at [addr], high byte at [addr+1].
    ///
    /// Per Pan Docs: https://gbdev.io/pandocs/CPU_Instruction_Set.html
    #[test]
    fn test_ld_nn_sp_stores_sp_little_endian() {
        // Program: LD (0x8000), SP  →  0x08 0x00 0x80
        let mut cpu = cpu_with(&[0x08, 0x00, 0x80]);
        cpu.regs.sp = 0x1234;
        cpu.execute();
        assert_eq!(
            cpu.bus.mem[0x8000], 0x34,
            "low byte of SP should be stored at [addr]"
        );
        assert_eq!(
            cpu.bus.mem[0x8001], 0x12,
            "high byte of SP should be stored at [addr+1]"
        );
    }

    /// LD (n16),SP with SP=0 stores 0x00 at both bytes.
    #[test]
    fn test_ld_nn_sp_stores_zero_sp() {
        let mut cpu = cpu_with(&[0x08, 0x00, 0x80]);
        cpu.regs.sp = 0x0000;
        cpu.execute();
        assert_eq!(cpu.bus.mem[0x8000], 0x00);
        assert_eq!(cpu.bus.mem[0x8001], 0x00);
    }

    /// LD (n16),SP should take 5 M-cycles (fetch + fetch_u16 + write lo + write hi).
    #[test]
    fn test_ld_nn_sp_takes_5_m_cycles() {
        let mut cpu = cpu_with(&[0x08, 0x00, 0x80]);
        cpu.execute();
        assert_eq!(cpu.cycles(), 5, "LD (n16),SP should take 5 M-cycles");
    }

    // -----------------------------------------------------------------------
    // IDU glitch notification — INC/DEC r16
    //
    // On DMG hardware the Increment/Decrement Unit outputs the OLD register
    // value on the address bus, which can corrupt OAM during Mode 2.
    // The CPU must call bus.notify_idu_glitch(old_value) with the register
    // value BEFORE the increment/decrement, and BEFORE the internal cycle.
    // -----------------------------------------------------------------------

    struct SpyBus {
        mem: [u8; 0x10000],
        idu_glitch_called: bool,
        idu_glitch_addr: Option<u16>,
    }

    impl SpyBus {
        fn new(program: &[u8]) -> Self {
            let mut mem = [0u8; 0x10000];
            mem[..program.len()].copy_from_slice(program);
            Self {
                mem,
                idu_glitch_called: false,
                idu_glitch_addr: None,
            }
        }
    }

    impl GbBus for SpyBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[addr as usize]
        }
        fn write(&mut self, addr: u16, val: u8) {
            self.mem[addr as usize] = val;
        }
        fn notify_idu_glitch(&mut self, addr: u16) {
            self.idu_glitch_called = true;
            self.idu_glitch_addr = Some(addr);
        }
    }

    fn cpu_with_spy(program: &[u8]) -> Sm83<SpyBus> {
        Sm83::new(SpyBus::new(program))
    }

    #[test]
    fn test_inc_bc_notifies_idu_glitch_with_old_value() {
        // INC BC (0x03): notify_idu_glitch should be called with BC's value BEFORE increment.
        let mut cpu = cpu_with_spy(&[0x03]);
        cpu.regs.set_bc(0xFE10);
        cpu.execute();
        assert!(
            cpu.bus.idu_glitch_called,
            "INC BC must call notify_idu_glitch"
        );
        assert_eq!(
            cpu.bus.idu_glitch_addr,
            Some(0xFE10),
            "INC BC must pass the pre-increment value to notify_idu_glitch"
        );
        assert_eq!(cpu.regs.bc(), 0xFE11, "BC must be incremented");
    }

    #[test]
    fn test_inc_de_notifies_idu_glitch_with_old_value() {
        let mut cpu = cpu_with_spy(&[0x13]);
        cpu.regs.set_de(0xFE00);
        cpu.execute();
        assert_eq!(cpu.bus.idu_glitch_addr, Some(0xFE00));
        assert_eq!(cpu.regs.de(), 0xFE01);
    }

    #[test]
    fn test_inc_hl_notifies_idu_glitch_with_old_value() {
        let mut cpu = cpu_with_spy(&[0x23]);
        cpu.regs.set_hl(0xFE9F);
        cpu.execute();
        assert_eq!(cpu.bus.idu_glitch_addr, Some(0xFE9F));
        assert_eq!(cpu.regs.hl(), 0xFEA0);
    }

    #[test]
    fn test_inc_sp_notifies_idu_glitch_with_old_value() {
        let mut cpu = cpu_with_spy(&[0x33]);
        cpu.regs.sp = 0xFEFF;
        cpu.execute();
        assert_eq!(cpu.bus.idu_glitch_addr, Some(0xFEFF));
        assert_eq!(cpu.regs.sp, 0xFF00);
    }

    #[test]
    fn test_dec_bc_notifies_idu_glitch_with_old_value() {
        let mut cpu = cpu_with_spy(&[0x0B]);
        cpu.regs.set_bc(0xFE10);
        cpu.execute();
        assert_eq!(cpu.bus.idu_glitch_addr, Some(0xFE10));
        assert_eq!(cpu.regs.bc(), 0xFE0F);
    }

    #[test]
    fn test_dec_de_notifies_idu_glitch_with_old_value() {
        let mut cpu = cpu_with_spy(&[0x1B]);
        cpu.regs.set_de(0xFE50);
        cpu.execute();
        assert_eq!(cpu.bus.idu_glitch_addr, Some(0xFE50));
    }

    #[test]
    fn test_dec_hl_notifies_idu_glitch_with_old_value() {
        let mut cpu = cpu_with_spy(&[0x2B]);
        cpu.regs.set_hl(0xFEAA);
        cpu.execute();
        assert_eq!(cpu.bus.idu_glitch_addr, Some(0xFEAA));
    }

    #[test]
    fn test_dec_sp_notifies_idu_glitch_with_old_value() {
        let mut cpu = cpu_with_spy(&[0x3B]);
        cpu.regs.sp = 0xFE01;
        cpu.execute();
        assert_eq!(cpu.bus.idu_glitch_addr, Some(0xFE01));
        assert_eq!(cpu.regs.sp, 0xFE00);
    }

    // -----------------------------------------------------------------------
    // IDU notification — LD A, [HLI] / LD A, [HLD]
    //
    // Per spec (Pan Docs "OAM Corruption Bug"): LD A, [HLI] and LD A, [HLD]
    // trigger the "Read During Increase/Decrease" pattern when HL points to OAM:
    // M2 performs a read from [HL] and an IDU inc/dec in the same M-cycle,
    // which calls notify_idu_with_prior_read (applies apply_oam_read_idu_corruption).
    // -----------------------------------------------------------------------

    struct IduReadSpyBus {
        mem: [u8; 0x10000],
        idu_with_prior_read_addr: Option<u16>,
    }

    impl IduReadSpyBus {
        fn new(program: &[u8]) -> Self {
            let mut mem = [0u8; 0x10000];
            mem[..program.len()].copy_from_slice(program);
            Self {
                mem,
                idu_with_prior_read_addr: None,
            }
        }
    }

    impl GbBus for IduReadSpyBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[addr as usize]
        }
        fn write(&mut self, addr: u16, val: u8) {
            self.mem[addr as usize] = val;
        }
        fn notify_idu_with_prior_read(&mut self, addr: u16) {
            self.idu_with_prior_read_addr = Some(addr);
        }
    }

    #[test]
    fn test_ld_a_hli_notifies_idu_with_prior_read_at_hl() {
        // LD A, [HLI] (0x2A): when HL points to OAM the read+IDU must trigger
        // "Read During Increase/Decrease" via notify_idu_with_prior_read with the
        // pre-increment HL value.
        let mut cpu = Sm83::new(IduReadSpyBus::new(&[0x2A]));
        cpu.regs.set_hl(0xFE10);
        cpu.execute();
        assert_eq!(
            cpu.bus.idu_with_prior_read_addr,
            Some(0xFE10),
            "LD A, [HLI] must call notify_idu_with_prior_read with HL before increment"
        );
        assert_eq!(cpu.regs.hl(), 0xFE11, "HL must be incremented");
    }

    #[test]
    fn test_ld_a_hld_notifies_idu_with_prior_read_at_hl() {
        // LD A, [HLD] (0x3A): same contract but with decrement.
        let mut cpu = Sm83::new(IduReadSpyBus::new(&[0x3A]));
        cpu.regs.set_hl(0xFE50);
        cpu.execute();
        assert_eq!(
            cpu.bus.idu_with_prior_read_addr,
            Some(0xFE50),
            "LD A, [HLD] must call notify_idu_with_prior_read with HL before decrement"
        );
        assert_eq!(cpu.regs.hl(), 0xFE4F, "HL must be decremented");
    }

    // -----------------------------------------------------------------------
    // IDU notification — PUSH rr / POP rr
    //
    // PUSH: each DEC SP fires notify_idu_glitch (write corruption per spec).
    // POP: per Pan Docs POP triggers only 3 events (not 4):
    //   M2: read [SP] + IDU INC SP → notify_idu_with_prior_read(sp0) (Read During IDU)
    //   M3: read [SP+1], no IDU    → notify_oam_read(sp1)            (plain read corruption)
    // -----------------------------------------------------------------------

    struct GlitchListBus {
        mem: [u8; 0x10000],
        glitch_addrs: Vec<u16>,
        idu_with_prior_read_addrs: Vec<u16>,
        oam_read_addrs: Vec<u16>,
        oam_write_addrs: Vec<u16>,
    }

    impl GlitchListBus {
        fn new(program: &[u8]) -> Self {
            let mut mem = [0u8; 0x10000];
            mem[..program.len()].copy_from_slice(program);
            Self {
                mem,
                glitch_addrs: Vec::new(),
                idu_with_prior_read_addrs: Vec::new(),
                oam_read_addrs: Vec::new(),
                oam_write_addrs: Vec::new(),
            }
        }
    }

    impl GbBus for GlitchListBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[addr as usize]
        }
        fn write(&mut self, addr: u16, val: u8) {
            self.mem[addr as usize] = val;
        }
        fn notify_idu_glitch(&mut self, addr: u16) {
            self.glitch_addrs.push(addr);
        }
        fn notify_idu_with_prior_read(&mut self, addr: u16) {
            self.idu_with_prior_read_addrs.push(addr);
        }
        fn notify_oam_read(&mut self, addr: u16) {
            self.oam_read_addrs.push(addr);
        }
        fn notify_oam_write(&mut self, addr: u16) {
            self.oam_write_addrs.push(addr);
        }
    }

    #[test]
    fn test_push_bc_notifies_idu_glitch_for_both_dec_sp() {
        // PUSH BC (0xC5): per Pan Docs "OAM Corruption Bug":
        //   M2: DEC SP only (IDU-only) → notify_idu_glitch with SP before decrement
        //   M3: write hi + DEC SP ("Write During Decrease" = single write) → notify_idu_glitch
        //   M4: write lo only (no IDU) → notify_oam_write with final SP address
        let mut cpu = Sm83::new(GlitchListBus::new(&[0xC5]));
        cpu.regs.set_bc(0x1234);
        cpu.regs.sp = 0xFE20;
        cpu.execute();
        assert_eq!(
            cpu.bus.glitch_addrs,
            vec![0xFE20, 0xFE1F],
            "PUSH must call notify_idu_glitch for both DEC SP (with pre-decrement values)"
        );
        assert_eq!(
            cpu.bus.oam_write_addrs,
            vec![0xFE1E],
            "PUSH M4 must call notify_oam_write with the final SP address"
        );
        assert_eq!(cpu.regs.sp, 0xFE1E, "SP should end up decremented by 2");
    }

    #[test]
    fn test_pop_bc_uses_read_idu_for_first_inc_sp_and_oam_read_for_second() {
        // POP BC (0xC1): per Pan Docs POP triggers only 3 OAM-corruption events:
        //   M2: read [SP=0xFE18] + IDU INC SP → notify_idu_with_prior_read(0xFE18)
        //   M3: read [SP=0xFE19], no IDU       → notify_oam_read(0xFE19)
        // notify_idu_glitch must NOT be called.
        let mut cpu = Sm83::new(GlitchListBus::new(&[0xC1]));
        cpu.regs.sp = 0xFE18;
        cpu.execute();
        assert_eq!(
            cpu.bus.idu_with_prior_read_addrs,
            vec![0xFE18],
            "POP M2: must call notify_idu_with_prior_read with pre-increment SP"
        );
        assert_eq!(
            cpu.bus.oam_read_addrs,
            vec![0xFE19],
            "POP M3: must call notify_oam_read with the second SP address"
        );
        assert!(
            cpu.bus.glitch_addrs.is_empty(),
            "POP must NOT call notify_idu_glitch"
        );
        assert_eq!(cpu.regs.sp, 0xFE1A, "SP should end up incremented by 2");
    }

    // -----------------------------------------------------------------------
    // STOP instruction (0x10) — speed switch interaction
    // -----------------------------------------------------------------------

    /// Test bus that can simulate a successful speed switch.
    struct SpeedSwitchBus {
        mem: [u8; 0x10000],
        switch_armed: bool,
        speed_switched: bool,
    }

    impl SpeedSwitchBus {
        fn new(program: &[u8], armed: bool) -> Self {
            let mut mem = [0u8; 0x10000];
            mem[..program.len()].copy_from_slice(program);
            Self {
                mem,
                switch_armed: armed,
                speed_switched: false,
            }
        }
    }

    impl GbBus for SpeedSwitchBus {
        fn read(&mut self, addr: u16) -> u8 {
            self.mem[addr as usize]
        }
        fn write(&mut self, addr: u16, val: u8) {
            self.mem[addr as usize] = val;
        }
        fn try_speed_switch(&mut self) -> bool {
            if self.switch_armed {
                self.switch_armed = false;
                self.speed_switched = true;
                true
            } else {
                false
            }
        }
    }

    #[test]
    fn test_stop_with_speed_switch_armed_does_not_stop() {
        // Given: STOP instruction with speed switch armed
        // 0x10, 0x00 = STOP
        let mut cpu = Sm83::new(SpeedSwitchBus::new(&[0x10, 0x00], true));
        // When: execute STOP
        cpu.execute();
        // Then: CPU is NOT stopped (speed switch consumed the STOP)
        assert!(!cpu.stopped, "CPU should not be stopped after speed switch");
        // And: speed switch was triggered
        assert!(
            cpu.bus.speed_switched,
            "Speed switch should have been triggered"
        );
        // And: PC advanced past STOP + operand byte
        assert_eq!(cpu.regs.pc, 2, "PC should advance past STOP instruction");
    }

    #[test]
    fn test_stop_without_speed_switch_armed_stops_cpu() {
        // Given: STOP instruction without speed switch armed
        let mut cpu = Sm83::new(SpeedSwitchBus::new(&[0x10, 0x00], false));
        // When: execute STOP
        cpu.execute();
        // Then: CPU is stopped (no speed switch)
        assert!(cpu.stopped, "CPU should be stopped when no speed switch");
        // And: speed switch was NOT triggered
        assert!(
            !cpu.bus.speed_switched,
            "Speed switch should not have been triggered"
        );
    }

    #[test]
    fn test_stopped_cpu_does_not_resume_for_vblank_interrupt() {
        // Given: STOP instruction followed by an instruction that would mutate A
        let mut cpu = Sm83::new(SpeedSwitchBus::new(&[0x10, 0x00, 0x3E, 0x42], false));
        cpu.ime = true;
        cpu.bus.write(0xFFFF, 0x01);

        // When: STOP executes and a VBlank interrupt is requested afterwards
        cpu.execute();
        cpu.bus.write(0xFF0F, 0x01);
        let pc_after_stop = cpu.regs.pc;
        cpu.execute();

        // Then: STOP remains active and the next opcode is not executed
        assert!(cpu.stopped, "CPU should remain stopped");
        assert_eq!(cpu.regs.pc, pc_after_stop, "PC should not advance in STOP");
        assert_ne!(
            cpu.regs.a, 0x42,
            "instruction after STOP should not execute"
        );
    }

    #[test]
    fn test_stopped_cpu_resumes_for_joypad_interrupt_request() {
        // Given: STOP instruction followed by an instruction that mutates A
        let mut cpu = Sm83::new(SpeedSwitchBus::new(&[0x10, 0x00, 0x3E, 0x42], false));

        // When: STOP executes and a joypad wake request appears afterwards
        cpu.execute();
        cpu.bus.write(0xFF0F, 0x10);
        cpu.execute();

        // Then: STOP is cleared and execution resumes at the next opcode
        assert!(!cpu.stopped, "CPU should leave STOP on joypad wake");
        assert_eq!(cpu.regs.a, 0x42, "instruction after STOP should execute");
    }
}
