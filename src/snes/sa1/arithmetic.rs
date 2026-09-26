//! SA-1 arithmetic unit: `$2250` MCNT, `$2251-$2254` MA/MB, `$2306-$230A` MR and `$230B` OF.
//!
//! Specification: fullsnes "SNES Cart SA-1 Arithmetic Maths". All of these registers are on the
//! SA-1 side only (fullsnes I/O map "Side" column), so only [`super::Sa1Bus`] reaches them.
//!
//! - MCNT bits 0-1 select 0=multiply, 1=divide, 2=multiply-sum; mode 3 is "Reserved" in fullsnes
//!   and is treated as multiply-sum here because both Mesen2 (`Sa1::ProcessMathOp`, sum mode is
//!   `MathOp & Sum`) and ares (`io.acm = data.bit(1)`) key sum mode on bit 1 alone. Writing bit
//!   1 = 1 clears the sum.
//! - Writing `$2254` starts the operation. MB is destroyed by every operation; MA only by divide.
//! - Multiply: signed 16 x signed 16 -> 32-bit result (bits 32-39 read 0).
//! - Multiply-sum: the signed product is added to a 40-bit accumulator. fullsnes only says OF is
//!   "set on 40bit multiply/addition overflows"; Mesen2 and ares agree it is bit 40 of the
//!   unmasked 64-bit sum, recomputed by every sum step and untouched by multiply/divide, which is
//!   what [`Sa1Arithmetic::execute`] does (so a sum stepping below zero sets it too).
//! - Divide: signed dividend / unsigned divisor -> 16-bit quotient in bits 0-15 and an UNSIGNED
//!   remainder in bits 16-31; division by zero gives 0 and 0. For a negative dividend fullsnes
//!   does not say how the unsigned remainder is formed. ares uses the Euclidean remainder
//!   (`(dividend % divisor + divisor) % divisor`); Mesen2 adds the divisor to the C remainder
//!   whenever the dividend is negative, which makes an exact negative division return
//!   remainder == divisor (-4 / 2 -> quotient -3, remainder 2). That is not a remainder at all,
//!   so ares's Euclidean form is used here; the two agree on every inexact division.
//! - Results are available immediately. Real hardware takes 5 (multiply/divide) or 6 (sum)
//!   SA-1 cycles, which Mesen2 models and ares does not; no known software reads MR that early.

/// MCNT bit 0: divide (when bit 1 is clear).
const MCNT_DIVIDE: u8 = 0x01;
/// MCNT bit 1: multiply-sum; writing it as 1 also clears the sum.
const MCNT_SUM: u8 = 0x02;
/// MR is 40 bits wide.
const MR_MASK: u64 = (1 << 40) - 1;

/// The SA-1's multiply / divide / cumulative-sum unit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sa1Arithmetic {
    /// `$2250` MCNT bits 0-1 (fullsnes reset value `$00`: multiply).
    control: u8,
    /// `$2251/$2252` MA: multiplicand or dividend, always signed.
    ma: u16,
    /// `$2253/$2254` MB: signed multiplier or unsigned divisor.
    mb: u16,
    /// `$2306-$230A` MR: the 40-bit result.
    mr: u64,
    /// `$230B` OF bit 7.
    overflow: bool,
}

impl Sa1Arithmetic {
    pub fn new() -> Self {
        Self::default()
    }

    /// Dispatches an SA-1-side write to `$2250-$2254`; other ports are ignored.
    pub fn write(&mut self, port: u16, value: u8) {
        match port {
            0x2250 => {
                self.control = value & (MCNT_DIVIDE | MCNT_SUM);
                if value & MCNT_SUM != 0 {
                    self.mr = 0;
                }
            }
            0x2251 => self.ma = (self.ma & 0xFF00) | u16::from(value),
            0x2252 => self.ma = (self.ma & 0x00FF) | (u16::from(value) << 8),
            0x2253 => self.mb = (self.mb & 0xFF00) | u16::from(value),
            0x2254 => {
                self.mb = (self.mb & 0x00FF) | (u16::from(value) << 8);
                self.execute();
            }
            _ => {}
        }
    }

