//! GBA affine background register file (BG2 and BG3).
//!
//! Stores the per-background affine parameters (`PA`, `PB`, `PC`, `PD`)
//! and reference points (`X`, `Y`) that drive rotation/scaling for BG2
//! and BG3 in tile modes 1 and 2 (and bitmap modes 3–5 for BG2).
//!
//! Per GBATek "LCD I/O BG Rotation/Scaling":
//!
//! * `BG2PA`/`BG2PB`/`BG2PC`/`BG2PD` (and the BG3 equivalents) are
//!   16-bit *signed* values in **8.8** fixed-point — i.e. `0x0100`
//!   represents `1.0`.
//! * `BG2X` / `BG2Y` (and BG3) are 32-bit *signed* values in **19.8**
//!   fixed-point but only **28 bits are used** — bit 27 is the sign bit
//!   and bits 28..31 are unused. The hardware sign-extends bit 27 into
//!   the upper bits when the register is read internally.
//! * All eight registers are **write-only**; reads of the I/O addresses
//!   return the bus open-bus / backing-store value, not the affine
//!   state. The bus dispatcher therefore routes only writes to this
//!   module.
//!
//! ## Mid-frame write semantics (GBATek)
//!
//! Per GBATek: *"If software writes to BG2X/BG2Y outside V-Blank, the value
//! is immediately applied to the internal register for the current scanline."*
//!
//! The [`BgAffine::write_x_low`], [`BgAffine::write_x_high`],
//! [`BgAffine::write_y_low`], and [`BgAffine::write_y_high`] methods
//! therefore update **both** the register (`x`/`y`) and the internal
//! accumulator (`internal_x`/`internal_y`) atomically.  After the write,
//! PB/PD accumulation on subsequent scanlines starts from the newly written
//! value.  At the next V-Blank, [`BgAffine::latch_reference_points`]
//! re-initialises the accumulators from the (now updated) register values,
//! which preserves the mid-frame write into the following frame.
//!
//! References:
//! * GBATek "LCD I/O BG Rotation/Scaling":
//!   <https://problemkaputt.de/gbatek.htm#lcdiobgrotationscaling>
//! * Tonc "Affine matrices":
//!   <https://www.coranac.com/tonc/text/affine.htm>

/// Register file for one affine background (BG2 or BG3).
///
/// Per GBATek "LCD I/O BG Rotation/Scaling": at power-on the hardware
/// initialises `PA` and `PD` to `0x0100` (1.0 in 8.8 fixed-point, giving an
/// identity transform). The emulator must reflect this so that code which
/// relies on the default identity mapping works correctly out of the box.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BgAffine {
    /// Affine parameter A — signed 8.8 fixed-point.
    pub pa: i16,
    /// Affine parameter B — signed 8.8 fixed-point.
    pub pb: i16,
    /// Affine parameter C — signed 8.8 fixed-point.
    pub pc: i16,
    /// Affine parameter D — signed 8.8 fixed-point.
    pub pd: i16,
    /// Reference point X — signed 19.8 fixed-point, 28 bits used.
    /// Stored sign-extended to a full `i32`.
    pub x: i32,
    /// Reference point Y — signed 19.8 fixed-point, 28 bits used.
    /// Stored sign-extended to a full `i32`.
    pub y: i32,
    /// Internal reference point X — latched from `x` at VBlank start,
    /// then incremented by `pb` after each visible scanline.
    /// Used by the renderer for the current scanline's texture origin.
    pub internal_x: i32,
    /// Internal reference point Y — latched from `y` at VBlank start,
    /// then incremented by `pd` after each visible scanline.
    pub internal_y: i32,
}

impl Default for BgAffine {
    /// Returns the power-on default state: identity transform (`PA = PD = 0x0100`,
    /// all other fields zero), matching real GBA hardware.
    fn default() -> Self {
        Self {
            pa: 0x0100,
            pb: 0,
            pc: 0,
            pd: 0x0100,
            x: 0,
            y: 0,
            internal_x: 0,
            internal_y: 0,
        }
    }
}

impl BgAffine {
    /// Latch internal reference points from the programmed register values.
    /// Called at the start of VBlank (scanline entering 160).
    pub fn latch_reference_points(&mut self) {
        self.internal_x = self.x;
        self.internal_y = self.y;
    }

    /// Increment internal reference points by PB/PD.
    /// Called after each visible scanline is rendered.
    pub fn increment_reference_points(&mut self) {
        self.internal_x = self.internal_x.wrapping_add(self.pb as i32);
        self.internal_y = self.internal_y.wrapping_add(self.pd as i32);
    }

    /// Write the low halfword (bits 0..15) of the X reference point.
    /// Preserves the existing high halfword and re-applies sign-extension
    /// from bit 27. Also immediately updates the internal reference point.
    pub fn write_x_low(&mut self, lo: u16) {
        self.x = sign_extend_28(((self.x as u32) & 0xFFFF_0000) | (lo as u32));
        self.internal_x = self.x;
    }

