use super::*;

impl Cpu {
    /// Update Zero and Negative flags based on a value
    pub(super) fn update_zero_and_negative_flags(&mut self, value: u8) {
        // Clear Z and N flags
        self.p &= !(FLAG_ZERO | FLAG_NEGATIVE);

        // Set Zero flag if value is 0
        if value == 0 {
            self.p |= FLAG_ZERO;
        }

        // Set Negative flag if bit 7 is set
        if value & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        }
    }

    /// Add with Carry - ADC operation
    pub(super) fn adc(&mut self, value: u8) {
        let carry = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        let sum = self.a as u16 + value as u16 + carry as u16;

        // Check for carry (result > 255)
        if sum > 0xFF {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        // Check for overflow
        // Overflow occurs when:
        // - Two positive numbers add to a negative result
        // - Two negative numbers add to a positive result
        let result = sum as u8;
        let overflow = (self.a ^ result) & (value ^ result) & 0x80;
        if overflow != 0 {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }

        self.a = result;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Bitwise AND - AND operation
    pub(super) fn and(&mut self, value: u8) {
        self.a &= value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Arithmetic Shift Left - ASL operation
    pub(super) fn asl(&mut self, value: u8) -> u8 {
        let carry = if value & 0x80 != 0 { FLAG_CARRY } else { 0 };
        let result = value << 1;
        self.p = (self.p & !FLAG_CARRY) | carry;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Bit Test - BIT operation
    pub(super) fn bit(&mut self, value: u8) {
        // Test bits: Zero flag is set based on A & value
        let result = self.a & value;
        if result == 0 {
            self.p |= FLAG_ZERO;
        } else {
            self.p &= !FLAG_ZERO;
        }

        // Copy bit 7 of value to Negative flag
        if value & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        } else {
            self.p &= !FLAG_NEGATIVE;
        }

        // Copy bit 6 of value to Overflow flag
        if value & 0x40 != 0 {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }
    }

    /// Compare operation - sets flags based on register - value
    fn compare(&mut self, register_value: u8, value: u8) {
        let result = register_value.wrapping_sub(value);

        // Set Carry flag if register >= value
        if register_value >= value {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        // Set Zero flag if register == value
        if register_value == value {
            self.p |= FLAG_ZERO;
        } else {
            self.p &= !FLAG_ZERO;
        }

        // Set Negative flag based on bit 7 of result
        if result & 0x80 != 0 {
            self.p |= FLAG_NEGATIVE;
        } else {
            self.p &= !FLAG_NEGATIVE;
        }
    }

    /// Compare - CMP operation
    pub(super) fn cmp(&mut self, value: u8) {
        self.compare(self.a, value);
    }

    /// Compare X Register - CPX operation
    pub(super) fn cpx(&mut self, value: u8) {
        self.compare(self.x, value);
    }

    /// Compare Y Register - CPY operation
    pub(super) fn cpy(&mut self, value: u8) {
        self.compare(self.y, value);
    }

    /// Decrement - DEC operation
    pub(super) fn dec(&mut self, value: u8) -> u8 {
        let result = value.wrapping_sub(1);
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Decrement and Compare - DCP undocumented operation
    pub(super) fn dcp(&mut self, addr: u16) {
        let value = self.read(addr);
        self.dummy_write(addr, value);
        // Real operation and write
        let result = self.dec(value);
        self.write(addr, result, false);
        self.cmp(result);
    }

    /// Load Accumulator and X - LAR undocumented operation
    /// Also known as LAS. ANDs memory with stack pointer, stores result in A, X, and SP
    pub(super) fn lar(&mut self, value: u8) {
        let result = self.sp & value;
        self.a = result;
        self.x = result;
        self.sp = result;
        self.update_zero_and_negative_flags(result);
    }

    /// AXS - undocumented operation
    /// Also known as SBX. Performs (A & X) - value -> X with carry flag behavior
    pub(super) fn axs(&mut self, value: u8) {
        let and_result = self.a & self.x;
        let (result, borrow) = and_result.overflowing_sub(value);
        self.x = result;
        // Set carry flag if no borrow occurred (like CMP/CPX/CPY)
        self.p = (self.p & !FLAG_CARRY) | if !borrow { FLAG_CARRY } else { 0 };
        self.update_zero_and_negative_flags(self.x);
    }

    /// ISB - undocumented operation
    /// Also known as ISC. Increments memory then performs SBC
    pub(super) fn isb(&mut self, addr: u16) {
        let value = self.read(addr);
        self.dummy_write(addr, value);
        // Increment the value
        let result = value.wrapping_add(1);
        // Write back
        self.write(addr, result, false);
        // Perform SBC with the incremented value
        self.sbc(result);
    }

    /// Exclusive OR - EOR operation
    pub(super) fn eor(&mut self, value: u8) {
        self.a ^= value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Increment - INC operation
    pub(super) fn inc(&mut self, value: u8) -> u8 {
        let result = value.wrapping_add(1);
        self.update_zero_and_negative_flags(result);
        result
    }

    /// RLA - Undocumented opcode: Rotate left memory then AND with accumulator
    pub(super) fn rla(&mut self, addr: u16) {
        let value = self.read(addr);
        // Dummy write
        self.dummy_write(addr, value);
        // Real operation and write
        let rotated = self.rol(value);
        self.write(addr, rotated, false);
        self.a &= rotated;
        self.update_zero_and_negative_flags(self.a);
    }

    /// RRA - Undocumented opcode: Rotate right memory then ADC with accumulator
    pub(super) fn rra(&mut self, addr: u16) {
        let value = self.read(addr);
        // Dummy write
        self.dummy_write(addr, value);
        // Real operation and write
        let rotated = self.ror(value);
        self.write(addr, rotated, false);
        self.adc(rotated);
    }

    /// SLO - Undocumented opcode: Shift left memory then ORA with accumulator
    pub(super) fn slo(&mut self, addr: u16) {
        let value = self.read(addr);
        // Dummy write
        self.dummy_write(addr, value);
        // Real operation and write
        let shifted = self.asl(value);
        self.write(addr, shifted, false);
        self.ora(shifted);
    }

    /// SRE - Undocumented opcode: Shift right memory then EOR with accumulator
    pub(super) fn sre(&mut self, addr: u16) {
        let value = self.read(addr);
        // Dummy write
        self.dummy_write(addr, value);
        // Real operation and write
        let shifted = self.lsr(value);
        self.write(addr, shifted, false);
        self.eor(shifted);
    }

    /// Load Accumulator - LDA operation
    pub(super) fn lda(&mut self, value: u8) {
        self.a = value;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Load X Register - LDX operation
    pub(super) fn ldx(&mut self, value: u8) {
        self.x = value;
        self.update_zero_and_negative_flags(self.x);
    }

    /// Load Y Register - LDY operation
    pub(super) fn ldy(&mut self, value: u8) {
        self.y = value;
        self.update_zero_and_negative_flags(self.y);
    }

    /// Logical Shift Right - LSR operation
    pub(super) fn lsr(&mut self, value: u8) -> u8 {
        // Bit 0 goes into carry flag
        if value & 0b00000001 != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        let result = value >> 1;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Logical Inclusive OR - ORA operation
    pub(super) fn ora(&mut self, value: u8) {
        self.set_a(self.a | value);
    }

    /// Decrement X Register - DEX operation
    pub(super) fn dex(&mut self) {
        self.x = self.x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.x);
    }

    /// Decrement Y Register - DEY operation
    pub(super) fn dey(&mut self) {
        self.y = self.y.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.y);
    }

    /// Increment Y Register - INY operation
    pub(super) fn iny(&mut self) {
        self.y = self.y.wrapping_add(1);
        self.update_zero_and_negative_flags(self.y);
    }

    /// Increment X Register - INX operation
    pub(super) fn inx(&mut self) {
        self.x = self.x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.x);
    }

    /// Transfer Accumulator to X - TAX operation
    pub(super) fn tax(&mut self) {
        self.x = self.a;
        self.update_zero_and_negative_flags(self.x);
    }

    /// Transfer Accumulator to Y - TAY operation
    pub(super) fn tay(&mut self) {
        self.y = self.a;
        self.update_zero_and_negative_flags(self.y);
    }

    /// Transfer X to Accumulator - TXA operation
    pub(super) fn txa(&mut self) {
        self.a = self.x;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Transfer Y to Accumulator - TYA operation
    pub(super) fn tya(&mut self) {
        self.a = self.y;
        self.update_zero_and_negative_flags(self.a);
    }

    /// Rotate Left - ROL operation
    pub(super) fn rol(&mut self, value: u8) -> u8 {
        let old_carry = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        // Bit 7 goes into carry flag
        if value & 0b10000000 != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        let result = (value << 1) | old_carry;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Rotate Right - ROR operation
    pub(super) fn ror(&mut self, value: u8) -> u8 {
        let old_carry = if self.p & FLAG_CARRY != 0 {
            0b10000000
        } else {
            0
        };
        // Bit 0 goes into carry flag
        if value & 0b00000001 != 0 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }
        let result = (value >> 1) | old_carry;
        self.update_zero_and_negative_flags(result);
        result
    }

    /// Subtract with Carry - SBC operation
    pub(super) fn sbc(&mut self, value: u8) {
        // SBC is equivalent to ADC with inverted value
        // A - M - (1 - C) = A + ~M + C
        let carry_in = if self.p & FLAG_CARRY != 0 { 1 } else { 0 };
        let inverted_value = !value;
        let result = self.a as u16 + inverted_value as u16 + carry_in;

        // Set carry flag if no borrow occurred (result >= 0x100)
        if result >= 0x100 {
            self.p |= FLAG_CARRY;
        } else {
            self.p &= !FLAG_CARRY;
        }

        // Set overflow flag if signed overflow occurred
        // Overflow occurs when subtracting different signs yields wrong sign
        // Same logic as ADC but with inverted value
        let a_sign = self.a & 0x80;
        let m_sign = inverted_value & 0x80;
        let result_sign = (result as u8) & 0x80;
        if a_sign == m_sign && a_sign != result_sign {
            self.p |= FLAG_OVERFLOW;
        } else {
            self.p &= !FLAG_OVERFLOW;
        }

        self.a = result as u8;
        self.update_zero_and_negative_flags(self.a);
    }
}
