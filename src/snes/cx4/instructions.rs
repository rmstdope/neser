//! The HG51B169 instruction set (fullsnes "SNES Cart Capcom CX4 - Opcodes").
//!
//! Every opcode is 16 bits: bits 15-10 select the operation, bits 9-8 a modifier (the
//! `A*1/A*2/A*100h/A*10000h` shift, a byte index, a register, or "far" for jumps), bits 7-0 an
//! operand (a source register or constant, an 8-bit immediate, or a jump target within the
//! 256-word page). fullsnes leaves most flag effects as `??`; those follow Mesen2
//! `Cx4.Instructions.cpp`.

use super::Cx4;
use super::data_rom::DATA_ROM;

/// The `A*1/A*2/A*100h/A*10000h` operand selected by opcode bits 9-8.
const SHIFT: [u32; 4] = [0, 1, 8, 16];

/// Sign-extends a 24-bit value.
fn sign_extend_24(value: u32) -> i32 {
    ((value << 8) as i32) >> 8
}

impl Cx4 {
    /// Executes one opcode, then spends its cycle.
    pub(super) fn exec(&mut self, opcode: u16) {
        let op = (opcode >> 8) as u8 & 0xFC;
        let modifier = (opcode >> 8) as u8 & 0x03;
        let operand = opcode as u8;
        let flags = (
            self.state.zero,
            self.state.carry,
            self.state.negative,
            self.state.overflow,
        );
        match op {
            0x08 => self.branch(true, modifier, operand),
            0x0C => self.branch(flags.0, modifier, operand),
            0x10 => self.branch(flags.1, modifier, operand),
            0x14 => self.branch(flags.2, modifier, operand),
            0x18 => self.branch(flags.3, modifier, operand),
            0x1C => self.wait(),
            0x24 => self.skip(modifier, operand),
            0x28 => self.call(true, modifier, operand),
            0x2C => self.call(flags.0, modifier, operand),
            0x30 => self.call(flags.1, modifier, operand),
            0x34 => self.call(flags.2, modifier, operand),
            0x38 => self.call(flags.3, modifier, operand),
            0x3C => self.ret(),
            0x40 => {
                self.state.memory_address_reg = (self.state.memory_address_reg + 1) & 0xFF_FFFF;
            }
            0x48 => {
                let value = self.source(operand);
                self.subtract(value, self.shifted_a(modifier));
            }
            0x4C => {
                self.subtract(u32::from(operand), self.shifted_a(modifier));
            }
            0x50 => {
                let value = self.source(operand);
                self.subtract(self.shifted_a(modifier), value);
            }
            0x54 => {
                self.subtract(self.shifted_a(modifier), u32::from(operand));
            }
            0x58 => self.sign_extend(modifier),
            0x60 => {
                let value = self.source(operand);
                self.load(modifier, value);
            }
            0x64 => self.load(modifier, u32::from(operand)),
            0x68 => self.read_ram(modifier, self.state.a),
            0x6C => {
                let address = self.state.data_pointer_reg + u32::from(operand);
                self.read_ram(modifier, address);
            }
            0x70 => self.state.rom_buffer = DATA_ROM[(self.state.a & 0x3FF) as usize],
            0x74 => {
                let index = usize::from(modifier) << 8 | usize::from(operand);
                self.state.rom_buffer = DATA_ROM[index];
            }
            0x7C => self.load_page_byte(modifier, operand),
            0x80 => {
                let value = self.source(operand);
                self.state.a = self.add(self.shifted_a(modifier), value);
            }
            0x84 => self.state.a = self.add(self.shifted_a(modifier), u32::from(operand)),
            0x88 => {
                let value = self.source(operand);
                self.state.a = self.subtract(value, self.shifted_a(modifier));
            }
            0x8C => self.state.a = self.subtract(u32::from(operand), self.shifted_a(modifier)),
            0x90 => {
                let value = self.source(operand);
                self.state.a = self.subtract(self.shifted_a(modifier), value);
            }
            0x94 => self.state.a = self.subtract(self.shifted_a(modifier), u32::from(operand)),
            0x98 => {
                let value = self.source(operand);
                self.multiply(sign_extend_24(value));
            }
            0x9C => self.multiply(i32::from(operand)),
            0xA0..=0xBC => {
                let value = if op & 0x04 == 0 {
                    self.source(operand)
                } else {
                    u32::from(operand)
                };
                let a = self.shifted_a(modifier);
                let result = match op & 0xF8 {
                    0xA0 => !a ^ value,
                    0xA8 => a ^ value,
                    0xB0 => a & value,
                    _ => a | value,
                };
                self.set_a_with_flags(result);
            }
            0xC0..=0xDC => {
                let amount = if op & 0x04 == 0 {
                    self.source(operand)
                } else {
                    u32::from(operand)
                } & 0x1F;
                self.shift(op & 0xF8, amount);
            }
            0xE0 => self.store(modifier, operand),
            0xE8 => self.write_ram(modifier, self.state.a),
            0xEC => {
                let address = self.state.data_pointer_reg + u32::from(operand);
                self.write_ram(modifier, address);
            }
            0xF0 => {
                let reg = usize::from(operand & 0x0F);
                std::mem::swap(&mut self.state.a, &mut self.state.regs[reg]);
            }
            0xFC => self.stop(),
            // nop (0000h), and the opcodes fullsnes lists as unused ("-").
            _ => {}
        }
        self.step(1);
    }

