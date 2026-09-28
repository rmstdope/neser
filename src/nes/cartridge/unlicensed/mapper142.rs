//! Mapper 142 - Kaiser KS7032.
//!
//! Specification: <https://www.nesdev.org/wiki/INES_Mapper_142>
//! The undocumented PRG-ROM enable is backed by Mesen2's `Kaiser202.h`.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 8 * 1024;

pub struct Mapper142 {
    base: BaseMapper,
    prg_regs: [u8; 4],
    bank_select: u8,
    prg_rom_at_6000: bool,
    irq_latch: u16,
    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
}

impl Mapper142 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            prg_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_prg_6000_banking();

        let mut mapper = Self {
            base,
            prg_regs: [0; 4],
            bank_select: 0,
            prg_rom_at_6000: false,
            irq_latch: 0,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        for slot in 0..3 {
            self.base.select_prg_page(slot, self.prg_regs[slot] as i16);
        }
        self.base.select_prg_page(3, -1);
        self.base.select_prg_6000_page(self.prg_regs[3] as i16);
    }
}

impl Mapper for Mapper142 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF if self.prg_rom_at_6000 => {
                self.base.try_read_prg_6000(addr).unwrap_or(0)
            }
            0x6000..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(0),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF if !self.prg_rom_at_6000 => {
                self.base.try_write_prg_ram(addr, value);
            }
            0x8000..=0x8FFF => {
                self.irq_latch = (self.irq_latch & 0xFFF0) | (value as u16 & 0x0F);
            }
            0x9000..=0x9FFF => {
                self.irq_latch = (self.irq_latch & 0xFF0F) | ((value as u16 & 0x0F) << 4);
            }
            0xA000..=0xAFFF => {
                self.irq_latch = (self.irq_latch & 0xF0FF) | ((value as u16 & 0x0F) << 8);
            }
            0xB000..=0xBFFF => {
                self.irq_latch = (self.irq_latch & 0x0FFF) | ((value as u16 & 0x0F) << 12);
            }
            0xC000..=0xCFFF => {
                self.irq_enabled = value & 0x02 != 0;
                if self.irq_enabled {
                    self.irq_counter = self.irq_latch;
                }
                self.irq_pending = false;
            }
            0xD000..=0xDFFF => self.irq_pending = false,
            0xE000..=0xEFFF => self.bank_select = value & 0x07,
            0xF000..=0xFFFF => {
                match self.bank_select {
                    1..=4 => self.prg_regs[(self.bank_select - 1) as usize] = value & 0x0F,
                    5 => self.prg_rom_at_6000 = value & 0x04 != 0,
                    _ => {}
                }
                self.update_banks();
            }
            _ => {}
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }

        self.irq_counter = self.irq_counter.wrapping_add(1);
        if self.irq_counter == 0xFFFF {
            self.irq_counter = self.irq_latch;
            self.irq_pending = true;
            self.irq_enabled = false;
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.prg_regs[0],
            self.prg_regs[1],
            self.prg_regs[2],
            self.prg_regs[3],
            self.bank_select,
            self.prg_rom_at_6000 as u8,
            self.irq_enabled as u8,
            self.irq_pending as u8,
            self.irq_latch as u8,
            (self.irq_latch >> 8) as u8,
            self.irq_counter as u8,
            (self.irq_counter >> 8) as u8,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 12 {
            return;
        }
        self.prg_regs.copy_from_slice(&data[..4]);
        self.bank_select = data[4];
        self.prg_rom_at_6000 = data[5] != 0;
        self.irq_enabled = data[6] != 0;
        self.irq_pending = data[7] != 0;
        self.irq_latch = u16::from_le_bytes([data[8], data[9]]);
        self.irq_counter = u16::from_le_bytes([data[10], data[11]]);
        self.update_banks();
    }

    fn reset(&mut self) {
        self.prg_regs = [0; 4];
        self.bank_select = 0;
        self.prg_rom_at_6000 = false;
        self.irq_latch = 0;
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 16;

    fn make_mapper() -> Mapper142 {
        Mapper142::new(MapperContext::new_for_test(
            142,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            vec![],
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_142_is_registered_in_factory() {
        assert!(
            create_mapper(MapperContext::new_for_test(
                142,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Vertical,
            ))
            .is_ok(),
            "mapper 142 must be creatable via factory"
        );
    }

    #[test]
    fn bank_registers_switch_their_prg_windows() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x02);
        mapper.write_prg(0xF000, 0x02);
        assert_eq!(mapper.read_prg(0xA000), 2);
    }

    #[test]
    fn bank_register_four_maps_prg_rom_at_6000_when_enabled() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x05);
        mapper.write_prg(0xF000, 0x04);
        mapper.write_prg(0xE000, 0x04);
        mapper.write_prg(0xF000, 0x02);

        assert_eq!(
            mapper.read_prg(0x6000),
            2,
            "the selected PRG-ROM bank must be visible at $6000"
        );
    }

    #[test]
    fn irq_fires_when_the_cycle_counter_reaches_ffff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x0E);
        mapper.write_prg(0x9000, 0x0F);
        mapper.write_prg(0xA000, 0x0F);
        mapper.write_prg(0xB000, 0x0F);
        mapper.write_prg(0xC000, 0x02);

        mapper.cpu_cycle();

        assert!(mapper.irq_pending());
    }
}
