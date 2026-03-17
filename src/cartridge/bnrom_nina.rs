//! Mapper 34 - BNROM / NINA-001
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::BaseMapper;
use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::NametableLayout;
use std::env;

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
/// - Submapper 0 / no submapper fallback: CHR-ROM absent => BNROM, CHR-ROM present => NINA-001/NINA-002
/// - Submapper 0 NINA path intentionally enables FCEUX-compatible mapper-34 hack behavior
///   used by some patched ROMs (e.g. hM34 set):
///   - PRG bank writes accepted at $8000-$FFFF
///   - $7FFD PRG register uses full value (not bit0-only)
///     This preserves gameplay/audio/graphics parity for those ROMs while keeping
///     strict NINA semantics for explicit NES 2.0 submapper 1.
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
///
/// Compatibility note:
/// - For iNES/submapper-0 mapper 34 ROM hacks, this implementation follows FCEUX's
///   long-standing hybrid mapper-34 behavior where NINA CHR banking and PRG writes
///   at $8000-$FFFF coexist. This mode is intentionally isolated to submapper 0.
pub struct BnromNinaMapper {
    base: BaseMapper,
    mirroring: NametableLayout,
    prg_bank: u8,
    chr_bank_low: u8,
    chr_bank_high: u8,
    is_nina: bool, // true for NINA-001, false for BNROM
    // Enabled only for mapper 34 submapper 0 + CHR-ROM fallback to match common
    // FCEUX-compatible hacked ROM expectations (hybrid NINA/BNROM PRG writes).
    mapper34_compat_mode: bool,
    trace_enabled: bool,
    trace_budget: u32,
}

impl BnromNinaMapper {
    fn trace_state(&mut self, event: &str, addr: u16, value: u8) {
        if !self.trace_enabled || self.trace_budget == 0 {
            return;
        }

        self.trace_budget -= 1;
        eprintln!(
            "[mapper34] {event} addr={addr:04X} val={value:02X} is_nina={} compat={} prg={} chr_lo={} chr_hi={}",
            self.is_nina,
            self.mapper34_compat_mode,
            self.prg_bank,
            self.chr_bank_low,
            self.chr_bank_high
        );
    }

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        // Detect variant using explicit NES 2.0 submappers where available.
        // Submapper 1 = NINA-001/NINA-002, submapper 2 = BNROM.
        // For submapper 0 / iNES fallback, match Mesen/FCEUX behavior:
        // CHR-ROM present => NINA, CHR-ROM absent => BNROM.
        let is_nina = match ctx.submapper {
            1 => true,
            2 => false,
            _ => !ctx.chr_rom.is_empty(),
        };
        // Compatibility mode for mapper 34 iNES/submapper 0 hacks.
        // Keeps strict hardware-style behavior for explicit submapper 1.
        let mapper34_compat_mode = is_nina && ctx.submapper == 0;

        let capabilities = MapperCapabilities {
            has_irq: false,
            has_chr_banking: is_nina,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: if is_nina {
                8
            } else {
                ctx.prg_ram_banks_8k as usize * 8
            },
            prg_bank_size_kb: 32,
            chr_bank_size_kb: if is_nina { 4 } else { 8 },
            trainer_jsr: false,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(0x8000); // 32KB
        if is_nina {
            base.configure_chr_banking(0x1000); // 4KB
        }

        let chr_bank_high = if is_nina { 1 } else { 0 };

        let trace_enabled = env::var_os("NESER_TRACE_MAPPER34").is_some();
        let trace_budget = env::var("NESER_TRACE_MAPPER34_LIMIT")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(200);

        let mut mapper = Self {
            base,
            mirroring,
            prg_bank: 0,
            chr_bank_low: 0,
            chr_bank_high,
            is_nina,
            mapper34_compat_mode,
            trace_enabled,
            trace_budget,
        };
        mapper.update_banks();
        mapper.trace_state("init", 0, 0);
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        if self.is_nina {
            self.base.select_chr_page(0, self.chr_bank_low as i16);
            self.base.select_chr_page(1, self.chr_bank_high as i16);
        }
        self.base.set_mirroring(self.mirroring);
    }
}