    /// `A` shifted by the `A*1/A*2/A*100h/A*10000h` modifier, as a 32-bit value (Mesen2 keeps
    /// the bits above 24 until the result is masked, which matters for the carry).
    fn shifted_a(&self, modifier: u8) -> u32 {
        self.state.a.wrapping_shl(SHIFT[usize::from(modifier)])
    }

    /// Reads a source operand (fullsnes "Lower Bits <op>"). `$2E`/`$2F` start an external
    /// ROM/RAM read at `ext_ptr` whose byte lands in `ext_dta` after the wait states.
    fn source(&mut self, operand: u8) -> u32 {
        let state = &mut self.state;
        match operand & 0x7F {
            0x00 => state.a,
            0x01 => (state.mult >> 24) as u32 & 0xFF_FFFF,
            0x02 => state.mult as u32 & 0xFF_FFFF,
            0x03 => state.memory_data_reg,
            0x08 => state.rom_buffer,
            0x0C => {
                u32::from(state.ram_buffer[2]) << 16
                    | u32::from(state.ram_buffer[1]) << 8
                    | u32::from(state.ram_buffer[0])
            }
            0x13 => state.memory_address_reg,
            0x1C => state.data_pointer_reg,
            0x20 => u32::from(state.pc),
            0x28 => u32::from(state.p),
            0x2E | 0x2F => {
                self.start_bus_access(operand & 0x7F == 0x2E, true);
                0
            }
            0x50 => 0x00_0000,
            0x51 => 0xFF_FFFF,
            0x52 => 0x00_FF00,
            0x53 => 0xFF_0000,
            0x54 => 0x00_FFFF,
            0x55 => 0xFF_FF00,
            0x56 => 0x80_0000,
            0x57 => 0x7F_FFFF,
            0x58 => 0x00_8000,
            0x59 => 0x00_7FFF,
            0x5A => 0xFF_7FFF,
            0x5B => 0xFF_FF7F,
            0x5C => 0x01_0000,
            0x5D => 0xFE_FFFF,
            0x5E => 0x00_0100,
            0x5F => 0x00_FEFF,
            reg @ 0x60..=0x7F => state.regs[usize::from(reg & 0x0F)],
            _ => 0,
        }
    }

