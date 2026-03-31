//! Mapper 018 - Jaleco SS 88006
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_018>
//! - Hardware reference: Mesen `JalecoSs88006`

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const IRQ_MASKS: [u16; 4] = [0xFFFF, 0x0FFF, 0x00FF, 0x000F];

pub struct Mapper18 {
    base: BaseMapper,
    prg_banks: [u8; 3],
    chr_banks: [u8; 8],
    irq_reload_nibbles: [u8; 4],
    irq_counter: u16,
    irq_counter_size: u8,
    irq_enabled: bool,
    irq_pending: bool,
}

impl Mapper18 {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KB
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KB

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: ctx.prg_ram_banks_8k as usize * 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            prg_banks: [0; 3],
            chr_banks: [0; 8],
            irq_reload_nibbles: [0; 4],
            irq_counter: 0,
            irq_counter_size: 0,
            irq_enabled: false,
            irq_pending: false,
        };
        mapper.update_all_banks();
        mapper
    }

    fn update_all_banks(&mut self) {
        for slot in 0..3 {
            self.base.select_prg_page(slot, self.prg_banks[slot] as i16);
        }
        self.base.select_prg_page(3, -1);

        for slot in 0..8 {
            self.base.select_chr_page(slot, self.chr_banks[slot] as i16);
        }
    }

    fn update_prg_bank(&mut self, bank: usize, value: u8, update_upper_bits: bool) {
        if update_upper_bits {
            self.prg_banks[bank] = (self.prg_banks[bank] & 0x0F) | ((value & 0x0F) << 4);
        } else {
            self.prg_banks[bank] = (self.prg_banks[bank] & 0xF0) | (value & 0x0F);
        }
        self.base.select_prg_page(bank, self.prg_banks[bank] as i16);
    }

    fn update_chr_bank(&mut self, bank: usize, value: u8, update_upper_bits: bool) {
        if update_upper_bits {
            self.chr_banks[bank] = (self.chr_banks[bank] & 0x0F) | ((value & 0x0F) << 4);
        } else {
            self.chr_banks[bank] = (self.chr_banks[bank] & 0xF0) | (value & 0x0F);
        }
        self.base.select_chr_page(bank, self.chr_banks[bank] as i16);
    }

    fn reload_irq_counter(&mut self) {
        self.irq_counter = (self.irq_reload_nibbles[0] as u16)
            | ((self.irq_reload_nibbles[1] as u16) << 4)
            | ((self.irq_reload_nibbles[2] as u16) << 8)
            | ((self.irq_reload_nibbles[3] as u16) << 12);
    }
}

impl Mapper for Mapper18 {
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

        let update_upper_bits = (addr & 0x0001) == 0x0001;
        let nibble = value & 0x0F;

        match addr & 0xF003 {
            0x8000 | 0x8001 => self.update_prg_bank(0, nibble, update_upper_bits),
            0x8002 | 0x8003 => self.update_prg_bank(1, nibble, update_upper_bits),
            0x9000 | 0x9001 => self.update_prg_bank(2, nibble, update_upper_bits),

            0xA000 | 0xA001 => self.update_chr_bank(0, nibble, update_upper_bits),
            0xA002 | 0xA003 => self.update_chr_bank(1, nibble, update_upper_bits),
            0xB000 | 0xB001 => self.update_chr_bank(2, nibble, update_upper_bits),
            0xB002 | 0xB003 => self.update_chr_bank(3, nibble, update_upper_bits),
            0xC000 | 0xC001 => self.update_chr_bank(4, nibble, update_upper_bits),
            0xC002 | 0xC003 => self.update_chr_bank(5, nibble, update_upper_bits),
            0xD000 | 0xD001 => self.update_chr_bank(6, nibble, update_upper_bits),
            0xD002 | 0xD003 => self.update_chr_bank(7, nibble, update_upper_bits),

            0xE000..=0xE003 => {
                self.irq_reload_nibbles[(addr & 0x0003) as usize] = nibble;
            }

            0xF000 => {
                self.irq_pending = false;
                self.reload_irq_counter();
            }
            0xF001 => {
                self.irq_pending = false;
                self.irq_enabled = (nibble & 0x01) != 0;
                self.irq_counter_size = if (nibble & 0x08) != 0 {
                    3
                } else if (nibble & 0x04) != 0 {
                    2
                } else if (nibble & 0x02) != 0 {
                    1
                } else {
                    0
                };
            }
            0xF002 => {
                let mirroring = match nibble & 0x03 {
                    0 => NametableLayout::Horizontal,
                    1 => NametableLayout::Vertical,
                    2 => NametableLayout::SingleScreenLower,
                    _ => NametableLayout::SingleScreenUpper,
                };
                self.base.set_mirroring(mirroring);
            }
            0xF003 => {}

            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        if !self.irq_enabled {
            return;
        }

        let mask = IRQ_MASKS[self.irq_counter_size as usize];
        let counter = (self.irq_counter & mask).wrapping_sub(1);
        if counter == 0 {
            self.irq_pending = true;
        }

        self.irq_counter = (self.irq_counter & !mask) | (counter & mask);
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mirroring = match self.base.mirroring() {
            NametableLayout::Horizontal => 0,
            NametableLayout::Vertical => 1,
            NametableLayout::SingleScreenLower => 2,
            NametableLayout::SingleScreenUpper => 3,
            _ => 0,
        };

        let mut data = Vec::with_capacity(3 + 8 + 4 + 6);
        data.extend_from_slice(&self.prg_banks);
        data.extend_from_slice(&self.chr_banks);
        data.extend_from_slice(&self.irq_reload_nibbles);
        data.push((self.irq_counter & 0x00FF) as u8);
        data.push((self.irq_counter >> 8) as u8);
        data.push(self.irq_counter_size);
        data.push(self.irq_enabled as u8);
        data.push(self.irq_pending as u8);
        data.push(mirroring);
        data
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 21 {
            return;
        }

        self.prg_banks.copy_from_slice(&data[0..3]);
        self.chr_banks.copy_from_slice(&data[3..11]);
        self.irq_reload_nibbles.copy_from_slice(&data[11..15]);
        self.irq_counter = (data[15] as u16) | ((data[16] as u16) << 8);
        self.irq_counter_size = data[17] & 0x03;
        self.irq_enabled = (data[18] & 0x01) != 0;
        self.irq_pending = (data[19] & 0x01) != 0;

        let mirroring = match data[20] & 0x03 {
            0 => NametableLayout::Horizontal,
            1 => NametableLayout::Vertical,
            2 => NametableLayout::SingleScreenLower,
            _ => NametableLayout::SingleScreenUpper,
        };
        self.base.set_mirroring(mirroring);
        self.update_all_banks();
    }