    /// `$2306-$230A` MR bytes and `$230B` OF (bits 0-6 read 0, as in Mesen2 and ares); `None`
    /// for any other port.
    pub fn read(&self, port: u16) -> Option<u8> {
        match port {
            0x2306..=0x230A => Some((self.mr >> (8 * (port - 0x2306))) as u8),
            0x230B => Some(u8::from(self.overflow) << 7),
            _ => None,
        }
    }

    fn execute(&mut self) {
        let product = i64::from(self.ma as i16) * i64::from(self.mb as i16);
        if self.control & MCNT_SUM != 0 {
            let sum = self.mr.wrapping_add(product as u64);
            self.overflow = sum & (1 << 40) != 0;
            self.mr = sum & MR_MASK;
        } else if self.control & MCNT_DIVIDE == 0 {
            self.mr = u64::from(product as u32);
        } else {
            self.mr = self.divide();
            self.ma = 0;
        }
        self.mb = 0;
    }

    fn divide(&self) -> u64 {
        if self.mb == 0 {
            return 0;
        }
        let dividend = i32::from(self.ma as i16);
        let divisor = i32::from(self.mb);
        let remainder = dividend.rem_euclid(divisor);
        let quotient = (dividend - remainder) / divisor;
        (u64::from(remainder as u16) << 16) | u64::from(quotient as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MULTIPLY: u8 = 0x00;
    const DIVIDE: u8 = 0x01;
    const SUM: u8 = 0x02;

    fn write_word(unit: &mut Sa1Arithmetic, low_port: u16, value: u16) {
        unit.write(low_port, value as u8);
        unit.write(low_port + 1, (value >> 8) as u8);
    }

    /// Runs one operation: MCNT, then MA, then MB (the `$2254` write starts it).
    fn run(unit: &mut Sa1Arithmetic, mode: u8, ma: u16, mb: u16) {
        unit.write(0x2250, mode);
        write_word(unit, 0x2251, ma);
        write_word(unit, 0x2253, mb);
    }

    fn result(unit: &Sa1Arithmetic) -> u64 {
        (0..5).fold(0u64, |acc, i| {
            acc | (u64::from(unit.read(0x2306 + i).expect("MR is readable")) << (8 * i))
        })
    }

    fn quotient_and_remainder(unit: &Sa1Arithmetic) -> (u16, u16) {
        let mr = result(unit);
        (mr as u16, (mr >> 16) as u16)
    }

    #[test]
    fn multiply_is_signed_16_by_16() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, MULTIPLY, 0xFFFE, 0x0003); // -2 * 3
        assert_eq!(result(&unit), 0xFFFF_FFFA, "32-bit signed product, bits 32-39 zero");

        run(&mut unit, MULTIPLY, 0x7FFF, 0x7FFF);
        assert_eq!(result(&unit), 0x3FFF_0001);

        run(&mut unit, MULTIPLY, 0x8000, 0x8000); // -32768 * -32768
        assert_eq!(result(&unit), 0x4000_0000);
    }

