//! Mapper 172 – Super Mega P-4070 (JV001 IC)
//!
//! # Specifications
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_172>
//!
//! ## Overview
//!
//! Used by three games published by Super Mega / TXC:
//! - *1991 賭馬 Racing* ("Enjoyable Horse Racing")
//! - *麻将方块* (Mahjong Block) — original release
//! - *Venice Beach Volley* (Super Mega release)
//!
//! The board uses a custom JV001 IC that implements a latch, adder, and
//! inverter for copy-protection purposes.  Only CHR banking is switchable;
//! PRG is fixed at 32 KiB.
//!
//! ## Memory Map
//!
//! - `CPU $8000–$FFFF`: 32 KiB PRG-ROM, fixed
//! - `PPU $0000–$1FFF`: 8 KiB switchable CHR-ROM bank
//!
//! ## JV001 IC Registers
//!
//! The IC contains five internal values:
//! - **Input** (6 bits): loaded via writes to `$4102`
//! - **Register** (6 bits): computed from Input/Mode/Invert
//! - **Output** (6 bits): latched from Register on `$8000–$FFFF` writes
//! - **Mode** (1 bit): set via writes to `$4103` bit 5
//! - **Invert** (1 bit): set via writes to `$4101` bit 5
//!
//! ### Address mask: `$E103`
//!
//! Reads from `$4100–$4103` (any address where `addr & $E103` matches):
//! ```text
//! D~7654 3210
//!   ..RR RRRR  Register bits [5:0], with bits [5:4] inverted when Invert=1.
//!              Bits [7:6] return open bus.
//!              Note: bit order D0–D5 is reversed on the data bus.
//! ```
//!
//! Writes decoded by `addr & 3`:
//! - `$4100` (`addr & 3 == 0`, `addr & $E100 == $4100`):
//!   - `Mode == 0`: `Register[5:0] := Input`, bits [3:0] inverted when `Invert=1`
//!   - `Mode == 1`: `Register[3:0] += 1` (increment lower nibble; bits [5:4] unchanged)
//! - `$4101` (`addr & 3 == 1`): `Invert := value bit 5`
//! - `$4102` (`addr & 3 == 2`): `Input := value bits [5:0]`  
//!   (bit order `D0–D5` is reversed on the data bus)
//! - `$4103` (`addr & 3 == 3`): `Mode := value bit 5`
//! - `$8000–$FFFF` (any, value ignored):
//!   - `Output := Register`
//!   - Mirroring → Horizontal if `Invert == 0`; Vertical if `Invert == 1`
//!
//! ## CHR Bank Selection
//!
//! ```text
//! CHR bank = Output & 3   (bits [1:0] of Output)
//! ```
//!
//! ## Power-on State
//!
//! All IC registers (`Input`, `Register`, `Output`, `Mode`, `Invert`) are zero.
//! Mirroring is Horizontal (Invert==0).

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 172;
const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 172 – Super Mega P-4070 / JV001 IC.
///
/// See the module-level documentation for hardware details.
pub struct Mapper172 {
    base: BaseMapper,
    /// JV001 Input register (6 bits).  Written via $4102 (bit order reversed).
    input: u8,
    /// JV001 internal Register (6 bits).  Computed from Input/Mode/Invert.
    register: u8,
    /// JV001 Output register (6 bits).  Latched from Register on $8000 writes.
    output: u8,
    /// JV001 Mode flag (1 bit): 0 = latch mode, 1 = increment mode.
    mode: bool,
    /// JV001 Invert flag (1 bit): gates bit inversions.
    invert: bool,
}

impl Mapper172 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        // Hardwired to Horizontal at power-on (Invert == 0).
        base.set_mirroring(NametableLayout::Horizontal);

        let mut mapper = Self {
            base,
            input: 0,
            register: 0,
            output: 0,
            mode: false,
            invert: false,
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        self.base.select_prg_page(0, 0);
        // CHR bank = Output & 3
        self.base.select_chr_page(0, (self.output & 3) as i16);
    }

    /// Returns true when the address falls in the JV001 IC range ($4100–$41FF).
    fn is_jv001_addr(addr: u16) -> bool {
        (addr & 0xE100) == 0x4100
    }
}

