//! GSU opcode semantics, from the fullsnes opcode tables ("SNES Cart GSU-n CPU MOV Opcodes",
//! "ALU Opcodes", "JMP and Prefix Opcodes").
//!
//! Prefixes: ALT1 (`$3D`), ALT2 (`$3E`) and ALT3 (`$3F`) select an opcode's variant; TO (`$1n`),
//! FROM (`$Bn`) and WITH (`$2n`) select Dreg/Sreg. Every opcode except the prefixes themselves and
//! the `Bxx` branches resets them afterwards (fullsnes: "that does really apply to ALL other
//! opcodes"). An opcode with no variant for the ALT bits set simply ignores them, which is
//! fullsnes' "ALT1/ALT2 prefixes are ignored if the opcode doesn't exist" and its "ALT3 does
//! reportedly mirror to ALT1" (each handler tests ALT1 before ALT2 where both exist).

use super::Gsu;

impl Gsu {
    pub(super) fn execute(&mut self, opcode: u8) {
        let n = opcode & 0x0F;
        match opcode {
            0x00 => self.op_stop(),
            0x01 => self.reset_prefixes(),
            0x03 => self.op_lsr(),
            0x04 => self.op_rol(),
            0x05 => self.op_branch(true),
            0x06 => self.op_branch(self.state.sign == self.state.overflow),
            0x07 => self.op_branch(self.state.sign != self.state.overflow),
            0x08 => self.op_branch(!self.state.zero),
            0x09 => self.op_branch(self.state.zero),
            0x0A => self.op_branch(!self.state.sign),
            0x0B => self.op_branch(self.state.sign),
            0x0C => self.op_branch(!self.state.carry),
            0x0D => self.op_branch(self.state.carry),
            0x0E => self.op_branch(!self.state.overflow),
            0x0F => self.op_branch(self.state.overflow),
            0x10..=0x1F => self.op_to_move(n),
            0x20..=0x2F => self.op_with(n),
            0x3C => self.op_loop(),
            0x3D => self.op_alt(true, false),
            0x3E => self.op_alt(false, true),
            0x3F => self.op_alt(true, true),
            0x4D => self.op_swap(),
            0x4F => self.op_not(),
            0x50..=0x5F => self.op_add_adc(n),
            0x60..=0x6F => self.op_sub_sbc_cmp(n),
            0x70 => self.op_merge(),
            0x71..=0x7F => self.op_and_bic(n),
            0x80..=0x8F => self.op_mult_umult(n),
            0x91..=0x94 => self.op_link(n),
            0x95 => self.op_sex(),
            0x96 => self.op_asr_div2(),
            0x97 => self.op_ror(),
            0x98..=0x9D => self.op_jmp_ljmp(n),
            0x9E => self.op_lob(),
            0x9F => self.op_fmult_lmult(),
            0xA0..=0xAF => self.op_ibt_lms_sms(n),
            0xB0..=0xBF => self.op_from_moves(n),
            0xC0 => self.op_hib(),
            0xC1..=0xCF => self.op_or_xor(n),
            0xD0..=0xDE => self.op_inc(n),
            0xE0..=0xEE => self.op_dec(n),
            0xF0..=0xFF => self.op_iwt_lm_sm(n),
            _ => self.reset_prefixes(),
        }
    }

    fn src(&self) -> u16 {
        self.state.r[usize::from(self.state.sreg)]
    }

    fn write_dest(&mut self, value: u16) {
        self.write_reg(self.state.dreg, value);
    }

    /// The second operand of the `$5n-$Cn` ALU opcodes: the constant `n` with ALT2, else Rn.
    fn alu_operand(&self, n: u8) -> u16 {
        if self.state.alt2 {
            u16::from(n)
        } else {
            self.state.r[usize::from(n)]
        }
    }

    fn set_sz(&mut self, value: u16) {
        self.state.sign = value & 0x8000 != 0;
        self.state.zero = value == 0;
    }

    /// Writes Dreg with a result whose S and Z flags follow it, and ends the opcode.
    fn finish_sz(&mut self, value: u16) {
        self.write_dest(value);
        self.set_sz(value);
        self.reset_prefixes();
    }

