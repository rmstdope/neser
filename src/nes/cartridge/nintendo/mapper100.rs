//! Mapper 100 - Nesticle MMC3
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_100>
//!
//! Known Limitations:
//! - Implemented as MMC3-compatible behavior. Hardware-accurate quirks specific to
//!   old emulator hacks are not modeled separately.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::nes::cartridge::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 100;

pub struct Mapper100 {
    mmc3: MMC3Mapper,
}

impl Mapper100 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new(ctx),
        }
    }
}

impl Mapper for Mapper100 {
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
        self.mmc3.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.mmc3.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.mmc3.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.mmc3.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value);
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn reset(&mut self) {
        self.mmc3.reset();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.mmc3.registers_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.mmc3.restore_registers(data);
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

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            100,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 100 should be implemented")
    }

    #[test]
    fn mapper_100_is_registered() {
        let mapper = create_mapper(MapperContext::new_for_test(
            100,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Horizontal,
        ));

        assert!(mapper.is_ok(), "Mapper 100 should be registered in factory");
    }

    #[test]
    fn prg_bank_switching_matches_mmc3_r6_at_8000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn mirroring_control_matches_mmc3_a000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn capabilities_report_irq_and_no_expansion_audio() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(caps.has_chr_banking);
        assert!(caps.has_dynamic_mirroring);
    }

    #[test]
    fn read_prg_open_bus_reads_wram_via_mmc3_delegate() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x3C);
        assert_eq!(mapper.read_prg_open_bus(0x6000, 0xAA), 0x3C);
    }
}
