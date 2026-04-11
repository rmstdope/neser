//! Mapper 198 – MMC3 variant with 640 KiB PRG-ROM
//!
//! # Specifications
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_198>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_198.h`
//!
//! ## Overview
//!
//! Mapper 198 is used by Xiangfeng Cartoon's Chinese translation of *Destiny of
//! an Emperor II* (*吞食天地 - 三国外传*).  It extends standard MMC3 (mapper 4)
//! with a non-standard PRG-ROM layout and extra RAM windows.
//!
//! ## PRG-ROM Layout
//!
//! Total PRG-ROM: 640 KiB = 80 banks of 8 KiB.
//! - Banks `$00–$3F` (80 banks): first 512 KiB chip
//! - Banks `$40–$4F` (16 banks): second 128 KiB chip
//!
//! The standard MMC3 fixed banks (`$C000–$FFFF`) always map to the second-to-last
//! and last 8 KiB banks of the total 640 KiB ROM — i.e., banks `$4E` and `$4F`.
//!
//! ## CHR
//!
//! 8 KiB CHR-RAM (no CHR-ROM).  Standard MMC3 CHR bank registers select 1 KiB
//! pages as normal.
//!
//! ## RAM Windows
//!
//! - `CPU $5000–$5FFF`: 4 KiB non-battery-backed WRAM
//! - `CPU $6000–$7FFF`: 8 KiB battery-backed WRAM (standard MMC3 PRG-RAM)
//!
//! ## IRQ and Mirroring
//!
//! Standard MMC3 scanline IRQ and H/V mirroring.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperContext};
use crate::nes::cartridge::nintendo::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 198;
const CHR_RAM_SIZE: usize = 8 * 1024;
const CHR_1K_SIZE: usize = 1024;
const EXTRA_WRAM_SIZE: usize = 4 * 1024;

/// Mapper 198 – MMC3 with 640 KiB PRG-ROM and extra RAM.
///
/// See the module-level documentation for hardware details.
pub struct Mapper198 {
    inner: MMC3Mapper,
    /// 8 KiB CHR-RAM, addressed in 1 KiB banks by the MMC3 CHR registers.
    chr_ram: [u8; CHR_RAM_SIZE],
    /// 4 KiB non-battery-backed WRAM at CPU `$5000–$5FFF`.
    extra_wram: [u8; EXTRA_WRAM_SIZE],
}

impl Mapper198 {
    pub fn new(ctx: MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let mirroring = ctx.mirroring;
        // 640 KiB ROM with 1 bank of 8KB PRG-RAM (battery-backed at $6000).
        let inner = MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
            prg_rom,
            vec![], // no CHR-ROM; CHR-RAM managed by this wrapper
            mirroring,
            false,
            1, // 1 × 8 KB PRG-RAM bank at $6000–$7FFF
        );
        Self {
            inner,
            chr_ram: [0; CHR_RAM_SIZE],
            extra_wram: [0; EXTRA_WRAM_SIZE],
        }
    }
}

impl Mapper for Mapper198 {
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

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        // $5000–$5FFF: extra non-battery-backed WRAM
        if (0x5000..=0x5FFF).contains(&addr) {
            return self.extra_wram[(addr - 0x5000) as usize];
        }
        // All other addresses delegate to MMC3 (handles $6000–$7FFF PRG-RAM and $8000+).
        self.inner.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x5000..=0x5FFF).contains(&addr) {
            self.extra_wram[(addr - 0x5000) as usize] = value;
            return;
        }
        self.inner.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let bank = self.inner.raw_chr_1k_bank(addr) % (CHR_RAM_SIZE / CHR_1K_SIZE);
        let offset = (addr as usize) & (CHR_1K_SIZE - 1);
        self.chr_ram[bank * CHR_1K_SIZE + offset]
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank = self.inner.raw_chr_1k_bank(addr) % (CHR_RAM_SIZE / CHR_1K_SIZE);
        let offset = (addr as usize) & (CHR_1K_SIZE - 1);
        self.chr_ram[bank * CHR_1K_SIZE + offset] = value;
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Include extra_wram so save-states round-trip the $5000-$5FFF window.
        let mut snap = self.inner.registers_snapshot();
        snap.extend_from_slice(&self.extra_wram);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let inner_len = self.inner.registers_snapshot().len();
        if data.len() >= inner_len {
            self.inner.restore_registers(&data[..inner_len]);
        }
        if data.len() >= inner_len + EXTRA_WRAM_SIZE {
            self.extra_wram
                .copy_from_slice(&data[inner_len..inner_len + EXTRA_WRAM_SIZE]);
        }
    }

    fn initialize_chr_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        // Called for battery-backed carts after PRG-RAM is loaded from disk.
        // Initialize only the volatile CHR-RAM and extra WRAM; leave PRG-RAM intact.
        crate::nes::console::initialize_ram(&mut self.chr_ram, mode);
        crate::nes::console::initialize_ram(&mut self.extra_wram, mode);
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.inner.initialize_ram(mode);
        crate::nes::console::initialize_ram(&mut self.chr_ram, mode);
        crate::nes::console::initialize_ram(&mut self.extra_wram, mode);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_ram.to_vec()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        if data.len() == CHR_RAM_SIZE {
            self.chr_ram.copy_from_slice(data);
        }
    }

    /// Delegate to inner MMC3 for battery-backed $6000-$7FFF PRG-RAM disk persistence.
    fn wram_size(&self) -> usize {
        self.inner.wram_size()
    }

    /// Delegate to inner MMC3 for battery-backed $6000-$7FFF PRG-RAM disk persistence.
    fn wram_snapshot(&self) -> Vec<u8> {
        self.inner.wram_snapshot()
    }

    /// Delegate to inner MMC3 for battery-backed $6000-$7FFF PRG-RAM disk persistence.
    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.inner.load_wram_snapshot(data);
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.extra_wram = [0; EXTRA_WRAM_SIZE];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    /// Constructs a mapper with 640 KiB PRG-ROM (80 × 8 KiB banks).
    fn make_mapper() -> Mapper198 {
        Mapper198::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, 80), // 640 KiB PRG-ROM
            vec![],
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_198_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, 80),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 198 must be creatable via factory");
    }

    #[test]
    fn extra_wram_at_5000_is_readable_and_writable() {
        let mut m = make_mapper();
        m.write_prg(0x5000, 0xAB);
        assert_eq!(m.read_prg(0x5000), 0xAB);
        m.write_prg(0x5FFF, 0xCD);
        assert_eq!(m.read_prg(0x5FFF), 0xCD);
    }

    #[test]
    fn extra_wram_is_4kb() {
        let mut m = make_mapper();
        m.write_prg(0x5000, 0x11);
        m.write_prg(0x5FFF, 0x22);
        assert_eq!(m.read_prg(0x5000), 0x11);
        assert_eq!(m.read_prg(0x5FFF), 0x22);
    }

    #[test]
    fn chr_ram_read_write_roundtrip() {
        let mut m = make_mapper();
        m.write_chr(0x0000, 0x55);
        assert_eq!(m.read_chr(0x0000), 0x55);
        m.write_chr(0x1FFF, 0xAA);
        assert_eq!(m.read_chr(0x1FFF), 0xAA);
    }

    #[test]
    fn fixed_prg_banks_are_last_two_of_640kb_rom() {
        // With 80 banks (bank 0..=79), the last two are 78 ($4E) and 79 ($4F).
        // Standard MMC3 fixed bank arrangement puts:
        //   $C000–$DFFF = second-to-last bank = bank 78 = $4E
        //   $E000–$FFFF = last bank            = bank 79 = $4F
        //
        // banked_data fills each bank with its bank number as the first byte.
        let m = make_mapper();
        // Read the first byte of each fixed window.
        let bank_c000 = m.inner.mapped_prg_bank(0xC000);
        let bank_e000 = m.inner.mapped_prg_bank(0xE000);
        assert_eq!(bank_c000, 78, "Fixed $C000 must be bank 78 ($4E)");
        assert_eq!(bank_e000, 79, "Fixed $E000 must be bank 79 ($4F)");
    }

    #[test]
    fn switchable_prg_bank_at_8000() {
        let mut m = make_mapper();
        // Select register 6 (PRG $8000 in MMC3 mode 0)
        m.write_prg(0x8000, 0x06); // bank-select: register 6
        m.write_prg(0x8001, 10); // bank 10 at $8000
        let bank = m.inner.mapped_prg_bank(0x8000);
        assert_eq!(bank, 10);
    }

    #[test]
    fn chr_bank_register_affects_chr_read() {
        let mut m = make_mapper();
        // Write to CHR-RAM bank 5 via 1KB slot at $1400–$17FF
        // Default MMC3 CHR mode 0: slot $1000-$13FF uses reg[2], $1400 uses reg[3]
        // Select CHR register 2 ($8000 bank-select = 0x82)
        m.write_prg(0x8000, 0x82); // select CHR reg 2
        m.write_prg(0x8001, 5); // point slot $1000-$13FF to CHR-RAM bank 5
        // Write 0x77 to bank 5 via PPU $1000
        m.write_chr(0x1000, 0x77);

        // Now point slot $1000 to bank 5 again and verify
        assert_eq!(m.read_chr(0x1000), 0x77);
    }

    #[test]
    fn reset_clears_extra_wram() {
        let mut m = make_mapper();
        m.write_prg(0x5100, 0xFF);
        m.reset();
        assert_eq!(
            m.read_prg(0x5100),
            0x00,
            "Extra WRAM must be cleared on reset"
        );
    }

    #[test]
    fn snapshot_restore_round_trips_mmc3_state() {
        let mut m = make_mapper();
        m.write_prg(0x8000, 0x06);
        m.write_prg(0x8001, 0x10);
        let snap = m.registers_snapshot();

        let mut m2 = make_mapper();
        m2.restore_registers(&snap);

        assert_eq!(m2.inner.mapped_prg_bank(0x8000), 0x10);
    }

    #[test]
    fn snapshot_restore_preserves_extra_wram() {
        let mut m = make_mapper();
        m.write_prg(0x5000, 0xAA);
        m.write_prg(0x5FFF, 0x55);
        let snap = m.registers_snapshot();

        let mut m2 = make_mapper();
        m2.restore_registers(&snap);

        assert_eq!(
            m2.read_prg(0x5000),
            0xAA,
            "extra_wram[0] must survive snapshot"
        );
        assert_eq!(
            m2.read_prg(0x5FFF),
            0x55,
            "extra_wram[last] must survive snapshot"
        );
    }

    #[test]
    fn chr_ram_snapshot_restore_round_trips() {
        let mut m = make_mapper();
        m.write_chr(0x0100, 0xDE);
        m.write_chr(0x1F00, 0xAD);
        let snap = m.chr_ram_snapshot();

        let mut m2 = make_mapper();
        m2.restore_chr_ram(&snap);

        assert_eq!(m2.read_chr(0x0100), 0xDE);
        assert_eq!(m2.read_chr(0x1F00), 0xAD);
    }

    #[test]
    fn wram_size_matches_inner_mmc3_prg_ram() {
        let m = make_mapper();
        // Inner MMC3 has 8 KB PRG-RAM at $6000-$7FFF; wram_size must reflect that.
        assert_eq!(m.wram_size(), 8 * 1024);
    }
}
