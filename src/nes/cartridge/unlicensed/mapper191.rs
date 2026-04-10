//! Mapper 191 – Waixing MMC3 clone with CHR-RAM at banks where bit 7 is set
//!
//! Specifications:
//! - Primary source: NESdev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_191.xhtml>
//!
//! ## Hardware behavior
//!
//! Operates identically to iNES Mapper 119 (TQROM) with two differences:
//! - CHR-RAM is 2 KiB instead of 8 KiB.
//! - **Bit 7** of the CHR bank register (A17) selects between CHR-RAM and
//!   CHR-ROM, instead of bit 6 (A16) as on TQROM.
//!
//! When the raw MMC3 CHR register value has bit 7 set, the address maps to
//! CHR-RAM; otherwise it maps to CHR-ROM using the lower 7 bits as the bank
//! index.
//!
//! CHR-RAM layout (2 KiB = 2 × 1 KiB pages):
//! - raw_bank & 0x01 == 0  →  CHR-RAM byte offset  0..=1023
//! - raw_bank & 0x01 == 1  →  CHR-RAM byte offset  1024..=2047
//!
//! All PRG, IRQ, and mirroring logic is standard MMC3.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::nes::cartridge::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 191;
const CHR_RAM_SIZE: usize = 2 * 1024;
const CHR_1K_BANK_SIZE: usize = 0x0400;
const CHR_BANK_MASK: usize = CHR_1K_BANK_SIZE - 1;
/// Bit 7 of the raw CHR register selects CHR-RAM.
const CHR_RAM_SELECT_BIT: usize = 0x80;
/// Mask for the CHR-ROM bank index when RAM is not selected.
const CHR_ROM_BANK_MASK: usize = 0x7F;
/// Mask for the 1 KiB page within the 2 KiB CHR-RAM.
const CHR_RAM_PAGE_MASK: usize = 0x01;

/// Mapper 191 – Waixing MMC3 variant with 2 KiB CHR-RAM (bit-7 select).
pub struct Mapper191 {
    pub(crate) inner: MMC3Mapper,
    chr_ram: [u8; CHR_RAM_SIZE],
}

impl Mapper191 {
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
        (raw_bank & CHR_RAM_SELECT_BIT) != 0
    }
}

