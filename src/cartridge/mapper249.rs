//! Mapper 249 - MMC3 variant with address scrambling
//!
//! Specifications:
//! - Fallback: Mesen2 implementation (no NesDev wiki page found)
//!   Source: <https://raw.githubusercontent.com/sourmesen/mesen2/master/Core/NES/Mappers/Mmc3Variants/MMC3_249.h>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 249 - MMC3 variant with PRG/CHR address scrambling
///
/// Hardware: MMC3 clone with hardware address line scrambling activated via
/// an extra register at `$5000`.
///
/// Specifications:
/// - Fallback: Mesen2 MMC3_249.h
/// - PRG-ROM: Up to 512KB (64 × 8KB banks, scrambled when ex_reg bit 1 set)
/// - CHR: CHR-ROM or CHR-RAM, 8KB total CHR space; 1KB banking with scrambling
/// - Mirroring: Software-controlled (standard MMC3)
/// - IRQ: Standard MMC3 scanline counter
///
/// Register at `$5000` (write):
/// - Bit 1 (`0x02`): when set, enables PRG and CHR bank address scrambling
///
/// PRG scrambling (when bit 1 of $5000 is set):
/// - For bank values < 0x20: permutes bits `[p4, p3, p2, p1, p0]` → `[p2, p1, p3, p4, p0]`
/// - For bank values ≥ 0x20: subtracts 0x20 then applies the CHR scrambling formula
///
/// CHR scrambling (when bit 1 of $5000 is set):
/// - Permutes bits `[p7..p0]` → `[p5, p4, p2, p7, p6, p3, p1, p0]`
pub struct Mapper249 {
    pub(crate) mmc3: MMC3Mapper,
    ex_reg: u8,
}

impl Mapper249 {
    const MAPPER_NUMBER: u8 = 249;
    const PRG_BANK_SIZE: usize = 0x2000; // 8KB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x0400; // 1KB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            ex_reg: 0,
        }
    }

    /// Returns whether address scrambling is active (bit 1 of ex_reg).
    fn scrambling_enabled(&self) -> bool {
        (self.ex_reg & 0x02) != 0
    }

    /// Scrambles a PRG bank number according to mapper 249 hardware behavior.
    ///
    /// For page < 0x20 (5-bit permutation):
    ///   `new = p0 | (p4<<1) | (p3<<2) | (p1<<3) | (p2<<4)`
    ///
    /// For page ≥ 0x20: subtract 0x20, then apply CHR scramble formula.
    fn scramble_prg_page(page: usize) -> usize {
        if page < 0x20 {
            (page & 0x01) | ((page >> 3) & 0x02) | ((page >> 1) & 0x04) | ((page << 2) & 0x18)
        } else {
            Self::scramble_chr_bank(page - 0x20)
        }
    }

    /// Scrambles a CHR 1KB bank number according to mapper 249 hardware behavior.
    ///
    /// 8-bit permutation: `new = p0 | p1 | (p3<<2) | (p7<<3) | (p6<<4) | (p2<<5) | (p4<<6) | (p5<<7)`
    fn scramble_chr_bank(page: usize) -> usize {
        (page & 0x03)
            | ((page >> 1) & 0x04)
            | ((page >> 4) & 0x08)
            | ((page >> 2) & 0x10)
            | ((page << 3) & 0x20)
            | ((page << 2) & 0xC0)
    }

    /// Returns the scrambled CHR bank index and the intra-bank byte offset for `addr`.
    fn scrambled_chr_bank_and_offset(&mut self, addr: u16) -> (usize, usize) {
        let scrambled = Self::scramble_chr_bank(self.mmc3.raw_chr_1k_bank(addr));
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        (scrambled, offset)
    }
}

