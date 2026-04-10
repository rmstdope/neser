//! Mapper 173 - TXC / Idea-Tek
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_173>
//!
//! Used by Idea-Tek's original ET-xx releases. Similar to INES Mapper 132
//! except that CHR A14 comes from the inverted Invert flag rather than from
//! Output bit 1.
//!
//! Memory map:
//! - CPU `$8000-$FFFF`: 32 KiB fixed PRG-ROM (no switching)
//! - PPU `$0000-$1FFF`: 8 KiB switchable CHR-ROM bank
//!
//! Register window (`$4100-$5FFF`, mask `$E100`):
//! ```text
//! read  $4100: [xxxx SRRR]  - S XOR Invert, then R (same as mapper 132)
//! write $4100: update R from P or ~P (or increment), based on flags
//! write $4101: [.... ...V]  - set Invert flag; CHR A14 = ~V immediately
//! write $4102: [.... SPPP]  - set S and P registers
//! write $4103: [.... ...C]  - set Increment mode
//! write $8000: latch R→Output and update CHR bank
//! ```
//!
//! CHR bank formula:
//! ```text
//! CHR bank = (Output & 1) | (!Invert << 1)
//! ```
//!
//! Writing `$4101` takes effect **immediately** on the CHR bank.
//!
//! Known Limitations:
//! - 小瑪琍 (Xiǎo Mǎlí) replaces the 32 KiB CHR-ROM with an 8 KiB UVEPROM;
//!   this variant is not distinguished here.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const REG_SELECT_R: u16 = 0;
const REG_INVERT_MODE: u16 = 1;
const REG_SET_P_AND_S: u16 = 2;
const REG_INCREMENT_MODE: u16 = 3;

/// Mapper 173 – TXC / Idea-Tek
pub struct Mapper173 {
    base: BaseMapper,
    p: u8,
    r: u8,
    s: bool,
    increment_mode: bool,
    invert_mode: bool,
    output: u8,
}

impl Mapper173 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self {
            base,
            p: 0,
            r: 0,
            s: false,
            increment_mode: false,
            invert_mode: false,
            output: 0,
        };
        mapper.apply_output_to_banks();
        mapper
    }

    fn status_nibble(&self) -> u8 {
        (((self.s ^ self.invert_mode) as u8) << 3) | (self.r & 0x07)
    }

    fn is_4100_masked_window(addr: u16) -> bool {
        addr & 0xE100 == 0x4100
    }

    fn is_status_read_address(addr: u16) -> bool {
        Self::is_4100_masked_window(addr) && (addr & 0x0003) == REG_SELECT_R
    }

    /// CHR bank = (Output[0]) | (!Invert << 1)
    fn apply_output_to_banks(&mut self) {
        self.base.select_prg_page(0, 0); // PRG always fixed (32 KB)
        let chr_bank = (self.output & 0x01) | ((!self.invert_mode as u8 & 0x01) << 1);
        self.base.select_chr_page(0, chr_bank as i16);
    }
}

