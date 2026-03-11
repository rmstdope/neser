//! Mapper 103 - UNL-PEC-586
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_103>
//!
//! Supported behavior:
//! - Fixed 32KB PRG-ROM at `$8000-$FFFF` (last four 8KB PRG banks).
//! - 8KB banked PRG window at `$6000-$7FFF` when PRG-RAM is disabled.
//! - 16KB internal work RAM visible at `$6000-$7FFF` and `$B800-$D7FF` when enabled.
//! - Mirroring controlled by bit 3 on writes to `$E000-$EFFF` (`1=Horizontal`, `0=Vertical`).
//! - CHR remains fixed to bank 0 (no CHR bank switching registers on this board).
//!
//! Known Limitations:
//! - No mapper-specific IRQ behavior (hardware does not expose mapper IRQs).
//! - No expansion audio behavior.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::cartridge::NametableLayout;

const MAPPER_NUMBER: u16 = 103;
const PRG_BANK_SIZE_8K: usize = 8 * 1024;
const CHR_BANK_SIZE_8K: usize = 8 * 1024;
const WORK_RAM_SIZE: usize = 16 * 1024;

pub struct Mapper103 {
    base: BaseMapper,
    initial_mirroring: NametableLayout,
    prg_ram_disabled: bool,
    prg_reg: u8,
    work_ram: Vec<u8>,
}

impl Mapper103 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 16,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_8K);
        base.configure_chr_banking(CHR_BANK_SIZE_8K);
        let mut mapper = Self {
            base,
            initial_mirroring: ctx.mirroring,
            prg_ram_disabled: false,
            prg_reg: 0,
            work_ram: vec![0; WORK_RAM_SIZE],
        };
        mapper.update_state();
        mapper
    }

    fn update_state(&mut self) {
        self.base.select_prg_page(0, -4);
        self.base.select_prg_page(1, -3);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);
        self.base.select_chr_page(0, 0);
    }

    fn read_prg_6000_banked(&self, addr: u16) -> u8 {
        let prg = self.base.prg_rom();
        let bank_count = prg.len() / PRG_BANK_SIZE_8K;
        if bank_count == 0 {
            return 0;
        }
        let bank = (self.prg_reg as usize) % bank_count;
        let offset = (addr - 0x6000) as usize;
        prg.get(bank * PRG_BANK_SIZE_8K + offset).copied().unwrap_or(0)
    }
}

impl Mapper for Mapper103 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_disabled {
                    self.read_prg_6000_banked(addr)
                } else {
                    self.work_ram[(addr - 0x6000) as usize]
                }
            }
            0xB800..=0xD7FF if !self.prg_ram_disabled => {
                self.work_ram[0x2000 + (addr - 0xB800) as usize]
            }
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.prg_ram_disabled {
                    self.read_prg_6000_banked(addr)
                } else {
                    self.work_ram[(addr - 0x6000) as usize]
                }
            }
            _ => self.base.read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr & 0xF000 {
            0x6000 | 0x7000 => {
                self.work_ram[(addr - 0x6000) as usize] = value;
            }
            0x8000 => {
                self.prg_reg = value & 0x0F;
                self.update_state();
            }
            0xB000 | 0xC000 | 0xD000 => {
                if (0xB800..0xD800).contains(&addr) {
                    self.work_ram[0x2000 + (addr - 0xB800) as usize] = value;
                }
            }
            0xE000 => {
                self.base.set_mirroring_hv((value & 0x08) != 0);
            }
            0xF000 => {
                self.prg_ram_disabled = (value & 0x10) != 0;
                self.update_state();
            }
            _ => {}
        }
    }

    fn wram_size(&self) -> usize {
        self.work_ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.work_ram.clone()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let copy_len = self.work_ram.len().min(data.len());
        self.work_ram[..copy_len].copy_from_slice(&data[..copy_len]);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.prg_ram_disabled as u8,
            self.prg_reg,
            self.base.mirroring().to_snapshot_byte(),
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_ram_disabled = data[0] != 0;
            self.prg_reg = data[1] & 0x0F;
            if let Some(&mirroring) = data.get(2) {
                self.base
                    .set_mirroring(NametableLayout::from_snapshot_byte(mirroring));
            }
            self.update_state();
        }
    }

    fn reset(&mut self) {
        self.prg_ram_disabled = false;
        self.prg_reg = 0;
        self.base.set_mirroring(self.initial_mirroring);
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_8K: usize = 11;
    const CHR_BANKS_8K: usize = 7;

    fn make_mapper() -> Mapper103 {
        Mapper103::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_8K, PRG_BANKS_8K),
            banked_data(CHR_BANK_SIZE_8K, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_103_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_8K, PRG_BANKS_8K),
            banked_data(CHR_BANK_SIZE_8K, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 103 must be creatable via factory");
    }

    #[test]
    fn reset_maps_last_32k_prg_and_chr_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 7);
        assert_eq!(mapper.read_prg(0xA000), 8);
        // $C000-$D7FF is shadowed by WRAM when PRG-RAM is enabled; use $D800 (outside WRAM window) to verify ROM bank
        assert_eq!(mapper.read_prg(0xD800), 9);
        assert_eq!(mapper.read_prg(0xE000), 10);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }

    #[test]
    fn prg_register_selects_6000_bank_when_prg_ram_is_disabled() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        mapper.write_prg(0xF000, 0x10);

        assert_eq!(mapper.read_prg(0x6000), 3);
        assert_eq!(mapper.read_prg(0x7FFF), 3);
    }

    #[test]
    fn work_ram_windows_are_readable_when_prg_ram_is_enabled() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x6000, 0x12);
        mapper.write_prg(0x7FFF, 0x34);
        mapper.write_prg(0xB800, 0x56);
        mapper.write_prg(0xD7FF, 0x78);

        assert_eq!(mapper.read_prg(0x6000), 0x12);
        assert_eq!(mapper.read_prg(0x7FFF), 0x34);
        assert_eq!(mapper.read_prg(0xB800), 0x56);
        assert_eq!(mapper.read_prg(0xD7FF), 0x78);
    }

    #[test]
    fn mirroring_bit_3_controls_horizontal_vs_vertical() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xE000, 0x08);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0xE000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn chr_bank_is_fixed_to_zero() {
        let mut mapper = make_mapper();
        let before = mapper.read_chr(0x0000);

        mapper.write_prg(0x8000, 0x0F);
        mapper.write_prg(0xE000, 0x08);
        mapper.write_prg(0xF000, 0x10);

        assert_eq!(before, 0);
        assert_eq!(mapper.read_chr(0x0000), 0);
    }
}
