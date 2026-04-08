//! Mapper 250 - MMC3 variant with address-encoded registers
//!
//! Specifications:
//! - Fallback: Mesen2 implementation (NesDev wiki page returns 403)
//!   Source: <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Mmc3Variants/MMC3_250.h>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 250 - MMC3 variant with address-encoded registers
///
/// Hardware: MMC3 clone where the register select (even/odd) is encoded in
/// address bit 10 instead of address bit 0, and the written value is taken
/// from the lower 8 bits of the address rather than the data bus.
///
/// Specifications:
/// - Fallback: Mesen2 MMC3_250.h
/// - PRG-ROM: Up to 512KB (standard MMC3)
/// - CHR: CHR-ROM or CHR-RAM, standard MMC3 banking
/// - Mirroring: Software-controlled (standard MMC3)
/// - IRQ: Standard MMC3 scanline counter
///
/// Write address remapping ($8000-$FFFF):
/// - Effective address = `(addr & 0xE000) | ((addr & 0x0400) >> 10)`
/// - Effective value   = `addr & 0xFF`
/// - The actual data byte on the bus is ignored for ROM-area writes
pub struct Mapper250 {
    pub(crate) mmc3: MMC3Mapper,
}

impl Mapper250 {
    const MAPPER_NUMBER: u16 = 250;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
        }
    }
}

