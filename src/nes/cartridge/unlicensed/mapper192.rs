//! Mapper 192 – Waixing FS308 board (MMC3 variant with 4 KiB CHR-RAM at banks 8–11)
//!
//! Specifications:
//! - Primary source: NESdev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_192.xhtml>
//!
//! ## Hardware behavior
//!
//! This is effectively iNES Mapper 074 (Waixing MMC3 with 2 KiB CHR-RAM at
//! banks 8–9) but with **twice as much CHR-RAM** (4 KiB = 4 × 1 KiB pages).
//! CHR bank numbers 8, 9, 10, and 11 map to CHR-RAM; all other bank numbers
//! map to CHR-ROM.
//!
//! CHR-RAM layout (4 KiB total):
//! - raw_bank  8  →  CHR-RAM bytes   0..=1023
//! - raw_bank  9  →  CHR-RAM bytes 1024..=2047
//! - raw_bank 10  →  CHR-RAM bytes 2048..=3071
//! - raw_bank 11  →  CHR-RAM bytes 3072..=4095
//!
//! All PRG banking, IRQ, and mirroring logic is standard MMC3.
//!
//! Note: When a NES 2.0 header is present, mappers 74 and 192 should be
//! emulated identically because NES 2.0 specifies CHR-RAM size separately.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::nes::cartridge::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 192;
const CHR_RAM_SIZE: usize = 4 * 1024;
const CHR_1K_BANK_SIZE: usize = 0x0400;
const CHR_BANK_MASK: usize = CHR_1K_BANK_SIZE - 1;
const CHR_RAM_FIRST_BANK: usize = 8;
const CHR_RAM_LAST_BANK: usize = 11;

/// Mapper 192 – Waixing MMC3 variant with 4 KiB CHR-RAM at banks 8–11.
pub struct Mapper192 {
    pub(crate) inner: MMC3Mapper,
    chr_ram: [u8; CHR_RAM_SIZE],
}

impl Mapper192 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            inner: MMC3Mapper::new(ctx),
            chr_ram: [0; CHR_RAM_SIZE],
        }
    }

    fn is_chr_ram_bank(bank: usize) -> bool {
        (CHR_RAM_FIRST_BANK..=CHR_RAM_LAST_BANK).contains(&bank)
    }

    fn chr_ram_index(bank: usize, offset: usize) -> usize {
        (bank - CHR_RAM_FIRST_BANK) * CHR_1K_BANK_SIZE + offset
    }
}

