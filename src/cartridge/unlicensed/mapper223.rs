//! Mapper 223 – duplicate of Mapper 199 (Waixing MMC3 variant with CHR-RAM/ROM interleaving)
//!
//! # Specifications
//! - Primary source: NesDev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_223.xhtml>
//!   > "iNES Mapper 223 appears to be a duplicate of iNES Mapper 199. Mapper 199 should be used."
//! - Fallback: Mesen2 does not implement mapper 223 (no entry in MapperFactory.cpp).
//!
//! ## Hardware behaviour
//!
//! Mapper 223 is hardware-identical to Mapper 199: an MMC3-based variant used by
//! Chinese pirate boards that extends the standard MMC3 with:
//!
//! - **8 KiB CHR-RAM** (pages 0–7) interleaved with CHR-ROM (pages 8+)
//! - **4 extra registers** `ex_regs[4]` initialised to `{0xFE, 0xFF, 0x01, 0x03}`
//! - **Extra PRG registers**: `$C000–$DFFF` and `$E000–$FFFF` windows controlled by
//!   `ex_regs[0]` and `ex_regs[1]` rather than the standard MMC3 fixed banks
//! - **Custom CHR slots 0–3**: PPU `$0000–$0FFF` always uses
//!   `reg[0]` / `ex_regs[2]` / `reg[1]` / `ex_regs[3]`
//! - **4-way mirroring** via `$A000` bits \[1:0\]: 0=Vertical, 1=Horizontal,
//!   2=SingleScreenLower, 3=SingleScreenUpper
//!
//! All functional details are identical to Mapper 199; see that module for
//! the full hardware description.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::unlicensed::mapper199::Mapper199;

/// Mapper 223 – hardware-identical to Mapper 199.
///
/// Implemented as a thin wrapper around [`Mapper199`] so all behaviour is
/// shared; only [`mapper_number`](Mapper::mapper_number) differs.
pub struct Mapper223(Mapper199);

impl Mapper223 {
    const MAPPER_NUMBER: u16 = 223;

    pub fn new(ctx: MapperContext) -> Self {
        Self(Mapper199::new(ctx))
    }
}

