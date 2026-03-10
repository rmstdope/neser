//! Mapper 116 - Someri Team / Whirlwind Manu multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_116>
//! - Mesen reference implementation: `Core/NES/Mappers/Unlicensed/Mapper116.h`
//!
//! Known Limitations:
//! - MMC1 consecutive-cycle serial-write ignore behavior is not modeled here because
//!   mapper 116 clone hardware behavior differs from Nintendo MMC1 timing.
//! - Submapper variants currently share the same behavior path.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

pub struct Mapper116 {
    mmc3: MMC3Mapper,
    mode: u8,

    vrc2_chr: [u8; 8],
    vrc2_prg: [u8; 2],
    vrc2_mirroring: u8,
    mmc3_mirroring: u8,

    mmc1_regs: [u8; 4],
    mmc1_buffer: u8,
    mmc1_shift: u8,
}

impl Mapper116 {
    const MODE_SELECT_MASK: u16 = 0x4100;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mmc3_mirroring = u8::from(matches!(ctx.mirroring, NametableLayout::Horizontal));
        let mut mapper = Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            mode: 0,
            vrc2_chr: [0xFF, 0xFF, 0xFF, 0xFF, 4, 5, 6, 7],
            vrc2_prg: [0, 1],
            vrc2_mirroring: 0,
            mmc3_mirroring,
            mmc1_regs: [0x0C, 0, 0, 0],
            mmc1_buffer: 0,
            mmc1_shift: 0,
        };
        mapper.update_state();
        mapper
    }

    fn active_mode(&self) -> u8 {
        self.mode & 0x03
    }

    fn outer_chr_bank(&self) -> u16 {
        if (self.mode & 0x04) != 0 { 0x100 } else { 0 }
    }

    fn update_state(&mut self) {
        self.update_prg();
        self.update_chr();
        self.update_mirroring();
    }

    fn update_prg(&mut self) {
        match self.active_mode() {
            0 => {
                let prg0 = self.vrc2_prg[0] as i16;
                let prg1 = self.vrc2_prg[1] as i16;
                self.base_mut().select_prg_page(0, prg0);
                self.base_mut().select_prg_page(1, prg1);
                self.base_mut().select_prg_page(2, -2);
                self.base_mut().select_prg_page(3, -1);
            }
            1 => {
                let prg_bank_8000 = self.mmc3.mapped_prg_bank(0x8000) as i16;
                let prg_bank_a000 = self.mmc3.mapped_prg_bank(0xA000) as i16;
                let prg_bank_c000 = self.mmc3.mapped_prg_bank(0xC000) as i16;
                let prg_bank_e000 = self.mmc3.mapped_prg_bank(0xE000) as i16;
                self.base_mut().select_prg_page(0, prg_bank_8000);
                self.base_mut().select_prg_page(1, prg_bank_a000);
                self.base_mut().select_prg_page(2, prg_bank_c000);
                self.base_mut().select_prg_page(3, prg_bank_e000);
            }
            _ => {
                let bank = (self.mmc1_regs[3] & 0x0F) as i16;
                if (self.mmc1_regs[0] & 0x08) != 0 {
                    if (self.mmc1_regs[0] & 0x04) != 0 {
                        let lo = bank << 1;
                        self.base_mut().select_prg_page(0, lo);
                        self.base_mut().select_prg_page(1, lo + 1);
                        self.base_mut().select_prg_page(2, -2);
                        self.base_mut().select_prg_page(3, -1);
                    } else {
                        let hi = bank << 1;
                        self.base_mut().select_prg_page(0, 0);
                        self.base_mut().select_prg_page(1, 1);
                        self.base_mut().select_prg_page(2, hi);
                        self.base_mut().select_prg_page(3, hi + 1);
                    }
                } else {
                    let start = (bank & 0x0E) << 1;
                    self.base_mut().select_prg_page(0, start);
                    self.base_mut().select_prg_page(1, start + 1);
                    self.base_mut().select_prg_page(2, start + 2);
                    self.base_mut().select_prg_page(3, start + 3);
                }
            }
        }
    }

    fn update_chr(&mut self) {
        let outer = self.outer_chr_bank() as i16;
        match self.active_mode() {
            0 => {
                let chr = self.vrc2_chr;
                for (slot, bank) in chr.iter().enumerate() {
                    self.base_mut()
                        .select_chr_page(slot, outer | (*bank as i16));
                }
            }
            1 => {
                for slot in 0..8 {
                    let slot_base_addr = (slot as u16) * 0x0400;
                    let bank = self.mmc3.mapped_chr_1k_bank(slot_base_addr) as i16;
                    self.base_mut().select_chr_page(slot, outer | bank);
                }
            }
            _ => {
                if (self.mmc1_regs[0] & 0x10) != 0 {
                    let lo = (self.mmc1_regs[1] as i16) << 2;
                    let hi = (self.mmc1_regs[2] as i16) << 2;
                    for offset in 0..4 {
                        self.base_mut()
                            .select_chr_page(offset, outer | (lo + offset as i16));
                        self.base_mut()
                            .select_chr_page(offset + 4, outer | (hi + offset as i16));
                    }
                } else {
                    let start = ((self.mmc1_regs[1] as i16) & 0x1E) << 2;
                    for offset in 0..8 {
                        self.base_mut()
                            .select_chr_page(offset, outer | (start + offset as i16));
                    }
                }
            }
        }
    }

    fn update_mirroring(&mut self) {
        match self.active_mode() {
            0 => {
                let mirror_h = (self.vrc2_mirroring & 0x01) != 0;
                self.base_mut().set_mirroring_hv(mirror_h)
            }
            1 => {
                let mirror_h = (self.mmc3_mirroring & 0x01) != 0;
                self.base_mut().set_mirroring_hv(mirror_h)
            }
            _ => match self.mmc1_regs[0] & 0x03 {
                0 => self
                    .base_mut()
                    .set_mirroring(NametableLayout::SingleScreenLower),
                1 => self
                    .base_mut()
                    .set_mirroring(NametableLayout::SingleScreenUpper),
                2 => self.base_mut().set_mirroring(NametableLayout::Vertical),
                _ => self.base_mut().set_mirroring(NametableLayout::Horizontal),
            },
        }
    }

    fn write_mode_select(&mut self, addr: u16, value: u8) {
        self.mode = value;
        if (addr & 0x01) != 0 {
            self.mmc1_regs[0] = 0x0C;
            self.mmc1_regs[3] = 0;
            self.mmc1_buffer = 0;
            self.mmc1_shift = 0;
        }
        self.update_state();
    }

    fn write_vrc2_register(&mut self, addr: u16, value: u8) {
        if (0xB000..=0xE003).contains(&addr) {
            let reg_index = ((((addr & 0x02) | (addr >> 10)) >> 1) + 2) & 0x07;
            if (addr & 0x01) == 0 {
                self.vrc2_chr[reg_index as usize] =
                    (self.vrc2_chr[reg_index as usize] & 0xF0) | (value & 0x0F);
            } else {
                self.vrc2_chr[reg_index as usize] =
                    (self.vrc2_chr[reg_index as usize] & 0x0F) | ((value & 0x0F) << 4);
            }
            self.update_chr();
            return;
        }

        match addr & 0xF000 {
            0x8000 => {
                self.vrc2_prg[0] = value;
                self.update_prg();
            }
            0x9000 => {
                self.vrc2_mirroring = value;
                self.update_mirroring();
            }
            0xA000 => {
                self.vrc2_prg[1] = value;
                self.update_prg();
            }
            _ => {}
        }
    }

    fn write_mmc1_register(&mut self, addr: u16, value: u8) {
        if (value & 0x80) != 0 {
            self.mmc1_regs[0] |= 0x0C;
            self.mmc1_buffer = 0;
            self.mmc1_shift = 0;
            self.update_state();
            return;
        }

        let reg_index = (addr >> 13).saturating_sub(4) as usize;
        self.mmc1_buffer |= (value & 0x01) << self.mmc1_shift;
        self.mmc1_shift += 1;

        if self.mmc1_shift == 5 {
            if reg_index < self.mmc1_regs.len() {
                self.mmc1_regs[reg_index] = self.mmc1_buffer;
            }
            self.mmc1_buffer = 0;
            self.mmc1_shift = 0;
            self.update_state();
        }
    }
}