impl Mapper for Mapper249 {
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
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                if self.scrambling_enabled() {
                    let raw_page = self.mmc3.mapped_prg_bank(addr);
                    let scrambled = Self::scramble_prg_page(raw_page);
                    let offset = (addr as usize) & Self::PRG_BANK_MASK;
                    self.mmc3.read_prg_at_bank(scrambled, offset)
                } else {
                    self.mmc3.read_prg(addr)
                }
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
        if addr == 0x5000 {
            self.ex_reg = value;
        } else {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if self.scrambling_enabled() {
            let (bank, offset) = self.scrambled_chr_bank_and_offset(addr);
            self.mmc3.read_chr_1k_at(bank, offset)
        } else {
            self.mmc3.base.read_chr(addr)
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.scrambling_enabled() {
            let (scrambled, offset) = self.scrambled_chr_bank_and_offset(addr);
            let count = self.mmc3.chr_bank_count_1k();
            let bank = if count > 0 { scrambled % count } else { 0 };
            self.mmc3.write_chr_1k_at(bank, offset, value);
        } else {
            self.mmc3.base.write_chr(addr, value);
        }
    }

    fn mapper_number(&self) -> u16 {
        u16::from(Self::MAPPER_NUMBER)
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
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.ex_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&outer, mmc3_data)) = data.split_last() {
            self.mmc3.restore_registers(mmc3_data);
            self.ex_reg = outer;
        }
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
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn create_mapper249(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            249, prg_rom, chr_rom, mirroring,
        ))
    }

    // --- Factory ---

    #[test]
    fn test_factory_creates_mapper_249() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical);
        assert!(mapper.is_ok());
    }

    // --- Normal mode: standard MMC3 PRG banking ---

    #[test]
    fn test_normal_mode_prg_no_scrambling() {
        // 48 banks of 8KB (non-power-of-two avoids false wrapping passes)
        let prg_rom = banked_data(8 * 1024, 48);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Do NOT write to $5000 → scrambling bit stays 0
        // Set R6 = 4 (bank select for $8000)
        mapper.write_prg(0x8000, 6); // bank_select → target R6
        mapper.write_prg(0x8001, 4); // R6 = 4

        // Without scrambling, reading $8000 returns byte from bank 4 = 4
        assert_eq!(mapper.read_prg(0x8000), 4);
    }

    // --- Extended mode: PRG scrambling formula 1 (page < 0x20) ---

    #[test]
    fn test_extended_mode_prg_scrambling_formula1() {
        // 48 banks of 8KB
        let prg_rom = banked_data(8 * 1024, 48);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Enable scrambling (bit 1)
        mapper.write_prg(0x5000, 0x02);

        // Set R6 = 4; formula1(4) = 16
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 4);

        // In extended mode, reading $8000 should return byte from scrambled bank 16
        assert_eq!(mapper.read_prg(0x8000), 16);
    }

    // --- Extended mode: PRG scrambling formula 2 (page >= 0x20) ---

    #[test]
    fn test_extended_mode_prg_scrambling_formula2() {
        // 64 banks of 8KB (512KB)
        let prg_rom = banked_data(8 * 1024, 64);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Enable scrambling
        mapper.write_prg(0x5000, 0x02);

        // Set R6 = 36 (0x24, which is >= 0x20); formula2(36) = 32
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 36);

        // In extended mode, reading $8000 should return byte from scrambled bank 32
        assert_eq!(mapper.read_prg(0x8000), 32);
    }

    // --- Extended mode: CHR scrambling ---

    #[test]
    fn test_extended_mode_chr_scrambling() {
        // 256 CHR banks of 1KB = 256KB CHR-ROM
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 256);
        let mut mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Enable scrambling
        mapper.write_prg(0x5000, 0x02);

        // Set R2 = 4; CHR mode 0: R2 maps to slot 4 → PPU $1000
        // chr_formula(4) = 32
        mapper.write_prg(0x8000, 2); // bank_select → target R2
        mapper.write_prg(0x8001, 4); // R2 = 4

        // In extended mode, reading PPU $1000 should return byte from scrambled CHR bank 32
        assert_eq!(mapper.read_chr(0x1000), 32);
    }

    // --- Normal mode: no CHR scrambling ---

    #[test]
    fn test_normal_mode_chr_no_scrambling() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 256);
        let mut mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // No write to $5000 → normal mode
        // Set R2 = 4; without scrambling: PPU $1000 → bank 4
        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0x8001, 4);

        assert_eq!(mapper.read_chr(0x1000), 4);
    }

    // --- Extended mode: fixed PRG banks are also scrambled ---

    #[test]
    fn test_extended_mode_fixed_prg_bank_last_scrambled() {
        // 64 banks (0-63); last bank = 63 (0x3F).
        // scramble_prg_page(63): 63 >= 0x20, so scramble_chr_bank(63 - 0x20) = scramble_chr_bank(31) = 103.
        // 103 % 64 = 39, so the result byte is 39.
        let prg_rom = banked_data(8 * 1024, 64);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Enable scrambling
        mapper.write_prg(0x5000, 0x02);

        // PRG mode 0: $E000-$FFFF = fixed last bank (63); formula2(63) = 103
        // But bank 103 % 64 = 39. Bank 39 is filled with 39.
        assert_eq!(mapper.read_prg(0xE000), 39);
    }

    // --- Mirroring ---

    #[test]
    fn test_mirroring_control() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xA000, 1); // bit 0 = 1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // --- PRG-RAM ---

    #[test]
    fn test_prg_ram_read_write() {
        let prg_rom = banked_data(8 * 1024, 4);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    // --- Save state ---

    #[test]
    fn test_registers_snapshot_includes_ex_reg() {
        let prg_rom = banked_data(8 * 1024, 48);
        let chr_rom = banked_data(1024, 8);
        let mut mapper =
            create_mapper249(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical).unwrap();

        mapper.write_prg(0x5000, 0x02);
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 4); // R6 = 4

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();
        restored.restore_registers(&snap);

        // After restore, scrambling should be active and R6=4 → bank 16
        assert_eq!(restored.read_prg(0x8000), 16);
    }

    // --- Scrambling on/off toggle ---

    #[test]
    fn test_toggle_scrambling_on_and_off() {
        let prg_rom = banked_data(8 * 1024, 48);
        let chr_rom = banked_data(1024, 8);
        let mut mapper = create_mapper249(prg_rom, chr_rom, NametableLayout::Vertical).unwrap();

        // Set R6 = 4
        mapper.write_prg(0x8000, 6);
        mapper.write_prg(0x8001, 4);

        // Normal mode: bank 4
        assert_eq!(mapper.read_prg(0x8000), 4);

        // Enable scrambling: bank 4 → 16
        mapper.write_prg(0x5000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 16);

        // Disable scrambling: back to bank 4
        mapper.write_prg(0x5000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 4);
    }
}