    // ---- Prefixes -------------------------------------------------------------------------

    /// ALT1/ALT2/ALT3. They clear B, so a WITH before them no longer turns `1n`/`Bn` into
    /// MOVE/MOVES (Mesen2 `ALT1`/`ALT2`/`ALT3`); Sreg/Dreg survive.
    fn op_alt(&mut self, alt1: bool, alt2: bool) {
        self.state.b_prefix = false;
        self.state.alt1 |= alt1;
        self.state.alt2 |= alt2;
    }

    /// `$1n`: TO Rn, or MOVE Rn,Rs after WITH.
    fn op_to_move(&mut self, n: u8) {
        if self.state.b_prefix {
            self.write_reg(n, self.src());
            self.reset_prefixes();
        } else {
            self.state.dreg = n;
        }
    }

    /// `$2n`: WITH Rn selects Rn as both Sreg and Dreg and sets B.
    fn op_with(&mut self, n: u8) {
        self.state.sreg = n;
        self.state.dreg = n;
        self.state.b_prefix = true;
    }

    /// `$Bn`: FROM Rn, or MOVES Rd,Rn after WITH (flags from the value, OV = bit 7).
    fn op_from_moves(&mut self, n: u8) {
        if self.state.b_prefix {
            let value = self.state.r[usize::from(n)];
            self.write_dest(value);
            self.state.overflow = value & 0x80 != 0;
            self.set_sz(value);
            self.reset_prefixes();
        } else {
            self.state.sreg = n;
        }
    }

    // ---- Control flow ---------------------------------------------------------------------

    /// `$05-$0F`: Bxx, R15 += signed offset relative to the byte after the operand. Branches are
    /// the one opcode family that leaves the prefixes set (fullsnes), so a prefix before the
    /// branch applies to the delay-slot opcode after it.
    fn op_branch(&mut self, taken: bool) {
        let offset = self.read_operand() as i8;
        if taken {
            self.write_reg(15, self.state.r[15].wrapping_add_signed(i16::from(offset)));
        }
    }

    /// `$98-$9D`: JMP Rn; with ALT1 LJMP Rn (PBR = Rn, R15 = Sreg), which also moves the code
    /// cache to the target and empties it (fullsnes "Code-Cache": "LJMP sets CBR to R15 AND
    /// FFF0h").
    fn op_jmp_ljmp(&mut self, n: u8) {
        let target_reg = self.state.r[usize::from(n)];
        if self.state.alt1 {
            self.state.pbr = target_reg as u8;
            let target = self.src();
            self.write_reg(15, target);
            self.state.cbr = target & 0xFFF0;
            self.invalidate_all_code_cache_lines();
        } else {
            self.write_reg(15, target_reg);
        }
        self.reset_prefixes();
    }

    /// `$3C`: LOOP, R12 -= 1 and jump to R13 unless it reached zero.
    fn op_loop(&mut self) {
        let counter = self.state.r[12].wrapping_sub(1);
        self.state.r[12] = counter;
        self.set_sz(counter);
        if counter != 0 {
            self.write_reg(15, self.state.r[13]);
        }
        self.reset_prefixes();
    }

    /// `$91-$94`: LINK #n, R11 = R15 + n (R15 already the next opcode's address).
    fn op_link(&mut self, n: u8) {
        self.state.r[11] = self.state.r[15].wrapping_add(u16::from(n));
        self.reset_prefixes();
    }

    // ---- Arithmetic -----------------------------------------------------------------------

    /// `$5n`: ADD Rn; ALT1 ADC Rn; ALT2 ADD #n; ALT3 ADC #n.
    fn op_add_adc(&mut self, n: u8) {
        let a = self.src();
        let b = self.alu_operand(n);
        let carry_in = u32::from(self.state.alt1 && self.state.carry);
        let result = u32::from(a) + u32::from(b) + carry_in;
        let value = result as u16;
        self.state.carry = result > 0xFFFF;
        self.state.overflow = !(a ^ b) & (b ^ value) & 0x8000 != 0;
        self.finish_sz(value);
    }

