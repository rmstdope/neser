//! Mapper 014 - SL1632 (MMC3/VRC2 hybrid)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_014>
//! - Related behavior source: <https://www.nesdev.org/wiki/INES_Mapper_116>
//!
//! Known Limitations:
//! - CHR outer-bit behavior in MMC3 mode follows widely used emulator behavior,
//!   but some hardware edge cases remain unverified.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

pub struct Mapper14 {
    mmc3: MMC3Mapper,
    mode: u8,
    vrc_prg_regs: [u8; 2],
    vrc_chr_regs: [u8; 8],
    vrc_mirroring: u8,
}

impl Mapper14 {
    const SUPERVISOR_REGISTER: u16 = 0xA131;

    const MODE_MMC3: u8 = 0x02;
    const CHR_OUTER_LOW_HALF: u8 = 0x08;
    const CHR_OUTER_MID_HALF: u8 = 0x20;
    const CHR_OUTER_HIGH_HALF: u8 = 0x80;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mut mapper = Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            mode: 0,
            vrc_prg_regs: [0; 2],
            vrc_chr_regs: [0; 8],
            vrc_mirroring: 0,
        };
        mapper.update_vrc_state();
        mapper
    }

    fn in_mmc3_mode(&self) -> bool {
        (self.mode & Self::MODE_MMC3) != 0
    }

    fn update_vrc_state(&mut self) {
        let prg0 = self.vrc_prg_regs[0] as i16;
        let prg1 = self.vrc_prg_regs[1] as i16;
        let chr = self.vrc_chr_regs;
        let mirror_h = (self.vrc_mirroring & 0x01) != 0;

        let base = self.base_mut();
        base.select_prg_page(0, prg0);
        base.select_prg_page(1, prg1);
        base.select_prg_page(2, -2);
        base.select_prg_page(3, -1);

        for (slot, bank) in chr.iter().enumerate() {
            base.select_chr_page(slot, *bank as i16);
        }

        base.set_mirroring_hv(mirror_h);
    }

    fn vrc_chr_outer_bank(&self, addr: u16) -> usize {
        let bit = if addr < 0x1000 {
            Self::CHR_OUTER_LOW_HALF
        } else if addr < 0x1800 {
            Self::CHR_OUTER_MID_HALF
        } else {
            Self::CHR_OUTER_HIGH_HALF
        };

        if (self.mode & bit) != 0 { 0x100 } else { 0 }
    }

    fn vrc_chr_reg_index(addr: u16) -> usize {
        debug_assert!((0xB000..=0xEFFF).contains(&addr));
        let upper = ((addr >> 12) & 0x07) as usize;
        let low = ((addr >> 1) & 0x01) as usize;
        ((upper - 3) << 1) + low
    }
}

