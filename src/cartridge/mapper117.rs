//! Mapper 117 - Fu Guang
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_117>
//! - Mesen2 reference: `Core/NES/Mappers/Unlicensed/Mapper117.h`
//!
//! Known Limitations:
//! - IRQ timing is modeled as a CPU-cycle countdown with reload/start/ack controls.

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 117 - Fu Guang
pub struct Mapper117 {
    base: BaseMapper,
    prg_regs: [u8; 3],
    chr_regs: [u8; 8],
    irq: CpuCycleIrq,
}

impl Mapper117 {
    const PRG_BANK_SIZE: usize = 0x2000;
    const CHR_BANK_SIZE: usize = 0x0400;
    const SNAPSHOT_SIZE: usize = 15;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(NametableLayout::Vertical);

        let mut mapper = Self {
            base,
            prg_regs: [0, 1, 2],
            chr_regs: [0; 8],
            irq: CpuCycleIrq::new(CpuCycleIrqMode::DownToZero),
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_regs[0] as i16);
        self.base.select_prg_page(1, self.prg_regs[1] as i16);
        self.base.select_prg_page(2, self.prg_regs[2] as i16);
        self.base.select_prg_page(3, -1);

        for (slot, bank) in self.chr_regs.iter().enumerate() {
            self.base.select_chr_page(slot, *bank as i16);
        }
    }

    fn set_power_on_state(&mut self) {
        self.prg_regs = [0, 1, 2];
        self.chr_regs = [0; 8];
        self.base.set_mirroring(NametableLayout::Vertical);
        self.irq.set_reload(0);
        self.irq.set_counter(0);
        self.irq.set_enabled(false);
        self.irq.set_pending(false);
        self.update_banks();
    }
}

