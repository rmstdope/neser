//! Mapper 199 – MMC3 variant with CHR-RAM/ROM interleaving and 4-way mirroring
//!
//! # Specifications
//! - Primary source: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_199.h`
//! - Fallback: puNES `src/mappers/mapper_199.c`
//!
//! ## Hardware overview
//!
//! Mapper 199 is an MMC3-based variant used by Chinese pirate boards.
//! It extends the standard MMC3 (mapper 4) with:
//!
//! - **8 KiB CHR-RAM** (pages 0–7) interleaved with CHR-ROM (pages 8+)
//! - **4 extra registers** `ex_regs[4]` initialised to `{0xFE, 0xFF, 0x01, 0x03}`
//! - **Extra PRG registers**: $C000–$DFFF and $E000–$FFFF windows controlled by
//!   `ex_regs[0]` and `ex_regs[1]` rather than the standard MMC3 fixed banks
//! - **Custom CHR slots 0–3**: PPU $0000–$0FFF always uses
//!   `reg[0]` / `ex_regs[2]` / `reg[1]` / `ex_regs[3]`
//! - **4-way mirroring** via `$A000` bits \[1:0\]: 0=Vertical, 1=Horizontal,
//!   2=SingleScreenLower, 3=SingleScreenUpper
//!
//! ## Extra register writes
//!
//! When bit 3 of the bank-select register (`$8000`) is set, writes to `$8001`
//! update `ex_regs[bank_select & 0x03]` instead of the standard MMC3 bank data
//! register.  After the write, PRG and CHR mapping is updated internally.
//!
//! ## PRG banking
//!
//! Standard MMC3 handles `$8000`–`$BFFF`.  `$C000`–`$DFFF` is an 8 KiB window
//! selected by `ex_regs[0]` (default `0xFE` = second-last bank).
//! `$E000`–`$FFFF` is an 8 KiB window selected by `ex_regs[1]`
//! (default `0xFF` = last bank).
//!
//! ## CHR banking
//!
//! PPU `$0000–$0FFF` (slots 0–3) are always mapped as follows, regardless of
//! the CHR mode bit:
//! - Slot 0 (`$0000–$03FF`): `chr_bank_reg(0)` (raw, not 2KB-aligned)
//! - Slot 1 (`$0400–$07FF`): `ex_regs[2]` (default 1)
//! - Slot 2 (`$0800–$0BFF`): `chr_bank_reg(1)` (raw, not 2KB-aligned)
//! - Slot 3 (`$0C00–$0FFF`): `ex_regs[3]` (default 3)
//!
//! PPU `$1000–$1FFF` (slots 4–7) follow the standard MMC3 raw CHR bank mapping.
//!
//! For any computed bank value:
//! - Bank 0–7  → 1 KiB page from **CHR-RAM** (8 KiB total)
//! - Bank 8+   → 1 KiB page from **CHR-ROM** (bank index used as-is, wrapping
//!   against total CHR-ROM bank count)
//!
//! ## Known Limitations
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::ines::NametableLayout;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

const CHR_RAM_SIZE: usize = 8 * 1024;
const CHR_RAM_PAGE_SIZE: usize = 1024;
/// Pages 0–7 map to CHR-RAM; pages 8+ map to CHR-ROM.
const CHR_RAM_PAGE_COUNT: usize = 8;

/// Mapper 199 – MMC3 variant with CHR-RAM/ROM interleaving and 4-way mirroring.
pub struct Mapper199 {
    inner: MMC3Mapper,
    /// Extra registers initialised to `{0xFE, 0xFF, 0x01, 0x03}`.
    /// - ex_regs[0]: PRG page for $C000–$DFFF (default 0xFE = second-last)
    /// - ex_regs[1]: PRG page for $E000–$FFFF (default 0xFF = last)
    /// - ex_regs[2]: CHR slot-1 bank (default 1)
    /// - ex_regs[3]: CHR slot-3 bank (default 3)
    ex_regs: [u8; 4],
    /// 8 KiB CHR-RAM for CHR pages 0–7.
    chr_ram: [u8; CHR_RAM_SIZE],
}