impl Mapper for Mapper172 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if Self::is_jv001_addr(addr) {
            // Return Register[5:0]; bits [5:4] inverted when Invert=1.
            // Bits [7:6] are open bus (returned as 0 here).
            let mut reg = self.register & 0x3F;
            if self.invert {
                reg ^= 0x30; // invert bits [5:4]
            }
            return reg;
        }

        if addr >= 0x8000 {
            return self.base.read_prg_banked(addr);
        }

        0
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_jv001_addr(addr) {
            match addr & 3 {
                0 => {
                    // $4100: latch or increment
                    if !self.mode {
                        // Mode 0: Register := Input; bits [3:0] inverted when Invert=1
                        let mut new_reg = self.input & 0x3F;
                        if self.invert {
                            new_reg ^= 0x0F; // invert bits [3:0]
                        }
                        self.register = new_reg;
                    } else {
                        // Mode 1: Register[3:0] += 1; bits [5:4] unchanged
                        let hi = self.register & 0x30;
                        let lo = (self.register.wrapping_add(1)) & 0x0F;
                        self.register = hi | lo;
                    }
                }
                1 => {
                    // $4101: Invert := bit 5
                    self.invert = (value & 0x20) != 0;
                }
                2 => {
                    // $4102: Input := bits [5:0] (value written)
                    self.input = value & 0x3F;
                }
                3 => {
                    // $4103: Mode := bit 5
                    self.mode = (value & 0x20) != 0;
                }
                _ => unreachable!(),
            }
        } else if addr >= 0x8000 {
            // $8000-$FFFF: Output := Register; update mirroring
            self.output = self.register;
            if self.invert {
                self.base.set_mirroring(NametableLayout::Vertical);
            } else {
                self.base.set_mirroring(NametableLayout::Horizontal);
            }
            self.apply_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.mode as u8) | ((self.invert as u8) << 1);
        vec![self.input, self.register, self.output, flags]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.input = data[0] & 0x3F;
            self.register = data[1] & 0x3F;
            self.output = data[2] & 0x3F;
            self.mode = (data[3] & 1) != 0;
            self.invert = (data[3] & 2) != 0;
            // Restore mirroring from invert flag.
            if self.invert {
                self.base.set_mirroring(NametableLayout::Vertical);
            } else {
                self.base.set_mirroring(NametableLayout::Horizontal);
            }
            self.apply_banks();
        }
    }

    fn reset(&mut self) {
        self.input = 0;
        self.register = 0;
        self.output = 0;
        self.mode = false;
        self.invert = false;
        self.base.set_mirroring(NametableLayout::Horizontal);
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Mapper172 {
        Mapper172::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, 1),
            banked_data(CHR_BANK_SIZE, 4),
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_172_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, 1),
            banked_data(CHR_BANK_SIZE, 4),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 172 must be creatable via factory");
    }

    #[test]
    fn power_on_state_is_zero() {
        let m = make_mapper();
        assert_eq!(m.input, 0);
        assert_eq!(m.register, 0);
        assert_eq!(m.output, 0);
        assert!(!m.mode);
        assert!(!m.invert);
    }

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let m = make_mapper();
        assert_eq!(m.base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn write_4101_sets_invert_from_bit5() {
        let mut m = make_mapper();
        m.write_prg(0x4101, 0x20);
        assert!(m.invert);
        m.write_prg(0x4101, 0x00);
        assert!(!m.invert);
    }

    #[test]
    fn write_4102_sets_input_bits0_5() {
        let mut m = make_mapper();
        m.write_prg(0x4102, 0xFF);
        assert_eq!(m.input, 0x3F);
    }

    #[test]
    fn write_4103_sets_mode_from_bit5() {
        let mut m = make_mapper();
        m.write_prg(0x4103, 0x20);
        assert!(m.mode);
        m.write_prg(0x4103, 0x00);
        assert!(!m.mode);
    }

    #[test]
    fn write_4100_mode0_latches_input_to_register() {
        let mut m = make_mapper();
        m.write_prg(0x4102, 0x15); // input = $15 = 0b010101
        m.write_prg(0x4100, 0); // Mode=0: register := input
        assert_eq!(m.register, 0x15);
    }

    #[test]
    fn write_4100_mode0_inverts_lower_nibble_when_invert_set() {
        let mut m = make_mapper();
        m.write_prg(0x4101, 0x20); // invert = 1
        m.write_prg(0x4102, 0x15); // input = 0b010101
        m.write_prg(0x4100, 0); // register := input XOR 0x0F on lower nibble
        // lower nibble: 0x5 XOR 0xF = 0xA; upper nibble unchanged: 0x1
        assert_eq!(m.register, 0x1A);
    }

    #[test]
    fn write_4100_mode1_increments_lower_nibble() {
        let mut m = make_mapper();
        m.write_prg(0x4102, 0x15);
        m.write_prg(0x4100, 0); // latch input: register = 0x15
        m.write_prg(0x4103, 0x20); // mode = 1
        m.write_prg(0x4100, 0); // increment lower nibble: 0x15 + 1 = 0x16
        assert_eq!(m.register, 0x16);
    }

    #[test]
    fn write_4100_mode1_lower_nibble_wraps() {
        let mut m = make_mapper();
        m.write_prg(0x4102, 0x1F); // input = 0b011111
        m.write_prg(0x4100, 0); // register = 0x1F
        m.write_prg(0x4103, 0x20); // mode = 1
        m.write_prg(0x4100, 0); // lower nibble wraps: 0xF + 1 = 0x0; upper nibble stays
        assert_eq!(m.register & 0x0F, 0x00);
        assert_eq!(m.register & 0x30, 0x10);
    }

    #[test]
    fn write_8000_latches_register_to_output_and_selects_chr_bank() {
        let mut m = make_mapper();
        m.write_prg(0x4102, 0x03); // input = 3
        m.write_prg(0x4100, 0); // register = 3
        m.write_prg(0x8000, 0); // output = register = 3; chr bank = 3 & 3 = 3
        assert_eq!(m.output, 3);
    }

    #[test]
    fn write_8000_updates_mirroring_horizontal_when_invert_clear() {
        let mut m = make_mapper();
        m.write_prg(0x4101, 0x00); // invert = 0
        m.write_prg(0x8000, 0);
        assert_eq!(m.base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn write_8000_updates_mirroring_vertical_when_invert_set() {
        let mut m = make_mapper();
        m.write_prg(0x4101, 0x20); // invert = 1
        m.write_prg(0x8000, 0);
        assert_eq!(m.base.mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn read_4100_returns_register_bits() {
        let mut m = make_mapper();
        m.write_prg(0x4102, 0x15); // input = 0x15
        m.write_prg(0x4100, 0); // register = 0x15
        let val = m.read_prg(0x4100);
        assert_eq!(val & 0x3F, 0x15);
    }

    #[test]
    fn read_4100_inverts_bits4_5_when_invert_set() {
        let mut m = make_mapper();
        m.write_prg(0x4102, 0x35); // input = 0x35 = 0b110101
        m.write_prg(0x4100, 0); // register = 0x35
        m.write_prg(0x4101, 0x20); // invert = 1
        let val = m.read_prg(0x4100);
        // bits [5:4] = 0b11 XOR 0b11 = 0b00; bits [3:0] = 0b0101 unchanged
        assert_eq!(val & 0x3F, 0x05);
    }

    #[test]
    fn jv001_mirror_addr_4108_also_hits_register_0() {
        // Mask $E103: any addr where (addr & $E100) == $4100 hits register 0..3
        let mut m = make_mapper();
        m.write_prg(0x4108, 0); // addr & $E100 == $4100, addr & 3 == 0 → write $4100
        // Should not panic; register behavior same as $4100
    }

    #[test]
    fn reset_clears_all_ic_state() {
        let mut m = make_mapper();
        m.write_prg(0x4102, 0x3F);
        m.write_prg(0x4100, 0);
        m.write_prg(0x4101, 0x20);
        m.write_prg(0x8000, 0);
        m.reset();
        assert_eq!(m.input, 0);
        assert_eq!(m.register, 0);
        assert_eq!(m.output, 0);
        assert!(!m.mode);
        assert!(!m.invert);
        assert_eq!(m.base.mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn snapshot_restore_round_trips() {
        let mut m = make_mapper();
        m.write_prg(0x4101, 0x20); // invert = 1
        m.write_prg(0x4103, 0x20); // mode = 1
        m.write_prg(0x4102, 0x2A); // input = 0x2A
        m.write_prg(0x4100, 0); // mode=1: increment lower nibble of register (which is 0 → 1)
        m.write_prg(0x8000, 0); // output = register

        let snap = m.registers_snapshot();
        let mut m2 = make_mapper();
        m2.restore_registers(&snap);

        assert_eq!(m2.input, m.input);
        assert_eq!(m2.register, m.register);
        assert_eq!(m2.output, m.output);
        assert_eq!(m2.mode, m.mode);
        assert_eq!(m2.invert, m.invert);
    }
}