impl Mapper for Mapper117 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0x8002 => {
                self.prg_regs[(addr & 0x0003) as usize] = value;
                self.update_banks();
            }
            0xA000..=0xA007 => {
                self.chr_regs[(addr & 0x0007) as usize] = value;
                self.update_banks();
            }
            0xC001 => self.irq.set_reload(value as u16),
            0xC002 => self.irq.acknowledge(),
            0xC003 => self.irq.reload_counter(),
            0xD000 => {
                self.base.set_mirroring(if (value & 0x01) != 0 {
                    NametableLayout::Horizontal
                } else {
                    NametableLayout::Vertical
                });
            }
            0xE000 => {
                self.irq.set_enabled((value & 0x01) != 0);
                self.irq.acknowledge();
            }
            _ => {}
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq.is_pending()
    }

    fn cpu_cycle(&mut self) {
        self.irq.tick();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(Self::SNAPSHOT_SIZE);
        data.push(matches!(self.base.mirroring(), NametableLayout::Vertical) as u8);
        data.extend_from_slice(&self.prg_regs);
        data.extend_from_slice(&self.chr_regs);
        data.push((self.irq.enabled() as u8) | ((self.irq.is_pending() as u8) << 1));
        data.push(self.irq.reload() as u8);
        data.push(self.irq.counter() as u8);
        data
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < Self::SNAPSHOT_SIZE {
            return;
        }

        self.base.set_mirroring(if data[0] != 0 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        });
        self.prg_regs.copy_from_slice(&data[1..4]);
        self.chr_regs.copy_from_slice(&data[4..12]);

        let irq_flags = data[12];
        self.irq.set_enabled((irq_flags & 0x01) != 0);
        self.irq.set_pending((irq_flags & 0x02) != 0);
        self.irq.set_reload(data[13] as u16);
        self.irq.set_counter(data[14] as u16);
        self.update_banks();
    }

    fn reset(&mut self) {
        self.set_power_on_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 11;
    const CHR_BANKS: usize = 13;

    fn make_mapper() -> Mapper117 {
        Mapper117::new(MapperContext::new_for_test(
            117,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_117_is_registered() {
        let mapper = create_mapper(MapperContext::new_for_test(
            117,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(mapper.is_ok());
    }

    #[test]
    fn prg_8000_bfff_is_switchable_and_last_8k_is_fixed() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x8000, 4);
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8002, 6);

        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xA000), 5);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), (PRG_BANKS - 1) as u8);
    }

    #[test]
    fn write_to_8003_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 4);
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8002, 6);

        mapper.write_prg(0x8003, 0);

        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xA000), 5);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), (PRG_BANKS - 1) as u8);
    }

    #[test]
    fn chr_1k_banks_are_independently_switchable() {
        let mut mapper = make_mapper();

        for slot in 0..8u16 {
            mapper.write_prg(0xA000 + slot, (slot + 3) as u8);
        }

        for slot in 0..8u16 {
            let expected = ((slot + 3) as usize % CHR_BANKS) as u8;
            assert_eq!(mapper.read_chr(slot * 0x400), expected);
        }
    }

    #[test]
    fn d000_bit0_controls_mirroring_vertical_horizontal() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xD000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0xD000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn irq_fires_after_configured_cycle_count_when_enabled() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xC001, 3);
        mapper.write_prg(0xC003, 0);
        mapper.write_prg(0xE000, 1);
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        assert!(!mapper.irq_pending());
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());
    }

    #[test]
    fn irq_does_not_fire_while_disabled() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xC001, 3);
        mapper.write_prg(0xC003, 0);
        mapper.write_prg(0xE000, 0);
        for _ in 0..8 {
            mapper.cpu_cycle();
        }
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn irq_acknowledge_clears_pending() {
        let mut mapper = make_mapper();

        mapper.write_prg(0xC001, 1);
        mapper.write_prg(0xC003, 0);
        mapper.write_prg(0xE000, 1);
        mapper.cpu_cycle();
        assert!(mapper.irq_pending());

        mapper.write_prg(0xC002, 0);
        assert!(!mapper.irq_pending());
    }

    #[test]
    fn snapshot_restore_roundtrip_preserves_mapper_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 4);
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8002, 6);
        mapper.write_prg(0xA000, 7);
        mapper.write_prg(0xA007, 8);
        mapper.write_prg(0xD000, 1);
        mapper.write_prg(0xC001, 5);
        mapper.write_prg(0xC003, 0);
        mapper.write_prg(0xE000, 1);
        mapper.cpu_cycle();
        mapper.cpu_cycle();

        let snapshot = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_prg(0xA000), mapper.read_prg(0xA000));
        assert_eq!(restored.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
        assert_eq!(restored.read_chr(0x1C00), mapper.read_chr(0x1C00));
        assert_eq!(restored.get_mirroring(), mapper.get_mirroring());
        assert_eq!(restored.irq.reload(), mapper.irq.reload());
        assert_eq!(restored.irq.counter(), mapper.irq.counter());
        assert_eq!(restored.irq.enabled(), mapper.irq.enabled());
        assert_eq!(restored.irq_pending(), mapper.irq_pending());
    }

    #[test]
    fn reset_restores_power_on_defaults() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 4);
        mapper.write_prg(0x8001, 5);
        mapper.write_prg(0x8002, 6);
        mapper.write_prg(0xA000, 7);
        mapper.write_prg(0xD000, 1);
        mapper.write_prg(0xC001, 3);
        mapper.write_prg(0xC003, 0);
        mapper.write_prg(0xE000, 1);
        mapper.cpu_cycle();

        mapper.reset();

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xA000), 1);
        assert_eq!(mapper.read_prg(0xC000), 2);
        assert_eq!(mapper.read_prg(0xE000), (PRG_BANKS - 1) as u8);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(mapper.irq.reload(), 0);
        assert_eq!(mapper.irq.counter(), 0);
        assert!(!mapper.irq.enabled());
        assert!(!mapper.irq_pending());
    }
}