impl Mapper for Mapper192 {
    fn base(&self) -> &BaseMapper {
        &self.inner.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.inner.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.inner)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.inner)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.inner.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.inner.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.inner.write_prg(addr, value);
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.inner.initialize_ram(mode);
        crate::nes::console::initialize_ram(&mut self.chr_ram, mode);
    }

    fn initialize_chr_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        crate::nes::console::initialize_ram(&mut self.chr_ram, mode);
    }

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & CHR_BANK_MASK;
        if Self::is_chr_ram_bank(raw_bank) {
            self.chr_ram[Self::chr_ram_index(raw_bank, offset)]
        } else {
            let wrapped_bank = self.inner.mapped_chr_1k_bank(ppu_addr);
            self.inner.read_chr_1k_at(wrapped_bank, offset)
        }
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & CHR_BANK_MASK;
        if Self::is_chr_ram_bank(raw_bank) {
            self.chr_ram[Self::chr_ram_index(raw_bank, offset)] = value;
        } else {
            let wrapped_bank = self.inner.mapped_chr_1k_bank(ppu_addr);
            self.inner.write_chr_1k_at(wrapped_bank, offset, value);
        }
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        self.inner.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.inner.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.inner.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        snap.extend_from_slice(&self.chr_ram);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let mmc3_snapshot_len = self.inner.registers_snapshot().len();
        if data.len() >= mmc3_snapshot_len + CHR_RAM_SIZE {
            let (mmc3_data, chr_ram_data) = data.split_at(data.len() - CHR_RAM_SIZE);
            self.inner.restore_registers(mmc3_data);
            self.chr_ram.copy_from_slice(chr_ram_data);
        } else {
            self.inner.restore_registers(data);
            self.chr_ram.fill(0);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
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
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts prevent modulo-wrapping false positives.
    const PRG_BANKS: usize = 6; // 6 × 8 KiB
    // CHR-ROM: 14 × 1 KiB banks. Banks 8–11 = CHR-RAM; 0–7 and 12–13 = CHR-ROM.
    const CHR_ROM_1K_BANKS: usize = 14;

    fn make_mapper() -> Mapper192 {
        Mapper192::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ─────────────────────────────────────────────────────────

    #[test]
    fn mapper_192_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 192 must be registered in the factory"
        );
    }

    // ── PRG banking ───────────────────────────────────────────────────────────

    #[test]
    fn prg_fixed_last_bank_at_e000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Fixed-last PRG bank at $E000 must be the last bank"
        );
    }

    // ── CHR-ROM reads (banks outside 8–11) ────────────────────────────────────

    #[test]
    fn chr_rom_bank_7_is_chr_rom() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 7); // bank 7 < 8 → CHR-ROM
        assert_eq!(mapper.read_chr(0x1000), 7, "Bank 7 must read from CHR-ROM");
    }

    #[test]
    fn chr_rom_bank_12_is_chr_rom() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 12); // bank 12 > 11 → CHR-ROM
        assert_eq!(
            mapper.read_chr(0x1000),
            12 % CHR_ROM_1K_BANKS as u8,
            "Bank 12 must read from CHR-ROM"
        );
    }

    // ── CHR-RAM reads/writes (banks 8–11) ─────────────────────────────────────

    #[test]
    fn chr_ram_bank_8_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 8);
        mapper.write_chr(0x1000, 0xAA);
        assert_eq!(
            mapper.read_chr(0x1000),
            0xAA,
            "Bank 8 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_bank_9_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 9);
        mapper.write_chr(0x1400, 0xBB);
        assert_eq!(
            mapper.read_chr(0x1400),
            0xBB,
            "Bank 9 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_bank_10_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 10);
        mapper.write_chr(0x1800, 0xCC);
        assert_eq!(
            mapper.read_chr(0x1800),
            0xCC,
            "Bank 10 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_bank_11_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0101); // R5
        mapper.write_prg(0x8001, 11);
        mapper.write_chr(0x1C00, 0xDD);
        assert_eq!(
            mapper.read_chr(0x1C00),
            0xDD,
            "Bank 11 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_banks_8_to_11_are_independent() {
        let mut mapper = make_mapper();
        // Assign each CHR-RAM bank to a different 1 KiB slot
        mapper.write_prg(0x8000, 0b0000_0010); // R2 → $1000
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0011); // R3 → $1400
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b0000_0100); // R4 → $1800
        mapper.write_prg(0x8001, 10);
        mapper.write_prg(0x8000, 0b0000_0101); // R5 → $1C00
        mapper.write_prg(0x8001, 11);

        mapper.write_chr(0x1000, 0x08);
        mapper.write_chr(0x1400, 0x09);
        mapper.write_chr(0x1800, 0x0A);
        mapper.write_chr(0x1C00, 0x0B);

        assert_eq!(mapper.read_chr(0x1000), 0x08, "bank 8 must be 0x08");
        assert_eq!(mapper.read_chr(0x1400), 0x09, "bank 9 must be 0x09");
        assert_eq!(mapper.read_chr(0x1800), 0x0A, "bank 10 must be 0x0A");
        assert_eq!(mapper.read_chr(0x1C00), 0x0B, "bank 11 must be 0x0B");
    }

    #[test]
    fn chr_rom_bank_not_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5); // bank 5 = CHR-ROM
        let original = mapper.read_chr(0x1000);
        mapper.write_chr(0x1000, 0xFF);
        assert_eq!(
            mapper.read_chr(0x1000),
            original,
            "CHR-ROM must be read-only"
        );
    }

    // ── CHR-RAM boundary: banks 7 and 12 not redirected ───────────────────────

    #[test]
    fn bank_7_is_not_chr_ram() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 7);
        let rom_val = mapper.read_chr(0x1000);
        // Write to bank 8
        mapper.write_prg(0x8001, 8);
        mapper.write_chr(0x1000, 0xFF);
        // Switch back to bank 7
        mapper.write_prg(0x8001, 7);
        assert_eq!(
            mapper.read_chr(0x1000),
            rom_val,
            "Bank 7 must still read CHR-ROM after bank-8 write"
        );
    }

    #[test]
    fn bank_12_is_not_chr_ram() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 12);
        let rom_val = mapper.read_chr(0x1000);
        // Write to bank 11
        mapper.write_prg(0x8001, 11);
        mapper.write_chr(0x1000, 0xFF);
        // Switch back to bank 12
        mapper.write_prg(0x8001, 12);
        assert_eq!(
            mapper.read_chr(0x1000),
            rom_val,
            "Bank 12 must still read CHR-ROM after bank-11 write"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 8); // CHR-RAM bank 8
        mapper.write_chr(0x1000, 0x77);
        mapper.write_prg(0x8001, 9); // CHR-RAM bank 9
        mapper.write_chr(0x1000, 0x88);

        let snap = mapper.registers_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        mapper2.write_prg(0x8000, 0b0000_0010);
        mapper2.write_prg(0x8001, 8);
        assert_eq!(
            mapper2.read_chr(0x1000),
            0x77,
            "bank 8 CHR-RAM must survive snapshot round-trip"
        );
        mapper2.write_prg(0x8001, 9);
        assert_eq!(
            mapper2.read_chr(0x1000),
            0x88,
            "bank 9 CHR-RAM must survive snapshot round-trip"
        );
    }

    // ── IRQ delegation ────────────────────────────────────────────────────────

    #[test]
    fn mmc3_irq_works() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE001, 0);

        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 192");
    }
}
