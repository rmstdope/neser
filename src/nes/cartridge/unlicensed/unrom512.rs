//! Mapper 030 - UNROM-512 (homebrew)
//!
//! Specifications:
//! - Fallback: Mesen2 `Core/NES/Mappers/Homebrew/UnRom512.h`
//! - NesDev wiki: <https://www.nesdev.org/wiki/INES_Mapper_030>
//!
//! Register (`$8000-$FFFF` writes): `[M CC PPPPP]`
//! - `PPPPP` (bits 4-0): 16KB PRG bank at `$8000-$BFFF`; last bank fixed at `$C000-$FFFF`
//! - `CC` (bits 6-5): 8KB CHR-RAM bank at `$0000-$1FFF` (4 banks, 32KB total)
//! - `M` (bit 7): mirroring toggle, behaviour depends on submapper and header:
//!   - Submapper 3: 1=Vertical, 0=Horizontal
//!   - Otherwise (single-screen header): 1=SingleScreenUpper, 0=SingleScreenLower
//!   - Otherwise (H/V header): mirroring bit ignored (fixed from header)
//!
//! Bus conflicts:
//! - Submapper 0 (default, no battery): bus conflicts apply
//! - Submapper 1: no bus conflicts
//! - Submapper 2: bus conflicts apply
//! - Submapper 3 (V/H switchable): no bus conflicts
//!
//! Known Limitations:
//! - NES 2.0 self-flash PRG-ROM write emulation is not implemented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::common::ChrMemory;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const CHR_RAM_SIZE: usize = 32 * 1024;

const PRG_BANK_MASK: u8 = 0x1F; // bits 4-0
const CHR_BANK_SHIFT: u8 = 5; // bits 6-5
const CHR_BANK_MASK: u8 = 0x03;
const MIRRORING_BIT: u8 = 0x80; // bit 7

/// Mapper 030 – UNROM-512 homebrew board.
pub struct Unrom512Mapper {
    base: BaseMapper,
    register: u8,
    enable_mirroring_bit: bool,
    submapper3_mode: bool,
}

impl Unrom512Mapper {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let submapper = ctx.submapper;
        let header_mirroring = ctx.mirroring;

        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: ctx.prg_ram_banks_8k as usize * 8,
            ..Default::default()
        };

        let bus_conflicts = submapper == 0 || submapper == 2;
        let submapper3_mode = submapper == 3;

        let enable_mirroring_bit = submapper3_mode
            || matches!(
                header_mirroring,
                NametableLayout::SingleScreenLower | NametableLayout::SingleScreenUpper
            );

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.set_chr_memory(ChrMemory::new_ram(CHR_RAM_SIZE));
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.set_bus_conflicts(bus_conflicts);
        base.select_prg_page(0, 0);
        base.select_prg_page(1, -1);
        base.select_chr_page(0, 0);

        let initial_mirroring = if submapper3_mode {
            NametableLayout::Vertical
        } else {
            header_mirroring
        };
        base.set_mirroring(initial_mirroring);

        Self {
            base,
            register: 0,
            enable_mirroring_bit,
            submapper3_mode,
        }
    }

    fn decode_mirroring(&self, value: u8) -> NametableLayout {
        let mirror_bit_set = (value & MIRRORING_BIT) != 0;
        if self.submapper3_mode {
            if mirror_bit_set {
                NametableLayout::Vertical
            } else {
                NametableLayout::Horizontal
            }
        } else if mirror_bit_set {
            NametableLayout::SingleScreenUpper
        } else {
            NametableLayout::SingleScreenLower
        }
    }

    fn apply_register(&mut self, value: u8) {
        self.register = value;
        self.base.select_prg_page(0, (value & PRG_BANK_MASK) as i16);
        self.base.select_prg_page(1, -1);
        self.base
            .select_chr_page(0, ((value >> CHR_BANK_SHIFT) & CHR_BANK_MASK) as i16);

        if self.enable_mirroring_bit {
            self.base.set_mirroring(self.decode_mirroring(value));
        }
    }
}

