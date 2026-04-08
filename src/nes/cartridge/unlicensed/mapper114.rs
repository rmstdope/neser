//! Mapper 114 - MMC3-based clone with register scrambling.
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_114>
//! - Mesen reference: `MMC3_114`

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

pub struct Mapper114 {
    mmc3: MMC3Mapper,
    outer_reg: u8,
    pending_bank_data_write: bool,
}

impl Mapper114 {
    const MAPPER_NUMBER: u16 = 114;
    const PRG_BANK_SIZE: usize = 0x2000;
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400;
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;
    const SECURITY: [u8; 8] = [0, 3, 1, 5, 6, 7, 2, 4];
    const BANK_SELECT_MODE_BITS_MASK: u8 = 0xC0;
    const SECURITY_INDEX_MASK: u8 = 0x07;
    const USE_ALTERNATE_IRQ: bool = false;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(
                ctx.prg_rom,
                ctx.chr_rom,
                ctx.mirroring,
                Self::USE_ALTERNATE_IRQ,
            ),
            outer_reg: 0,
            pending_bank_data_write: false,
        }
    }

    fn mapped_prg_bank(&self, addr: u16) -> usize {
        if (self.outer_reg & 0x80) == 0 {
            return self.mmc3.mapped_prg_bank(addr);
        }

        let base = ((self.outer_reg & 0x0F) as usize) << 1;
        let slot = ((addr as usize).saturating_sub(0x8000) >> 13) & 0x03;
        base | (slot & 0x01)
    }
}

impl Mapper for Mapper114 {
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
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let bank = self.mapped_prg_bank(addr);
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Outer bank register lives in the $5000–$5FFF range.
        if (0x5000..=0x5FFF).contains(&addr) {
            self.outer_reg = value;
            return;
        }

        // Delegate WRAM/PRG-RAM writes to the underlying MMC3 in the $6000–$7FFF window.
        if (0x6000..=0x7FFF).contains(&addr) {
            self.mmc3.write_prg(addr, value);
            return;
        }
        match addr & 0xE001 {
            0x8001 => self.mmc3.write_prg(0xA000, value),
            0xA000 => {
                let translated = (value & Self::BANK_SELECT_MODE_BITS_MASK)
                    | Self::SECURITY[(value & Self::SECURITY_INDEX_MASK) as usize];
                self.mmc3.write_prg(0x8000, translated);
                self.pending_bank_data_write = true;
            }
            0xA001 => self.mmc3.write_prg(0xC000, value),
            0xC000 => {
                if self.pending_bank_data_write {
                    self.pending_bank_data_write = false;
                    self.mmc3.write_prg(0x8001, value);
                }
            }
            0xC001 => self.mmc3.write_prg(0xC001, value),
            0xE000 => self.mmc3.write_prg(0xE000, value),
            0xE001 => self.mmc3.write_prg(0xE001, value),
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.mmc3.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.mmc3.mapped_chr_1k_bank(addr);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.outer_reg = 0;
        self.pending_bank_data_write = false;
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = self.mmc3.registers_snapshot();
        snapshot.push(self.outer_reg);
        snapshot.push(self.pending_bank_data_write as u8);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }
        let (mmc3_data, tail) = data.split_at(data.len() - 2);
        self.mmc3.restore_registers(mmc3_data);
        self.outer_reg = tail[0];
        self.pending_bank_data_write = tail[1] != 0;
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.mmc3.capabilities()
    }

    fn wram_size(&self) -> usize {
        self.mmc3.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.mmc3.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.mmc3.load_wram_snapshot(data);
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS_8K: usize = 48;
    const CHR_BANKS_1K: usize = 96;

    fn make_mapper(submapper: u8) -> Box<dyn Mapper> {
        create_mapper(
            MapperContext::new_for_test(
                114,
                banked_data(8 * 1024, PRG_BANKS_8K),
                banked_data(1024, CHR_BANKS_1K),
                NametableLayout::Horizontal,
            )
            .with_submapper(submapper),
        )
        .expect("Mapper 114 should be implemented")
    }

    #[test]
    fn mapper_114_is_registered_for_submapper_0_and_1() {
        for submapper in [0, 1] {
            let result = create_mapper(
                MapperContext::new_for_test(
                    114,
                    banked_data(8 * 1024, PRG_BANKS_8K),
                    banked_data(1024, CHR_BANKS_1K),
                    NametableLayout::Horizontal,
                )
                .with_submapper(submapper),
            );

            assert!(
                result.is_ok(),
                "Mapper 114 should be creatable for submapper {}",
                submapper
            );
        }
    }

    #[test]
    fn mapper_114_prg_chr_mirroring_and_irq_follow_expected_behavior() {
        for submapper in [0, 1] {
            let mut mapper = make_mapper(submapper);

            // Mapper 114 uses a split bank-write flow:
            // $A000 updates MMC3 bank-select (with low-bit security scramble),
            // then the following $C000 writes the bank data once.
            mapper.write_prg(0xA000, 0x04);
            mapper.write_prg(0xC000, 0x05);
            assert_eq!(mapper.read_prg(0x8000), 5);

            mapper.write_prg(0xA000, 0x06);
            mapper.write_prg(0xC000, 0x09);
            assert_eq!(mapper.read_chr(0x1000), 9);

            mapper.write_prg(0x8001, 0x01);
            assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
            mapper.write_prg(0x8001, 0x00);
            assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

            mapper.write_prg(0xA001, 1);
            mapper.write_prg(0xC001, 0);
            mapper.write_prg(0xE001, 0);
            for _ in 0..2 {
                mapper.ppu_address_changed(0x0FFF);
                for _ in 0..3 {
                    mapper.cpu_cycle();
                }
                mapper.ppu_address_changed(0x1000);
            }
            assert!(mapper.irq_pending());
        }
    }

    #[test]
    fn mapper_114_outer_reg_override_controls_prg_windows() {
        for submapper in [0, 1] {
            let mut mapper = make_mapper(submapper);

            // Baseline MMC3 mapping (bit7 clear): select MMC3 R6 via mapper-114's $A000 path.
            mapper.write_prg(0xA000, 0x04);
            mapper.write_prg(0xC000, 0x07);
            assert_eq!(
                mapper.read_prg(0x8000),
                7,
                "R6=7 should map $8000 before outer override is enabled"
            );
            assert_eq!(mapper.read_prg(0xA000), 0);
            assert_eq!(mapper.read_prg(0xC000), 46);
            assert_eq!(mapper.read_prg(0xE000), 47);

            // Override mode (0x83): bit7 enables override, low nibble 0x03 selects pair 6/7.
            mapper.write_prg(0x5000, 0x83);
            assert_eq!(mapper.read_prg(0x8000), 6);
            assert_eq!(mapper.read_prg(0xA000), 7);
            assert_eq!(mapper.read_prg(0xC000), 6);
            assert_eq!(mapper.read_prg(0xE000), 7);

            // Clearing bit7 restores MMC3-selected mapping.
            mapper.write_prg(0x5000, 0x00);
            assert_eq!(mapper.read_prg(0x8000), 7);
            assert_eq!(mapper.read_prg(0xA000), 0);
            assert_eq!(mapper.read_prg(0xC000), 46);
            assert_eq!(mapper.read_prg(0xE000), 47);
        }
    }

    #[test]
    fn mapper_114_wram_window_writes_are_forwarded_to_mmc3() {
        for submapper in [0, 1] {
            let mut mapper = make_mapper(submapper);
            mapper.write_prg(0x6000, 0x5A);
            assert_eq!(mapper.read_prg_open_bus(0x6000, 0x00), 0x5A);
        }
    }
}
