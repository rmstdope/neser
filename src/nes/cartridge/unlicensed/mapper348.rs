//! Mapper 348 - BMC-830118-C (MMC3 variant)
//!
//! Specifications:
//! - Mesen reference: `Bmc830118C`
//! - Mapper family: MMC3 variant
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 348 - BMC-830118-C (MMC3 variant)
///
/// Additional register:
/// - `$6800-$68FF` write-only outer register `reg`
///   - `reg[3:2]` extends PRG/CHR bank selection.
///   - When `reg[3:2] == 0b11`, upper PRG windows are forced:
///     - `$C000-$DFFF`: `0x32 | (low_window0 & 0x0F)`
///     - `$E000-$FFFF`: `0x32 | (low_window1 & 0x0F)`
pub struct Mapper348 {
    mmc3: MMC3Mapper,
    outer_reg: u8,
}

impl Mapper348 {
    const PRG_BANK_SIZE: usize = 0x2000;
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;
    const SPECIAL_MODE_FIXED_PRG_BASE: usize = 0x32;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            outer_reg: 0,
        }
    }

    fn outer_bank_high_bits(&self) -> usize {
        (self.outer_reg & 0x0C) as usize
    }

    fn special_mode(&self) -> bool {
        self.outer_bank_high_bits() == 0x0C
    }

    fn apply_chr_outer_bits(&self, raw_1k_bank: usize) -> usize {
        (self.outer_bank_high_bits() << 5) | (raw_1k_bank & 0x7F)
    }

    fn apply_prg_outer_bits(&self, raw_8k_page: u8) -> usize {
        (self.outer_bank_high_bits() << 2) | (usize::from(raw_8k_page) & 0x0F)
    }

    fn forced_special_mode_prg_bank(&self, source_window_addr: u16) -> usize {
        let low_nibble = usize::from(self.mmc3.raw_prg_8k_page_number(source_window_addr)) & 0x0F;
        Self::SPECIAL_MODE_FIXED_PRG_BASE | low_nibble
    }

    fn mapped_chr_bank_and_offset(&self, addr: u16) -> (usize, usize) {
        let raw_bank = self.mmc3.raw_chr_1k_bank(addr);
        let bank = self.apply_chr_outer_bits(raw_bank);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        (bank, offset)
    }

    fn mapped_prg_bank_for_addr(&self, addr: u16) -> usize {
        if self.special_mode() {
            match addr {
                0x8000..=0x9FFF => {
                    self.apply_prg_outer_bits(self.mmc3.raw_prg_8k_page_number(0x8000))
                }
                0xA000..=0xBFFF => {
                    self.apply_prg_outer_bits(self.mmc3.raw_prg_8k_page_number(0xA000))
                }
                0xC000..=0xDFFF => self.forced_special_mode_prg_bank(0x8000),
                0xE000..=0xFFFF => self.forced_special_mode_prg_bank(0xA000),
                _ => 0,
            }
        } else {
            let raw = self.mmc3.raw_prg_8k_page_number(addr);
            self.apply_prg_outer_bits(raw)
        }
    }
}

impl Mapper for Mapper348 {
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
            return 0;
        }

        let bank = self.mapped_prg_bank_for_addr(addr);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6800..=0x68FF).contains(&addr) {
            self.outer_reg = value;
        } else if (0x8000..=0xFFFF).contains(&addr) {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn mapper_number(&self) -> u16 {
        348
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let (bank, offset) = self.mapped_chr_bank_and_offset(addr);
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let (bank, offset) = self.mapped_chr_bank_and_offset(addr);
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.outer_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&outer, mmc3_data)) = data.split_last() {
            self.outer_reg = outer;
            self.mmc3.restore_registers(mmc3_data);
        }
    }

    fn reset(&mut self) {
        self.outer_reg = 0;
        self.mmc3.reset();
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
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS_8K: usize = 64;
    const CHR_BANKS_1K: usize = 256;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            348,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("Mapper 348 should be implemented")
    }

    #[test]
    fn mapper_348_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            348,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 348 must be registered in the mapper factory"
        );
    }

    #[test]
    fn outer_register_bits_2_3_extend_chr_bank_selection() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x01);
        let chr_before = mapper.read_chr(0x0000);

        mapper.write_prg(0x6800, 0x04);
        let chr_after = mapper.read_chr(0x0000);

        assert_ne!(
            chr_before, chr_after,
            "Writing $6800 with bit2 set should change CHR bank high bits"
        );
    }

    #[test]
    fn special_mode_reg_0x0c_forces_upper_prg_windows_from_0x32_or_low_nibble() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x6800, 0x0C);

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x09);
        mapper.write_prg(0x8000, 0x07);
        mapper.write_prg(0x8001, 0x03);

        let bank_low_0 = mapper.read_prg(0x8000);
        let bank_low_1 = mapper.read_prg(0xA000);
        let bank_high_0 = mapper.read_prg(0xC000);
        let bank_high_1 = mapper.read_prg(0xE000);

        assert_eq!(bank_high_0, 0x32 | (bank_low_0 & 0x0F));
        assert_eq!(bank_high_1, 0x32 | (bank_low_1 & 0x0F));
    }

    #[test]
    fn mirroring_and_capabilities_follow_mmc3_expectations() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xA000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        let caps = mapper.capabilities();
        assert!(caps.has_irq);
        assert!(caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
        assert!(!caps.has_expansion_audio);
    }

    #[test]
    fn writes_to_6000_region_except_6800_68ff_do_not_change_outer_register() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x6800, 0x00);
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x01);
        let baseline = mapper.read_chr(0x0000);

        mapper.write_prg(0x6000, 0x04);
        mapper.write_prg(0x67FF, 0x04);
        mapper.write_prg(0x6900, 0x04);
        mapper.write_prg(0x7FFF, 0x04);

        assert_eq!(mapper.read_chr(0x0000), baseline);

        mapper.write_prg(0x6800, 0x04);
        assert_ne!(mapper.read_chr(0x0000), baseline);
    }
}
