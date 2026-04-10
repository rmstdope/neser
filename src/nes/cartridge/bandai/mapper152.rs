//! Mapper 152 - Bandai 74161/32 with one-screen mirroring control
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_152>
//!
//! This is the same as INES Mapper 070 (Bandai 74161/32) except that bit 7
//! of the data latch controls nametable mirroring:
//!
//! Register (`$8000-$FFFF`, bus conflicts):
//! ```text
//! 7  bit  0
//! ---- ----
//! MPPP CCCC
//! ||||  |||
//! |+++--+++-- CHR bank (8 KB at PPU $0000)
//! |+--------- PRG bank (16 KB at CPU $8000)
//! +---------- Mirroring: 0 = 1-screen A, 1 = 1-screen B
//! ```
//!
//! - CPU `$8000–$BFFF`: 16 KiB switchable PRG-ROM bank
//! - CPU `$C000–$FFFF`: 16 KiB PRG-ROM fixed to the last bank
//! - PPU `$0000–$1FFF`: 8 KiB switchable CHR bank
//! - Bus conflicts: write value is ANDed with PRG-ROM at the same address
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 152 – Bandai 74161/32 (with one-screen mirroring control)
pub struct Mapper152 {
    base: BaseMapper,
    prg_bank: u8,
    chr_bank: u8,
    mirroring_b: bool,
}

impl Mapper152 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        base.set_bus_conflicts(true);
        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
            mirroring_b: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_prg_page(1, -1); // fixed last bank
        self.base.select_chr_page(0, self.chr_bank as i16);
        self.base.set_mirroring(if self.mirroring_b {
            NametableLayout::SingleScreenUpper
        } else {
            NametableLayout::SingleScreenLower
        });
    }
}

impl Mapper for Mapper152 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr < 0x8000 {
            return;
        }
        let effective = self.base.apply_bus_conflict(addr, value);
        self.mirroring_b = (effective & 0x80) != 0;
        self.prg_bank = (effective >> 4) & 0x07;
        self.chr_bank = effective & 0x0F;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank, self.mirroring_b as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            self.mirroring_b = data[2] != 0;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.mirroring_b = false;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 3;
    const CHR_BANKS: usize = 5;

    fn make_mapper() -> Mapper152 {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        Mapper152::new(MapperContext::new_for_test(
            152,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    /// PRG where byte at offset 0 = 0xFF (bus conflict passthrough at $8000)
    /// and byte at offset 1 = bank index (probe via $8001 / $C001).
    fn make_prg_with_conflict_passthrough() -> Vec<u8> {
        let mut prg = vec![0xFF; PRG_BANK_SIZE * PRG_BANKS];
        for bank in 0..PRG_BANKS {
            prg[bank * PRG_BANK_SIZE + 1] = bank as u8;
        }
        prg
    }

    fn make_mapper_passthrough() -> Mapper152 {
        let prg = make_prg_with_conflict_passthrough();
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        Mapper152::new(MapperContext::new_for_test(
            152,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper_152_is_registered_in_factory() {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            152,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 152 must be creatable via factory");
    }

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_fixed_to_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000-$FFFF must be fixed to last PRG bank"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 at power-on");
    }

    #[test]
    fn prg_bank_switches_via_bits_6_4() {
        let mut mapper = make_mapper_passthrough();
        // bits[6:4] = 0b010 = 2 → PRG bank 2; bus conflict: 0x20 & 0xFF = 0x20
        mapper.write_prg(0x8000, 0b0010_0000);
        assert_eq!(mapper.read_prg(0x8001), 2, "PRG bank 2 from bits[6:4]=010");
    }

    #[test]
    fn chr_bank_switches_via_bits_3_0() {
        let mut mapper = make_mapper_passthrough();
        mapper.write_prg(0x8000, 0b0000_0011);
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank 3 from bits[3:0]=0011");
    }

    #[test]
    fn prg_c000_stays_fixed_after_register_write() {
        let mut mapper = make_mapper_passthrough();
        // PRG bank 1: bits[6:4]=001=1 → 0x10; bus conflict: 0x10 & 0xFF = 0x10
        mapper.write_prg(0x8000, 0x10);
        // $C000-$FFFF = page 1 = fixed last bank. Probe at $C001 (offset 1 of last bank)
        assert_eq!(
            mapper.read_prg(0xC001),
            (PRG_BANKS - 1) as u8,
            "$C000-$FFFF must remain fixed after bank switch"
        );
    }

    #[test]
    fn bit_7_zero_selects_1screen_a() {
        let mut mapper = make_mapper_passthrough();
        mapper.write_prg(0x8000, 0x00); // bit 7 = 0
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "bit 7 = 0 must select 1-screen A"
        );
    }

    #[test]
    fn bit_7_one_selects_1screen_b() {
        let mut mapper = make_mapper_passthrough();
        mapper.write_prg(0x8000, 0x80); // bit 7 = 1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenUpper,
            "bit 7 = 1 must select 1-screen B"
        );
    }

    #[test]
    fn bus_conflicts_and_value_with_prg_rom() {
        // banked_data: bank 0 byte 0 = 0. Write 0xFF → 0xFF & 0x00 = 0x00 → bank 0 / chr 0
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Bus conflict: 0xFF & 0x00 = 0x00 → bank 0"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "Bus conflict: chr 0");
    }

    #[test]
    fn register_responds_to_full_8000_ffff_range() {
        let mut mapper = make_mapper_passthrough();
        // Write 0x10 (bits[6:4]=001=1) to $FFFF; bus conflict: 0x10 & 0xFF = 0x10
        mapper.write_prg(0xFFFF, 0b0001_0000);
        assert_eq!(
            mapper.read_prg(0x8001),
            1,
            "Register at $FFFF sets PRG bank 1"
        );
    }

    #[test]
    fn snapshot_round_trips() {
        let mut mapper = make_mapper_passthrough();
        mapper.write_prg(0x8000, 0b1001_0011); // M=1, PRG=1, CHR=3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper_passthrough();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8001), mapper.read_prg(0x8001));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
        assert_eq!(restored.get_mirroring(), mapper.get_mirroring());
    }

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper_passthrough();
        mapper.write_prg(0x8000, 0b1001_0011);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8001), 0, "PRG bank 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 after reset");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "1-screen A after reset"
        );
    }
}