impl Mapper for Mapper191 {
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
            let page = raw_bank & CHR_RAM_PAGE_MASK;
            self.chr_ram[page * CHR_1K_BANK_SIZE + offset]
        } else {
            let rom_bank = self.inner.mapped_chr_1k_bank(ppu_addr) & CHR_ROM_BANK_MASK;
            self.inner.read_chr_1k_at(rom_bank, offset)
        }
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        let raw_bank = self.inner.raw_chr_1k_bank(ppu_addr);
        let offset = (ppu_addr as usize) & CHR_BANK_MASK;
        if Self::is_chr_ram(raw_bank) {
            let page = raw_bank & CHR_RAM_PAGE_MASK;
            self.chr_ram[page * CHR_1K_BANK_SIZE + offset] = value;
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
            let chr_ram_data = &rest[..CHR_RAM_SIZE];
            self.inner.restore_registers(mmc3_data);
            self.chr_ram.copy_from_slice(chr_ram_data);
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

    // Non-power-of-two bank counts prevent modulo-wrapping false positives.
    const PRG_BANKS: usize = 6; // 6 × 8 KiB
    // CHR-ROM: 10 × 1 KiB banks (non-power-of-two).
    // raw banks 0..=127 with bit 7 clear → CHR-ROM; bit 7 set → CHR-RAM.
    const CHR_ROM_1K_BANKS: usize = 10;

    fn make_mapper() -> Mapper191 {
        Mapper191::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ─────────────────────────────────────────────────────────

    #[test]
    fn mapper_191_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 191 must be registered in the factory"
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

    #[test]
    fn prg_bank_select_via_r6() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0110); // R6
        mapper.write_prg(0x8001, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "R6=2 must map $8000 to PRG bank 2"
        );
    }

    // ── CHR-ROM reads (bit 7 clear) ───────────────────────────────────────────

    #[test]
    fn chr_rom_read_for_bank_0() {
        let mut mapper = make_mapper();
        // CHR mode 0: R0 selects 2 KiB at PPU $0000 (even-aligned: bank 4/5 pair)
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 4); // raw bank 4, bit 7=0 → CHR-ROM
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "Bank 4 (bit 7 clear) must read from CHR-ROM"
        );
    }

    #[test]
    fn chr_rom_read_for_bank_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 7); // raw bank 7, bit 7=0 → CHR-ROM
        assert_eq!(
            mapper.read_chr(0x1000),
            7,
            "Bank 7 (bit 7 clear) must read from CHR-ROM"
        );
    }

    // ── CHR-RAM reads/writes (bit 7 set) ─────────────────────────────────────

    #[test]
    fn chr_ram_selected_when_bit7_set() {
        let mut mapper = make_mapper();
        // R2 at PPU $1000; raw bank = 0x80 (bit 7 set) → page 0 of CHR-RAM
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 0x80);
        mapper.write_chr(0x1000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x1000),
            0xAB,
            "Bank 0x80 (bit 7 set, page 0) must be CHR-RAM"
        );
    }

    #[test]
    fn chr_ram_page1_selected_when_bit7_and_bit0_set() {
        let mut mapper = make_mapper();
        // R2 at PPU $1000; raw bank = 0x81 → page 1 of CHR-RAM
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 0x81);
        mapper.write_chr(0x1000, 0xCD);
        assert_eq!(
            mapper.read_chr(0x1000),
            0xCD,
            "Bank 0x81 (bit 7 set, page 1) must be CHR-RAM"
        );
    }

    #[test]
    fn chr_ram_pages_are_independent() {
        let mut mapper = make_mapper();
        // Set R2 → 0x80 (page 0) and R3 → 0x81 (page 1)
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 0x80);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 0x81);

        mapper.write_chr(0x1000, 0x11); // page 0 at $1000
        mapper.write_chr(0x1400, 0x22); // page 1 at $1400

        // Re-select to read
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 0x80);
        assert_eq!(mapper.read_chr(0x1000), 0x11, "CHR-RAM page 0 must be 0x11");
        mapper.write_prg(0x8000, 0b0000_0011);
        mapper.write_prg(0x8001, 0x81);
        assert_eq!(mapper.read_chr(0x1400), 0x22, "CHR-RAM page 1 must be 0x22");
    }

    #[test]
    fn chr_rom_bank_not_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5); // bit 7 clear → CHR-ROM
        let original = mapper.read_chr(0x1000);
        mapper.write_chr(0x1000, 0xFF);
        assert_eq!(
            mapper.read_chr(0x1000),
            original,
            "CHR-ROM must be read-only"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 0x80); // CHR-RAM page 0
        mapper.write_chr(0x1000, 0x55);

        let snap = mapper.registers_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        mapper2.write_prg(0x8000, 0b0000_0010);
        mapper2.write_prg(0x8001, 0x80);
        assert_eq!(
            mapper2.read_chr(0x1000),
            0x55,
            "CHR-RAM must survive snapshot round-trip"
        );
    }

    #[test]
    fn truncated_snapshot_restores_mmc3_but_not_chr_ram() {
        let mut mapper = make_mapper();
        // Set a CHR register so page 0 → CHR-ROM bank 2
        mapper.write_prg(0x8000, 0b0000_0000); // select CHR R0
        mapper.write_prg(0x8001, 2); // bank 2

        // Build a snapshot that only contains the MMC3 portion (no CHR-RAM bytes)
        let full_snap = mapper.registers_snapshot();
        let mmc3_only = &full_snap[..full_snap.len() - CHR_RAM_SIZE];

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(mmc3_only);

        // MMC3 state (bank selection) should be restored
        mapper2.write_prg(0x8000, 0b0000_0000);
        mapper2.write_prg(0x8001, 2);
        assert_eq!(
            mapper2.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "MMC3 register state must be restored from truncated snapshot"
        );
    }

    // ── IRQ delegation ────────────────────────────────────────────────────────

    #[test]
    fn mmc3_irq_works() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1); // latch = 1
        mapper.write_prg(0xC001, 0); // reload
        mapper.write_prg(0xE001, 0); // enable

        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 191");
    }
}