impl Mapper199 {
    const MAPPER_NUMBER: u16 = 199;
    const CHR_BANK_MASK: usize = CHR_RAM_PAGE_SIZE - 1;

    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            inner: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            ex_regs: [0xFE, 0xFF, 0x01, 0x03],
            chr_ram: [0; CHR_RAM_SIZE],
        }
    }

    /// Returns `true` when `bank` is in the CHR-RAM region (pages 0–7).
    fn is_chr_ram(bank: usize) -> bool {
        bank < CHR_RAM_PAGE_COUNT
    }

    /// Returns the effective 1 KiB CHR bank for `ppu_addr`.
    ///
    /// Slots 0–3 (PPU `$0000–$0FFF`) use the custom mapper-199 mapping;
    /// slots 4–7 (PPU `$1000–$1FFF`) use the standard MMC3 raw CHR bank.
    fn effective_chr_bank(&self, ppu_addr: u16) -> usize {
        let chr_addr = (ppu_addr & 0x1FFF) as usize;
        let bank = match chr_addr {
            0x0000..=0x03FF => self.inner.chr_bank_reg(0),
            0x0400..=0x07FF => self.ex_regs[2],
            0x0800..=0x0BFF => self.inner.chr_bank_reg(1),
            0x0C00..=0x0FFF => self.ex_regs[3],
            _ => return self.inner.raw_chr_1k_bank(ppu_addr),
        };
        bank as usize
    }
}

impl Mapper for Mapper199 {
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
        let offset = (addr & 0x1FFF) as usize;
        match addr {
            0xC000..=0xDFFF => self
                .inner
                .read_prg_at_bank(self.ex_regs[0] as usize, offset),
            0xE000..=0xFFFF => self
                .inner
                .read_prg_at_bank(self.ex_regs[1] as usize, offset),
            _ => self.inner.read_prg(addr),
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        self.inner.read_prg_open_bus(addr, open_bus)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Intercept odd writes to $8000–$9FFF when bank_select bit 3 is set.
        // These writes go to ex_regs instead of the standard MMC3 bank data reg.
        if addr & 0xE001 == 0x8001 && (self.inner.bank_select_reg() & 0x08) != 0 {
            let idx = (self.inner.bank_select_reg() & 0x03) as usize;
            self.ex_regs[idx] = value;
            return;
        }

        // Intercept even writes to $A000–$BFFF for 4-way mirroring.
        if addr & 0xE001 == 0xA000 {
            let mirroring = match value & 0x03 {
                0 => NametableLayout::Vertical,
                1 => NametableLayout::Horizontal,
                2 => NametableLayout::SingleScreenLower,
                _ => NametableLayout::SingleScreenUpper,
            };
            self.inner.base.set_mirroring(mirroring);
            return;
        }

        self.inner.write_prg(addr, value);
    }

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        let bank = self.effective_chr_bank(ppu_addr);
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        if Self::is_chr_ram(bank) {
            self.chr_ram[bank * CHR_RAM_PAGE_SIZE + offset]
        } else {
            self.inner.read_chr_1k_at(bank, offset)
        }
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        let bank = self.effective_chr_bank(ppu_addr);
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        if Self::is_chr_ram(bank) {
            self.chr_ram[bank * CHR_RAM_PAGE_SIZE + offset] = value;
        }
        // CHR-ROM writes are silently ignored.
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.inner.initialize_ram(mode);
        crate::console::initialize_ram(&mut self.chr_ram, mode);
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
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
        snap.extend_from_slice(&self.ex_regs);
        snap.extend_from_slice(&self.chr_ram);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        const MMC3_SNAP_LEN: usize = 16;
        const EX_REGS_LEN: usize = 4;
        const EXTRA_LEN: usize = EX_REGS_LEN + CHR_RAM_SIZE;

        if data.len() >= MMC3_SNAP_LEN + EXTRA_LEN {
            let (mmc3_data, rest) = data.split_at(data.len() - EXTRA_LEN);
            self.inner.restore_registers(mmc3_data);
            self.ex_regs.copy_from_slice(&rest[..EX_REGS_LEN]);
            self.chr_ram.copy_from_slice(&rest[EX_REGS_LEN..]);
        } else {
            // Legacy / truncated snapshot: restore only the MMC3 core portion.
            self.inner.restore_registers(data);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            ..MapperCapabilities::default()
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    fn make_mapper(prg_kb: usize, chr_kb: usize) -> Box<dyn Mapper> {
        let prg_rom = vec![0u8; prg_kb * 1024];
        let chr_rom = vec![0u8; chr_kb * 1024];
        create_mapper(MapperContext::new_for_test(
            199,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("mapper 199 creation should succeed")
    }

    /// PRG-ROM: each 8 KiB bank is filled with its bank index value.
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
            199,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("mapper 199 creation should succeed")
    }

    /// CHR-ROM: each 1 KiB bank is filled with its bank index value.
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
            199,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
        .expect("mapper 199 creation should succeed")
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    #[test]
    fn mapper_199_is_registered_in_factory() {
        let mapper = make_mapper(64, 128);
        assert_eq!(mapper.mapper_number(), 199);
    }

    // -----------------------------------------------------------------------
    // PRG banking — $8000–$BFFF (standard MMC3)
    // -----------------------------------------------------------------------

    #[test]
    fn prg_8000_uses_mmc3_reg6() {
        let mut mapper = make_mapper_prg_pattern(8);
        mapper.write_prg(0x8000, 0x06); // bank_select → reg 6
        mapper.write_prg(0x8001, 0x02); // reg 6 = bank 2
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 should read bank 2 via MMC3 reg 6"
        );
    }

    #[test]
    fn prg_a000_uses_mmc3_reg7() {
        let mut mapper = make_mapper_prg_pattern(8);
        mapper.write_prg(0x8000, 0x07); // bank_select → reg 7
        mapper.write_prg(0x8001, 0x03); // reg 7 = bank 3
        assert_eq!(
            mapper.read_prg(0xA000),
            3,
            "$A000 should read bank 3 via MMC3 reg 7"
        );
    }

    // -----------------------------------------------------------------------
    // PRG banking — $C000–$FFFF (ex_regs[0] and ex_regs[1])
    // -----------------------------------------------------------------------

    #[test]
    fn prg_c000_defaults_to_second_last_bank() {
        // 8 banks × 8 KiB; default ex_regs[0]=0xFE; 0xFE % 8 = 6 (second-last).
        let mapper = make_mapper_prg_pattern(8);
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 defaults to second-last bank (0xFE % 8 = 6)"
        );
    }