impl Mapper for Unrom512Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.base.try_write_prg_ram(addr, value);
            return;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            let effective = self.base.apply_bus_conflict(addr, value);
            self.apply_register(effective);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.apply_register(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    /// Create a mapper 30 with submapper 1 (no bus conflicts) and H mirroring.
    fn make_mapper30() -> Box<dyn Mapper> {
        let prg_rom = banked_data(16 * 1024, 8);
        create_mapper(
            MapperContext::new_for_test(30, prg_rom, Vec::new(), NametableLayout::Horizontal)
                .with_submapper(1),
        )
        .expect("mapper 30 should be implemented")
    }

    #[test]
    fn mapper_30_is_registered() {
        let prg_rom = banked_data(16 * 1024, 8);
        let result = create_mapper(
            MapperContext::new_for_test(30, prg_rom, Vec::new(), NametableLayout::Horizontal)
                .with_submapper(1),
        );
        assert!(result.is_ok(), "mapper 30 must be available in factory");
    }

    #[test]
    fn default_prg_banks_are_bank0_and_last() {
        let mapper = make_mapper30();
        // Page 0 ($8000-$BFFF) = bank 0 = filled with 0x00
        assert_eq!(mapper.read_prg(0x8000), 0x00);
        // Page 1 ($C000-$FFFF) = last bank (7) = filled with 0x07
        assert_eq!(mapper.read_prg(0xC000), 0x07);
    }

    #[test]
    fn prg_bank_switching_via_bits_4_0() {
        let mut mapper = make_mapper30();

        mapper.write_prg(0x8000, 0b0000_0101); // PRG=5
        assert_eq!(mapper.read_prg(0x8000), 5);
        // last bank stays fixed
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn chr_ram_is_32kb() {
        let mapper = make_mapper30();
        assert_eq!(mapper.chr_ram_snapshot().len(), CHR_RAM_SIZE);
    }

    #[test]
    fn chr_bank_switching_via_bits_6_5() {
        let mut mapper = make_mapper30();

        // Switch to CHR bank 2 (bits 6:5 = 0b10 → reg = 0b0100_0000), then write
        mapper.write_prg(0x8000, 0b0100_0000);
        mapper.write_chr(0x0200, 0xBE);

        // Switch to bank 0 — should not see the value written to bank 2
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.read_chr(0x0200), 0x00);

        // Switch back to bank 2 — should see the written value
        mapper.write_prg(0x8000, 0b0100_0000);
        assert_eq!(mapper.read_chr(0x0200), 0xBE);
    }

    #[test]
    fn mirroring_fixed_from_header_for_horizontal() {
        let mapper = make_mapper30();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // Writing with bit 7 set should NOT change mirroring
        let mut mapper = make_mapper30();
        mapper.write_prg(0x8000, 0b1000_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_fixed_from_header_for_vertical() {
        let prg_rom = banked_data(16 * 1024, 8);
        let mut mapper = create_mapper(
            MapperContext::new_for_test(30, prg_rom, Vec::new(), NametableLayout::Vertical)
                .with_submapper(1),
        )
        .expect("mapper 30");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // Writing with bit 7 set should NOT change mirroring
        mapper.write_prg(0x8000, 0b1000_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_one_screen_switchable_when_header_is_single_screen() {
        let prg_rom = banked_data(16 * 1024, 8);
        let mut mapper = create_mapper(
            MapperContext::new_for_test(
                30,
                prg_rom,
                Vec::new(),
                NametableLayout::SingleScreenLower,
            )
            .with_submapper(1),
        )
        .expect("mapper 30");

        // Default is lower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // bit 7 = 1 → upper
        mapper.write_prg(0x8000, 0b1000_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        // bit 7 = 0 → lower
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn mirroring_submapper3_bit7_selects_vertical_horizontal() {
        let prg_rom = banked_data(16 * 1024, 8);
        let mut mapper = create_mapper(
            MapperContext::new_for_test(30, prg_rom, Vec::new(), NametableLayout::Horizontal)
                .with_submapper(3),
        )
        .expect("mapper 30");

        // SubMapper 3 starts with Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // bit 7 = 0 → Horizontal
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // bit 7 = 1 → Vertical
        mapper.write_prg(0x8000, 0b1000_0000);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn bus_conflicts_disabled_for_submapper1() {
        // With bus conflicts disabled, writes should take effect directly
        let mut mapper = make_mapper30(); // submapper 1
        // PRG ROM bank 3 contains all 0x03 bytes; writing 0x05 selects bank 5
        mapper.write_prg(0x8000, 0x05);
        // If no bus conflict: bank 5 mapped (filled with 5)
        assert_eq!(mapper.read_prg(0x8000), 5);
    }

    #[test]
    fn bus_conflicts_apply_for_submapper0() {
        // Submapper 0 (default, no battery) has bus conflicts
        // PRG ROM bank 0 contains all 0x00 bytes
        // Writing 0x05 to addr in bank 0: effective = 0x05 & 0x00 = 0x00 → bank 0 selected
        let prg_rom = banked_data(16 * 1024, 8);
        let mut mapper = create_mapper(MapperContext::new_for_test(
            30,
            prg_rom,
            Vec::new(),
            NametableLayout::Horizontal,
        ))
        .expect("mapper 30");

        // At boot, $8000 maps to bank 0 (all 0x00 bytes)
        // Writing 0x05 → bus conflict: 0x05 & 0x00 = 0x00 → stays on bank 0
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 0); // bank 0 (bus conflict forced it to 0)
    }

    #[test]
    fn registers_snapshot_restore_roundtrip() {
        let mut mapper = make_mapper30();
        // Set PRG=5, CHR=2 (0b0100_0101), mirroring unchanged
        mapper.write_prg(0x8000, 0b0100_0101);

        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper30();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_prg(0xC000), mapper.read_prg(0xC000));
    }

    const CHR_RAM_SIZE: usize = 32 * 1024;
}