impl Mapper for BnromNinaMapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank_low = 0;
        self.chr_bank_high = if self.is_nina { 1 } else { 0 };
        self.update_banks();
        self.trace_state("reset", 0, 0);
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // NINA-001 registers overlap PRG-RAM at $7FFD-$7FFF.
        // A write updates both the register and underlying RAM byte.
        if self.is_nina && (0x7FFD..=0x7FFF).contains(&addr) {
            let _ = self.base.try_write_prg_ram(addr, value);

            match addr {
                0x7FFD => {
                    // Strict NINA (submapper 1): bit0 PRG bank select.
                    // Submapper 0 compatibility mode: accept full register value
                    // to match FCEUX mapper-34 hack behavior.
                    if self.mapper34_compat_mode {
                        self.prg_bank = value;
                    } else {
                        self.prg_bank = value & 0x01;
                    }
                }
                0x7FFE => {
                    self.chr_bank_low = value;
                }
                0x7FFF => {
                    self.chr_bank_high = value;
                }
                _ => {}
            }
            self.update_banks();
            self.trace_state("write_nina_reg", addr, value);
            return;
        }

        // Submapper 0 compatibility mode accepts PRG bank writes at $8000-$FFFF
        // (FCEUX mapper-34 hybrid behavior used by several hM34 ROMs).
        if self.is_nina && self.mapper34_compat_mode && addr >= 0x8000 {
            self.prg_bank = value;
            self.update_banks();
            self.trace_state("write_nina_compat_prg", addr, value);
            return;
        }

        if self.base.try_write_prg_ram(addr, value) {
            return;
        }

        // BNROM: Any write to $8000-$FFFF sets PRG bank
        // NINA-001: Writes to $8000-$FFFF are ignored (uses $7FFD-$7FFF instead)
        if !self.is_nina && addr >= 0x8000 {
            // BNROM has AND-type bus conflicts: effective value = write_value & rom_value_at_addr.
            let rom_value = self.base.read_prg_banked(addr);
            self.prg_bank = value & rom_value;
            self.update_banks();
            self.trace_state("write_bnrom_bank", addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if self.is_nina {
            self.base.read_chr_banked(addr)
        } else {
            self.base.read_chr(addr)
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if self.is_nina {
            self.base.write_chr_banked(addr, value);
        } else {
            self.base.write_chr(addr, value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank_low, self.chr_bank_high]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.prg_bank = value;
        }
        if let Some(&value) = data.get(1) {
            self.chr_bank_low = value;
        }
        if let Some(&value) = data.get(2) {
            self.chr_bank_high = value;
        }
        self.update_banks();
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

        // Submapper 0 compatibility mode uses full-value PRG bank register.
        mapper.write_prg(0x7FFD, 3);
        assert_eq!(mapper.read_prg(0x8000), 30);

        mapper.write_prg(0x7FFD, 2);
        assert_eq!(mapper.read_prg(0x8000), 20);
    }

    #[test]
    fn test_nina001_submapper1_prg_bank_is_bit0_only() {
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

        let mut mapper = BnromNinaMapper::new(
            MapperContext::new_for_test(
                34,
                prg_rom,
                vec![0; 64 * 1024],
                NametableLayout::Horizontal,
            )
            .with_submapper(1),
        );

        mapper.write_prg(0x7FFD, 3);
        assert_eq!(mapper.read_prg(0x8000), 10);

        mapper.write_prg(0x7FFD, 2);
        assert_eq!(mapper.read_prg(0x8000), 0);
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

        // NINA powers on with CHR banks mapped as 0/1 for the two 4KB windows.
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1000), 20);

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
    fn test_nina001_reset_restores_default_prg_and_chr_mapping() {
        let mut prg_rom = vec![0; 128 * 1024];
        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank * 10) as u8;
            }
        }

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
        mapper.write_prg(0x7FFE, 5);
        mapper.write_prg(0x7FFF, 7);

        mapper.reset();

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 1);
        assert_eq!(mapper.read_chr(0x1000), 2);
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

        let mut mapper = BnromNinaMapper::new(
            MapperContext::new_for_test(
                34,
                prg_rom,
                vec![0; 32 * 1024],
                NametableLayout::Horizontal,
            )
            .with_submapper(1),
        );

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
    fn test_nina001_submapper0_accepts_8000_prg_writes_for_compat() {
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

        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_prg(0x8000), 20);

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_prg(0x8000), 30);
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

        // Submapper 0 fallback follows Mesen/FCEUX compatibility:
        // any CHR-ROM payload indicates NINA wiring.
        let mapper_nina_8k_chr = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 32 * 1024],
            vec![0; 8 * 1024],
            NametableLayout::Horizontal,
        ));
        assert!(mapper_nina_8k_chr.is_nina);

        // >8KB CHR ROM defaults to NINA-001/NINA-002 when submapper is absent
        let mapper_nina_32k_chr = BnromNinaMapper::new(MapperContext::new_for_test(
            34,
            vec![0; 32 * 1024],
            vec![0; 32 * 1024],
            NametableLayout::Horizontal,
        ));
        assert!(mapper_nina_32k_chr.is_nina);

        // Explicit submapper 0 (NES 2.0) uses the same CHR-ROM presence fallback.
        let mapper_submapper_0_no_chr = BnromNinaMapper::new(
            MapperContext::new_for_test(
                34,
                vec![0; 32 * 1024],
                vec![],
                NametableLayout::Horizontal,
            )
            .with_submapper(0),
        );
        assert!(!mapper_submapper_0_no_chr.is_nina);

        let mapper_submapper_0_with_chr = BnromNinaMapper::new(
            MapperContext::new_for_test(
                34,
                vec![0; 32 * 1024],
                vec![0; 8 * 1024],
                NametableLayout::Horizontal,
            )
            .with_submapper(0),
        );
        assert!(mapper_submapper_0_with_chr.is_nina);

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
        let mut mapper = BnromNinaMapper::new(
            MapperContext::new_for_test(
                34,
                vec![0xFF; 128 * 1024],
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0);
        assert_eq!(mapper.wram_size(), 0);
    }

    #[test]
    fn test_mapper34_capabilities_reflect_variant_ram_size() {
        let mapper_bnrom = BnromNinaMapper::new(
            MapperContext::new_for_test(
                34,
                vec![0xFF; 128 * 1024],
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
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
