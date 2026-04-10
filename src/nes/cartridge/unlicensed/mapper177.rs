//! Mapper 177 - Hengge Dianzi
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 177 - Hengge Dianzi (恒格电子)
///
/// Hardware: Custom logic on Hengge Dianzi boards.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_177>
/// - PRG-ROM: Up to 512 KB (32 KB banks, switchable at $8000–$FFFF)
/// - CHR: 8 KB CHR-RAM
/// - PRG-RAM: 8 KB battery-backed WRAM at $6000–$7FFF
/// - Mirroring: Software-controlled (bit 5 of register)
///
/// Register: any write to $8000–$FFFF
///
/// ```text
/// 7 bit 0
/// ---------
/// $8000-FFFF: ..MP PPPP
///              |+-++++- Select 32 KiB PRG-ROM bank
///              +------- Nametable mirroring: 0=Vertical, 1=Horizontal
/// ```
pub struct Mapper177 {
    base: BaseMapper,
    reg: u8,
}

impl Mapper177 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            max_prg_ram_kb: 8,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        let mapper = Self { base, reg: 0 };
        mapper
    }

    fn apply_register(&mut self, value: u8) {
        self.reg = value;
        self.base.select_prg_page(0, (value & 0x1F) as i16);
        if (value & 0x20) != 0 {
            self.base.set_mirroring(NametableLayout::Horizontal);
        } else {
            self.base.set_mirroring(NametableLayout::Vertical);
        }
    }
}

impl Mapper for Mapper177 {
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
        if addr >= 0x8000 {
            self.apply_register(value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.apply_register(value);
        }
    }

    fn reset(&mut self) {
        self.apply_register(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_mapper177(prg_rom: Vec<u8>) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            177,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper177_instantiates_via_factory() {
        let prg_rom = banked_data(32 * 1024, 2);
        assert!(
            create_mapper177(prg_rom).is_ok(),
            "Mapper 177 must be creatable via the factory"
        );
    }

    #[test]
    fn initial_prg_bank_is_zero() {
        let prg_rom = banked_data(32 * 1024, 4);
        let mapper = create_mapper177(prg_rom).unwrap();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should be PRG bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "$FFFF should be PRG bank 0 at power-on"
        );
    }

    #[test]
    fn prg_bank_switches_on_write() {
        let prg_rom = banked_data(32 * 1024, 6);
        let mut mapper = create_mapper177(prg_rom).unwrap();

        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 should be PRG bank 3");
        assert_eq!(mapper.read_prg(0xFFFF), 3, "$FFFF should be PRG bank 3");
    }

    #[test]
    fn prg_bank_selectable_from_any_address_in_range() {
        let prg_rom = banked_data(32 * 1024, 6);
        let mut mapper = create_mapper177(prg_rom).unwrap();

        mapper.write_prg(0xC000, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "Write to $C000 should switch PRG bank"
        );

        mapper.write_prg(0xFFFF, 4);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "Write to $FFFF should switch PRG bank"
        );
    }

    #[test]
    fn prg_bank_bits_only_lower_5_used() {
        let prg_rom = banked_data(32 * 1024, 6);
        let mut mapper = create_mapper177(prg_rom).unwrap();

        // Write 0b1100_0011 = 0xC3; bits 4-0 = 3, bits 7-5 ignored for bank
        mapper.write_prg(0x8000, 0b1100_0011);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Only bits 4-0 should be used for PRG bank selection"
        );
    }

    #[test]
    fn prg_bank_wraps_to_rom_size() {
        let prg_rom = banked_data(32 * 1024, 4); // 4 banks
        let mut mapper = create_mapper177(prg_rom).unwrap();

        mapper.write_prg(0x8000, 5); // bank 5 % 4 = 1
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Bank 5 should wrap to bank 1 (5 % 4)"
        );
    }

    #[test]
    fn mirroring_bit5_clear_selects_vertical() {
        let prg_rom = banked_data(32 * 1024, 2);
        let mut mapper = create_mapper177(prg_rom).unwrap();

        mapper.write_prg(0x8000, 0b0000_0000); // bit 5 clear
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Bit 5 clear should select vertical mirroring"
        );
    }

    #[test]
    fn mirroring_bit5_set_selects_horizontal() {
        let prg_rom = banked_data(32 * 1024, 2);
        let mut mapper = create_mapper177(prg_rom).unwrap();

        mapper.write_prg(0x8000, 0b0010_0000); // bit 5 set
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Bit 5 set should select horizontal mirroring"
        );
    }

    #[test]
    fn chr_ram_is_readable_and_writable() {
        let prg_rom = banked_data(32 * 1024, 2);
        let mut mapper = create_mapper177(prg_rom).unwrap();

        mapper.write_chr(0x0000, 0xAB);
        mapper.write_chr(0x1FFF, 0xCD);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR-RAM at $0000 should be writable"
        );
        assert_eq!(
            mapper.read_chr(0x1FFF),
            0xCD,
            "CHR-RAM at $1FFF should be writable"
        );
    }

    #[test]
    fn prg_ram_at_6000_is_accessible() {
        let prg_rom = banked_data(32 * 1024, 2);
        let mut mapper = create_mapper177(prg_rom).unwrap();

        mapper.write_prg(0x6000, 0x42);
        mapper.write_prg(0x7FFF, 0x99);
        assert_eq!(
            mapper.read_prg(0x6000),
            0x42,
            "PRG-RAM at $6000 should be writable"
        );
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0x99,
            "PRG-RAM at $7FFF should be writable"
        );
    }

    #[test]
    fn snapshot_restore_preserves_prg_bank_and_mirroring() {
        let prg_rom = banked_data(32 * 1024, 6);
        let mut mapper = create_mapper177(prg_rom.clone()).unwrap();

        mapper.write_prg(0x8000, 0b0010_0011); // bank=3, horizontal mirroring
        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper177(prg_rom).unwrap();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            3,
            "PRG bank 3 should be restored"
        );
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Horizontal,
            "Horizontal mirroring should be restored"
        );
    }

    #[test]
    fn reset_restores_initial_state() {
        let prg_rom = banked_data(32 * 1024, 6);
        let mut mapper = create_mapper177(prg_rom).unwrap();

        mapper.write_prg(0x8000, 0b0010_0101); // bank=5, horizontal
        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Reset should restore PRG bank 0"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Reset should restore vertical mirroring"
        );
    }
}
