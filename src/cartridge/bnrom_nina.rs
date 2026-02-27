//! Mapper 34 - BNROM / NINA-001
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::NametableLayout;
use crate::cartridge::common::{BankSwitch, BankedRom, ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};

// Memory size constants
const PRG_BANK_SIZE: usize = 0x8000; // 32KB
const CHR_FALLBACK_THRESHOLD_SIZE: usize = 0x2000; // 8KB
const NINA_CHR_BANK_SIZE: usize = 0x1000; // 4KB

/// Mapper 34 - BNROM / NINA-001
///
/// Hardware: Two different hardware types sharing the same mapper number
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_034>
/// - BNROM: <https://www.nesdev.org/wiki/BNROM>
/// - NINA-001: <https://www.nesdev.org/wiki/NINA-001>
/// - PRG-ROM: Up to 128KB (4 32KB banks)
/// - PRG-RAM: 8KB at $6000-$7FFF for NINA-001/NINA-002, none for BNROM
/// - CHR: 8KB CHR-RAM (BNROM) or up to 64KB CHR-ROM (NINA-001)
/// - Mirroring: Fixed horizontal or vertical
///
/// Detection:
/// - Submapper 1 denotes NINA-001/NINA-002
/// - Submapper 2 denotes BNROM
/// - Without submapper: CHR-ROM size 0-8 KiB => BNROM, above 8 KiB => NINA-001/NINA-002
///
/// BNROM variant:
/// - Bank select at $8000-$FFFF (any write selects 32KB PRG bank)
/// - Used in Deadly Towers, Mashou (Japan)
///
/// NINA-001 variant:
/// - PRG bank select at $7FFD
/// - CHR bank select at $7FFE (PPU $0000-$0FFF, 4KB)
/// - CHR bank select at $7FFF (PPU $1000-$1FFF, 4KB)
/// - Used in Impossible Mission II, Puzzle, Rad Racket
pub struct BnromNinaMapper {
    prg_rom: BankedRom,
    prg_ram: PrgRam,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    prg_bank: BankSwitch,
    chr_bank_low: BankSwitch,
    chr_bank_high: BankSwitch,
    is_nina: bool, // true for NINA-001, false for BNROM
}

impl BnromNinaMapper {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        // Detect variant per NESdev iNES mapper 34 guidance.
        // Submapper 1 = NINA-001/NINA-002, submapper 2 = BNROM.
        // Without submapper info, CHR-ROM 0-8 KiB => BNROM, >8 KiB => NINA.
        let is_nina = match ctx.submapper {
            1 => true,
            2 => false,
            _ => chr_rom.len() > CHR_FALLBACK_THRESHOLD_SIZE,
        };

        // NINA uses two independent 4KB CHR windows ($0000 and $1000).
        // BNROM has unbanked CHR memory (typically 8KB CHR-RAM).
        let (chr_bank_low, chr_bank_high) = if is_nina {
            let bank = BankSwitch::from_rom(&chr_rom, NINA_CHR_BANK_SIZE);
            (bank, bank)
        } else {
            (BankSwitch::new(1), BankSwitch::new(1))
        };
        let prg_bank = BankSwitch::from_rom(&prg_rom, PRG_BANK_SIZE);
        let prg_ram_size = if is_nina { DEFAULT_PRG_RAM_SIZE } else { 0 };

        Self {
            prg_rom: BankedRom::new(prg_rom, PRG_BANK_SIZE),
            prg_ram: PrgRam::new(prg_ram_size),
            chr_memory: ChrMemory::new(chr_rom),
            mirroring,
            prg_bank,
            chr_bank_low,
            chr_bank_high,
            is_nina,
        }
    }

    fn nina_chr_index(&self, addr: u16) -> usize {
        let window_offset = (addr & 0x0FFF) as usize;
        let bank_offset = if addr < 0x1000 {
            self.chr_bank_low.offset(NINA_CHR_BANK_SIZE)
        } else {
            self.chr_bank_high.offset(NINA_CHR_BANK_SIZE)
        };
        bank_offset + window_offset
    }
}