impl Mapper for Mapper116 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mapper_number(&self) -> u16 {
        116
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            if (addr & Self::MODE_SELECT_MASK) == Self::MODE_SELECT_MASK {
                self.write_mode_select(addr, value);
            } else if (0x6000..=0x7FFF).contains(&addr) {
                self.mmc3.write_prg(addr, value);
            }
            return;
        }

        match self.active_mode() {
            0 => self.write_vrc2_register(addr, value),
            1 => {
                if (addr & 0xE001) == 0xA000 {
                    self.mmc3_mirroring = value;
                }
                self.mmc3.write_prg(addr, value)
            }
            _ => self.write_mmc1_register(addr, value),
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if self.active_mode() == 1 {
            return self.mmc3.read_prg(addr);
        }

        if let Some(value) = self.base().try_read_prg_6000(addr) {
            return value;
        }
        if let Some(value) = self.base().try_read_prg_ram(addr) {
            return value;
        }
        match addr {
            0x8000..=0xFFFF => self.base().read_prg_rom(addr),
            _ => 0,
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if self.active_mode() != 1 {
            return self.base().read_chr(addr);
        }

        let mmc3_bank = self.mmc3.mapped_chr_1k_bank(addr) as u16;
        let final_bank = self.outer_chr_bank() | mmc3_bank;
        let offset = (addr as usize) & 0x03FF;
        self.mmc3.read_chr_1k_at(final_bank as usize, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.active_mode() != 1 {
            self.base_mut().write_chr(addr, value);
            return;
        }

        let mmc3_bank = self.mmc3.mapped_chr_1k_bank(addr) as u16;
        let final_bank = self.outer_chr_bank() | mmc3_bank;
        let offset = (addr as usize) & 0x03FF;
        self.mmc3
            .write_chr_1k_at(final_bank as usize, offset, value);
    }

    fn irq_pending(&self) -> bool {
        self.active_mode() == 1 && self.mmc3.irq_pending()
    }

    fn cpu_cycle(&mut self) {
        if self.active_mode() == 1 {
            self.mmc3.cpu_cycle();
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        if self.active_mode() == 1 {
            self.mmc3.ppu_address_changed(addr);
        }
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        self.mode = 0;
        self.vrc2_chr = [0xFF, 0xFF, 0xFF, 0xFF, 4, 5, 6, 7];
        self.vrc2_prg = [0, 1];
        self.vrc2_mirroring = 0;
        self.mmc3_mirroring = 0;
        self.mmc1_regs = [0x0C, 0, 0, 0];
        self.mmc1_buffer = 0;
        self.mmc1_shift = 0;
        self.update_state();
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
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

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut out = vec![self.mode, self.vrc2_mirroring, self.mmc3_mirroring];
        out.extend_from_slice(&self.vrc2_prg);
        out.extend_from_slice(&self.vrc2_chr);
        out.extend_from_slice(&self.mmc1_regs);
        out.push(self.mmc1_buffer);
        out.push(self.mmc1_shift);
        out.extend(self.mmc3.registers_snapshot());
        out
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 19 {
            return;
        }
        self.mode = data[0];
        self.vrc2_mirroring = data[1];
        self.mmc3_mirroring = data[2];
        self.vrc2_prg.copy_from_slice(&data[3..5]);
        self.vrc2_chr.copy_from_slice(&data[5..13]);
        self.mmc1_regs.copy_from_slice(&data[13..17]);
        self.mmc1_buffer = data[17];
        self.mmc1_shift = data[18];
        self.mmc3.restore_registers(&data[19..]);
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const MAPPER_ID: u16 = 116;
    // Non-power-of-two sizes avoid modulo-wrapping false positives in bank assertions.
    const PRG_BANKS_8K: usize = 37;
    const CHR_BANKS_1K: usize = 385;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS_8K);
        let chr = banked_data(1024, CHR_BANKS_1K);
        create_mapper(MapperContext::new_for_test(
            MAPPER_ID,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 116 should be implemented")
    }

    fn write_mmc1_register<M: Mapper + ?Sized>(mapper: &mut M, addr: u16, value: u8) {
        for shift in 0..5 {
            let bit = (value >> shift) & 0x01;
            mapper.write_prg(addr, bit);
        }
    }

    #[test]
    fn mapper_116_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_ID,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 116 must be registered in the factory"
        );
    }

    #[test]
    fn vrc2_mode_controls_prg_chr_and_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0xA000, 5);
        mapper.write_prg(0xB000, 0x0A);
        mapper.write_prg(0xB001, 0x03);
        mapper.write_prg(0x9000, 1);

        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xA000), 5);
        assert_eq!(mapper.read_chr(0x0000), 0x3A);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mmc3_mode_controls_prg_mirroring_and_irq() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x01);

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 7);
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.read_prg(0x8000), 7);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE001, 0);

        mapper.ppu_address_changed(0x0FFF);
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        mapper.ppu_address_changed(0x1000);
        assert!(!mapper.irq_pending());

        mapper.ppu_address_changed(0x0FFF);
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        mapper.ppu_address_changed(0x1000);
        assert!(mapper.irq_pending());
    }

    #[test]
    fn mmc3_mode_reapplies_prg_and_mirroring_on_mode_entry_without_new_mmc3_writes() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4100, 0x01);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0xA000, 1);

        mapper.write_prg(0x4100, 0x00);
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0x9000, 0);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x4100, 0x01);

        assert_eq!(
            mapper.read_prg(0x8000),
            9,
            "MMC3 PRG mapping should be restored immediately on mode entry"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "MMC3 mirroring should be restored immediately on mode entry"
        );
    }

    #[test]
    fn wram_read_write_is_routed_to_mapper_prg_ram() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x01);

        mapper.write_prg(0x6000, 0x5A);
        mapper.write_prg(0x6E00, 0xA5);

        assert_eq!(mapper.read_prg(0x6000), 0x5A);
        assert_eq!(mapper.read_prg(0x6E00), 0xA5);
    }

    #[test]
    fn mmc1_mode_controls_prg_chr_and_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4100, 0x02);

        write_mmc1_register(&mut *mapper, 0x8000, 0x0F);
        write_mmc1_register(&mut *mapper, 0xA000, 0x08);
        write_mmc1_register(&mut *mapper, 0xE000, 0x02);

        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_chr(0x0000), 0x20);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mapper_116_submapper_2_is_accepted() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_ID,
                banked_data(8 * 1024, PRG_BANKS_8K),
                banked_data(1024, CHR_BANKS_1K),
                NametableLayout::Horizontal,
            )
            .with_submapper(2),
        );
        assert!(result.is_ok(), "Mapper 116 submapper 2 should be creatable");
    }
}