impl Mapper for Mapper14 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr == Self::SUPERVISOR_REGISTER {
            self.mode = value;
        }

        if self.in_mmc3_mode() {
            self.mmc3.write_prg(addr, value);
            return;
        }

        if (0xB000..=0xEFFF).contains(&addr) {
            let reg = Self::vrc_chr_reg_index(addr);
            let low_nibble = (addr & 0x01) == 0;
            if low_nibble {
                self.vrc_chr_regs[reg] = (self.vrc_chr_regs[reg] & 0xF0) | (value & 0x0F);
            } else {
                self.vrc_chr_regs[reg] = (self.vrc_chr_regs[reg] & 0x0F) | ((value & 0x0F) << 4);
            }
        } else {
            match addr & 0xF003 {
                0x8000 => self.vrc_prg_regs[0] = value,
                0x9000 => self.vrc_mirroring = value,
                0xA000 => self.vrc_prg_regs[1] = value,
                _ => {}
            }
        }

        self.update_vrc_state();
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if self.in_mmc3_mode() {
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
        if !self.in_mmc3_mode() {
            return self.base().read_chr(addr);
        }

        let mmc3_bank = self.mmc3.mapped_chr_1k_bank(addr);
        let final_bank = self.vrc_chr_outer_bank(addr) | mmc3_bank;
        let offset = (addr as usize) & 0x03FF;
        self.mmc3.read_chr_1k_at(final_bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.in_mmc3_mode() {
            self.base_mut().write_chr(addr, value);
            return;
        }

        let mmc3_bank = self.mmc3.mapped_chr_1k_bank(addr);
        let final_bank = self.vrc_chr_outer_bank(addr) | mmc3_bank;
        let offset = (addr as usize) & 0x03FF;
        self.mmc3.write_chr_1k_at(final_bank, offset, value);
    }

    fn irq_pending(&self) -> bool {
        self.in_mmc3_mode() && self.mmc3.irq_pending()
    }

    fn cpu_cycle(&mut self) {
        if self.in_mmc3_mode() {
            self.mmc3.cpu_cycle();
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        if self.in_mmc3_mode() {
            self.mmc3.ppu_address_changed(addr);
        }
    }

    fn reset(&mut self) {
        self.mode = 0;
        self.vrc_prg_regs = [0; 2];
        self.vrc_chr_regs = [0; 8];
        self.vrc_mirroring = 0;
        self.update_vrc_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut out = vec![self.mode, self.vrc_mirroring];
        out.extend_from_slice(&self.vrc_prg_regs);
        out.extend_from_slice(&self.vrc_chr_regs);
        out.extend(self.mmc3.registers_snapshot());
        out
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 12 {
            self.mode = data[0];
            self.vrc_mirroring = data[1];
            self.vrc_prg_regs.copy_from_slice(&data[2..4]);
            self.vrc_chr_regs.copy_from_slice(&data[4..12]);
            self.mmc3.restore_registers(&data[12..]);
            if !self.in_mmc3_mode() {
                self.update_vrc_state();
            }
        }
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
        8 * 1024
    }

    fn mapper_number(&self) -> u8 {
        14
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper14;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::{banked_data, banked_data_with_upper_marker};

    const PRG_BANKS_8K: usize = 41;
    const CHR_BANKS_1K: usize = 513;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS_8K);
        let chr = banked_data(1024, CHR_BANKS_1K);
        create_mapper(MapperContext::new_for_test(
            14,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 14 should be implemented")
    }

    fn make_mapper_ext() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS_8K);
        let chr = banked_data_with_upper_marker(1024, CHR_BANKS_1K);
        create_mapper(MapperContext::new_for_test(
            14,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 14 should be implemented")
    }

    fn make_mapper_direct() -> Mapper14 {
        Mapper14::new(MapperContext::new_for_test(
            14,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_14_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            14,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 14 must be registered in the factory"
        );
    }

    #[test]
    fn supervisor_defaults_to_vrc2_mode() {
        let mapper = make_mapper_direct();
        assert!(
            !mapper.in_mmc3_mode(),
            "power-on mode should default to VRC2"
        );
    }

    #[test]
    fn vrc2_prg_registers_control_8000_and_a000_windows() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0xA000, 5);
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xA000), 5);
    }

    #[test]
    fn vrc2_chr_nibble_registers_program_chr_banks() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xB000, 0x0A); // low nibble for chr reg 0
        mapper.write_prg(0xB001, 0x03); // high nibble for chr reg 0 => 0x3A

        assert_eq!(mapper.read_chr(0x0000), 0x3A);
    }

    #[test]
    fn supervisor_selects_mmc3_mode() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA131, 0x02); // mode bit

        mapper.write_prg(0x8000, 0x06); // MMC3 R6
        mapper.write_prg(0x8001, 4);

        assert_eq!(mapper.read_prg(0x8000), 4);
    }

    #[test]
    fn mmc3_mode_uses_outer_chr_bit_for_low_pattern_group() {
        let mut mapper = make_mapper_ext();

        mapper.write_prg(0xA131, 0x0A); // MMC3 mode + CHR outer bit for $0000-$0FFF
        mapper.write_prg(0x8000, 0x00); // MMC3 R0 (2 KB pair)
        mapper.write_prg(0x8001, 4);

        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "outer CHR bit should select upper marker"
        );
    }

    #[test]
    fn vrc2_mirroring_uses_9000_bit0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x9000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn register_snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA131, 0x0A);
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 3);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
    }
}