    /// `$6n`: SUB Rn; ALT1 SBC Rn; ALT2 SUB #n; ALT3 CMP Rn (flags only).
    fn op_sub_sbc_cmp(&mut self, n: u8) {
        let (alt1, alt2) = (self.state.alt1, self.state.alt2);
        let a = self.src();
        let b = if alt2 && !alt1 {
            u16::from(n)
        } else {
            self.state.r[usize::from(n)]
        };
        let borrow_in = i32::from(alt1 && !alt2 && !self.state.carry);
        let result = i32::from(a) - i32::from(b) - borrow_in;
        let value = result as u16;
        self.state.carry = result >= 0;
        self.state.overflow = (a ^ b) & (a ^ value) & 0x8000 != 0;
        self.set_sz(value);
        if !(alt1 && alt2) {
            self.write_dest(value);
        }
        self.reset_prefixes();
    }

    /// `$7n` (n = 1..15): AND Rn; ALT1 BIC Rn; ALT2 AND #n; ALT3 BIC #n.
    fn op_and_bic(&mut self, n: u8) {
        let b = self.alu_operand(n);
        let value = if self.state.alt1 {
            self.src() & !b
        } else {
            self.src() & b
        };
        self.finish_sz(value);
    }

    /// `$Cn` (n = 1..15): OR Rn; ALT1 XOR Rn; ALT2 OR #n; ALT3 XOR #n.
    fn op_or_xor(&mut self, n: u8) {
        let b = self.alu_operand(n);
        let value = if self.state.alt1 {
            self.src() ^ b
        } else {
            self.src() | b
        };
        self.finish_sz(value);
    }

    fn op_not(&mut self) {
        self.finish_sz(!self.src());
    }

    /// `$Dn` (n = 0..14): INC Rn. Rn itself, not Dreg.
    fn op_inc(&mut self, n: u8) {
        let value = self.state.r[usize::from(n)].wrapping_add(1);
        self.write_reg(n, value);
        self.set_sz(value);
        self.reset_prefixes();
    }

    /// `$En` (n = 0..14): DEC Rn.
    fn op_dec(&mut self, n: u8) {
        let value = self.state.r[usize::from(n)].wrapping_sub(1);
        self.write_reg(n, value);
        self.set_sz(value);
        self.reset_prefixes();
    }

    // ---- Shifts and rotates ---------------------------------------------------------------

    fn op_lsr(&mut self) {
        let a = self.src();
        self.state.carry = a & 1 != 0;
        self.finish_sz(a >> 1);
    }

    /// `$96`: ASR; with ALT1 DIV2, which is ASR except that -1 gives 0 (fullsnes).
    fn op_asr_div2(&mut self) {
        let a = self.src();
        self.state.carry = a & 1 != 0;
        let value = if self.state.alt1 && a == 0xFFFF {
            0
        } else {
            ((a as i16) >> 1) as u16
        };
        self.finish_sz(value);
    }

    fn op_rol(&mut self) {
        let a = self.src();
        let value = a << 1 | u16::from(self.state.carry);
        self.state.carry = a & 0x8000 != 0;
        self.finish_sz(value);
    }

    fn op_ror(&mut self) {
        let a = self.src();
        let value = a >> 1 | u16::from(self.state.carry) << 15;
        self.state.carry = a & 1 != 0;
        self.finish_sz(value);
    }

    // ---- Byte operations ------------------------------------------------------------------

    fn op_swap(&mut self) {
        self.finish_sz(self.src().rotate_right(8));
    }

    fn op_sex(&mut self) {
        self.finish_sz(self.src() as u8 as i8 as u16);
    }

    /// LOB and HIB set SF from bit 7 of their byte result (fullsnes "SF=Bit7").
    fn finish_byte(&mut self, value: u8) {
        self.write_dest(u16::from(value));
        self.state.sign = value & 0x80 != 0;
        self.state.zero = value == 0;
        self.reset_prefixes();
    }

    fn op_lob(&mut self) {
        self.finish_byte(self.src() as u8);
    }

    fn op_hib(&mut self) {
        self.finish_byte((self.src() >> 8) as u8);
    }

