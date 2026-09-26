//! uPD77C25 opcode execution (fullsnes "uPD77C25 Opcode Encoding", "ALU and LD Instructions",
//! "JP Instructions").

use super::{
    DATA_WORDS, Flags, RAM_WORDS, SR_RQM, SR_SIC, SR_SOC, SR_WRITE_PROTECTED, STACK_LEVELS,
    Upd77c25,
};

impl Upd77c25 {
    pub(super) fn execute(&mut self, opcode: u32) {
        match opcode >> 22 {
            0b00 => self.exec_alu(opcode),
            0b01 => {
                // RT=1: the ALU instruction, then a return.
                self.exec_alu(opcode);
                let s = &mut self.state;
                s.sp = ((usize::from(s.sp) + STACK_LEVELS - 1) % STACK_LEVELS) as u8;
                s.pc = s.stack[usize::from(s.sp)];
            }
            0b10 => self.exec_jump(opcode),
            _ => self.load(opcode & 0x0F, ((opcode >> 6) & 0xFFFF) as u16),
        }
    }

    fn ram(&self, addr: u16) -> u16 {
        self.state.ram[usize::from(addr) & (RAM_WORDS - 1)]
    }

    fn data_rom(&self, addr: u16) -> u16 {
        self.firmware.data[usize::from(addr) & (DATA_WORDS - 1)]
    }

    fn exec_alu(&mut self, opcode: u32) {
        let source = self.source((opcode >> 4) & 0x0F);
        let alu_op = (opcode >> 16) & 0x0F;
        if alu_op != 0 {
            self.alu(opcode, alu_op, source);
        }

        let dst = opcode & 0x0F;
        self.load(dst, source);

        if dst != 0x04 {
            let s = &mut self.state;
            let mut dp = s.dp;
            match (opcode >> 13) & 0x03 {
                1 => dp = (dp & 0xF0) | ((dp + 1) & 0x0F),
                2 => dp = (dp & 0xF0) | (dp.wrapping_sub(1) & 0x0F),
                3 => dp &= 0xF0,
                _ => {}
            }
            let dph = ((opcode >> 9) & 0x0F) as u16;
            s.dp = (dp ^ (dph << 4)) & 0xFF;
        }
        if opcode & 0x100 != 0 && dst != 0x05 {
            self.state.rp = self.state.rp.wrapping_sub(1) & 0x3FF;
        }
    }

    fn alu(&mut self, opcode: u32, alu_op: u32, source: u16) {
        let use_b = opcode & 0x8000 != 0;
        let (acc, mut flags, other_carry) = if use_b {
            (self.state.b, self.state.flags_b, self.state.flags_a.c)
        } else {
            (self.state.a, self.state.flags_a, self.state.flags_b.c)
        };
        let mut p = match (opcode >> 20) & 0x03 {
            0 => self.ram(self.state.dp),
            1 => source,
            2 => self.state.m,
            _ => self.state.n,
        };
        let oc = u16::from(other_carry);
        let result = match alu_op {
            0x01 => acc | p,
            0x02 => acc & p,
            0x03 => acc ^ p,
            0x04 => acc.wrapping_sub(p),
            0x05 => acc.wrapping_add(p),
            0x06 => acc.wrapping_sub(p).wrapping_sub(oc),
            0x07 => acc.wrapping_add(p).wrapping_add(oc),
            0x08 => {
                p = 1;
                acc.wrapping_sub(1)
            }
            0x09 => {
                p = 1;
                acc.wrapping_add(1)
            }
            0x0A => !acc,
            0x0B => (acc >> 1) | (acc & 0x8000),
            0x0C => (acc << 1) | oc,
            0x0D => (acc << 2) | 0x03,
            0x0E => (acc << 4) | 0x0F,
            _ => acc.rotate_left(8),
        };

        flags.z = result == 0;
        flags.s0 = result & 0x8000 != 0;
        if !flags.ov1 {
            flags.s1 = flags.s0;
        }
        match alu_op {
            0x04..=0x09 => {
                let adds = alu_op & 1 != 0;
                let overflow = (acc ^ result) & (p ^ if adds { result } else { acc });
                flags.ov0 = overflow & 0x8000 != 0;
                if flags.ov0 && flags.ov1 {
                    flags.ov1 = flags.s0 == flags.s1;
                } else {
                    flags.ov1 |= flags.ov0;
                }
                flags.c = (acc ^ p ^ result ^ overflow) & 0x8000 != 0;
            }
            0x0B => {
                flags.c = acc & 1 != 0;
                flags.ov0 = false;
                flags.ov1 = false;
            }
            0x0C => {
                flags.c = acc & 0x8000 != 0;
                flags.ov0 = false;
                flags.ov1 = false;
            }
            _ => {
                flags.c = false;
                flags.ov0 = false;
                flags.ov1 = false;
            }
        }

        if use_b {
            self.state.b = result;
            self.state.flags_b = flags;
        } else {
            self.state.a = result;
            self.state.flags_a = flags;
        }
    }

