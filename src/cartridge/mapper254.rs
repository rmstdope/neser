//! Mapper 254 - Pikachu Y2K (MMC3 variant with copy protection)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_254>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 254 - Pikachu Y2K (MMC3 variant with copy protection)
///
/// Hardware: MMC3 clone with PRG-RAM read protection.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_254>
/// - Standard MMC3 banking, mirroring, and IRQ
/// - Copy protection via PRG-RAM read XOR scheme
///
/// Protection mechanism:
/// - Write to $6000-$7FFF: Activates protection flag
/// - Write to $8000-$9FFF (even): Clears protection flag (standard bank select)
/// - Read from $6000-$7FFF when protected: Returns 0 XOR LUT[addr & 3]
///   instead of actual PRG-RAM contents
/// - LUT: [0x00, 0xFF, 0x55, 0xAA]
///
/// Once protection is cleared (by writing to $8000), PRG-RAM reads normally.
pub struct Mapper254 {
    mmc3: MMC3Mapper,
    protection_active: bool,
}

impl Mapper254 {
    const MAPPER_NUMBER: u8 = 254;
    const PROTECTION_LUT: [u8; 4] = [0x00, 0xFF, 0x55, 0xAA];

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        Self {
            mmc3: MMC3Mapper::new(prg_rom, chr_rom, mirroring),
            protection_active: false,
        }
    }
}

impl Mapper for Mapper254 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.protection_active {
                    // Return XOR'd value from LUT
                    Self::PROTECTION_LUT[(addr & 3) as usize]
                } else {
                    self.mmc3.read_prg(addr)
                }
            }
            _ => self.mmc3.read_prg(addr),
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                if self.protection_active {
                    Self::PROTECTION_LUT[(addr & 3) as usize]
                } else {
                    self.mmc3.read_prg_open_bus(addr, open_bus)
                }
            }
            _ => self.mmc3.read_prg_open_bus(addr, open_bus),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // Any write to PRG-RAM area activates protection
                self.protection_active = true;
                // Still forward the write to MMC3 for PRG-RAM storage
                self.mmc3.write_prg(addr, value);
            }
            0x8000..=0x9FFF if (addr & 1) == 0 => {
                // Even address in $8000-$9FFF (bank select) clears protection
                self.protection_active = false;
                self.mmc3.write_prg(addr, value);
            }
            _ => {
                self.mmc3.write_prg(addr, value);
            }
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.mmc3.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mmc3.get_mirroring()
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
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

    fn ppu_address_changed(&mut self, addr: u16) {
        self.mmc3.ppu_address_changed(addr);
    }

    fn cpu_cycle(&mut self) {
        self.mmc3.cpu_cycle();
    }

    fn irq_pending(&self) -> bool {
        self.mmc3.irq_pending()
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.mmc3.chr_ram_snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.mmc3.restore_chr_ram(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.protection_active as u8);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // MMC3 snapshot is 16 bytes, our extra byte is at index 16
        if data.len() > 16 {
            self.mmc3.restore_registers(&data[..16]);
            self.protection_active = data[16] != 0;
        } else {
            self.mmc3.restore_registers(data);
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper254(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new(254, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn test_factory_creates_mapper_254() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok());
    }

    #[test]
    fn test_prg_ram_read_without_protection() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Write to PRG-RAM, which activates protection
        mapper.write_prg(0x6000, 0xAB);

        // Clear protection by writing to $8000 (even)
        mapper.write_prg(0x8000, 0);

        // Now PRG-RAM should be readable normally
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    #[test]
    fn test_protection_activates_on_prg_ram_write() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Write to PRG-RAM → activates protection
        mapper.write_prg(0x6000, 0xAB);

        // Read should return LUT value, not actual data
        assert_eq!(mapper.read_prg(0x6000), 0x00); // LUT[0] = 0x00
        assert_eq!(mapper.read_prg(0x6001), 0xFF); // LUT[1] = 0xFF
        assert_eq!(mapper.read_prg(0x6002), 0x55); // LUT[2] = 0x55
        assert_eq!(mapper.read_prg(0x6003), 0xAA); // LUT[3] = 0xAA
    }

    #[test]
    fn test_protection_cleared_by_bank_select() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Activate protection
        mapper.write_prg(0x6000, 0x42);

        // Verify protection is active
        assert_eq!(mapper.read_prg(0x6000), 0x00); // LUT value

        // Clear by writing to $8000 (even = bank select)
        mapper.write_prg(0x8000, 0);

        // Now should read actual PRG-RAM
        assert_eq!(mapper.read_prg(0x6000), 0x42);
    }

    #[test]
    fn test_protection_lut_wraps_at_4() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0); // Activate protection

        // LUT wraps: $6004 & 3 = 0, $6005 & 3 = 1, etc.
        assert_eq!(mapper.read_prg(0x6004), 0x00);
        assert_eq!(mapper.read_prg(0x6005), 0xFF);
    }

    #[test]
    fn test_odd_address_does_not_clear_protection() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0x42); // Activate protection

        // Write to $8001 (odd) should NOT clear protection
        mapper.write_prg(0x8001, 0);

        // Protection should still be active
        assert_eq!(mapper.read_prg(0x6000), 0x00); // LUT value
    }

    #[test]
    fn test_standard_mmc3_prg_banking() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Standard MMC3: set R6=3
        mapper.write_prg(0x8000, 6); // Select reg 6
        mapper.write_prg(0x8001, 3); // R6 = 3

        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper =
            create_mapper254(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        // Activate protection
        mapper.write_prg(0x6000, 0x42);

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);

        // Protection should be restored
        assert_eq!(restored.read_prg(0x6000), 0x00); // LUT value (protection active)
    }
}
