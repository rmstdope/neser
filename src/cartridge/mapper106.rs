//! Mapper 106 - Unlicensed board (Mesen2 reference implementation)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_106>
//! - Mesen2 reference: `Core/NES/Mappers/Unlicensed/Mapper106.h`
//!
//! Known Limitations:
//! - Expansion audio hardware (if present on specific clone boards) is not modeled.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 106;
const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 1024;
const PRG_SLOTS: usize = 4;
const CHR_SLOTS: usize = 8;
const BANKING_SNAPSHOT_SIZE: usize = PRG_SLOTS + CHR_SLOTS;

pub struct Mapper106 {
    base: BaseMapper,
    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
}

impl Mapper106 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            max_prg_ram_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
        };
        mapper.reset();
        mapper
    }

    fn set_power_on_state(&mut self) {
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        for slot in 0..PRG_SLOTS {
            self.base.select_prg_page(slot, -1);
        }
    }
}

impl Mapper for Mapper106 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Delegate to PRG-RAM handling first (e.g., $6000-$7FFF) if enabled.
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }

        // Only decode mapper registers for $8000-$FFFF.
        if addr < 0x8000 {
            return;
        }

        match addr & 0x0F {
            0 | 2 => self
                .base
                .select_chr_page((addr & 0x0F) as usize, (value & 0xFE) as i16),
            1 | 3 => self
                .base
                .select_chr_page((addr & 0x0F) as usize, (value | 0x01) as i16),
            4..=7 => self
                .base
                .select_chr_page((addr & 0x0F) as usize, value as i16),
            8 | 0x0B => self
                .base
                .select_prg_page(((addr & 0x0F) - 8) as usize, ((value & 0x0F) | 0x10) as i16),
            9 | 0x0A => self
                .base
                .select_prg_page(((addr & 0x0F) - 8) as usize, (value & 0x1F) as i16),
            0x0D => {
                self.irq_enabled = false;
                self.irq_counter = 0;
                self.irq_pending = false;
            }
            0x0E => {
                self.irq_counter = (self.irq_counter & 0xFF00) | value as u16;
            }
            0x0F => {
                self.irq_counter = (self.irq_counter & 0x00FF) | ((value as u16) << 8);
                self.irq_enabled = true;
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        if self.irq_enabled {
            self.irq_counter = self.irq_counter.wrapping_add(1);
            if self.irq_counter == 0 {
                self.irq_pending = true;
                self.irq_enabled = false;
            }
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut data = self.base.banking_snapshot();
        data.push((self.irq_counter & 0xFF) as u8);
        data.push((self.irq_counter >> 8) as u8);
        data.push(self.irq_enabled as u8);
        data.push(self.irq_pending as u8);
        data
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.base.restore_banking(data);
        if data.len() >= BANKING_SNAPSHOT_SIZE + 4 {
            self.irq_counter = (data[BANKING_SNAPSHOT_SIZE] as u16)
                | ((data[BANKING_SNAPSHOT_SIZE + 1] as u16) << 8);
            self.irq_enabled = data[BANKING_SNAPSHOT_SIZE + 2] != 0;
            self.irq_pending = data[BANKING_SNAPSHOT_SIZE + 3] != 0;
        }
    }

    fn reset(&mut self) {
        self.set_power_on_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_8K: usize = 48;
    const CHR_BANKS_1K: usize = 67;

    fn make_mapper() -> Mapper106 {
        Mapper106::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_8K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_106_is_registered() {
        let mapper = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS_8K),
            banked_data(CHR_BANK_SIZE, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ));
        assert!(mapper.is_ok(), "Mapper 106 must be registered in factory");
    }

    #[test]
    fn header_defined_prg_ram_is_allocated_and_writable_at_6000_7fff() {
        let metadata = MapperContext {
            prg_ram_banks_8k: 2,
            ..MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS_8K),
                banked_data(CHR_BANK_SIZE, CHR_BANKS_1K),
                NametableLayout::Vertical,
            )
        };
        let mut mapper = create_mapper(metadata).expect("Mapper 106 should be created");

        assert_eq!(mapper.wram_size(), 16 * 1024);

        mapper.write_prg(0x6000, 0xA5);
        mapper.write_prg(0x7FFF, 0x5A);
        assert_eq!(mapper.read_prg(0x6000), 0xA5);
        assert_eq!(mapper.read_prg(0x7FFF), 0x5A);
    }

    #[test]
    fn prg_slots_follow_mapper_106_register_decode() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8008, 0x07);
        mapper.write_prg(0x8009, 0x05);
        mapper.write_prg(0x800A, 0x06);
        mapper.write_prg(0x800B, 0x02);

        assert_eq!(mapper.read_prg(0x8000), 0x17);
        assert_eq!(mapper.read_prg(0xA000), 0x05);
        assert_eq!(mapper.read_prg(0xC000), 0x06);
        assert_eq!(mapper.read_prg(0xE000), 0x12);
    }

    #[test]
    fn chr_slots_apply_even_odd_rules_for_registers_0_to_3() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0x8001, 0x08);
        mapper.write_prg(0x8002, 0x05);
        mapper.write_prg(0x8003, 0x04);
        mapper.write_prg(0x8004, 0x11);
        mapper.write_prg(0x8005, 0x12);

        assert_eq!(mapper.read_chr(0x0000), 0x08);
        assert_eq!(mapper.read_chr(0x0400), 0x09);
        assert_eq!(mapper.read_chr(0x0800), 0x04);
        assert_eq!(mapper.read_chr(0x0C00), 0x05);
        assert_eq!(mapper.read_chr(0x1000), 0x11);
        assert_eq!(mapper.read_chr(0x1400), 0x12);
    }

    #[test]
    fn mirroring_remains_fixed_from_header() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01);
        mapper.write_prg(0x9000, 0x00);
        mapper.write_prg(0xA000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn irq_counter_overflow_sets_pending_and_disable_until_ack() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x800E, 0xFF);
        mapper.write_prg(0x800F, 0xFF);
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ should assert on 16-bit overflow");

        mapper.cpu_cycle();
        assert!(
            mapper.irq_pending(),
            "IRQ remains pending until disabled/acknowledged by register $D"
        );

        mapper.write_prg(0x800D, 0);
        assert!(!mapper.irq_pending(), "Writing $D should acknowledge IRQ");
    }

    #[test]
    fn capabilities_report_irq_without_expansion_audio() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(caps.has_chr_banking);
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 1);
    }
}
