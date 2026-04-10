//! Mapper 194 – Waixing MMC3 clone with CHR-RAM at banks 0–1
//!
//! Specifications:
//! - Primary source: NESdev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_194.xhtml>
//!
//! ## Hardware behavior
//!
//! This is a Waixing pirate board containing an MMC3 clone. It is identical
//! to a standard MMC3 except that CHR bank numbers **0 and 1** map to CHR-RAM
//! instead of CHR-ROM; all other bank numbers map to CHR-ROM.
//!
//! CHR-RAM layout (2 KiB = 2 × 1 KiB pages):
//! - raw_bank 0  →  CHR-RAM bytes    0..=1023
//! - raw_bank 1  →  CHR-RAM bytes 1024..=2047
//!
//! CHR-ROM is used for bank numbers ≥ 2.
//!
//! All PRG banking, IRQ, and mirroring logic is standard MMC3.
//!
//! Known games: Dai-2-Ji – Super Robot Taisen (As)
//!
//! See also: mappers 074, 191, 192, 195 (other Waixing MMC3+CHR-RAM boards)

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::nes::cartridge::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 194;
const CHR_RAM_SIZE: usize = 2 * 1024;
const CHR_1K_BANK_SIZE: usize = 0x0400;
const CHR_BANK_MASK: usize = CHR_1K_BANK_SIZE - 1;
const CHR_RAM_FIRST_BANK: usize = 0;
const CHR_RAM_LAST_BANK: usize = 1;

/// Mapper 194 – Waixing MMC3 variant with 2 KiB CHR-RAM at banks 0–1.
pub struct Mapper194 {
    pub(crate) inner: MMC3Mapper,
    chr_ram: [u8; CHR_RAM_SIZE],
}

impl Mapper194 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            inner: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            chr_ram: [0; CHR_RAM_SIZE],
        }
    }

    fn is_chr_ram_bank(bank: usize) -> bool {
        bank >= CHR_RAM_FIRST_BANK && bank <= CHR_RAM_LAST_BANK
    }

    fn chr_ram_index(bank: usize, offset: usize) -> usize {
        bank * CHR_1K_BANK_SIZE + offset
    }
}

impl Mapper for Mapper194 {
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
        if data.len() >= CHR_RAM_SIZE {
            let (mmc3_data, chr_ram_data) = data.split_at(data.len() - CHR_RAM_SIZE);
            self.inner.restore_registers(mmc3_data);
            self.chr_ram.copy_from_slice(chr_ram_data);
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
    // CHR-ROM: 10 × 1 KiB banks. Banks 0–1 = CHR-RAM; banks 2–9 = CHR-ROM.
    const CHR_ROM_1K_BANKS: usize = 10;

    fn make_mapper() -> Mapper194 {
        Mapper194::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ─────────────────────────────────────────────────────────

    #[test]
    fn mapper_194_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_ROM_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 194 must be registered in the factory"
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

    // ── CHR-RAM reads/writes (banks 0 and 1) ──────────────────────────────────

    #[test]
    fn chr_ram_bank_0_is_writable() {
        let mut mapper = make_mapper();
        // CHR mode 0: R0 selects 2 KiB at PPU $0000; write bank 0 (even-aligned, R0 & 0xFE = 0)
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 0); // raw bank 0 → CHR-RAM page 0
        mapper.write_chr(0x0000, 0xAA);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAA,
            "Bank 0 must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_bank_1_is_writable() {
        let mut mapper = make_mapper();
        // CHR mode 0: R0 with bank=0 maps $0000-$07FF; bank 1 maps $0400-$07FF
        mapper.write_prg(0x8000, 0b0000_0000); // R0 (raw = 0, so 0 & 0xFE = 0; bank+1 = 1)
        mapper.write_prg(0x8001, 0); // R0=0: $0000-$03FF=bank0, $0400-$07FF=bank1
        mapper.write_chr(0x0400, 0xBB);
        assert_eq!(
            mapper.read_chr(0x0400),
            0xBB,
            "Bank 1 (at $0400 with R0=0) must be CHR-RAM (writable)"
        );
    }

    #[test]
    fn chr_ram_pages_0_and_1_are_independent() {
        let mut mapper = make_mapper();
        // With R0=0: $0000-$03FF = bank 0, $0400-$07FF = bank 1
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 0);

        mapper.write_chr(0x0000, 0x11);
        mapper.write_chr(0x0400, 0x22);

        assert_eq!(mapper.read_chr(0x0000), 0x11, "bank 0 CHR-RAM must be 0x11");
        assert_eq!(mapper.read_chr(0x0400), 0x22, "bank 1 CHR-RAM must be 0x22");
    }

    // ── CHR-ROM reads (banks ≥ 2) ─────────────────────────────────────────────

    #[test]
    fn chr_rom_bank_2_is_chr_rom() {
        let mut mapper = make_mapper();
        // CHR mode 0: R2 → PPU $1000; bank 2 (bit 7 clear, not in 0–1) → CHR-ROM
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 2); // raw bank 2 → CHR-ROM
        assert_eq!(mapper.read_chr(0x1000), 2, "Bank 2 must read from CHR-ROM");
    }

    #[test]
    fn chr_rom_bank_9_is_chr_rom() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 9); // raw bank 9 → CHR-ROM
        assert_eq!(
            mapper.read_chr(0x1000),
            9 % CHR_ROM_1K_BANKS as u8,
            "Bank 9 must read from CHR-ROM"
        );
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

    // ── CHR-RAM boundary: bank 2 is not CHR-RAM ───────────────────────────────

    #[test]
    fn bank_2_is_not_chr_ram() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 2); // CHR-ROM
        let rom_val = mapper.read_chr(0x1000);

        // Write to bank 1 (CHR-RAM)
        mapper.write_prg(0x8001, 1);
        mapper.write_chr(0x1000, 0xFF);

        // Bank 2 must still read CHR-ROM
        mapper.write_prg(0x8001, 2);
        assert_eq!(
            mapper.read_chr(0x1000),
            rom_val,
            "Bank 2 must still read CHR-ROM after bank-1 write"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // Write to CHR-RAM banks 0 and 1
        mapper.write_prg(0x8000, 0b0000_0000); // R0
        mapper.write_prg(0x8001, 0);
        mapper.write_chr(0x0000, 0x55);
        mapper.write_chr(0x0400, 0x66);

        let snap = mapper.registers_snapshot();
        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        mapper2.write_prg(0x8000, 0b0000_0000);
        mapper2.write_prg(0x8001, 0);
        assert_eq!(
            mapper2.read_chr(0x0000),
            0x55,
            "bank 0 CHR-RAM must survive snapshot round-trip"
        );
        assert_eq!(
            mapper2.read_chr(0x0400),
            0x66,
            "bank 1 CHR-RAM must survive snapshot round-trip"
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
        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 194");
    }
}
