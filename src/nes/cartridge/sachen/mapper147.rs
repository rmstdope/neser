//! Mapper 147 – Sachen SA-0036 (JV001 ASIC)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_147>
//!
//! The board uses the JV001 ASIC (same chip family as mapper 36 / TXC 22000 but
//! with a 4-bit accumulator, initial invert=true, and a different output formula).
//!
//! Memory map:
//! - CPU `$8000–$FFFF`: 32 KiB switchable PRG bank
//! - PPU `$0000–$1FFF`: 8 KiB switchable CHR bank
//!
//! Register interface:
//! - **Write** (`$4020–$5FFF` or `$8000–$FFFF`): rotate value right by 2 bits then
//!   pass to JV001 chip; if address ≥ `$8000` also trigger a state update.
//! - **Read** (`$4020–$5FFF`, `(addr & 0x103) == 0x100`): rotate chip read left by
//!   2 bits and return result.
//!
//! JV001 chip specifics (differs from TXC 22000):
//! - Accumulator and staging masks are 4 bits (0x0F) instead of 3 bits (0x07).
//! - Initial `invert` flag is `true` (not `false`).
//! - Output formula: `(accumulator & 0x0F) | (inverter & 0xF0)`.
//!
//! Bank selection from output byte:
//! - PRG bank = `((output & 0x20) >> 4) | (output & 0x01)` (2-bit bank)
//! - CHR bank = `(output & 0x1E) >> 1` (4-bit bank)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

/// JV001 chip state.
struct TxcJv001Chip {
    accumulator: u8,
    inverter: u8,
    staging: u8,
    output: u8,
    increment_mode: bool,
    invert: bool,
}

impl TxcJv001Chip {
    fn new() -> Self {
        Self {
            accumulator: 0,
            inverter: 0,
            staging: 0,
            output: 0,
            increment_mode: false,
            invert: true, // JV001 starts inverted
        }
    }

    fn invert_mask(&self) -> u8 {
        if self.invert { 0xFF } else { 0 }
    }

    fn read(&self) -> u8 {
        (self.accumulator & 0x0F) | ((self.inverter ^ self.invert_mask()) & 0xF0)
    }

    fn latch_accumulator(&mut self) {
        if self.increment_mode {
            self.accumulator = self.accumulator.wrapping_add(1);
        } else {
            self.accumulator =
                ((self.accumulator & 0xF0) | (self.staging & 0x0F)) ^ self.invert_mask();
        }
    }

    fn latch_output(&mut self) {
        self.output = (self.accumulator & 0x0F) | (self.inverter & 0xF0);
    }

    fn write(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            match addr & 0xE103 {
                0x4100 => self.latch_accumulator(),
                0x4101 => self.invert = value & 0x01 != 0,
                0x4102 => {
                    self.staging = value & 0x0F;
                    self.inverter = value & 0xF0;
                }
                0x4103 => self.increment_mode = value & 0x01 != 0,
                _ => {}
            }
        } else {
            self.latch_output();
        }
    }

    fn output(&self) -> u8 {
        self.output
    }
}

pub struct Mapper147 {
    base: BaseMapper,
    chip: TxcJv001Chip,
}

impl Mapper147 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.select_prg_page(0, 0);
        base.select_chr_page(0, 0);
        Self {
            base,
            chip: TxcJv001Chip::new(),
        }
    }

    fn update_state(&mut self) {
        let output = self.chip.output();
        let prg_bank = ((output & 0x20) >> 4) | (output & 0x01);
        let chr_bank = (output & 0x1E) >> 1;
        self.base.select_prg_page(0, prg_bank as i16);
        self.base.select_chr_page(0, chr_bank as i16);
    }

    fn is_chip_read_address(addr: u16) -> bool {
        (0x4020..=0x5FFF).contains(&addr) && (addr & 0x0103) == 0x0100
    }

    fn is_handled_write_address(addr: u16) -> bool {
        (0x4020..=0x5FFF).contains(&addr) || (0x8000..=0xFFFF).contains(&addr)
    }

    fn rotate_right(value: u8) -> u8 {
        ((value & 0xFC) >> 2) | ((value & 0x03) << 6)
    }

    fn rotate_left(value: u8) -> u8 {
        ((value & 0x3F) << 2) | ((value & 0xC0) >> 6)
    }
}