    /// Writes a destination register (the `mov <op>,A` operand encoding).
    fn write_register(&mut self, reg: u8, value: u32) {
        let value = value & 0xFF_FFFF;
        let state = &mut self.state;
        match reg & 0x7F {
            0x01 => state.mult = (state.mult & 0xFF_FFFF) | u64::from(value) << 24,
            0x02 => state.mult = (state.mult & 0xFFFF_FF00_0000) | u64::from(value),
            0x03 => state.memory_data_reg = value,
            0x08 => state.rom_buffer = value,
            0x0C => state.ram_buffer = [value as u8, (value >> 8) as u8, (value >> 16) as u8],
            0x13 => state.memory_address_reg = value,
            0x1C => state.data_pointer_reg = value,
            0x20 => state.pc = value as u8,
            0x28 => state.p = (value & 0x7FFF) as u16,
            0x2E | 0x2F => self.start_bus_access(reg & 0x7F == 0x2E, false),
            reg @ 0x60..=0x7F => state.regs[usize::from(reg & 0x0F)] = value,
            _ => {}
        }
    }

    /// Queues an external access at `ext_ptr`: `$2E` pays the ROM wait states, `$2F` the RAM.
    fn start_bus_access(&mut self, rom: bool, reading: bool) {
        let state = &mut self.state;
        state.bus_enabled = true;
        state.bus_reading = reading;
        state.bus_writing = !reading;
        state.bus_delay_cycles = if rom {
            state.rom_access_delay
        } else {
            state.ram_access_delay
        };
        state.bus_address = state.memory_address_reg;
    }

    fn load(&mut self, destination: u8, value: u32) {
        let state = &mut self.state;
        match destination {
            0 => state.a = value,
            1 => state.memory_data_reg = value,
            2 => state.memory_address_reg = value,
            _ => state.p = (value & 0x7FFF) as u16,
        }
    }

    fn store(&mut self, source: u8, reg: u8) {
        match source {
            0 => self.write_register(reg, self.state.a),
            1 => self.write_register(reg, self.state.memory_data_reg),
            _ => {}
        }
    }

    fn load_page_byte(&mut self, byte: u8, value: u8) {
        let p = self.state.p;
        match byte {
            0 => self.state.p = (p & 0x7F00) | u16::from(value),
            1 => self.state.p = (p & 0x00FF) | u16::from(value & 0x7F) << 8,
            _ => {}
        }
    }

    fn add(&mut self, a: u32, b: u32) -> u32 {
        let result = a.wrapping_add(b);
        let state = &mut self.state;
        state.carry = result > 0xFF_FFFF;
        state.negative = result & 0x80_0000 != 0;
        state.overflow = !(a ^ b) & (a ^ result) & 0x80_0000 != 0;
        state.zero = result & 0xFF_FFFF == 0;
        result & 0xFF_FFFF
    }

    /// `a - b`; carry is set when no borrow occurred (fullsnes "CX4 CPU Misc").
    fn subtract(&mut self, a: u32, b: u32) -> u32 {
        let result = a.wrapping_sub(b) as i32;
        let state = &mut self.state;
        state.carry = result >= 0;
        state.negative = result & 0x80_0000 != 0;
        state.overflow = !(a ^ b) & (a ^ result as u32) & 0x80_0000 != 0;
        state.zero = result == 0;
        result as u32 & 0xFF_FFFF
    }

    /// Signed 24x24 multiply into the 48-bit MH:ML (fullsnes: "result is signed 48bit").
    fn multiply(&mut self, value: i32) {
        let product = i64::from(value) * i64::from(sign_extend_24(self.state.a));
        self.state.mult = product as u64 & 0xFFFF_FFFF_FFFF;
    }

    fn set_a_with_flags(&mut self, value: u32) {
        self.state.a = value & 0xFF_FFFF;
        self.state.zero = self.state.a == 0;
        self.state.negative = self.state.a & 0x80_0000 != 0;
    }

