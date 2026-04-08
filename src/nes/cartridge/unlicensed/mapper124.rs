//! Mapper 124 – Squeedo (SGMT3)
//!
//! Specifications:
//! - NesDev: <https://www.nesdev.org/wiki/INES_Mapper_124>
//!
//! Hardware: Kevtris "Squeedo" homebrew NES development platform.
//! The Squeedo cartridge couples a PIC18F microcontroller to the NES bus via
//! SPI.  For emulation purposes, the NES-CPU-facing register interface is
//! equivalent to a standard MMC3 banked-ROM configuration.
//!
//! Known Limitations:
//! - The PIC18F coprocessor (SPI channel, GPIO, custom audio) is not
//!   emulated.  Only the MMC3-compatible PRG/CHR/IRQ register interface is
//!   implemented here.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::nes::cartridge::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 124;

/// Mapper 124 – Squeedo (SGMT3)
///
/// Exposes a standard MMC3 register interface to the NES CPU:
/// - PRG-ROM: up to 8 MiB via 8 KiB bank windows at $8000/$A000/$C000/$E000
/// - CHR-ROM/RAM: up to 8 KiB per window via standard MMC3 CHR registers
/// - Nametable mirroring: programmable (horizontal / vertical)
/// - IRQ: standard MMC3 A12-based scanline counter
pub struct Mapper124 {
    mmc3: MMC3Mapper,
}

impl Mapper124 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new(ctx),
        }
    }
}

impl Mapper for Mapper124 {
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

    const PRG_BANKS_8K: usize = 8;
    const CHR_BANKS_1K: usize = 16;

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            124,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("mapper 124 should be implemented")
    }

    #[test]
    fn mapper_124_is_registered_in_factory() {
        let _m = make_mapper();
    }

    #[test]
    fn mapper_124_prg_banking_matches_mmc3() {
        let mut m124 = make_mapper();
        let mut mmc3 = create_mapper(MapperContext::new_for_test(
            4,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("MMC3 (mapper 4) should be implemented");

        // Select PRG bank 3 into the $8000 window via R6
        for m in [&mut m124, &mut mmc3] {
            m.write_prg(0x8000, 0x06); // bank_select = R6
            m.write_prg(0x8001, 3);
        }
        assert_eq!(
            m124.read_prg(0x8000),
            mmc3.read_prg(0x8000),
            "mapper 124 PRG bank selection must match MMC3"
        );
    }

    #[test]
    fn mapper_124_chr_banking_matches_mmc3() {
        let mut m124 = make_mapper();
        let mut mmc3 = create_mapper(MapperContext::new_for_test(
            4,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("MMC3 (mapper 4) should be implemented");

        // Select CHR 2KB pair at PPU $0000 via R0
        for m in [&mut m124, &mut mmc3] {
            m.write_prg(0x8000, 0x00); // R0
            m.write_prg(0x8001, 4);
        }
        assert_eq!(
            m124.read_chr(0x0000),
            mmc3.read_chr(0x0000),
            "mapper 124 CHR bank selection must match MMC3"
        );
    }

    #[test]
    fn mapper_124_mirroring_matches_mmc3() {
        let mut m124 = make_mapper();
        m124.write_prg(0xA000, 0x01); // set horizontal
        assert_eq!(
            m124.get_mirroring(),
            NametableLayout::Horizontal,
            "mapper 124 mirroring must follow MMC3 $A000 register"
        );
    }

    #[test]
    fn mapper_124_irq_matches_mmc3() {
        let mut m124 = make_mapper();
        let mut mmc3 = create_mapper(MapperContext::new_for_test(
            4,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
        .expect("MMC3 should be implemented");

        for m in [&mut m124, &mut mmc3] {
            m.write_prg(0xC000, 0x01); // IRQ latch = 1
            m.write_prg(0xC001, 0x00); // reload
            m.write_prg(0xE001, 0x00); // enable
        }
        for _ in 0..2 {
            m124.ppu_address_changed(0x0FFF);
            mmc3.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                m124.cpu_cycle();
                mmc3.cpu_cycle();
            }
            m124.ppu_address_changed(0x1000);
            mmc3.ppu_address_changed(0x1000);
        }
        assert_eq!(
            m124.irq_pending(),
            mmc3.irq_pending(),
            "mapper 124 IRQ behaviour must match MMC3"
        );
    }
}