impl Mapper for Mapper250 {
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
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => self.mmc3.write_prg(addr, value),
            0x8000..=0xFFFF => {
                let effective_addr = (addr & 0xE000) | ((addr & 0x0400) >> 10);
                let effective_value = (addr & 0xFF) as u8;
                self.mmc3.write_prg(effective_addr, effective_value);
            }
            _ => {}
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.mmc3.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value);
    }

    fn mapper_number(&self) -> u16 {
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

    fn registers_snapshot(&self) -> Vec<u8> {
        self.mmc3.registers_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.mmc3.restore_registers(data);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_mapper250(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            250, prg_rom, chr_rom, mirroring,
        ))
    }

    // --- Factory ---

    #[test]
    fn test_factory_creates_mapper_250() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok());
    }

    // --- Address-encoded register select ---
    //
    // In mapper 250, the even/odd distinction is determined by bit 10 of the
    // address (not bit 0). bit 10 = 0 → bank select (even), bit 10 = 1 → bank data (odd).
    // The value written to the MMC3 register is addr & 0xFF (not the data bus byte).

    /// Writing to $8006 (bit 10 = 0, addr & 0xFF = 6) selects MMC3 register R6.
    /// Then writing to $8404 (bit 10 = 1 = odd = bank data, addr & 0xFF = 4) sets R6 = 4.
    /// Reading $8000 must return byte from bank 4 (not some other bank).
    #[test]
    fn test_prg_bank_select_via_address_bits() {
        // 48 banks of 8KB (non-power-of-two avoids false wrapping)
        let prg_rom = banked_data(8 * 1024, 48);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Select register R6 (addr & 0xFF = 6, bit 10 = 0 → bank select)
        mapper.write_prg(0x8006, 0xFF);
        // Write bank data = 4 (bit 10 = 1 → bank data, addr & 0xFF = 4)
        mapper.write_prg(0x8404, 0xFF);

        // PRG $8000 should map to bank 4
        assert_eq!(mapper.read_prg(0x8000), 4);
    }

    /// The data bus byte is completely ignored; only addr & 0xFF is used as value.
    #[test]
    fn test_data_bus_byte_ignored() {
        let prg_rom = banked_data(8 * 1024, 48);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Select R6; write value 7 via address, but data bus = 0x00 (should be ignored)
        mapper.write_prg(0x8006, 0x00); // bank select, reg 6
        mapper.write_prg(0x8407, 0x00); // bank data, value = 7 (addr & 0xFF)

        // Must read bank 7, not bank 0
        assert_eq!(mapper.read_prg(0x8000), 7);
    }

    /// Standard MMC3 even-address bank-select interpretation does NOT apply.
    /// Writing to $8000 with value 6 should NOT select R6 (in mapper 250, the
    /// value on the data bus is irrelevant; addr & 0xFF = 0 → selects R0 instead).
    #[test]
    fn test_standard_mmc3_write_does_not_select_r6() {
        let prg_rom = banked_data(8 * 1024, 48);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // If this were standard MMC3: write(0x8000, 6) selects R6; write(0x8001, 4) → R6=4
        // In mapper 250: write(0x8000, 6) → bank_select=0 (R0 selected!); write(0x8401, 4) → bank_data=1 for R0
        // addr & 0xFF for 0x8000 = 0 → selects R0; for 0x8401 = 1 → sets R0=1 (a CHR register, not PRG R6)
        mapper.write_prg(0x8000, 6); // selects R0 (not R6) because addr & 0xFF = 0
        mapper.write_prg(0x8401, 4); // bank data, value=1 (addr & 0xFF=1 for 0x8401) → sets CHR R0=1

        // These "MMC3-style" writes only touched a CHR register; PRG bank at $8000 is still the
        // power-on default (bank 0), not any bank we tried to select via the data bus.
        assert_eq!(mapper.read_prg(0x8000), 0);

        // Now use mapper250-correct address encoding to select bank 5 for $8000:
        mapper.write_prg(0x8006, 0xFF); // bank select R6
        mapper.write_prg(0x8405, 0xFF); // bank data = 5 (0x8405 & 0xFF = 5, bit10=1)
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    // --- PRG-RAM passthrough ---

    #[test]
    fn test_prg_ram_read_write() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    // --- Mirroring ---
    //
    // In mapper 250, mirroring is set via $A000-$BFFF window with bit 10 = 0 (even).
    // Effective value = addr & 0xFF. To set horizontal (bit 0 = 1): write to addr
    // with addr & 0xE000 = $A000, bit10 = 0, addr & 0xFF = 1 → e.g. $A001.

    #[test]
    fn test_mirroring_horizontal() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        // Write mirroring = horizontal: bit10=0 (even→mirroring), addr&0xFF=1 (bit0=1→horizontal)
        mapper.write_prg(0xA001, 0x00); // effective addr=$A000, value=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_mirroring_vertical() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Horizontal).unwrap();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        // Write mirroring = vertical: addr&0xFF=0 (bit0=0→vertical), bit10=0
        mapper.write_prg(0xA000, 0xFF); // effective addr=$A000, value=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // --- CHR banking ---

    #[test]
    fn test_chr_banking_via_address_encoding() {
        let prg_rom = banked_data(8 * 1024, 4);
        // 48 CHR banks of 1KB (non-power-of-two)
        let chr_rom = banked_data(1024, 48);
        let mut mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Select R2 (CHR 1KB at PPU $1000 in CHR mode 0):
        // addr & 0xFF = 2, bit10 = 0 → bank select
        mapper.write_prg(0x8002, 0xFF);
        // Set R2 = 7: bit10 = 1 → bank data, addr & 0xFF = 7
        mapper.write_prg(0x8407, 0xFF);

        // In MMC3 CHR mode 0, R2 maps to PPU $1000
        assert_eq!(mapper.read_chr(0x1000), 7);
    }

    // --- Fixed PRG banks (last and second-last) ---

    #[test]
    fn test_fixed_last_prg_bank() {
        // 48 banks; last bank (47) should always be at $E000
        let prg_rom = banked_data(8 * 1024, 48);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Without any writes, MMC3 default: $E000-$FFFF = last bank
        assert_eq!(mapper.read_prg(0xE000), 47);
    }

    // --- Snapshot / restore ---

    #[test]
    fn test_registers_snapshot_and_restore() {
        let prg_rom = banked_data(8 * 1024, 48);
        let chr_rom = banked_data(1024, 8);
        let mut mapper =
            create_mapper250(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        // Select R6 = 10 via mapper250 address encoding
        mapper.write_prg(0x8006, 0xFF); // bank select, R6
        mapper.write_prg(0x840A, 0xFF); // bank data, value=10 (0x840A & 0xFF = 10)

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&snap);

        // After restore, R6 should be 10 → $8000 reads bank 10
        assert_eq!(restored.read_prg(0x8000), 10);
    }

    // --- mapper_number ---

    #[test]
    fn test_mapper_number() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper250(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        assert_eq!(mapper.mapper_number(), 250);
    }
}