    fn sign_extend(&mut self, mode: u8) {
        let a = self.state.a;
        match mode {
            1 => self.set_a_with_flags(a as u8 as i8 as u32),
            2 => self.set_a_with_flags(a as u16 as i16 as u32),
            _ => {}
        }
    }

    /// shr/sar/ror/shl by `amount`; an amount of 24 or more leaves A unchanged (Mesen2).
    fn shift(&mut self, op: u8, amount: u32) {
        let a = self.state.a;
        let result = if amount >= 24 {
            a
        } else {
            match op {
                0xC0 => a >> amount,
                0xC8 => (sign_extend_24(a) >> amount) as u32,
                0xD0 => a >> amount | a << (24 - amount),
                _ => a << amount,
            }
        };
        self.set_a_with_flags(result);
    }

    /// A data RAM address: RAM is 3 KB, and `C00h-FFFh` folds onto `800h-BFFh` (Mesen2).
    fn ram_address(address: u32) -> usize {
        let address = (address & 0xFFF) as usize;
        if address >= 0xC00 {
            address - 0x400
        } else {
            address
        }
    }

    fn read_ram(&mut self, byte: u8, address: u32) {
        if byte < 3 {
            self.state.ram_buffer[usize::from(byte)] =
                self.state.data_ram[Self::ram_address(address)];
        }
    }

    fn write_ram(&mut self, byte: u8, address: u32) {
        if byte < 3 {
            self.state.data_ram[Self::ram_address(address)] =
                self.state.ram_buffer[usize::from(byte)];
        }
    }

    /// jmp/jz/jc/js/jv: a taken jump costs two more cycles; modifier bit set means "far",
    /// taking the page from the page register.
    fn branch(&mut self, taken: bool, far: u8, target: u8) {
        if taken {
            if far != 0 {
                self.state.pb = self.state.p;
            }
            self.state.pc = target;
            self.step(2);
        }
    }

    fn call(&mut self, taken: bool, far: u8, target: u8) {
        if taken {
            let state = &mut self.state;
            state.stack[usize::from(state.sp)] = u32::from(state.pb) << 8 | u32::from(state.pc);
            state.sp = (state.sp + 1) & 0x07;
            self.branch(true, far, target);
        }
    }

    fn ret(&mut self) {
        let state = &mut self.state;
        state.sp = state.sp.wrapping_sub(1) & 0x07;
        let value = state.stack[usize::from(state.sp)];
        state.pb = (value >> 8) as u16 & 0x7FFF;
        state.pc = value as u8;
        self.step(2);
    }

    /// Skips the next opcode when the flag the modifier selects (overflow, carry, zero,
    /// negative) equals operand bit 0.
    fn skip(&mut self, flag: u8, when_set: u8) {
        let state = &self.state;
        let value = match flag {
            0 => state.overflow,
            1 => state.carry,
            2 => state.zero,
            _ => state.negative,
        };
        if value == (when_set & 0x01 != 0) {
            self.state.pc = self.state.pc.wrapping_add(1);
            if self.state.pc == 0 {
                self.switch_cache_page();
            }
            self.step(1);
        }
    }