impl Mapper for Mapper173 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if Self::is_status_read_address(addr) {
            return self.status_nibble();
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            return self.base.read_prg_rom(addr);
        }
        0
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if Self::is_status_read_address(addr) {
            return (open_bus & 0xF0) | self.status_nibble();
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_4100_masked_window(addr) {
            match addr & 0x0003 {
                REG_SELECT_R => {
                    if self.increment_mode {
                        self.r = self.r.wrapping_add(1) & 0x07;
                    } else if self.invert_mode {
                        self.r = (!self.p) & 0x07;
                    } else {
                        self.r = self.p & 0x07;
                    }
                }
                REG_INVERT_MODE => {
                    self.invert_mode = value & 0x01 != 0;
                    // CHR A14 changes immediately on invert flag write
                    self.apply_output_to_banks();
                }
                REG_SET_P_AND_S => {
                    self.s = value & 0x08 != 0;
                    self.p = value & 0x07;
                }
                REG_INCREMENT_MODE => {
                    self.increment_mode = value & 0x01 != 0;
                }
                _ => {}
            }
            return;
        }

        if (addr & 0x8000) != 0 {
            self.output = self.r & 0x07;
            self.apply_output_to_banks();
        }
    }

    fn reset(&mut self) {
        self.p = 0;
        self.r = 0;
        self.s = false;
        self.increment_mode = false;
        self.invert_mode = false;
        self.output = 0;
        self.apply_output_to_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.p,
            self.r,
            self.s as u8,
            self.increment_mode as u8,
            self.invert_mode as u8,
            self.output,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 6 {
            return;
        }
        self.p = data[0] & 0x07;
        self.r = data[1] & 0x07;
        self.s = data[2] & 0x01 != 0;
        self.increment_mode = data[3] & 0x01 != 0;
        self.invert_mode = data[4] & 0x01 != 0;
        self.output = data[5] & 0x07;
        self.apply_output_to_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS_32K: usize = 1;
    const CHR_BANKS_8K: usize = 4;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(32 * 1024, PRG_BANKS_32K);
        let chr = banked_data(8 * 1024, CHR_BANKS_8K);
        create_mapper(MapperContext::new_for_test(
            173,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 173 must be creatable")
    }

    fn set_r(mapper: &mut dyn Mapper, p: u8, invert: bool, increment: bool, s: bool) {
        mapper.write_prg(0x4102, ((s as u8) << 3) | (p & 0x07));
        mapper.write_prg(0x4101, u8::from(invert));
        mapper.write_prg(0x4103, u8::from(increment));
        mapper.write_prg(0x4100, 0);
    }

    #[test]
    fn mapper_173_is_registered_in_factory() {
        let prg = banked_data(32 * 1024, PRG_BANKS_32K);
        let chr = banked_data(8 * 1024, CHR_BANKS_8K);
        let result = create_mapper(MapperContext::new_for_test(
            173,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 173 must be creatable via factory");
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper();
        // Invert=0 at power-on: chr = (0 & 1) | (!0 << 1) = 0 | 2 = 2
        // Since !invert is true (invert=0), CHR A14=1, so bank 2
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "power-on: invert=0 → CHR bank = (0&1)|(1<<1) = 2"
        );
    }

    // --- CHR bank formula: (Output & 1) | (!Invert << 1) ---

    #[test]
    fn invert_zero_gives_chr_a14_high() {
        let mut mapper = make_mapper();
        // Set R=0 via P=0, invert=0 (not set)
        set_r(mapper.as_mut(), 0, false, false, false);
        mapper.write_prg(0x8000, 0); // latch R=0 → output=0
        // CHR = (0 & 1) | (!false << 1) = 0 | 2 = 2
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "Invert=0 → CHR bank bit 1 = 1 (bank 2)"
        );
    }

    #[test]
    fn invert_one_gives_chr_a14_low() {
        let mut mapper = make_mapper();
        // Set invert=1 directly
        mapper.write_prg(0x4102, 0b000); // P=0, S=0
        mapper.write_prg(0x4101, 1); // invert=1
        mapper.write_prg(0x4103, 0);
        mapper.write_prg(0x4100, 0); // R = ~P & 7 (invert=1) = 7
        mapper.write_prg(0x8000, 0); // output = R = 7

        // CHR = (7 & 1) | (!true << 1) = 1 | 0 = 1
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "Invert=1 → CHR bank bit 1 = 0, bit 0 from output = 1"
        );
    }

    #[test]
    fn output_bit_0_selects_chr_bank_within_pair() {
        let mut mapper = make_mapper();
        // invert=1, so CHR = (output & 1) | 0
        mapper.write_prg(0x4101, 1); // invert=1

        // R = 0 (no increment, no invert applied to P copy since it hasn't been explicitly set)
        // Actually R starts at 0, so output=0 → CHR = 0
        mapper.write_prg(0x4102, 0b001); // P=1, S=0
        mapper.write_prg(0x4103, 0);
        mapper.write_prg(0x4101, 1); // invert stays 1
        mapper.write_prg(0x4100, 0); // R = ~P since invert=1: ~1 & 7 = 6
        mapper.write_prg(0x8000, 0); // output = 6
        // CHR = (6 & 1) | (!1 << 1) = 0 | 0 = 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "output=6, invert=1 → CHR bank 0"
        );

        // P=1, invert=1: R = ~1 & 7 = 6, output=6, chr=0
        mapper.write_prg(0x4102, 0b111); // P=7
        mapper.write_prg(0x4101, 1);
        mapper.write_prg(0x4100, 0); // R = ~7 & 7 = 0
        mapper.write_prg(0x8000, 0); // output = 0
        // CHR = (0 & 1) | 0 = 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "output=0, invert=1 → CHR bank 0"
        );
    }

    #[test]
    fn writing_4101_immediately_updates_chr_bank() {
        let mut mapper = make_mapper();
        // Start with invert=0 → CHR bank 2 (output=0, !invert=1)
        mapper.write_prg(0x4101, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "invert=0 immediately → CHR bank 2"
        );

        // Now set invert=1 → CHR bank 0 (output=0, !invert=0)
        mapper.write_prg(0x4101, 1);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "invert=1 immediately → CHR bank 0 (without $8000 write)"
        );
    }

    #[test]
    fn read_4100_returns_status_nibble() {
        let mut mapper = make_mapper();
        set_r(mapper.as_mut(), 0b101, false, false, true); // S=1, R=0b101
        assert_eq!(
            mapper.read_prg(0x4100) & 0x0F,
            0b1101,
            "Status nibble: S XOR V=0 → 1, RRR=101"
        );
    }

    #[test]
    fn prg_is_fixed_32k_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG always fixed to bank 0");
    }

    #[test]
    fn snapshot_round_trips() {
        let mut mapper = make_mapper();
        set_r(mapper.as_mut(), 0b011, true, false, true);
        mapper.write_prg(0x8000, 0);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "CHR bank preserved by snapshot"
        );
        assert_eq!(
            restored.read_prg(0x4100) & 0x0F,
            mapper.read_prg(0x4100) & 0x0F,
            "Status nibble preserved by snapshot"
        );
    }

    #[test]
    fn reset_clears_all_state() {
        let mut mapper = make_mapper();
        set_r(mapper.as_mut(), 0b111, true, true, true);
        mapper.write_prg(0x8000, 0);
        mapper.reset();

        assert_eq!(mapper.read_prg(0x4100) & 0x0F, 0, "Status 0 after reset");
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "CHR bank 2 after reset (invert=0 → !invert=1)"
        );
    }
}
