//! Mapper 195 – Waixing MMC3 with CHR-RAM at banks 0–3
//!
//! Specifications:
//! - Primary source: NESdev wiki <https://www.nesdev.org/wiki/INES_Mapper_195>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_ChrRam.h`
//!   instantiated as `MMC3_ChrRam(0x00, 0x03, 4)` for this mapper number
//!
//! ## Overview
//!
//! An MMC3-variant board used by several Waixing titles (e.g. *Contra Fighter*,
//! *Diablo*).  All PRG, IRQ, and mirroring behaviour is identical to standard
//! MMC3 (iNES mapper 4).
//!
//! The only difference is CHR memory selection: when the value stored in an
//! MMC3 CHR bank register falls in the range `0x00–0x03`, that 1 KiB page is
//! sourced from internal 4 KiB **CHR-RAM** instead of from CHR-ROM.  Bank
//! register values ≥ 4 select CHR-ROM as normal, with the ROM bank index equal
//! to the register value.
//!
//! ## CHR-RAM layout (4 KiB = 4 × 1 KiB pages)
//!
//! | raw CHR bank value | memory |
//! |--------------------|--------|
//! | `0x00`             | CHR-RAM page 0 (`0x000–0x3FF`) |
//! | `0x01`             | CHR-RAM page 1 (`0x400–0x7FF`) |
//! | `0x02`             | CHR-RAM page 2 (`0x800–0xBFF`) |
//! | `0x03`             | CHR-RAM page 3 (`0xC00–0xFFF`) |
//! | `≥ 0x04`           | CHR-ROM, bank index = raw value |

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::nes::cartridge::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 195;
/// 4 KiB CHR-RAM (4 × 1 KiB pages).
const CHR_RAM_SIZE: usize = 4 * 1024;
const CHR_1K_BANK_SIZE: usize = 0x0400;
const CHR_BANK_MASK: usize = CHR_1K_BANK_SIZE - 1;
/// Raw CHR bank values 0–3 map to CHR-RAM.
const CHR_RAM_LAST_BANK: usize = 3;

/// Mapper 195 – Waixing MMC3 variant with 4 KiB CHR-RAM (banks 0–3).
pub struct Mapper195 {
    pub(crate) inner: MMC3Mapper,
    chr_ram: [u8; CHR_RAM_SIZE],
}

impl Mapper195 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            inner: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            chr_ram: [0; CHR_RAM_SIZE],
        }
    }

    fn is_chr_ram(raw_bank: usize) -> bool {
        raw_bank <= CHR_RAM_LAST_BANK
    }
}