    /// Waits for a pending external access to finish ("finish ext_dta").
    fn wait(&mut self) {
        if self.state.bus_enabled {
            self.step(u64::from(self.state.bus_delay_cycles));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::data_rom::DATA_ROM;
    use super::*;
    use crate::snes::ppu::SnesVideoRegion;
    use std::rc::Rc;

    fn fresh() -> Cx4 {
        let mut cx4 = Cx4::new(Rc::new(Vec::new()), SnesVideoRegion::Ntsc);
        cx4.state.stopped = false;
        cx4
    }

    fn with_a(a: u32) -> Cx4 {
        let mut cx4 = fresh();
        cx4.state.a = a;
        cx4
    }

    fn exec_all(cx4: &mut Cx4, opcodes: &[u16]) {
        for &opcode in opcodes {
            cx4.exec(opcode);
        }
    }

    // --- moves and operands ---------------------------------------------------------------

    #[test]
    fn mov_a_immediate_and_register_operands() {
        let mut cx4 = fresh();
        cx4.state.regs[3] = 0x12_3456;
        exec_all(&mut cx4, &[0x6463]); // mov A,63h
        assert_eq!(cx4.state.a, 0x63);
        exec_all(&mut cx4, &[0x6063]); // mov A,R3
        assert_eq!(cx4.state.a, 0x12_3456);
    }

    /// fullsnes "Lower Bits <op>": the sixteen constants 50h-5Fh.
    #[test]
    fn source_operands_50_to_5f_are_the_fullsnes_constants() {
        let constants = [
            0x00_0000, 0xFF_FFFF, 0x00_FF00, 0xFF_0000, 0x00_FFFF, 0xFF_FF00, 0x80_0000, 0x7F_FFFF,
            0x00_8000, 0x00_7FFF, 0xFF_7FFF, 0xFF_FF7F, 0x01_0000, 0xFE_FFFF, 0x00_0100, 0x00_FEFF,
        ];
        for (i, &constant) in constants.iter().enumerate() {
            let mut cx4 = fresh();
            cx4.exec(0x6050 + i as u16);
            assert_eq!(cx4.state.a, constant, "operand {:02X}", 0x50 + i);
        }
    }

    #[test]
    fn mov_selects_a_ext_dta_ext_ptr_or_page_by_bits_9_8() {
        let mut cx4 = fresh();
        cx4.state.regs[0] = 0xAB_CDEF;
        exec_all(&mut cx4, &[0x6160, 0x6260, 0x6360]); // mov ext_dta/ext_ptr/page, R0
        assert_eq!(cx4.state.memory_data_reg, 0xAB_CDEF);
        assert_eq!(cx4.state.memory_address_reg, 0xAB_CDEF);
        assert_eq!(cx4.state.p, 0x4DEF, "page register is 15 bits");
        exec_all(&mut cx4, &[0x6712]); // mov page,12h
        assert_eq!(cx4.state.p, 0x12);
    }

    #[test]
    fn store_writes_a_or_ext_dta_to_a_register() {
        let mut cx4 = with_a(0x11_2233);
        cx4.state.memory_data_reg = 0x44_5566;
        exec_all(&mut cx4, &[0xE065, 0xE166, 0xE01C, 0xE013]); // mov R5,A / R6,ext_dta / ram_ptr,A / ext_ptr,A
        assert_eq!(cx4.state.regs[5], 0x11_2233);
        assert_eq!(cx4.state.regs[6], 0x44_5566);
        assert_eq!(cx4.state.data_pointer_reg, 0x11_2233);
        assert_eq!(cx4.state.memory_address_reg, 0x11_2233);
    }

    #[test]
    fn xchg_swaps_a_with_a_register() {
        let mut cx4 = with_a(0x00_0001);
        cx4.state.regs[9] = 0x00_0002;
        cx4.exec(0xF009);
        assert_eq!((cx4.state.a, cx4.state.regs[9]), (2, 1));
    }

    #[test]
    fn page_loads_set_the_low_or_high_byte() {
        let mut cx4 = fresh();
        exec_all(&mut cx4, &[0x7C34, 0x7DFF]); // mov page.lsb,34h / page.msb,FFh
        assert_eq!(cx4.state.p, 0x7F34);
    }

    #[test]
    fn inc_ext_ptr_increments_the_24_bit_address() {
        let mut cx4 = fresh();
        cx4.state.memory_address_reg = 0xFF_FFFF;
        cx4.exec(0x4000);
        assert_eq!(cx4.state.memory_address_reg, 0);
    }

    // --- arithmetic ------------------------------------------------------------------------

    #[test]
    fn add_carries_out_of_24_bits_and_sets_zero() {
        let mut cx4 = with_a(0xFF_FFFF);
        cx4.exec(0x8401); // add A,A,01h
        assert_eq!(cx4.state.a, 0);
        assert!(cx4.state.carry && cx4.state.zero && !cx4.state.negative);
    }

    #[test]
    fn add_shift_selector_multiplies_a_by_1_2_100h_or_10000h() {
        for (modifier, expected) in [
            (0, 0x12 + 1),
            (1, 0x24 + 1),
            (2, 0x1200 + 1),
            (3, 0x12_0001),
        ] {
            let mut cx4 = with_a(0x12);
            cx4.exec(0x8401 | modifier << 8);
            assert_eq!(cx4.state.a, expected, "modifier {modifier}");
        }
    }

    /// fullsnes "CX4 CPU Misc": carry is cleared on borrow.
    #[test]
    fn sub_clears_carry_on_borrow_and_sets_negative() {
        let mut cx4 = with_a(0);
        cx4.exec(0x9401); // sub A,A,01h
        assert_eq!(cx4.state.a, 0xFF_FFFF);
        assert!(!cx4.state.carry && cx4.state.negative && !cx4.state.zero);

        let mut cx4 = with_a(5);
        cx4.exec(0x9403);
        assert_eq!(cx4.state.a, 2);
        assert!(cx4.state.carry);
    }

    #[test]
    fn reverse_sub_subtracts_a_from_the_operand() {
        let mut cx4 = with_a(3);
        cx4.state.regs[0] = 10;
        cx4.exec(0x8860); // sub A,R0,A
        assert_eq!(cx4.state.a, 7);
        let mut cx4 = with_a(3);
        cx4.exec(0x8C0A); // sub A,0Ah,A
        assert_eq!(cx4.state.a, 7);
        assert!(cx4.state.carry);
    }

    #[test]
    fn cmp_sets_flags_without_changing_a() {
        let mut cx4 = with_a(5);
        cx4.exec(0x5405); // cmp A,05h
        assert_eq!(cx4.state.a, 5);
        assert!(cx4.state.zero && cx4.state.carry);
        cx4.exec(0x5406);
        assert!(!cx4.state.zero && !cx4.state.carry && cx4.state.negative);
        cx4.exec(0x4C06); // cmp 06h,A
        assert!(!cx4.state.zero && cx4.state.carry);
    }

    #[test]
    fn smul_is_a_signed_24_by_24_multiply_into_mh_ml() {
        let mut cx4 = with_a(0xFF_FFFE); // -2
        cx4.exec(0x9C03); // smul MH:ML,A,03h
        exec_all(&mut cx4, &[0x0000, 0x6001]); // nop, mov A,MH
        assert_eq!(cx4.state.a, 0xFF_FFFF);
        cx4.exec(0x6002); // mov A,ML
        assert_eq!(cx4.state.a, 0xFF_FFFA);

        let mut cx4 = with_a(0x7F_FFFF);
        cx4.state.regs[0] = 0x7F_FFFF;
        exec_all(&mut cx4, &[0x9860, 0x0000, 0x6001]);
        assert_eq!(cx4.state.a, 0x3F_FFFF);
        cx4.exec(0x6002);
        assert_eq!(cx4.state.a, 0x00_0001);
    }

    #[test]
    fn sign_extend_from_byte_or_word() {
        let mut cx4 = with_a(0x12_3480);
        cx4.exec(0x5900);
        assert_eq!(cx4.state.a, 0xFF_FF80);
        assert!(cx4.state.negative);
        let mut cx4 = with_a(0x12_8000);
        cx4.exec(0x5A00);
        assert_eq!(cx4.state.a, 0xFF_8000);
        let mut cx4 = with_a(0x12_0034);
        cx4.exec(0x5A00);
        assert_eq!(cx4.state.a, 0x00_0034);
    }

    // --- logic and shifts ------------------------------------------------------------------

    #[test]
    fn logic_operations_set_zero_and_negative() {
        let mut cx4 = with_a(0xF0);
        cx4.exec(0xB40F); // and
        assert_eq!(cx4.state.a, 0);
        assert!(cx4.state.zero);
        let mut cx4 = with_a(0xF0);
        cx4.exec(0xBC0F); // or
        assert_eq!(cx4.state.a, 0xFF);
        let mut cx4 = with_a(0xFF);
        cx4.exec(0xAC0F); // xor
        assert_eq!(cx4.state.a, 0xF0);
        let mut cx4 = with_a(0x00);
        cx4.exec(0xA40F); // xnor
        assert_eq!(cx4.state.a, 0xFF_FFF0);
        assert!(cx4.state.negative);
    }

    #[test]
    fn shifts_and_rotate() {
        let mut cx4 = with_a(0x80_0000);
        cx4.exec(0xC404); // shr A,04h
        assert_eq!(cx4.state.a, 0x08_0000);
        let mut cx4 = with_a(0x80_0000);
        cx4.exec(0xCC04); // sar A,04h
        assert_eq!(cx4.state.a, 0xF8_0000);
        let mut cx4 = with_a(0x00_00F1);
        cx4.exec(0xDC14); // shl A,14h
        assert_eq!(cx4.state.a, 0x10_0000);
        let mut cx4 = with_a(0x00_0001);
        cx4.exec(0xD401); // ror A,01h
        assert_eq!(cx4.state.a, 0x80_0000);
        let mut cx4 = with_a(0x00_0100);
        cx4.state.regs[1] = 8;
        cx4.exec(0xC061); // shr A,R1
        assert_eq!(cx4.state.a, 1);
    }

    #[test]
    fn a_shift_of_24_or_more_leaves_a_unchanged() {
        let mut cx4 = with_a(0x12_3456);
        cx4.exec(0xC418);
        assert_eq!(cx4.state.a, 0x12_3456);
    }

    // --- data RAM and data ROM -------------------------------------------------------------

    #[test]
    fn ram_byte_moves_address_by_ram_ptr_plus_immediate() {
        let mut cx4 = fresh();
        cx4.state.data_ram[0x215] = 0xAB;
        cx4.state.data_pointer_reg = 0x200;
        cx4.exec(0x6D15); // movb ram_dta.mid,cx4ram[ram_ptr+15h]
        cx4.exec(0x600C); // mov A,ram_dta
        assert_eq!(cx4.state.a, 0x00_AB00);
        cx4.exec(0xEC20); // movb cx4ram[ram_ptr+20h],ram_dta.lsb
        cx4.exec(0xED21); // movb cx4ram[ram_ptr+21h],ram_dta.mid
        assert_eq!(cx4.state.data_ram[0x220..0x222], [0x00, 0xAB]);
    }

    #[test]
    fn ram_byte_moves_address_by_a_and_fold_c00_to_800() {
        let mut cx4 = with_a(0xC10);
        cx4.state.data_ram[0x810] = 0x5A;
        cx4.exec(0x6A00); // movb ram_dta.msb,cx4ram[A]
        assert_eq!(cx4.state.ram_buffer[2], 0x5A);
        cx4.state.a = 0x010;
        cx4.exec(0xEA00); // movb cx4ram[A],ram_dta.msb
        assert_eq!(cx4.state.data_ram[0x010], 0x5A);
    }

    #[test]
    fn ram_dta_writes_and_reads_as_a_24_bit_register() {
        let mut cx4 = with_a(0x12_3456);
        exec_all(&mut cx4, &[0xE00C, 0x6450, 0x600C]); // mov ram_dta,A / mov A,50h / mov A,ram_dta
        assert_eq!(cx4.state.a, 0x12_3456);
    }

    #[test]
    fn data_rom_reads_by_a_or_by_ten_bit_immediate() {
        let mut cx4 = with_a(0x405); // index 3FFh-masked: 005h
        cx4.exec(0x7000);
        cx4.exec(0x6008); // mov A,rom_dta
        assert_eq!(cx4.state.a, DATA_ROM[0x005]);
        cx4.exec(0x7640); // entry 240h: sin(45°)
        cx4.exec(0x6008);
        assert_eq!(cx4.state.a, 0xB5_04F3);
    }

    // --- control flow ----------------------------------------------------------------------

    #[test]
    fn jmp_within_the_page_and_far_through_the_page_register() {
        let mut cx4 = fresh();
        cx4.state.pb = 1;
        cx4.state.p = 7;
        cx4.exec(0x0842);
        assert_eq!((cx4.state.pb, cx4.state.pc), (1, 0x42));
        cx4.exec(0x0A10);
        assert_eq!((cx4.state.pb, cx4.state.pc), (7, 0x10));
    }

    #[test]
    fn conditional_jumps_follow_zero_carry_negative_and_overflow() {
        for (opcode, set_flag) in [
            (
                0x0C20_u16,
                (|c: &mut Cx4| c.state.zero = true) as fn(&mut Cx4),
            ),
            (0x1020, |c: &mut Cx4| c.state.carry = true),
            (0x1420, |c: &mut Cx4| c.state.negative = true),
            (0x1820, |c: &mut Cx4| c.state.overflow = true),
        ] {
            let mut cx4 = fresh();
            cx4.state.pc = 5;
            cx4.exec(opcode);
            assert_eq!(cx4.state.pc, 5, "{opcode:04X} not taken");
            set_flag(&mut cx4);
            cx4.exec(opcode);
            assert_eq!(cx4.state.pc, 0x20, "{opcode:04X} taken");
        }
    }

    #[test]
    fn call_and_ret_nest_through_the_eight_level_stack() {
        let mut cx4 = fresh();
        cx4.state.pb = 2;
        cx4.state.pc = 0x11;
        cx4.state.p = 3;
        cx4.exec(0x2A40); // call far 40h
        assert_eq!((cx4.state.pb, cx4.state.pc), (3, 0x40));
        cx4.exec(0x3C00); // ret
        assert_eq!((cx4.state.pb, cx4.state.pc), (2, 0x11));
    }

    #[test]
    fn conditional_call_is_skipped_when_its_flag_is_clear() {
        let mut cx4 = fresh();
        cx4.state.pc = 0x11;
        cx4.exec(0x2C40); // callz 40h
        assert_eq!(cx4.state.pc, 0x11);
        assert_eq!(cx4.state.sp, 0);
    }

    #[test]
    fn skip_passes_over_the_next_opcode_when_the_flag_matches() {
        let mut cx4 = fresh();
        cx4.state.pc = 0x10;
        cx4.state.carry = true;
        cx4.exec(0x2501); // skipc
        assert_eq!(cx4.state.pc, 0x11);
        cx4.exec(0x2500); // skipnc
        assert_eq!(cx4.state.pc, 0x11);
        cx4.state.zero = false;
        cx4.exec(0x2600); // skipnz
        assert_eq!(cx4.state.pc, 0x12);
    }

    #[test]
    fn stop_halts_and_raises_the_snes_irq_unless_disabled() {
        let mut cx4 = fresh();
        cx4.exec(0xFC00);
        assert!(cx4.state.stopped && cx4.state.irq_flag && cx4.irq_line());

        let mut cx4 = fresh();
        cx4.state.irq_disabled = true;
        cx4.exec(0xFC00);
        assert!(cx4.state.stopped && !cx4.irq_line());
    }

    #[test]
    fn every_opcode_costs_at_least_one_cycle_and_taken_jumps_three() {
        let mut cx4 = fresh();
        cx4.exec(0x0000);
        assert_eq!(cx4.state.cycle_count, 1);
        cx4.exec(0x0842);
        assert_eq!(cx4.state.cycle_count, 4);
    }
}
