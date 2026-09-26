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
            0xA0..=0xAF => self.op_ibt_lms_sms(n),
            0xF0..=0xFF => self.op_iwt_lm_sm(n),
            _ => self.reset_prefixes(),
        }
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
