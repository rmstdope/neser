//! Mapper 047 - MMC3 multicart (Super Spike V'Ball + Nintendo World Cup)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_047>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 047 - MMC3-based 2-in-1 multicart
///
/// Hardware: MMC3 with a single outer block register
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_047>
/// - PRG-ROM: 256 KiB (2 × 128 KiB blocks)
/// - CHR: 256 KiB (2 × 128 KiB blocks)
/// - No PRG-RAM ($6000-$7FFF is the outer block register)
///
/// Block register ($6000-$7FFF): [.... ...B]
/// - Writable only when MMC3's PRG-RAM is enabled and write-protected is clear ($A001)
/// - B selects one of two 128 KiB blocks for both PRG and CHR
///
/// Bank mapping within a block:
/// - PRG bank = (B << 4) | (MMC3_bank & 0x0F)
/// - CHR 1KB bank = (B << 7) | (MMC3_1k_bank & 0x7F)
///
/// Known games: Super Spike V'Ball + Nintendo World Cup
pub struct Mapper47 {
    pub(crate) mmc3: MMC3Mapper,
    block: u8,
}

impl Mapper47 {
    const MAPPER_NUMBER: u8 = 47;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB (same as MMC3)
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400; // 1 KiB (same as MMC3)
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            block: 0,
        }
    }

    fn apply_prg_block(&self, raw_bank: usize) -> usize {
        (raw_bank & 0x0F) | ((self.block as usize) << 4)
    }

    fn apply_chr_block(&self, raw_bank: usize) -> usize {
        (raw_bank & 0x7F) | ((self.block as usize) << 7)
    }
}

impl Mapper for Mapper47 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0; // No PRG-RAM
        }
        let raw_bank = self.mmc3.mapped_prg_bank(addr);
        let final_bank = self.apply_prg_block(raw_bank);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(final_bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            if self.mmc3.is_prg_ram_writable() {
                self.block = value & 0x01;
            }
        } else {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let raw_bank = self.mmc3.mapped_chr_1k_bank(addr);
        let final_bank = self.apply_chr_block(raw_bank);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(final_bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let raw_bank = self.mmc3.mapped_chr_1k_bank(addr);
        let final_bank = self.apply_chr_block(raw_bank);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(final_bank, offset, value);
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0 // No PRG-RAM; $6000-$7FFF is the outer block register
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.block);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&block, mmc3_data)) = data.split_last() {
            self.block = block & 0x01;
            self.mmc3.restore_registers(mmc3_data);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 2 blocks × 16 PRG banks = 32 banks × 8 KiB = 256 KiB PRG
    // 2 blocks × 128 CHR 1K banks = 256 CHR 1K banks = 256 KiB CHR
    const PRG_BANKS: usize = 32;
    const CHR_1K_BANKS: usize = 256;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        create_mapper(MapperContext::new_for_test(
            47,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 47 should be implemented")
    }

    // --- Factory ---

    #[test]
    fn mapper_47_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            47,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 47 must be registered in the factory"
        );
    }

    // --- PRG block selection ---

    #[test]
    fn prg_defaults_to_block_0() {
        // Bank at $E000 (fixed last) in block 0 = bank 15 (= 0x0F)
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            15,
            "Fixed last bank in block 0 must be bank 15"
        );
    }

    #[test]
    fn prg_block1_shifts_fixed_last_bank_by_16() {
        // Fixed last bank in block 1 = bank 31 (0x1F)
        let mut mapper = make_mapper();
        // Enable PRG-RAM write in MMC3 first ($A001 = 0x80)
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x01); // select block 1
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "Fixed last bank in block 1 must be bank 31"
        );
    }

    #[test]
    fn prg_mmc3_r6_banking_within_block0() {
        let mut mapper = make_mapper();
        // Select register R6, set to bank 3 → $8000 reads bank 3 (block 0)
        mapper.write_prg(0x8000, 0b0000_0110); // bank_select = R6, mode 0
        mapper.write_prg(0x8001, 3);
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn prg_mmc3_r6_banking_within_block1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80); // enable PRG-RAM write
        mapper.write_prg(0x6000, 0x01); // block 1
        mapper.write_prg(0x8000, 0b0000_0110); // R6
        mapper.write_prg(0x8001, 3); // R6 = 3 → final = (3 & 0x0F) | 0x10 = 19
        assert_eq!(
            mapper.read_prg(0x8000),
            19,
            "Block 1 R6=3 must map to bank 19"
        );
    }

    #[test]
    fn block_reg_not_writable_when_prg_ram_disabled() {
        let mut mapper = make_mapper();
        // $A001 = 0x00 → PRG-RAM disabled
        mapper.write_prg(0xA001, 0x00);
        mapper.write_prg(0x6000, 0x01); // should be ignored
        assert_eq!(
            mapper.read_prg(0xE000),
            15,
            "Block register must not be writable when PRG-RAM is disabled"
        );
    }

    #[test]
    fn block_reg_not_writable_when_prg_ram_write_protected() {
        let mut mapper = make_mapper();
        // $A001 = 0xC0 → PRG-RAM enabled but write-protected
        mapper.write_prg(0xA001, 0xC0);
        mapper.write_prg(0x6000, 0x01);
        assert_eq!(
            mapper.read_prg(0xE000),
            15,
            "Block register must not be writable when PRG-RAM is write-protected"
        );
    }

    // --- CHR block selection ---

    #[test]
    fn chr_defaults_to_block_0() {
        let mut mapper = make_mapper();
        // Set R2 = 5 → CHR bank at $1000 = 5 in block 0
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_chr(0x1000), 5);
    }

    #[test]
    fn chr_block1_shifts_bank_by_128() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80); // enable PRG-RAM write
        mapper.write_prg(0x6000, 0x01); // block 1
        // R2 = 5 → CHR bank = (5 & 0x7F) | 0x80 = 133
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5);
        assert_eq!(
            mapper.read_chr(0x1000),
            133,
            "Block 1 CHR R2=5 must map to CHR bank 133"
        );
    }

    // --- IRQ delegation ---

    #[test]
    fn irq_works_via_mmc3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1); // latch = 1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable IRQ

        // Two A12 rising edges with 3 CPU cycles low each
        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 47");
    }
}
