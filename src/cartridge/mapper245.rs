//! Mapper 245 - Waixing MMC3 variant
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_245>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 245 - Waixing MMC3 variant
///
/// Hardware: MMC3 clone with extended PRG addressing via CHR register.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_245>
/// - PRG-ROM: Up to 1MB (extended via CHR R0 bit 0 as PRG A18)
/// - CHR-RAM: 8KB (no CHR-ROM banking)
/// - Mirroring: Software-controlled (standard MMC3)
/// - IRQ: Standard MMC3 scanline counter
///
/// Differences from standard MMC3:
/// - Uses CHR-RAM only (no CHR-ROM banking)
/// - CHR register R0 bit 0 provides an extra PRG address line
/// - PRG bank = ((R0 & 1) << 5) | (mmc3_bank & 0x3F)
/// - This extends PRG addressing to 1MB (64 × 8KB × 2 = 128 banks)
pub struct Mapper245 {
    mmc3: MMC3Mapper,
}

impl Mapper245 {
    const MAPPER_NUMBER: u8 = 245;
    const PRG_BANK_SIZE: usize = 0x2000; // 8KB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        Self {
            mmc3: MMC3Mapper::new(prg_rom, chr_rom, mirroring),
        }
    }

    /// Gets the high PRG bit from MMC3 CHR register R0, bit 0.
    fn prg_high_bit(&self) -> usize {
        // R0 is CHR register 0 in MMC3 (mapped_chr_1k_bank at $0000 or $1000
        // depending on CHR mode). We use mapped_chr_1k_bank at $0000 with chr_mode=0
        // to get the raw R0 value. But actually, we need the raw register value.
        // We can get it from the mapped CHR bank for slot 0.
        // In CHR mode 0: R0 maps to $0000. In CHR mode 1: R0 maps to $1000.
        // The simplest approach: just look at the CHR bank at $0000 and $1000
        // and extract bit 0. For Mapper 245, R0 is always used for the high bit.
        //
        // Actually, we can peek at R0 via mapped_chr_1k_bank. In mode 0,
        // $0000-$07FF uses R0 & 0xFE. In mode 1, $1000-$13FF uses R0 & 0xFE.
        // Either way, the raw R0 bit0 is what we need.
        // mapped_chr_1k_bank already masks R0. Let's use both addresses to
        // recover it.
        //
        // Better approach: look at the snapshot registers to extract R0.
        // But the cleanest: the CHR bank at $0000 in mode 0 provides R0>>1
        // Since CHR-RAM has 8 banks (1KB each), and R0 low bit affects nothing
        // in CHR addressing, we need the raw register.
        //
        // The mapped_chr_1k_bank returns the 1KB bank index which is R0 for
        // the first slot. In standard CHR mode, R0 is used as a 2KB bank
        // selector (bit 0 is forced to 0 for even, 1 for odd). So
        // mapped_chr_1k_bank($0000) returns R0 & 0xFE in mode 0.
        // We can get bit 0 from the full R0 by checking $0000 and $0001 slots.
        //
        // Actually, the simplest: use registers_snapshot to peek at regs.
        let snap = self.mmc3.registers_snapshot();
        // MMC3 snapshot format: [bank_select, regs[0..8], irq_latch, ...]
        // regs[0] = R0
        if snap.len() > 1 {
            (snap[1] as usize) & 1
        } else {
            0
        }
    }

    fn apply_prg_extension(&self, raw_bank: usize) -> usize {
        let high = self.prg_high_bit();
        (raw_bank & 0x3F) | (high << 6)
    }
}

impl Mapper for Mapper245 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let raw_bank = self.mmc3.mapped_prg_bank(addr);
                let bank = self.apply_prg_extension(raw_bank);
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(bank, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.mmc3.write_prg(addr, value);
    }

    fn read_chr(&self, addr: u16) -> u8 {
        // CHR-RAM only, use standard MMC3 CHR mapping
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
        self.mmc3.registers_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.mmc3.restore_registers(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_chr_banking: false, // CHR-RAM only
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

    fn create_mapper245(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(245, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn test_factory_creates_mapper_245() {
        let prg_rom = banked_data(8 * 1024, 64);
        let chr_rom = vec![0u8; 8 * 1024]; // CHR-RAM
        let mapper = create_mapper245(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok());
    }

    #[test]
    fn test_standard_mmc3_prg_banking() {
        let prg_rom = banked_data(8 * 1024, 64);
        let chr_rom = vec![0u8; 8 * 1024];
        let mut mapper = create_mapper245(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set R6 = bank 5 (PRG bank at $8000 in mode 0)
        mapper.write_prg(0x8000, 6); // bank_select reg 6
        mapper.write_prg(0x8001, 5); // R6 = 5

        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn test_prg_extension_via_chr_r0() {
        // 128 banks of 8KB = 1MB PRG
        let prg_rom = banked_data(8 * 1024, 128);
        let chr_rom = vec![0u8; 8 * 1024];
        let mut mapper = create_mapper245(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set R0 = 1 (bit 0 = 1, extends PRG by adding bit 6)
        mapper.write_prg(0x8000, 0); // bank_select reg 0 (R0)
        mapper.write_prg(0x8001, 1); // R0 = 1 (bit 0 set)

        // Set R6 = 5 (PRG bank at $8000 in mode 0)
        mapper.write_prg(0x8000, 6); // bank_select reg 6
        mapper.write_prg(0x8001, 5); // R6 = 5

        // PRG bank should be (5 & 0x3F) | (1 << 6) = 5 | 64 = 69
        assert_eq!(mapper.read_prg(0x8000), 69);
    }

    #[test]
    fn test_prg_extension_bit_clear() {
        let prg_rom = banked_data(8 * 1024, 128);
        let chr_rom = vec![0u8; 8 * 1024];
        let mut mapper = create_mapper245(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set R0 = 0 (bit 0 = 0, no extension)
        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0x8001, 0);

        // Set R6 = 5
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 5);

        // PRG bank should be 5 (no high bit)
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn test_chr_ram_read_write() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = vec![]; // Empty = CHR-RAM
        let mut mapper = create_mapper245(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0x42);
        assert_eq!(mapper.read_chr(0x0000), 0x42);
    }

    #[test]
    fn test_mirroring() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = vec![0u8; 8 * 1024];
        let mut mapper = create_mapper245(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // MMC3 mirroring: write to $A000 (even)
        mapper.write_prg(0xA000, 1); // bit 0 = 1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_prg_ram() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = vec![0u8; 8 * 1024];
        let mut mapper = create_mapper245(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(8 * 1024, 128);
        let chr_rom = vec![0u8; 8 * 1024];
        let mut mapper =
            create_mapper245(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        // Set R0=1 (high PRG bit), R6=10
        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 10);

        let regs = mapper.registers_snapshot();

        let mut restored = create_mapper245(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&regs);

        // PRG bank = (10 & 0x3F) | (1 << 6) = 74
        assert_eq!(restored.read_prg(0x8000), 74);
    }
}