impl Mapper for Mapper147 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if Self::is_chip_read_address(addr) {
            return Self::rotate_left(self.chip.read());
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !Self::is_handled_write_address(addr) {
            return;
        }
        let rotated = Self::rotate_right(value);
        self.chip.write(addr, rotated);
        if addr >= 0x8000 {
            self.update_state();
        }
    }

    fn reset(&mut self) {
        self.chip = TxcJv001Chip::new();
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANK_32K: usize = 32 * 1024;
    const CHR_BANK_8K: usize = 8 * 1024;

    fn make_mapper147() -> Box<dyn crate::nes::cartridge::mapper::Mapper> {
        let prg = banked_data(PRG_BANK_32K, 3);
        let chr = banked_data(CHR_BANK_8K, 16);
        create_mapper(MapperContext::new_for_test(
            147,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 147 must be registered in factory")
    }

    #[test]
    fn mapper_147_is_registered_in_factory() {
        let prg = banked_data(PRG_BANK_32K, 3);
        let chr = banked_data(CHR_BANK_8K, 16);
        let result = create_mapper(MapperContext::new_for_test(
            147,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 147 must be creatable via factory");
    }

    #[test]
    fn power_on_selects_prg_bank_0() {
        let mapper = make_mapper147();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank 0 on power-on");
    }

    #[test]
    fn power_on_selects_chr_bank_0() {
        let mut mapper = make_mapper147();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 on power-on");
    }

    /// Initial chip read at $4100 (with invert=true):
    ///
    /// chip.read() = (0 & 0x0F) | ((0 ^ 0xFF) & 0xF0) = 0xF0
    /// rotate_left(0xF0) = ((0xF0 & 0x3F) << 2) | ((0xF0 & 0xC0) >> 6)
    ///                   = (0x30 << 2) | (0xC0 >> 6) = 0xC0 | 0x03 = 0xC3
    #[test]
    fn initial_protection_read_at_4100_returns_0xc3() {
        let mapper = make_mapper147();
        let val = mapper.read_prg_open_bus(0x4100, 0x00);
        assert_eq!(val, 0xC3, "initial JV001 read at $4100 should be 0xC3");
    }

    /// After disabling invert and setting accumulator = 0x01 via the chip:
    ///
    /// 1. Write $4101 ← 0x00  → rotate_right(0x00)=0x00; invert=false
    /// 2. Write $4102 ← 0x04  → rotate_right(0x04)=0x01; staging=0x01, inverter=0x00
    /// 3. Write $4100 ← 0x00  → rotate_right(0x00)=0x00; latch: acc=0x01 (invert=false)
    /// 4. Write $8000 ← 0x00  → output=(0x01&0x0F)|(0x00&0xF0)=0x01
    ///
    /// PRG=((0x01&0x20)>>4)|(0x01&0x01)=0|1=1; CHR=(0x01&0x1E)>>1=0
    #[test]
    fn prg_bank_switches_via_jv001_output() {
        let mut mapper = make_mapper147();

        mapper.write_prg(0x4101, 0x00); // set invert=false
        mapper.write_prg(0x4102, 0x04); // staging=0x01 (rotate_right(0x04)=0x01)
        mapper.write_prg(0x4100, 0x00); // latch
        mapper.write_prg(0x8000, 0x00); // update output → 0x01

        assert_eq!(mapper.read_prg(0x8000), 1, "PRG bank 1 after JV001 setup");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR still 0 with output=0x01");
    }

    /// After setting accumulator = 0x04 via the chip:
    ///
    /// 1. Write $4101 ← 0x00 → invert=false
    /// 2. Write $4102 ← 0x10 → rotate_right(0x10)=0x04; staging=0x04, inverter=0x00
    /// 3. Write $4100 ← 0x00 → latch: acc=0x04
    /// 4. Write $8000 ← 0x00 → output=0x04
    ///
    /// CHR=(0x04&0x1E)>>1=0x04>>1=2; PRG=0
    #[test]
    fn chr_bank_switches_via_jv001_output() {
        let mut mapper = make_mapper147();

        mapper.write_prg(0x4101, 0x00); // invert=false
        mapper.write_prg(0x4102, 0x10); // staging=0x04 (rotate_right(0x10)=0x04)
        mapper.write_prg(0x4100, 0x00); // latch
        mapper.write_prg(0x8000, 0x00); // update output → 0x04

        assert_eq!(mapper.read_chr(0x0000), 2, "CHR bank 2 with output=0x04");
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG still 0 with output=0x04");
    }

    #[test]
    fn reset_restores_power_on_banks() {
        let mut mapper = make_mapper147();

        mapper.write_prg(0x4101, 0x00);
        mapper.write_prg(0x4102, 0x04);
        mapper.write_prg(0x4100, 0x00);
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 1);

        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 after reset");
    }
}
