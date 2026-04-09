//! Mapper 11 - Color Dreams
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::nes::cartridge::mapper::MapperContext;
use crate::nes::cartridge::mapper_templates::DualBank32Mapper;

/// Mapper 11 - Color Dreams
///
/// Hardware: Simple unlicensed mapper with combined PRG/CHR banking
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/Color_Dreams>
/// - PRG-ROM: Up to 128KB (4 32KB banks)
/// - PRG-RAM: None
/// - CHR-ROM: Up to 128KB (16 8KB banks)
/// - Mirroring: Fixed horizontal or vertical
///
/// Common boards: Unlicensed Color Dreams boards
///
/// Submappers:
/// - Submapper 0: Standard with bus conflicts (74LS377 latch)
/// - Submapper 1: No bus conflicts (Free Fall prototype variant)
///
/// Notes:
/// - Single register at any write to $8000-$FFFF
/// - Register layout: CCCC LLPP
/// - Bits 0-1: Select 32KB PRG bank
/// - Bits 4-7: Select 8KB CHR bank
/// - Used in unlicensed games like Crystal Mines, Bible Adventures
/// - Some variants support different bank counts
///
/// Implementation:
/// - Wrapper enum that selects between bus-conflict and no-conflict variants
/// - Uses `DualBank32Mapper` template with PRG bits 0-1, CHR bits 4-7
pub enum ColorDreamsMapper {
    WithBusConflicts(DualBank32Mapper<0b0011, 0, 0b1111, 4, true, 11>),
    NoBusConflicts(DualBank32Mapper<0b0011, 0, 0b1111, 4, false, 11>),
}

impl ColorDreamsMapper {
    pub fn new(ctx: MapperContext) -> Self {
        // Submapper 1 is the no-bus-conflict variant (Free Fall prototype)
        // Submapper 0 (and default) has bus conflicts
        if ctx.submapper == 1 {
            Self::NoBusConflicts(DualBank32Mapper::new(ctx))
        } else {
            Self::WithBusConflicts(DualBank32Mapper::new(ctx))
        }
    }
}

impl crate::nes::cartridge::Mapper for ColorDreamsMapper {
    fn base(&self) -> &crate::nes::cartridge::base_mapper::BaseMapper {
        match self {
            Self::WithBusConflicts(m) => m.base(),
            Self::NoBusConflicts(m) => m.base(),
        }
    }

    fn base_mut(&mut self) -> &mut crate::nes::cartridge::base_mapper::BaseMapper {
        match self {
            Self::WithBusConflicts(m) => m.base_mut(),
            Self::NoBusConflicts(m) => m.base_mut(),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match self {
            Self::WithBusConflicts(m) => m.write_prg(addr, value),
            Self::NoBusConflicts(m) => m.write_prg(addr, value),
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        match self {
            Self::WithBusConflicts(m) => m.registers_snapshot(),
            Self::NoBusConflicts(m) => m.registers_snapshot(),
        }
    }

    fn restore_registers(&mut self, data: &[u8]) {
        match self {
            Self::WithBusConflicts(m) => m.restore_registers(data),
            Self::NoBusConflicts(m) => m.restore_registers(data),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;
    const BUS_CONFLICT_SAFE_ADDR: u16 = 0x8001;

    fn banked_prg_with_conflict_safe_write(num_banks: usize) -> Vec<u8> {
        let mut prg_rom = banked_data(32 * 1024, num_banks);
        for bank in 0..num_banks {
            prg_rom[bank * 32 * 1024 + 1] = 0xFF;
        }
        prg_rom
    }

    fn create_colordreams_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(
            MapperContext::new_for_test(11, prg_rom, chr_rom, mirroring).with_prg_ram_banks(0),
        )
    }

    #[test]
    fn test_colordreams_prg_and_chr_bank_selected_by_single_write() {
        // Mapper 11 (ColorDreams):
        // - Register layout: CCCC LLPP
        // - PRG: 32KB banks selected by bits 0-1 (PP)
        // - CHR: 8KB banks selected by bits 4-7 (CCCC)

        let prg_rom = banked_prg_with_conflict_safe_write(4);
        let chr_rom = banked_data(8 * 1024, 16);

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        // Initial banks should be 0.
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);

        // Select CHR bank 1 (bits 4-7) and PRG bank 2 (bits 0-1): 0b0001_0010 = 0x12
        mapper.write_prg(BUS_CONFLICT_SAFE_ADDR, 0x12);

        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xFFFF), 2);

        assert_eq!(mapper.read_chr(0x0000), 1);
        assert_eq!(mapper.read_chr(0x1FFF), 1);
    }