    fn exec_jump(&mut self, opcode: u32) {
        let brch = (opcode >> 13) & 0x1FF;
        let target = (((opcode & 0x03) << 11) | ((opcode >> 2) & 0x7FF)) as u16 & 0x7FF;
        let s = &self.state;
        let (fa, fb): (Flags, Flags) = (s.flags_a, s.flags_b);
        let condition = match brch {
            0x000 => {
                self.state.pc = self.state.so & 0x7FF;
                return;
            }
            0x100 | 0x101 => true,
            0x140 | 0x141 => {
                let s = &mut self.state;
                s.stack[usize::from(s.sp)] = s.pc;
                s.sp = ((usize::from(s.sp) + 1) % STACK_LEVELS) as u8;
                s.pc = target;
                return;
            }
            0x080 => !fa.c,
            0x082 => fa.c,
            0x084 => !fb.c,
            0x086 => fb.c,
            0x088 => !fa.z,
            0x08A => fa.z,
            0x08C => !fb.z,
            0x08E => fb.z,
            0x090 => !fa.ov0,
            0x092 => fa.ov0,
            0x094 => !fb.ov0,
            0x096 => fb.ov0,
            0x098 => !fa.ov1,
            0x09A => fa.ov1,
            0x09C => !fb.ov1,
            0x09E => fb.ov1,
            0x0A0 => !fa.s0,
            0x0A2 => fa.s0,
            0x0A4 => !fb.s0,
            0x0A6 => fb.s0,
            0x0A8 => !fa.s1,
            0x0AA => fa.s1,
            0x0AC => !fb.s1,
            0x0AE => fb.s1,
            0x0B0 => s.dp & 0x0F == 0,
            0x0B1 => s.dp & 0x0F != 0,
            0x0B2 => s.dp & 0x0F == 0x0F,
            0x0B3 => s.dp & 0x0F != 0x0F,
            0x0B4 => s.sr & SR_SIC == 0,
            0x0B6 => s.sr & SR_SIC != 0,
            0x0B8 => s.sr & SR_SOC == 0,
            0x0BA => s.sr & SR_SOC != 0,
            0x0BC => s.sr & SR_RQM == 0,
            0x0BE => s.sr & SR_RQM != 0,
            _ => false,
        };
        if condition {
            let self_loop = self.state.pc.wrapping_sub(1) & 0x7FF == target;
            if self_loop && (brch == 0x0BC || brch == 0x0BE) {
                self.state.in_rqm_loop = true;
            }
            self.state.pc = target;
        }
    }

    fn source(&mut self, src: u32) -> u16 {
        let s = &self.state;
        match src {
            0x00 => s.trb,
            0x01 => s.a,
            0x02 => s.b,
            0x03 => s.tr,
            0x04 => s.dp,
            0x05 => s.rp,
            0x06 => self.data_rom(s.rp),
            0x07 => 0x8000 - u16::from(s.flags_a.s1),
            0x08 => {
                self.state.sr |= SR_RQM;
                self.state.dr
            }
            0x09 => s.dr,
            0x0A => s.sr,
            0x0B | 0x0C => s.si,
            0x0D => s.k,
            0x0E => s.l,
            _ => self.ram(s.dp),
        }
    }

    fn load(&mut self, dst: u32, value: u16) {
        let rom_at_rp = self.data_rom(self.state.rp);
        let ram_at_dp_40 = self.ram(self.state.dp | 0x40);
        let s = &mut self.state;
        match dst {
            0x01 => s.a = value,
            0x02 => s.b = value,
            0x03 => s.tr = value,
            0x04 => s.dp = value & 0xFF,
            0x05 => s.rp = value & 0x3FF,
            0x06 => {
                s.dr = value;
                s.sr |= SR_RQM;
            }
            0x07 => s.sr = (s.sr & SR_WRITE_PROTECTED) | (value & !SR_WRITE_PROTECTED),
            0x08 | 0x09 => s.so = value,
            0x0A => s.k = value,
            0x0B => {
                s.k = value;
                s.l = rom_at_rp;
            }
            0x0C => {
                s.l = value;
                s.k = ram_at_dp_40;
            }
            0x0D => s.l = value,
            0x0E => s.trb = value,
            0x0F => {
                let addr = usize::from(s.dp) & (RAM_WORDS - 1);
                s.ram[addr] = value;
            }
            _ => {}
        }
    }
}