    /// `$70`: MERGE, Dreg = R7.hi:R8.hi with fullsnes' own flag rules.
    fn op_merge(&mut self) {
        let value = (self.state.r[7] & 0xFF00) | (self.state.r[8] >> 8);
        self.write_dest(value);
        self.state.sign = value & 0x8080 != 0;
        self.state.overflow = value & 0xC0C0 != 0;
        self.state.carry = value & 0xE0E0 != 0;
        self.state.zero = value & 0xF0F0 != 0;
        self.reset_prefixes();
    }

    // ---- Multiplies -----------------------------------------------------------------------

    /// `$8n`: MULT Rn; ALT1 UMULT Rn; ALT2 MULT #n; ALT3 UMULT #n. 8x8 bits, 16-bit result.
    fn op_mult_umult(&mut self, n: u8) {
        let a = self.src() as u8;
        let b = self.alu_operand(n) as u8;
        let value = if self.state.alt1 {
            u16::from(a) * u16::from(b)
        } else {
            (i16::from(a as i8) * i16::from(b as i8)) as u16
        };
        self.finish_sz(value);
        // fullsnes lists 1 or 2 cycles by CFGR MS0; the extra is Mesen2's `Step(HighSpeedMode ?
        // 1 : 2)` in master clocks.
        let extra = if self.state.cfgr & 0x20 != 0 { 1 } else { 2 };
        self.step(extra);
    }

    /// `$9F`: FMULT (Dreg = high word of Sreg * R6); ALT1 LMULT (also R4 = low word). CY is bit
    /// 15 of the 32-bit product. R4 is written before Dreg, so LMULT with Dreg=R4 leaves the high
    /// word there, as fullsnes reports.
    fn op_fmult_lmult(&mut self) {
        let product = i32::from(self.src() as i16) * i32::from(self.state.r[6] as i16);
        if self.state.alt1 {
            self.write_reg(4, product as u16);
        }
        let value = (product >> 16) as u16;
        self.state.carry = product & 0x8000 != 0;
        self.finish_sz(value);
        // fullsnes: 4 or 8 cycles (FMULT), by CFGR MS0; Mesen2 charges
        // `(HighSpeedMode ? 3 : 7) * (ClockSelect ? 1 : 2)` master clocks on top of the fetch.
        let cycles = if self.state.cfgr & 0x20 != 0 { 3 } else { 7 };
        let per_cycle = if self.state.clock_21mhz { 1 } else { 2 };
        self.step(cycles * per_cycle);
    }

    /// Resets B, ALT1, ALT2, Sreg and Dreg after an opcode.
    pub(super) fn reset_prefixes(&mut self) {
        let s = &mut self.state;
        s.b_prefix = false;
        s.alt1 = false;
        s.alt2 = false;
        s.sreg = 0;
        s.dreg = 0;
    }

    /// STOP: GO=0 and IRQ=1, "even if IRQ is disabled in CFGR.IRQ" (fullsnes). The byte after
    /// STOP has already been prefetched, which is why R15 ends at `$+2`. The prefetch is replaced
    /// by a NOP so a restart does not execute that stale byte (Mesen2 `Gsu::STOP`).
    fn op_stop(&mut self) {
        self.state.go = false;
        self.state.irq = true;
        self.state.program_prefetch = super::NOP_OPCODE;
        self.reset_prefixes();
    }

    /// `$An`: IBT Rn,#pp; with ALT1 LMS Rn,(yy); with ALT2 SMS (yy),Rn.
    fn op_ibt_lms_sms(&mut self, n: u8) {
        let value = self.read_operand() as i8 as u16;
        self.write_reg(n, value);
        self.reset_prefixes();
    }

    /// `$Fn`: IWT Rn,#yyxx; with ALT1 LM Rn,(hilo); with ALT2 SM (hilo),Rn.
    fn op_iwt_lm_sm(&mut self, n: u8) {
        let lo = self.read_operand();
        let hi = self.read_operand();
        self.write_reg(n, u16::from_le_bytes([lo, hi]));
        self.reset_prefixes();
    }
}