    #[test]
    fn multiply_keeps_ma_and_clears_mb() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, MULTIPLY, 0x0010, 0x0003);
        assert_eq!(result(&unit), 0x30);
        // fullsnes: MA "is kept intact after multiplication", MB "gets destroyed". So writing only
        // MB's high byte multiplies the kept MA by $0200 (MB's low byte is now 0).
        unit.write(0x2254, 0x02);
        assert_eq!(result(&unit), 0x2000);
    }

    #[test]
    fn divide_negative_dividend_gives_unsigned_remainder() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, DIVIDE, (-7i16) as u16, 2);
        assert_eq!(quotient_and_remainder(&unit), ((-4i16) as u16, 1));

        run(&mut unit, DIVIDE, 100, 7);
        assert_eq!(quotient_and_remainder(&unit), (14, 2));
    }

    #[test]
    fn divide_by_an_unsigned_divisor_above_0x7fff() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, DIVIDE, 0x7FFF, 0x8000); // 32767 / 32768: the divisor is unsigned
        assert_eq!(quotient_and_remainder(&unit), (0, 0x7FFF));
    }

    #[test]
    fn divide_exact_negative_gives_zero_remainder() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, DIVIDE, (-4i16) as u16, 2);
        assert_eq!(quotient_and_remainder(&unit), ((-2i16) as u16, 0));
    }

    #[test]
    fn divide_by_zero_gives_zero_and_zero() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, MULTIPLY, 0x1234, 0x5678); // leave a non-zero result behind
        run(&mut unit, DIVIDE, 0x1234, 0);
        assert_eq!(quotient_and_remainder(&unit), (0, 0));
    }

    #[test]
    fn divide_clears_ma() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, DIVIDE, 100, 7);
        // Switch to multiply and supply only MB: MA was destroyed by the division.
        unit.write(0x2250, MULTIPLY);
        write_word(&mut unit, 0x2253, 5);
        assert_eq!(result(&unit), 0);
    }

    #[test]
    fn sum_accumulates_signed_products_over_40_bits() {
        let mut unit = Sa1Arithmetic::new();
        unit.write(0x2250, SUM);
        for _ in 0..4 {
            write_word(&mut unit, 0x2251, 0x7FFF);
            write_word(&mut unit, 0x2253, 0x7FFF);
        }
        assert_eq!(result(&unit), 4 * 0x3FFF_0001, "sum exceeds 32 bits into MR bits 32-39");

        write_word(&mut unit, 0x2251, 0xFFFF); // -1
        write_word(&mut unit, 0x2253, 0x0001);
        assert_eq!(result(&unit), 4 * 0x3FFF_0001 - 1);
    }

    #[test]
    fn writing_mcnt_bit1_clears_the_sum() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, SUM, 3, 4);
        assert_eq!(result(&unit), 12);
        run(&mut unit, SUM, 5, 6);
        assert_eq!(result(&unit), 30, "a sum run without re-writing MCNT would give 42");
    }

    #[test]
    fn sum_overflow_sets_of_bit7_and_the_next_sum_recomputes_it() {
        let mut unit = Sa1Arithmetic::new();
        assert_eq!(unit.read(0x230B), Some(0x00));
        run(&mut unit, SUM, 0xFFFF, 0x0001); // 0 + (-1) wraps below zero, carrying out of bit 39
        assert_eq!(result(&unit), 0xFF_FFFF_FFFF);
        assert_eq!(unit.read(0x230B), Some(0x80));
        // Adding +1 back brings the 40-bit sum to 0 with no carry out: OF clears.
        write_word(&mut unit, 0x2251, 0x0001);
        write_word(&mut unit, 0x2253, 0x0001);
        assert_eq!(result(&unit), 0);
        assert_eq!(unit.read(0x230B), Some(0x80), "0xFF_FFFF_FFFF + 1 carries out of bit 39");
        write_word(&mut unit, 0x2251, 0x0001);
        write_word(&mut unit, 0x2253, 0x0001);
        assert_eq!(unit.read(0x230B), Some(0x00));
    }

    #[test]
    fn mode_3_acts_as_sum() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, 0x03, 3, 4);
        write_word(&mut unit, 0x2251, 3);
        write_word(&mut unit, 0x2253, 4);
        assert_eq!(result(&unit), 24);
    }

    #[test]
    fn multiply_and_divide_leave_the_overflow_flag_alone() {
        let mut unit = Sa1Arithmetic::new();
        run(&mut unit, SUM, 0xFFFF, 0x0001);
        assert_eq!(unit.read(0x230B), Some(0x80));
        run(&mut unit, MULTIPLY, 2, 3);
        run(&mut unit, DIVIDE, 6, 3);
        assert_eq!(unit.read(0x230B), Some(0x80));
    }

    #[test]
    fn only_the_result_and_overflow_ports_are_readable() {
        let unit = Sa1Arithmetic::new();
        assert!(unit.read(0x2305).is_none());
        assert!(unit.read(0x230C).is_none());
        assert!(unit.read(0x230B).is_some());
    }
}