    #[test]
    fn prg_e000_defaults_to_last_bank() {
        // Default ex_regs[1]=0xFF; 0xFF % 8 = 7 (last).
        let mapper = make_mapper_prg_pattern(8);
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 defaults to last bank (0xFF % 8 = 7)"
        );
    }

    #[test]
    fn ex_reg0_controls_prg_c000_window() {
        let mut mapper = make_mapper_prg_pattern(8);
        mapper.write_prg(0x8000, 0x08); // bit3=1, reg index 0
        mapper.write_prg(0x8001, 0x01); // ex_regs[0] = 1
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 should map to bank 1 via ex_regs[0]"
        );
    }

    #[test]
    fn ex_reg1_controls_prg_e000_window() {
        let mut mapper = make_mapper_prg_pattern(8);
        mapper.write_prg(0x8000, 0x09); // bit3=1, reg index 1
        mapper.write_prg(0x8001, 0x02); // ex_regs[1] = 2
        assert_eq!(
            mapper.read_prg(0xE000),
            2,
            "$E000 should map to bank 2 via ex_regs[1]"
        );
    }

    // -----------------------------------------------------------------------
    // CHR banking — slots 0–3 (custom override)
    // -----------------------------------------------------------------------

    #[test]
    fn chr_slot0_uses_raw_chr_bank_reg0() {
        let mut mapper = make_mapper_chr_pattern(32);
        // Set reg[0] = 10 (CHR-ROM page 10, >= 8)
        mapper.write_prg(0x8000, 0x00); // bank_select → CHR reg 0 (bit3=0)
        mapper.write_prg(0x8001, 10); // reg[0] = 10
        assert_eq!(
            mapper.read_chr(0x0000),
            10,
            "Slot 0 should use raw reg[0]=10 → CHR-ROM bank 10"
        );
    }

    #[test]
    fn chr_slot1_defaults_to_ex_reg2_page1_chr_ram() {
        let mut mapper = make_mapper_chr_pattern(32);
        // Default ex_regs[2]=1 → CHR-RAM page 1
        mapper.write_chr(0x0400, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0400),
            0xAB,
            "Slot 1 uses ex_regs[2]=1 (CHR-RAM page 1)"
        );
    }

    #[test]
    fn chr_slot2_uses_raw_chr_bank_reg1() {
        let mut mapper = make_mapper_chr_pattern(32);
        mapper.write_prg(0x8000, 0x01); // bank_select → CHR reg 1 (bit3=0)
        mapper.write_prg(0x8001, 12); // reg[1] = 12
        assert_eq!(
            mapper.read_chr(0x0800),
            12,
            "Slot 2 should use raw reg[1]=12 → CHR-ROM bank 12"
        );
    }

    #[test]
    fn chr_slot3_defaults_to_ex_reg3_page3_chr_ram() {
        let mut mapper = make_mapper_chr_pattern(32);
        // Default ex_regs[3]=3 → CHR-RAM page 3
        mapper.write_chr(0x0C00, 0xCD);
        assert_eq!(
            mapper.read_chr(0x0C00),
            0xCD,
            "Slot 3 uses ex_regs[3]=3 (CHR-RAM page 3)"
        );
    }

    #[test]
    fn ex_reg2_write_controls_chr_slot1() {
        let mut mapper = make_mapper_chr_pattern(32);
        // Change ex_regs[2] to CHR-ROM page 10
        mapper.write_prg(0x8000, 0x0A); // bit3=1, reg index 2
        mapper.write_prg(0x8001, 10); // ex_regs[2] = 10
        assert_eq!(
            mapper.read_chr(0x0400),
            10,
            "Slot 1 now uses ex_regs[2]=10 → CHR-ROM bank 10"
        );
    }

    #[test]
    fn ex_reg3_write_controls_chr_slot3() {
        let mut mapper = make_mapper_chr_pattern(32);
        mapper.write_prg(0x8000, 0x0B); // bit3=1, reg index 3
        mapper.write_prg(0x8001, 11); // ex_regs[3] = 11
        assert_eq!(
            mapper.read_chr(0x0C00),
            11,
            "Slot 3 now uses ex_regs[3]=11 → CHR-ROM bank 11"
        );
    }

    // -----------------------------------------------------------------------
    // CHR banking — slots 4–7 (standard MMC3 raw bank)
    // -----------------------------------------------------------------------

    #[test]
    fn chr_slots4_to_7_use_standard_mmc3_mapping() {
        let mut mapper = make_mapper_chr_pattern(32);
        // In CHR mode 0 (bit7=0), slots 4–7 use regs 2–5.
        mapper.write_prg(0x8000, 0x02); // bank_select → CHR reg 2 (bit3=0)
        mapper.write_prg(0x8001, 8); // reg[2] = 8 (CHR-ROM page 8)
        assert_eq!(
            mapper.read_chr(0x1000),
            8,
            "Slot 4 uses CHR reg 2 = 8 (CHR-ROM page 8)"
        );
    }

    // -----------------------------------------------------------------------
    // CHR-RAM / CHR-ROM routing
    // -----------------------------------------------------------------------

    #[test]
    fn chr_pages_0_to_7_route_to_chr_ram() {
        let mut mapper = make_mapper_chr_pattern(32);
        // Slot 0 → reg[0]=0 → CHR-RAM page 0
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x00);
        mapper.write_chr(0x0000, 0x42);
        assert_eq!(mapper.read_chr(0x0000), 0x42, "CHR page 0 → CHR-RAM");
    }

    #[test]
    fn chr_pages_8_plus_route_to_chr_rom() {
        let mut mapper = make_mapper_chr_pattern(32);
        // Slot 0 → reg[0]=10 → CHR-ROM page 10
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 10);
        assert_eq!(mapper.read_chr(0x0000), 10, "CHR page 10 → CHR-ROM bank 10");
    }

    #[test]
    fn chr_write_to_rom_page_is_ignored() {
        let mut mapper = make_mapper_chr_pattern(32);
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 10); // CHR-ROM
        let before = mapper.read_chr(0x0000);
        mapper.write_chr(0x0000, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            before,
            "Write to CHR-ROM page should be ignored"
        );
    }

    #[test]
    fn chr_ram_pages_are_independent() {
        let mut mapper = make_mapper_chr_pattern(32);
        // Page 0 via slot 0 (reg[0]=0)
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x00);
        mapper.write_chr(0x0000, 0x11);
        // Page 1 via slot 1 (ex_regs[2]=1 default)
        mapper.write_chr(0x0400, 0x22);
        // Verify independence
        assert_eq!(mapper.read_chr(0x0000), 0x11, "CHR-RAM page 0 = 0x11");
        assert_eq!(mapper.read_chr(0x0400), 0x22, "CHR-RAM page 1 = 0x22");
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
    // Save state snapshot / restore round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn snapshot_round_trips_ex_regs_and_chr_ram() {
        let mut mapper = make_mapper(64, 128);

        // Set ex_regs via $8001 writes with bank_select bit 3 set
        mapper.write_prg(0x8000, 0x08);
        mapper.write_prg(0x8001, 0x04); // ex_regs[0] = 4
        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0x8001, 0x05); // ex_regs[1] = 5

        // Write a distinctive byte to CHR-RAM page 0
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0x00); // slot 0 → CHR-RAM page 0
        mapper.write_chr(0x0000, 0xAB);

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper(64, 128);
        mapper2.restore_registers(&snap);

        // ex_regs should be restored (verified via PRG reads)
        assert_eq!(
            mapper2.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "ex_regs[0] should survive snapshot round-trip"
        );
        assert_eq!(
            mapper2.read_prg(0xE000),
            mapper.read_prg(0xE000),
            "ex_regs[1] should survive snapshot round-trip"
        );

        // CHR-RAM should be restored
        mapper2.write_prg(0x8000, 0x00);
        mapper2.write_prg(0x8001, 0x00);
        assert_eq!(
            mapper2.read_chr(0x0000),
            0xAB,
            "CHR-RAM page 0 should survive snapshot round-trip"
        );
    }

    #[test]
    fn legacy_mmc3_snapshot_restores_without_panic() {
        let mut mapper = make_mapper(64, 128);
        let mmc3_snap = mapper.registers_snapshot()[..16].to_vec();
        mapper.restore_registers(&mmc3_snap); // should not panic
    }
}