    /// Write the high halfword (bits 16..31) of the X reference point.
    /// Only bits 0..11 of `hi` are meaningful (the full register is
    /// 28 bits); bits 28..31 are discarded and bit 27 is sign-extended.
    /// Also immediately updates the internal reference point.
    pub fn write_x_high(&mut self, hi: u16) {
        self.x = sign_extend_28(((hi as u32) << 16) | ((self.x as u32) & 0x0000_FFFF));
        self.internal_x = self.x;
    }

    /// Write the low halfword (bits 0..15) of the Y reference point.
    /// Also immediately updates the internal reference point.
    pub fn write_y_low(&mut self, lo: u16) {
        self.y = sign_extend_28(((self.y as u32) & 0xFFFF_0000) | (lo as u32));
        self.internal_y = self.y;
    }

    /// Write the high halfword (bits 16..31) of the Y reference point.
    /// Also immediately updates the internal reference point.
    pub fn write_y_high(&mut self, hi: u16) {
        self.y = sign_extend_28(((hi as u32) << 16) | ((self.y as u32) & 0x0000_FFFF));
        self.internal_y = self.y;
    }
}

/// Sign-extend a 28-bit value (held in the low 28 bits of a `u32`) into
/// a full `i32`. Bit 27 is the sign bit; bits 28..31 of the input are
/// discarded.
fn sign_extend_28(value: u32) -> i32 {
    // Mask to 28 bits, then arithmetic shift left/right by 4 to copy
    // bit 27 into bits 28..31.
    let masked = (value & 0x0FFF_FFFF) as i32;
    (masked << 4) >> 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_hardware_power_on_values() {
        // GBATek: BG2PA and BG2PD power on to 0x0100 (identity), all other
        // fields are 0. This mirrors real GBA hardware behaviour.
        let a = BgAffine::default();
        assert_eq!(a.pa, 0x0100, "PA should default to 0x0100 (identity)");
        assert_eq!(a.pb, 0);
        assert_eq!(a.pc, 0);
        assert_eq!(a.pd, 0x0100, "PD should default to 0x0100 (identity)");
        assert_eq!(a.x, 0);
        assert_eq!(a.y, 0);
    }

    #[test]
    fn pa_pb_pc_pd_are_signed_8_8_fixed_point() {
        // Identity scale (1.0) fits exactly in 8.8.
        let a = BgAffine {
            pa: 0x0100_u16 as i16,
            pd: 0x0100_u16 as i16,
            // Top-bit-set values must be interpreted as negative (e.g.
            // -1.0 is 0xFF00 in 8.8 — i.e. i16 -256).
            pb: 0xFF00_u16 as i16,
            ..Default::default()
        };
        assert_eq!(a.pa, 0x0100);
        assert_eq!(a.pd, 0x0100);
        assert_eq!(a.pb, -256);
    }

    #[test]
    fn x_low_then_high_preserves_low_halfword() {
        let mut a = BgAffine::default();
        a.write_x_low(0x1234);
        a.write_x_high(0x0005);
        assert_eq!(a.x, 0x0005_1234);
    }

    #[test]
    fn x_high_then_low_preserves_high_halfword() {
        let mut a = BgAffine::default();
        a.write_x_high(0x000A);
        a.write_x_low(0xBEEF);
        assert_eq!(a.x, 0x000A_BEEF);
    }

    #[test]
    fn x_high_discards_bits_above_27() {
        // Bits 28..31 of the high halfword are unused on hardware. A
        // value in those bits must not affect the stored reference
        // point.
        let mut a = BgAffine::default();
        a.write_x_low(0x0001);
        a.write_x_high(0xF000); // bits 28..31 set
        // Bit 27 is also clear → result is positive, top nibble masked.
        assert_eq!(a.x, 0x0000_0001);
    }

    #[test]
    fn x_sign_extends_bit_27() {
        // Set bit 27 of the high halfword: hi[11] = 1 ⇒ value should
        // sign-extend to a negative i32.
        let mut a = BgAffine::default();
        a.write_x_low(0x0000);
        a.write_x_high(0x0800); // bit 27 of full 32-bit value
        // 28-bit value = 0x0800_0000, sign bit set ⇒ -0x0800_0000 in i32.
        assert_eq!(a.x, -0x0800_0000);
    }

    #[test]
    fn y_independent_of_x() {
        let mut a = BgAffine::default();
        a.write_x_low(0x1111);
        a.write_x_high(0x0002);
        a.write_y_low(0xABCD);
        a.write_y_high(0x0003);
        assert_eq!(a.x, 0x0002_1111);
        assert_eq!(a.y, 0x0003_ABCD);
    }

    #[test]
    fn y_sign_extends_bit_27() {
        let mut a = BgAffine::default();
        a.write_y_high(0x0FFF);
        a.write_y_low(0xFFFF);
        // 28-bit value 0x0FFF_FFFF, bit 27 set ⇒ -1.
        assert_eq!(a.y, -1);
    }
}