    #[test]
    fn test_colordreams_register_layout_is_cccc_llpp() {
        // CCCC LLPP: CHR uses upper nibble, PRG uses low two bits

        let prg_rom = banked_prg_with_conflict_safe_write(4); // 4 PRG banks max
        let chr_rom = banked_data(8 * 1024, 16); // 16 CHR banks max

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        mapper.write_prg(BUS_CONFLICT_SAFE_ADDR, 0xE3); // CHR=14, lockout bits=0, PRG=3
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_chr(0x0000), 14);
    }

    #[test]
    fn test_colordreams_mirroring_is_fixed_from_header() {
        let prg_rom = banked_data(32 * 1024, 2);
        let chr_rom = banked_data(8 * 1024, 2);

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("ColorDreams (mapper 11) should be implemented");

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Bank select write should not affect mirroring.
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_colordreams_bank_wrapping() {
        // Test that bank selection wraps when ROM is smaller than max banks

        let prg_rom = banked_prg_with_conflict_safe_write(2); // 2 PRG banks available
        let chr_rom = banked_data(8 * 1024, 2); // Only 2 CHR banks

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        // PRG selects from bits 0-1. Bank 3 wraps to bank 1 (3 % 2 = 1).
        mapper.write_prg(BUS_CONFLICT_SAFE_ADDR, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 1);

        // CHR selects from bits 4-7. Bank 15 wraps to bank 1 (15 % 2 = 1).
        mapper.write_prg(BUS_CONFLICT_SAFE_ADDR, 0xF0);
        assert_eq!(mapper.read_chr(0x0000), 1);
    }

    #[test]
    fn test_colordreams_registers_snapshot_restores_banks() {
        let prg_rom = banked_prg_with_conflict_safe_write(4);
        let chr_rom = banked_data(8 * 1024, 4);

        let mut mapper = create_colordreams_mapper(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        )
        .expect("ColorDreams (mapper 11) should be implemented");

        // Select CHR bank 2 and PRG bank 3 (0b0010_0011 = 0x23)
        mapper.write_prg(BUS_CONFLICT_SAFE_ADDR, 0x23);

        let snapshot = mapper.registers_snapshot();

        let mut restored = create_colordreams_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_chr(0x0000), 2);
    }

    #[test]
    fn test_colordreams_banked_rom_replacement() {
        use crate::nes::cartridge::common::BankedRom;
        use crate::nes::cartridge::test_helpers::banked_data;

        const PRG_BANK_SIZE: usize = 32 * 1024;
        const CHR_BANK_SIZE: usize = 8 * 1024;

        // Create test ROM with distinct data per bank
        let prg_rom = banked_data(PRG_BANK_SIZE, 4);
        let chr_rom = banked_data(CHR_BANK_SIZE, 4);

        // Create BankedRom instances like the mapper would
        let prg_banked = BankedRom::new(prg_rom.clone(), PRG_BANK_SIZE);
        let chr_banked = BankedRom::new(chr_rom.clone(), CHR_BANK_SIZE);

        // Test reading from different banks
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(1, 0), 1);
        assert_eq!(prg_banked.read(2, 0), 2);
        assert_eq!(prg_banked.read(3, 0), 3);

        assert_eq!(chr_banked.read(0, 0), 0);
        assert_eq!(chr_banked.read(1, 0), 1);
        assert_eq!(chr_banked.read(2, 0), 2);
        assert_eq!(chr_banked.read(3, 0), 3);

        // Test bank wrapping for PRG (4 banks)
        assert_eq!(prg_banked.read(4, 0), 0); // wraps to bank 0
        assert_eq!(prg_banked.read(5, 0), 1); // wraps to bank 1
        assert_eq!(prg_banked.read(7, 0), 3); // wraps to bank 3
        assert_eq!(prg_banked.read(8, 0), 0); // wraps to bank 0
    }

    #[test]
    fn test_colordreams_open_bus() {
        let mapper = ColorDreamsMapper::new(
            MapperContext::new_for_test(
                11,
                vec![0; 128 * 1024],
                vec![0; 128 * 1024],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x55), 0x55);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x66), 0x66);
        assert_eq!(mapper.read_prg_open_bus(0x6000, 0x77), 0x77);
        assert_eq!(mapper.read_prg_open_bus(0x7FFF, 0x88), 0x88);
    }

    #[test]
    fn test_colordreams_reports_no_prg_ram() {
        let mapper = create_colordreams_mapper(
            vec![0; 128 * 1024],
            vec![0; 128 * 1024],
            NametableLayout::Horizontal,
        )
        .expect("ColorDreams (mapper 11) should be implemented");

        assert_eq!(mapper.wram_size(), 0);
        assert_eq!(mapper.capabilities().max_prg_ram_kb, 0);
    }

    #[test]
    fn test_colordreams_applies_bus_conflicts() {
        let mut prg_rom = vec![0; 4 * 32 * 1024];
        for byte in &mut prg_rom[32 * 1024..2 * 32 * 1024] {
            *byte = 0x01;
        }
        let chr_rom = banked_data(8 * 1024, 16);

        let mut mapper = create_colordreams_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("ColorDreams (mapper 11) should be implemented");

        // Initial bank at $8000 reads 0x00, so conflict masks write 0x01 -> 0x00.
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 0x00);
    }

    // ========================================================================
    // Submapper 1: No Bus Conflicts
    // ========================================================================

    fn create_colordreams_submapper1_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(
            MapperContext::new_for_test(11, prg_rom, chr_rom, mirroring)
                .with_submapper(1)
                .with_prg_ram_banks(0),
        )
    }

    #[test]
    fn test_colordreams_submapper1_does_not_have_bus_conflicts() {
        // Submapper 1 should NOT apply bus conflicts (Free Fall prototype variant)
        // Create ROM where each bank is filled with its bank number
        let mut prg_rom = banked_data(32 * 1024, 4);
        // Overwrite bank 0 address $0000 with 0x00 (where we'll write the bank select)
        prg_rom[0] = 0x00;
        let chr_rom = banked_data(8 * 1024, 16);

        let mut mapper =
            create_colordreams_submapper1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
                .expect("ColorDreams submapper 1 should be implemented");

        // Write 0x01 to select bank 1 at address $8000.
        // In submapper 0 with bus conflicts, this would AND with ROM value 0x00 → bank 0
        // In submapper 1 without bus conflicts, write value 0x01 is used directly → bank 1
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Submapper 1 should select bank 1 without bus conflicts"
        );
    }

    #[test]
    fn test_colordreams_submapper1_chr_banking_works_without_conflicts() {
        // Test that CHR banking also works without bus conflicts
        let prg_rom = vec![0xFF; 4 * 32 * 1024]; // All 0xFF to avoid PRG conflicts
        let chr_rom = banked_data(8 * 1024, 16);

        let mut mapper =
            create_colordreams_submapper1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
                .expect("ColorDreams submapper 1 should be implemented");

        // Select CHR bank 5 via bits 4-7: 0x50
        mapper.write_prg(0x8000, 0x50);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "Submapper 1 should select CHR bank 5"
        );

        // Select CHR bank 10 via bits 4-7: 0xA0
        mapper.write_prg(0x8000, 0xA0);
        assert_eq!(
            mapper.read_chr(0x0000),
            10,
            "Submapper 1 should select CHR bank 10"
        );
    }

    #[test]
    fn test_colordreams_submapper1_combined_prg_and_chr() {
        // Test combined PRG and CHR selection without bus conflicts
        let prg_rom = banked_data(32 * 1024, 4);
        let chr_rom = banked_data(8 * 1024, 16);

        let mut mapper =
            create_colordreams_submapper1_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
                .expect("ColorDreams submapper 1 should be implemented");

        // Select PRG bank 2 and CHR bank 7: 0b0111_0010 = 0x72
        mapper.write_prg(0x8000, 0x72);
        assert_eq!(mapper.read_prg(0x8000), 2, "Should select PRG bank 2");
        assert_eq!(mapper.read_chr(0x0000), 7, "Should select CHR bank 7");
    }
}
