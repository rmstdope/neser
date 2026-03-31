//! Mapper 036 - TXC 01-22000-400 (PCM063 family)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_036>
//!
//! Known Limitations:
//! - Gluk no-ASIC compatibility path (NINA-06-like fallback) is not implemented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const TXC_MASK: u8 = 0x07;

#[derive(Clone, Copy)]
struct Txc22000Chip {
    accumulator: u8,
    inverter: u8,
    staging: u8,
    output: u8,
    increase: bool,
    invert: bool,
}

impl Txc22000Chip {
    fn new() -> Self {
        Self {
            accumulator: 0,
            inverter: 0,
            staging: 0,
            output: 0,
            increase: false,
            invert: false,
        }
    }

    fn read(&self) -> u8 {
        (self.accumulator & TXC_MASK) | (self.inverter & !TXC_MASK)
    }

    fn write(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            match addr & 0xE103 {
                0x4100 => {
                    if self.increase {
                        self.accumulator = self.accumulator.wrapping_add(1);
                    } else {
                        self.accumulator = ((self.accumulator & !TXC_MASK)
                            | (self.staging & TXC_MASK))
                            ^ (if self.invert { 0xFF } else { 0x00 });
                    }
                }
                0x4101 => {
                    self.invert = (value & 0x01) != 0;
                }
                0x4102 => {
                    self.staging = value & TXC_MASK;
                    self.inverter = value & !TXC_MASK;
                }
                0x4103 => {
                    self.increase = (value & 0x01) != 0;
                }
                _ => {}
            }
        } else {
            self.output = (self.accumulator & 0x0F) | ((self.inverter & 0x08) << 1);
        }
    }
}

pub struct Mapper36 {
    base: BaseMapper,
    txc: Txc22000Chip,
    chr_bank: u8,
}

impl Mapper36 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.select_prg_page(0, 0);
        base.select_chr_page(0, 0);

        Self {
            base,
            txc: Txc22000Chip::new(),
            chr_bank: 0,
        }
    }

    fn update_state(&mut self) {
        self.base
            .select_prg_page(0, (self.txc.output & 0x03) as i16);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }

    fn status_read_for_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        let mut value = open_bus;
        if (addr & 0x0103) == 0x0100 {
            value = (open_bus & 0xCF) | ((self.txc.read() << 4) & 0x30);
        }
        value
    }
}

impl Mapper for Mapper36 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x4100..=0x5FFF => (self.txc.read() << 4) & 0x30,
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if (0x4100..=0x5FFF).contains(&addr) {
            return self.status_read_for_open_bus(addr, open_bus);
        }

        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !((0x4100..=0x5FFF).contains(&addr) || (0x8000..=0xFFFF).contains(&addr)) {
            return;
        }

        if (addr & 0xF200) == 0x4200 {
            self.chr_bank = value;
        }

        self.txc.write(addr, (value >> 4) & 0x03);
        self.update_state();
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn reset(&mut self) {
        self.txc = Txc22000Chip::new();
        self.chr_bank = 0;
        self.update_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.txc.accumulator,
            self.txc.inverter,
            self.txc.staging,
            self.txc.output,
            self.txc.increase as u8,
            self.txc.invert as u8,
            self.chr_bank,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 7 {
            return;
        }

        self.txc.accumulator = data[0];
        self.txc.inverter = data[1];
        self.txc.staging = data[2];
        self.txc.output = data[3];
        self.txc.increase = (data[4] & 0x01) != 0;
        self.txc.invert = (data[5] & 0x01) != 0;
        self.chr_bank = data[6];
        self.update_state();
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(32 * 1024, 4);
        let chr = banked_data(8 * 1024, 16);
        create_mapper(MapperContext::new_for_test(
            36,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 36 should be implemented")
    }

    #[test]
    fn mapper_36_is_registered() {
        let mapper = create_mapper(MapperContext::new_for_test(
            36,
            banked_data(32 * 1024, 4),
            banked_data(8 * 1024, 16),
            NametableLayout::Horizontal,
        ));
        assert!(mapper.is_ok(), "Mapper 36 must be registered in factory");
    }

    #[test]
    fn prg_bank_switch_uses_txc_output_low_two_bits() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4102, 0x20);
        mapper.write_prg(0x4101, 0x00);
        mapper.write_prg(0x4103, 0x00);
        mapper.write_prg(0x4100, 0x00);
        mapper.write_prg(0x8000, 0x00);

        assert_eq!(mapper.read_prg(0x8000), 2);
    }

    #[test]
    fn chr_bank_comes_from_4200_window_data() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4200, 0x09);

        assert_eq!(mapper.read_chr(0x0000), 9);
    }

    #[test]
    fn mirroring_remains_header_controlled() {
        let mut mapper = make_mapper();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x4101, 0x01);
        mapper.write_prg(0x4200, 0x0F);
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn snapshot_restore_roundtrip_preserves_state() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x4102, 0x30);
        mapper.write_prg(0x4100, 0x00);
        mapper.write_prg(0x4200, 0x0B);
        mapper.write_prg(0x8000, 0x00);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_chr(0x0000), mapper.read_chr(0x0000));
    }
}
