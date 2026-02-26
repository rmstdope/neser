//! Mapper 254 - Pikachu Y2K (MMC3 variant with copy protection)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_254>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};
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
/// - On power-on, protection is active
/// - Write to any address where (addr & $E001) == $8000: Clears protection
/// - Write to any address where (addr & $E001) == $A001: Updates XOR mask byte
/// - Read from $6000-$7FFF while protected: Returns PRG-RAM[addr] XOR mask
///
/// Once protection is cleared, PRG-RAM reads normally.
pub struct Mapper254 {
    mmc3: MMC3Mapper,
    prg_ram: PrgRam,
    protection_disabled: bool,
    xor_mask: u8,
}

impl Mapper254 {
    const MAPPER_NUMBER: u8 = 254;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            protection_disabled: false,
            xor_mask: 0,
        }
    }
}

impl Mapper for Mapper254 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let value = self.prg_ram.try_read(addr).unwrap_or(0);
                if self.protection_disabled {
                    value
                } else {
                    value ^ self.xor_mask
                }
            }
            _ => self.mmc3.read_prg(addr),
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.read_prg(addr),
            _ => self.mmc3.read_prg_open_bus(addr, open_bus),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        if addr >= 0x8000 {
            match addr & 0xE001 {
                0x8000 => {
                    self.protection_disabled = true;
                }
                0xA001 => {
                    self.xor_mask = value;
                }
                _ => {}
            }
        }

        if addr >= 0x8000 {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
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
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
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
        snap.push(self.protection_disabled as u8);
        snap.push(self.xor_mask);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        // MMC3 snapshot is 16 bytes.
        // New mapper254 data appends 2 bytes: [protection_disabled, xor_mask]
        // Legacy mapper254 data appended 1 byte: [protection_active]
        if data.len() >= 18 {
            self.mmc3.restore_registers(&data[..16]);
            self.protection_disabled = data[16] != 0;
            self.xor_mask = data[17];
        } else if data.len() >= 17 {
            self.mmc3.restore_registers(&data[..16]);
            let legacy_protection_active = data[16] != 0;
            self.protection_disabled = !legacy_protection_active;
            self.xor_mask = 0;
        } else {
            self.mmc3.restore_registers(data);
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.prg_ram.initialize(mode);
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
        create_mapper(MapperContext::new_for_test(
            254, prg_rom, chr_rom, mirroring,
        ))
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

        // Protection is active by default but xor_mask starts at 0
        mapper.write_prg(0x6000, 0xAB);

        // Clear protection by writing to a mirrored $8000 address
        mapper.write_prg(0x8000, 0);

        // Now PRG-RAM should be readable normally
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    #[test]
    fn test_protection_uses_xor_mask_from_a001() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Write data and set XOR mask via mirrored $A001 address
        mapper.write_prg(0x6000, 0xAB);
        mapper.write_prg(0xA003, 0xFF);

        // Protected reads return PRG-RAM XOR mask
        assert_eq!(mapper.read_prg(0x6000), 0xAB ^ 0xFF);
        assert_eq!(mapper.read_prg(0x6001), 0xFF);
    }

    #[test]
    fn test_protection_cleared_by_bank_select() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Security active by default
        mapper.write_prg(0x6000, 0x42);
        mapper.write_prg(0xBFFF, 0x55);

        // Verify protection is active (PRG-RAM XOR mask)
        assert_eq!(mapper.read_prg(0x6000), 0x42 ^ 0x55);

        // Clear by writing to exact $8000
        mapper.write_prg(0x8000, 0);

        // Now should read actual PRG-RAM
        assert_eq!(mapper.read_prg(0x6000), 0x42);
    }

    #[test]
    fn test_a001_mirrors_update_xor_mask() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0x22);
        mapper.write_prg(0xA001, 0x55);
        assert_eq!(mapper.read_prg(0x6000), 0x22 ^ 0x55);

        mapper.write_prg(0xB001, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0x22 ^ 0xAA);
    }

    #[test]
    fn test_wram_ignores_mmc3_prg_ram_disable() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0x42);

        // MMC3 PRG-RAM disable bit pattern at $A001 should not disable mapper254 WRAM reads.
        mapper.write_prg(0xA001, 0x00);

        // Security still active with xor_mask=0, so WRAM is readable.
        assert_eq!(mapper.read_prg(0x6000), 0x42);
    }

    #[test]
    fn test_8002_clears_protection() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0x42);
        mapper.write_prg(0xA001, 0x55);

        // $8002 mirrors $8000 in MMC3 decode and must clear protection.
        mapper.write_prg(0x8002, 0x00);

        assert_eq!(mapper.read_prg(0x6000), 0x42);
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

        // Set security state + xor mask
        mapper.write_prg(0x6000, 0x42);
        mapper.write_prg(0xA001, 0x55);

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper254(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);

        // Security state + xor mask should be restored.
        assert_eq!(restored.read_prg(0x6001), 0x55);
    }
}