impl Mapper for Mapper195 {
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

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & CHR_BANK_MASK;
        if Self::is_chr_ram(raw_bank) {
            self.chr_ram[raw_bank * CHR_1K_BANK_SIZE + offset]
        } else {
            let rom_bank = self.inner.mapped_chr_1k_bank(ppu_addr);
            self.inner.read_chr_1k_at(rom_bank, offset)
        }
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & CHR_BANK_MASK;
        if Self::is_chr_ram(raw_bank) {
            self.chr_ram[raw_bank * CHR_1K_BANK_SIZE + offset] = value;
        } else {
            let mapped = self.inner.mapped_chr_1k_bank(ppu_addr);
            self.inner.write_chr_1k_at(mapped, offset, value);
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
        let mmc3_len = self.inner.registers_snapshot().len();
        if data.len() >= mmc3_len + CHR_RAM_SIZE {
            let (mmc3_data, rest) = data.split_at(mmc3_len);
            self.inner.restore_registers(mmc3_data);
            self.chr_ram.copy_from_slice(&rest[..CHR_RAM_SIZE]);
        } else {
            self.inner.restore_registers(data);
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

    // 6 × 8 KiB PRG banks (non-power-of-two).
    const PRG_BANKS: usize = 6;
    // CHR-ROM: 10 × 1 KiB banks (values 4–13 map to CHR-ROM; 0–3 map to CHR-RAM).
    const CHR_ROM_1K_BANKS: usize = 10;

    fn make_mapper() -> Mapper195 {
        Mapper195::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_195_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 195 must be registered in the factory"
        );
    }

    #[test]
    fn prg_fixed_last_bank_at_e000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Fixed-last PRG bank at $E000 must be the last bank"
        );
    }

    #[test]
    fn chr_ram_selected_for_bank_0() {
        let mut mapper = make_mapper();
        // R2 at PPU $1000; raw bank 0 → CHR-RAM page 0
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 0);
        mapper.write_chr(0x1000, 0xAA);
        assert_eq!(
            mapper.read_chr(0x1000),
            0xAA,
            "Bank 0 must route to CHR-RAM"
        );
    }

    #[test]
    fn chr_ram_selected_for_bank_3() {
        let mut mapper = make_mapper();
        // R3 at PPU $1400; raw bank 3 → CHR-RAM page 3
        mapper.write_prg(0x8000, 0b0000_0011); // select R3
        mapper.write_prg(0x8001, 3);
        mapper.write_chr(0x1400, 0xBB);
        assert_eq!(
            mapper.read_chr(0x1400),
            0xBB,
            "Bank 3 must route to CHR-RAM"
        );
    }

    #[test]
    fn chr_ram_pages_are_independent() {
        let mut mapper = make_mapper();
        // Write different values to CHR-RAM pages 0 and 1
        mapper.write_prg(0x8000, 0b0000_0000); // select R0
        mapper.write_prg(0x8001, 0); // raw bank 0 → CHR-RAM page 0
        mapper.write_chr(0x0000, 0x11);

        mapper.write_prg(0x8000, 0b0000_0000); // R0 selects 2KB at $0000-$07FF
        mapper.write_prg(0x8001, 2); // raw bank 2 (even-aligned: 2,3) → CHR-RAM pages 2,3
        mapper.write_chr(0x0000, 0x22);

        // Re-select page 0 and verify original value is intact
        mapper.write_prg(0x8001, 0); // back to bank 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0x11,
            "CHR-RAM page 0 must be independent from page 2"
        );
    }

    #[test]
    fn chr_rom_selected_for_bank_4() {
        let mut mapper = make_mapper();
        // R2 at PPU $1000; raw bank 4 → CHR-ROM bank 4 (value 4 in banked_data = 4)
        mapper.write_prg(0x8000, 0b0000_0010); // select R2
        mapper.write_prg(0x8001, 4);
        assert_eq!(mapper.read_chr(0x1000), 4, "Bank 4 must read from CHR-ROM");
    }

    #[test]
    fn chr_rom_selected_for_bank_9() {
        let mut mapper = make_mapper();
        // R3 at PPU $1400; raw bank 9 → CHR-ROM bank 9 % 10 = 9
        mapper.write_prg(0x8000, 0b0000_0011); // select R3
        mapper.write_prg(0x8001, 9);
        assert_eq!(mapper.read_chr(0x1400), 9, "Bank 9 must read from CHR-ROM");
    }

    #[test]
    fn chr_rom_write_is_ignored() {
        let mut mapper = make_mapper();
        // R2 at PPU $1000; raw bank 5 (ROM) — write must not alter CHR-ROM
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 5);
        let before = mapper.read_chr(0x1000);
        mapper.write_chr(0x1000, 0xFF);
        assert_eq!(
            mapper.read_chr(0x1000),
            before,
            "Write to CHR-ROM-backed slot must be a no-op"
        );
    }

    #[test]
    fn snapshot_preserves_chr_ram() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 1);
        mapper.write_chr(0x1000, 0xCC);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // After restore, R2 should still point to bank 1, CHR-RAM page 1
        assert_eq!(
            restored.read_chr(0x1000),
            0xCC,
            "CHR-RAM must survive snapshot round-trip"
        );
    }
}
