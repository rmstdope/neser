//! Mapper 242 - 43272 (address-latch PRG switch with mirroring control)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_242>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 242 - 43272 (address-latch PRG switch)
///
/// Hardware: Simple 32KB PRG bank switching via address bits.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_242>
/// - PRG-ROM: Up to 256KB (8 x 32KB banks)
/// - CHR: 8KB CHR-RAM
/// - Mirroring: Programmable (H/V) via address bit 0
///
/// On write to $8000-$FFFF:
/// - Address bits A[3:1] select 32KB PRG bank
/// - Address bit A[0] selects mirroring: 0=Vertical, 1=Horizontal
pub struct Mapper242 {
    base: BaseMapper,
}

impl Mapper242 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        Self { base }
    }
}

impl Mapper for Mapper242 {
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
        if (0x8000..=0xFFFF).contains(&addr) {
            // Address bits A[3:1] select PRG bank
            let bank = ((addr >> 1) & 0x07) as i16;
            self.base.select_prg_page(0, bank);
            // Address bit A[0] selects mirroring
            self.base.set_mirroring_hv(addr & 0x01 != 0);
        }
        let _ = value; // Data value is ignored
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.base.banking_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.base.restore_banking(data);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper242(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            242, prg_rom, chr_rom, mirroring,
        ))
    }

    #[test]
    fn test_factory_creates_mapper_242() {
        let prg_rom = banked_data(32 * 1024, 4);
        let mapper = create_mapper242(prg_rom, vec![], NametableLayout::Vertical);
        assert!(mapper.is_ok(), "Mapper 242 should be creatable via factory");
    }

    #[test]
    fn test_initial_prg_bank_is_zero() {
        let prg_rom = banked_data(32 * 1024, 4);
        let mapper = create_mapper242(prg_rom, vec![], NametableLayout::Vertical).unwrap();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn test_prg_bank_selected_by_address_bits() {
        let prg_rom = banked_data(32 * 1024, 8);
        let mut mapper = create_mapper242(prg_rom, vec![], NametableLayout::Vertical).unwrap();

        // Write to address with A[3:1]=011 (bank 3): addr $8006 or $8007
        mapper.write_prg(0x8006, 0);
        assert_eq!(mapper.read_prg(0x8000), 3);

        // Write to address with A[3:1]=101 (bank 5): addr $800A
        mapper.write_prg(0x800A, 0);
        assert_eq!(mapper.read_prg(0x8000), 5);

        // Write to address with A[3:1]=111 (bank 7): addr $800E
        mapper.write_prg(0x800E, 0);
        assert_eq!(mapper.read_prg(0x8000), 7);
    }

    #[test]
    fn test_mirroring_controlled_by_address_bit_0() {
        let prg_rom = banked_data(32 * 1024, 4);
        let mut mapper = create_mapper242(prg_rom, vec![], NametableLayout::Vertical).unwrap();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // A[0]=1 -> Horizontal
        mapper.write_prg(0x8001, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // A[0]=0 -> Vertical
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_prg_bank_wrapping() {
        let prg_rom = banked_data(32 * 1024, 4); // 4 banks
        let mut mapper = create_mapper242(prg_rom, vec![], NametableLayout::Vertical).unwrap();

        // A[3:1]=101 (bank 5), should wrap to bank 1 (5 % 4)
        mapper.write_prg(0x800A, 0);
        assert_eq!(mapper.read_prg(0x8000), 1);
    }

    #[test]
    fn test_chr_ram_read_write() {
        let prg_rom = banked_data(32 * 1024, 2);
        let mut mapper = create_mapper242(prg_rom, vec![], NametableLayout::Vertical).unwrap();

        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1FFF, 0xBB);
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1FFF), 0xBB);
    }

    #[test]
    fn test_data_value_is_ignored() {
        let prg_rom = banked_data(32 * 1024, 8);
        let mut mapper = create_mapper242(prg_rom, vec![], NametableLayout::Vertical).unwrap();

        // Bank is determined by address bits, not data
        mapper.write_prg(0x8004, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 2); // A[3:1]=010 = bank 2

        mapper.write_prg(0x8004, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 2); // Same address, same bank
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(32 * 1024, 8);
        let mut mapper =
            create_mapper242(prg_rom.clone(), vec![], NametableLayout::Vertical).unwrap();

        // Set bank 5 and horizontal mirroring
        mapper.write_prg(0x800B, 0); // A[3:1]=101, A[0]=1
        assert_eq!(mapper.read_prg(0x8000), 5);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper242(prg_rom, vec![], NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);
        assert_eq!(restored.read_prg(0x8000), 5);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_prg_ram_at_6000_7fff() {
        let prg_rom = banked_data(32 * 1024, 2);
        let mut mapper = create_mapper242(prg_rom, vec![], NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0x42);
        mapper.write_prg(0x7FFF, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0x42);
        assert_eq!(mapper.read_prg(0x7FFF), 0xAB);
    }
}