    fn reset(&mut self) {
        self.prg_banks = [0; 3];
        self.chr_banks = [0; 8];
        self.irq_reload_nibbles = [0; 4];
        self.irq_counter = 0;
        self.irq_counter_size = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.base.set_mirroring(NametableLayout::Horizontal);
        self.update_all_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper18;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 32;
    const CHR_BANKS: usize = 128;

    fn make_mapper() -> Mapper18 {
        Mapper18::new(MapperContext::new_for_test(
            18,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_18_is_registered() {
        let mapper = create_mapper(MapperContext::new_for_test(
            18,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(mapper.is_ok(), "mapper 18 must be registered");
    }

    #[test]
    fn prg_banks_use_nibble_pairs_and_last_bank_is_fixed() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x0A);
        mapper.write_prg(0x8001, 0x03); // bank0 = 0x3A -> 26 (mod 32)
        mapper.write_prg(0x8002, 0x03);
        mapper.write_prg(0x8003, 0x00); // bank1 = 3
        mapper.write_prg(0x9000, 0x05);
        mapper.write_prg(0x9001, 0x00); // bank2 = 5

        assert_eq!(mapper.read_prg(0x8000), 26);
        assert_eq!(mapper.read_prg(0xA000), 3);
        assert_eq!(mapper.read_prg(0xC000), 5);
        assert_eq!(mapper.read_prg(0xE000), (PRG_BANKS - 1) as u8);
    }

    #[test]
    fn chr_banks_use_nibble_pairs_per_1kb_slot() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xA000, 0x02);
        mapper.write_prg(0xA001, 0x01); // chr0 = 0x12 = 18
        mapper.write_prg(0xD002, 0x0F);
        mapper.write_prg(0xD003, 0x00); // chr7 = 0x0F = 15

        assert_eq!(mapper.read_chr(0x0000), 18);
        assert_eq!(mapper.read_chr(0x1C00), 15);
    }

    #[test]
    fn mirroring_register_supports_all_four_modes() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xF002, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        mapper.write_prg(0xF002, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xF002, 2);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        mapper.write_prg(0xF002, 3);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn irq_counter_decrements_when_enabled_and_acknowledges() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xE000, 0x01);
        mapper.write_prg(0xE001, 0x00);
        mapper.write_prg(0xE002, 0x00);
        mapper.write_prg(0xE003, 0x00);

        mapper.write_prg(0xF001, 0x01); // enable IRQ, 16-bit mode
        mapper.write_prg(0xF000, 0x00); // reload + acknowledge

        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ must fire when counter reaches zero"
        );

        mapper.write_prg(0xF001, 0x00); // acknowledge + disable
        assert!(!mapper.irq_pending(), "IRQ must clear on acknowledge");
    }

    #[test]
    fn irq_counter_size_masks_to_selected_width() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xE000, 0x01);
        mapper.write_prg(0xE001, 0x01); // reload = 0x0011
        mapper.write_prg(0xE002, 0x00);
        mapper.write_prg(0xE003, 0x00);

        mapper.write_prg(0xF001, 0x05); // enable + 8-bit counter mode
        mapper.write_prg(0xF000, 0x00); // reload

        for _ in 0..16 {
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending());
        }
        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "8-bit masked counter should fire at 0x11"
        );
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x0C);
        mapper.write_prg(0x8001, 0x01);
        mapper.write_prg(0xA000, 0x07);
        mapper.write_prg(0xA001, 0x02);
        mapper.write_prg(0xF002, 0x03);

        mapper.write_prg(0xE000, 0x04);
        mapper.write_prg(0xE001, 0x03);
        mapper.write_prg(0xE002, 0x02);
        mapper.write_prg(0xE003, 0x01);
        mapper.write_prg(0xF001, 0x01);
        mapper.write_prg(0xF000, 0x00);
        mapper.cpu_cycle();

        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(
            snapshot,
            restored.registers_snapshot(),
            "snapshot/restore roundtrip must preserve mapper 18 register state"
        );
    }
}