impl Mapper for Mapper223 {
    fn base(&self) -> &BaseMapper {
        self.0.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.0.base_mut()
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        self.0.mmc3_delegate()
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        self.0.mmc3_delegate_mut()
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.0.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.0.write_prg(addr, value)
    }

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        self.0.read_chr(ppu_addr)
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        self.0.write_chr(ppu_addr, value)
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.0.initialize_ram(mode)
    }

    fn wram_size(&self) -> usize {
        self.0.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.0.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.0.load_wram_snapshot(data)
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.0.registers_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.0.restore_registers(data)
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.0.capabilities()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::ines::NametableLayout;
    use crate::cartridge::mapper::create_mapper;

    fn make_mapper(prg_kb: usize, chr_kb: usize) -> Box<dyn Mapper> {
        let prg_rom = vec![0u8; prg_kb * 1024];
        let chr_rom = vec![0u8; chr_kb * 1024];
        create_mapper(MapperContext::new_for_test(
            223,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("mapper 223 creation should succeed")
    }

    fn make_mapper_prg_pattern(prg_banks: usize) -> Box<dyn Mapper> {
        let mut prg_rom = vec![0u8; prg_banks * 8 * 1024];
        for bank in 0..prg_banks {
            let start = bank * 8 * 1024;
            for b in prg_rom[start..start + 8 * 1024].iter_mut() {
                *b = bank as u8;
            }
        }
        let chr_rom = vec![0u8; 128 * 1024];
        create_mapper(MapperContext::new_for_test(
            223,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("mapper 223 prg-pattern creation should succeed")
    }

    fn make_mapper_chr_pattern(chr_kb_banks: usize) -> Box<dyn Mapper> {
        let prg_rom = vec![0u8; 8 * 1024];
        let mut chr_rom = vec![0u8; chr_kb_banks * 1024];
        for bank in 0..chr_kb_banks {
            let start = bank * 1024;
            for b in chr_rom[start..start + 1024].iter_mut() {
                *b = bank as u8;
            }
        }
        create_mapper(MapperContext::new_for_test(
            223,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("mapper 223 chr-pattern creation should succeed")
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    #[test]
    fn mapper_223_is_registered_in_factory() {
        let mapper = make_mapper(64, 128);
        assert_eq!(mapper.mapper_number(), 223);
    }

    // -----------------------------------------------------------------------
    // PRG banking — mirrors mapper 199 behaviour
    // -----------------------------------------------------------------------

    #[test]
    fn prg_default_maps_last_two_8k_banks_to_c000_and_e000() {
        // 8 banks × 8 KiB = 64 KiB PRG-ROM (banks 0–7)
        let mapper = make_mapper_prg_pattern(8);
        // Default ex_regs: ex_regs[0]=0xFE (penultimate), ex_regs[1]=0xFF (last)
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 should map to bank 6 (0xFE mod 8)"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 should map to bank 7 (0xFF mod 8)"
        );
    }

    #[test]
    fn prg_bank_select_changes_8000_bfff_window() {
        // 16 banks × 8 KiB = 128 KiB PRG-ROM (banks 0–15)
        let mut mapper = make_mapper_prg_pattern(16);
        // MMC3 bank_select=6 (PRG mode 0), bank_data=3 → $8000 maps to bank 3
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 should map to bank 3");
    }

    #[test]
    fn prg_ex_regs_control_c000_and_e000_windows() {
        // 16 banks × 8 KiB = 128 KiB PRG-ROM (banks 0–15)
        let mut mapper = make_mapper_prg_pattern(16);
        // Write ex_regs[0]=2 via bank_select $08, ex_regs[1]=3 via $09
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0x8001, 0x02);
        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0x8001, 0x03);
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "$C000 should map to ex_regs[0]=2"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            3,
            "$E000 should map to ex_regs[1]=3"
        );
    }

    // -----------------------------------------------------------------------
    // CHR banking — pages 0–7 are CHR-RAM, pages 8+ are CHR-ROM
    // -----------------------------------------------------------------------

    #[test]
    fn chr_rom_bank_8_reads_correctly() {
        // 32 1-KiB CHR-ROM banks (bank 0–31), filled with index value
        let mut mapper = make_mapper_chr_pattern(32);
        // Select CHR-ROM page 8 for PPU $1000 via MMC3 CHR register R2
        mapper.write_prg(0x8000, 0x02); // bank_select = reg 2 (1 KiB CHR bank at $1000)
        mapper.write_prg(0x8001, 0x08); // page 8
        // $1000 should now read from CHR-ROM page 8
        assert_eq!(mapper.read_chr(0x1000), 8, "CHR-ROM page 8 should read 8");
    }

    #[test]
    fn chr_ram_page_0_is_writable() {
        let mut mapper = make_mapper(64, 128);
        // bank_select=0 → slot 0 uses CHR-bank reg 0 (default 0 → CHR-RAM page 0)
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x00);
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR-RAM page 0 write should be readable"
        );
    }

    // -----------------------------------------------------------------------
    // 4-way mirroring via $A000
    // -----------------------------------------------------------------------

    #[test]
    fn mirroring_0_is_vertical() {
        let mut mapper = make_mapper(64, 128);
        mapper.write_prg(0xA000, 0x00);
        assert_eq!(mapper.base().mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_1_is_horizontal() {
        let mut mapper = make_mapper(64, 128);
        mapper.write_prg(0xA000, 0x01);
        assert_eq!(mapper.base().mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_2_is_single_screen_lower() {
        let mut mapper = make_mapper(64, 128);
        mapper.write_prg(0xA000, 0x02);
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::SingleScreenLower
        );
    }

    #[test]
    fn mirroring_3_is_single_screen_upper() {
        let mut mapper = make_mapper(64, 128);
        mapper.write_prg(0xA000, 0x03);
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::SingleScreenUpper
        );
    }

    // -----------------------------------------------------------------------
    // Capabilities
    // -----------------------------------------------------------------------

    #[test]
    fn capabilities_has_dynamic_mirroring() {
        let mapper = make_mapper(64, 128);
        assert!(mapper.capabilities().has_dynamic_mirroring);
    }

    // -----------------------------------------------------------------------
    // Save state round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn snapshot_round_trips_registers_and_chr_ram() {
        let mut mapper = make_mapper(64, 128);
        // Set ex_regs[0]=4 and ex_regs[1]=5
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0x8001, 0x04);
        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0x8001, 0x05);
        // Write a byte to CHR-RAM page 0
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x00);
        mapper.write_chr(0x0000, 0xCC);

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper(64, 128);
        mapper2.restore_registers(&snap);

        assert_eq!(
            mapper2.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "ex_regs[0] should survive snapshot"
        );
        assert_eq!(
            mapper2.read_prg(0xE000),
            mapper.read_prg(0xE000),
            "ex_regs[1] should survive snapshot"
        );
        mapper2.write_prg(0x8000, 0x00);
        mapper2.write_prg(0x8001, 0x00);
        assert_eq!(
            mapper2.read_chr(0x0000),
            0xCC,
            "CHR-RAM should survive snapshot"
        );
    }
}