impl Mapper for BnromNinaMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        // PRG ROM at $8000-$FFFF (32KB switchable bank)
        match addr {
            0x8000..=0xFFFF => self
                .prg_rom
                .read_with_base(self.prg_bank.current(), 0x8000, addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // NINA-001 registers overlap PRG-RAM at $7FFD-$7FFF.
        // A write updates both the register and underlying RAM byte.
        if self.is_nina && (0x7FFD..=0x7FFF).contains(&addr) {
            let _ = self.prg_ram.try_write(addr, value);

            match addr {
                0x7FFD => {
                    self.prg_bank.set(value);
                }
                0x7FFE => {
                    self.chr_bank_low.set(value);
                }
                0x7FFF => {
                    self.chr_bank_high.set(value);
                }
                _ => {}
            }
            return;
        }

        if self.prg_ram.try_write(addr, value) {
            return;
        }

        // BNROM: Any write to $8000-$FFFF sets PRG bank
        // NINA-001: Writes to $8000-$FFFF are ignored (uses $7FFD-$7FFF instead)
        if !self.is_nina && (0x8000..=0xFFFF).contains(&addr) {
            // BNROM has AND-type bus conflicts: effective value = write_value & rom_value_at_addr.
            let rom_value = self.read_prg(addr);
            self.prg_bank.set(value & rom_value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if self.is_nina {
            self.chr_memory.read_at_index(self.nina_chr_index(addr))
        } else {
            self.chr_memory.read(addr)
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.is_nina {
            let index = self.nina_chr_index(addr);
            self.chr_memory.write_at_index(index, value);
        } else {
            self.chr_memory.write(addr, value);
        }
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        34
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.prg_bank.raw(),
            self.chr_bank_low.raw(),
            self.chr_bank_high.raw(),
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.prg_bank.set(value);
        }
        if let Some(&value) = data.get(1) {
            self.chr_bank_low.set(value);
        }
        if let Some(&value) = data.get(2) {
            self.chr_bank_high.set(value);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: self.is_nina,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: if self.is_nina { 8 } else { 0 },
            prg_bank_size_kb: 32,
            chr_bank_size_kb: if self.is_nina { 4 } else { 8 },
            trainer_jsr: false,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::MapperContext;

    // BNROM tests (CHR-RAM)
    #[test]
    fn test_bnrom_prg_bank_switching() {
        // Create 128KB (4 banks of 32KB each) PRG ROM.
        // Fill with 0xFF so BNROM bus conflicts do not mask bank-select values.
        let mut prg_rom = vec![0xFF; 128 * 1024];

        // Mark each bank at offset +1 so writes at $8000 are still conflict-safe.
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            prg_rom[start + 1] = (bank * 10) as u8;
        }

        // Empty CHR ROM = BNROM variant
        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        // Initially bank 0
        assert_eq!(mapper.read_prg(0x8001), 0);

        // Switch to bank 1
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_prg(0x8001), 10);

        // Switch to bank 2
        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_prg(0x8001), 20);

        // Switch to bank 3
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0x8001), 30);
    }

    #[test]
    fn test_bnrom_chr_ram() {
        // BNROM uses CHR-RAM
        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));

        // CHR-RAM should be writable
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_bnrom_registers_and_chr_ram_snapshot_roundtrip() {
        let mut prg_rom = vec![0; 128 * 1024];

        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 3) as u8;
            }
        }

        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            prg_rom.clone(),
            vec![],
            NametableLayout::Horizontal,
        ));
        mapper.write_prg(0x8000, 2);
        mapper.write_chr(0x0000, 0x11);
        mapper.write_chr(0x1FFF, 0x22);

        let registers = mapper.registers_snapshot();
        let chr_ram = mapper.chr_ram_snapshot();

        let mut restored = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));
        restored.restore_registers(&registers);
        restored.restore_chr_ram(&chr_ram);

        assert_eq!(restored.read_prg(0x8000), 5);
        assert_eq!(restored.read_chr(0x0000), 0x11);
        assert_eq!(restored.read_chr(0x1FFF), 0x22);
    }

    #[test]
    fn test_bnrom_bank_select_anywhere() {
        // BNROM responds to any write in $8000-$FFFF
        // Fill with 0xFF so bus conflicts don't mask selected bank values in this test.
        let mut prg_rom = vec![0xFF; 128 * 1024];
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            prg_rom[start + 1] = (bank + 100) as u8;
        }

        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_prg(0x8001), 101);

        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_prg(0x8001), 102);

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_prg(0x8001), 103);
    }

    // NINA-001 tests (CHR ROM)
    #[test]
    fn test_nina001_prg_bank_switching() {
        // Create 128KB PRG ROM
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        // Non-empty CHR ROM = NINA-001 variant
        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            prg_rom,
            vec![0; 64 * 1024],
            NametableLayout::Horizontal,
        ));

        // Initially bank 0
        assert_eq!(mapper.read_prg(0x8000), 0);

        // NINA-001 uses only $7FFD for PRG bank select
        mapper.write_prg(0x7FFD, 1);
        assert_eq!(mapper.read_prg(0x8000), 10);

        mapper.write_prg(0x7FFD, 3);
        assert_eq!(mapper.read_prg(0x8000), 30);
    }

    #[test]
    fn test_nina001_chr_bank_switching() {
        // Create 64KB CHR ROM (16 banks of 4KB)
        let mut chr_rom = vec![0; 64 * 1024];
        for bank in 0..16 {
            let start = bank * 4 * 1024;
            let end = start + 4 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank * 20) as u8;
            }
        }

        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 128 * 1024],
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Initially both 4KB windows are bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1000), 0);

        // Select independent 4KB CHR banks
        mapper.write_prg(0x7FFE, 1); // $0000-$0FFF
        mapper.write_prg(0x7FFF, 2); // $1000-$1FFF
        assert_eq!(mapper.read_chr(0x0000), 20);
        assert_eq!(mapper.read_chr(0x1000), 40);

        // High values should wrap to available banks
        mapper.write_prg(0x7FFE, 7);
        mapper.write_prg(0x7FFF, 15);
        assert_eq!(mapper.read_chr(0x0000), 140);
        assert_eq!(mapper.read_chr(0x1000), 44);
    }

    #[test]
    fn test_nina001_7fff_only_updates_upper_chr_window_not_prg_bank() {
        // 4 PRG banks (32KB each) filled with unique markers
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        // 16 CHR banks (4KB each)
        let mut chr_rom = vec![0; 64 * 1024];
        for bank in 0..16 {
            let start = bank * 4 * 1024;
            let end = start + 4 * 1024;
            for byte in &mut chr_rom[start..end] {
                *byte = (bank + 1) as u8;
            }
        }

        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0x7FFD, 1);
        assert_eq!(mapper.read_prg(0x8000), 10);

        // $7FFF changes upper 4KB CHR bank only, not PRG bank
        mapper.write_prg(0x7FFF, 6);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_chr(0x1000), 7);
    }

    #[test]
    fn test_nina001_register_write_updates_prg_ram_overlay() {
        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 128 * 1024],
            vec![0; 64 * 1024],
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0x7FFD, 0x12);
        mapper.write_prg(0x7FFE, 0x23);
        mapper.write_prg(0x7FFF, 0x34);

        assert_eq!(mapper.read_prg(0x7FFD), 0x12);
        assert_eq!(mapper.read_prg(0x7FFE), 0x23);
        assert_eq!(mapper.read_prg(0x7FFF), 0x34);
    }

    #[test]
    fn test_nina001_ignores_8000_writes() {
        // NINA-001 should ignore writes to $8000-$FFFF (not a bank select region)
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            prg_rom,
            vec![0; 32 * 1024],
            NametableLayout::Horizontal,
        ));

        // Set bank via proper register
        mapper.write_prg(0x7FFD, 1);
        assert_eq!(mapper.read_prg(0x8000), 10);

        // Write to $8000 should not change bank
        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_prg(0x8000), 10); // Still bank 1

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_prg(0x8000), 10); // Still bank 1
    }

    #[test]
    fn test_bnrom_detection() {
        // Empty CHR ROM = BNROM
        let mapper_bnrom = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 32 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(!mapper_bnrom.is_nina);

        // 8KB CHR ROM with no submapper info remains BNROM per iNES mapper 34 heuristic
        let mapper_bnrom_8k_chr = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 32 * 1024],
            vec![0; 8 * 1024],
            NametableLayout::Horizontal,
        ));
        assert!(!mapper_bnrom_8k_chr.is_nina);

        // >8KB CHR ROM defaults to NINA-001/NINA-002 when submapper is absent
        let mapper_nina_32k_chr = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            NametableLayout::Horizontal,
        ));
        assert!(mapper_nina_32k_chr.is_nina);

        // NES 2.0 submapper 1 explicitly selects NINA-001/NINA-002
        let mapper_submapper_1 = BnromNinaMapper::new(
            MapperContext::new_for_test(
                34,
                vec![0; 32 * 1024],
                vec![0; 8 * 1024],
                NametableLayout::Horizontal,
            )
            .with_submapper(1),
        );
        assert!(mapper_submapper_1.is_nina);

        // NES 2.0 submapper 2 explicitly selects BNROM even with large CHR-ROM payload
        let mapper_submapper_2 = BnromNinaMapper::new(
            MapperContext::new_for_test(
                34,
                vec![0; 32 * 1024],
                vec![0; 32 * 1024],
                NametableLayout::Horizontal,
            )
            .with_submapper(2),
        );
        assert!(!mapper_submapper_2.is_nina);
    }

    #[test]
    fn test_bnrom_applies_and_type_bus_conflicts() {
        // Four 32KB PRG banks identified by value at $8001.
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..4 {
            let base = bank * 32 * 1024;
            prg_rom[base + 1] = (bank as u8) + 0x10;
        }

        // At reset bank 0 is active, so ROM[$8000] is PRG[0].
        // Set this byte to 0x02; writing 0x01 to $8000 must become effective value 0x00.
        prg_rom[0] = 0x02;

        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x8000, 0x01);
        assert_eq!(
            mapper.read_prg(0x8001),
            0x10,
            "BNROM must apply AND-type bus conflicts on bank-select writes"
        );
    }

    #[test]
    fn test_bnrom_has_no_prg_ram_per_spec() {
        let mut mapper = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0xFF; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0);
        assert_eq!(mapper.wram_size(), 0);
    }

    #[test]
    fn test_mapper34_capabilities_reflect_variant_ram_size() {
        let mapper_bnrom = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0xFF; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper_bnrom.capabilities().max_prg_ram_kb, 0);

        let mapper_nina = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0xFF; 128 * 1024],
            vec![0; 32 * 1024],
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper_nina.capabilities().max_prg_ram_kb, 8);
    }

    #[test]
    fn test_bnrom_mirroring() {
        let mapper_h = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);

        let mapper_v = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 128 * 1024],
            vec![],
            NametableLayout::Vertical,
        ));
        assert_eq!(mapper_v.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_nina001_mirroring() {
        let mapper_h = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 128 * 1024],
            vec![0; 32 * 1024],
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);

        let mapper_v = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 128 * 1024],
            vec![0; 32 * 1024],
            NametableLayout::Vertical,
        ));
        assert_eq!(mapper_v.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_bnrom_banked_rom_replacement() {
        use crate::cartridge::common::BankedRom;
        use crate::cartridge::test_helpers::banked_data;

        const PRG_BANK_SIZE: usize = 0x8000; // 32KB

        let prg_rom = banked_data(PRG_BANK_SIZE, 4);
        let prg_banked = BankedRom::new(prg_rom, PRG_BANK_SIZE);

        // Test basic bank reading
        assert_eq!(prg_banked.read(0, 0), 0);
        assert_eq!(prg_banked.read(1, 0), 1);
        assert_eq!(prg_banked.read(2, 0), 2);
        assert_eq!(prg_banked.read(3, 0), 3);

        // Test bank wrapping
        assert_eq!(prg_banked.read(4, 0), 0);
        assert_eq!(prg_banked.read(7, 0), 3);
    }
}
