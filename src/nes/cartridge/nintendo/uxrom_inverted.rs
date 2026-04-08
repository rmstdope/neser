//! Mapper 180 - UxROM (inverted / Crazy Climber variant)
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::mapper_templates::SimpleBankedPrgMapper;

/// Mapper 180 - UxROM (inverted, UNROM board)
///
/// Hardware: Simple PRG banking with fixed first bank (Crazy Climber variant)
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_180>
/// - PRG-ROM: Up to 128KB (8 × 16KB banks)
/// - PRG-RAM: None
/// - CHR: 8KB CHR-RAM fixed (no CHR-ROM support)
/// - Mirroring: Fixed horizontal or vertical (solder pads)
///
/// Notes:
/// - Fixed 16KB PRG bank 0 at $8000–$BFFF
/// - Switchable 16KB PRG bank at $C000–$FFFF (selected by write to $8000–$FFFF, bits 0–2)
/// - Bus conflicts: AND-type (same hardware family as UxROM)
/// - Used exclusively by Crazy Climber (Nichibutsu)
///
/// Implementation:
/// - Uses `SimpleBankedPrgMapper` template with `FIXED_LAST = false` for inverted bank layout
pub type UxromInvertedMapper = SimpleBankedPrgMapper<16, 180, false>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::MapperContext;
    use crate::cartridge::{Mapper, NametableLayout};

    fn make_128k_prg() -> Vec<u8> {
        // 8 banks of 16KB; each bank is filled with its own bank number
        (0u8..8).flat_map(|bank| vec![bank; 16 * 1024]).collect()
    }

    #[test]
    fn test_lower_window_fixed_to_bank_0() {
        // Spec: CPU $8000–$BFFF is always fixed to PRG bank 0
        let mut mapper = UxromInvertedMapper::new(
            MapperContext::new_for_test(180, make_128k_prg(), vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0)
                .with_unspecified_prg_ram_size()
                .with_submapper(2),
        );

        // Initially bank 0 at $8000
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should read bank 0 initially"
        );
        assert_eq!(
            mapper.read_prg(0xBFFF),
            0,
            "$BFFF should read bank 0 initially"
        );

        // After switching to bank 3, lower window must still be bank 0
        mapper.write_prg(0x8000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must remain fixed to bank 0 after write"
        );
        assert_eq!(
            mapper.read_prg(0xBFFF),
            0,
            "$BFFF must remain fixed to bank 0 after write"
        );

        // After switching to bank 7, lower window must still be bank 0
        mapper.write_prg(0x8000, 7);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must remain fixed to bank 0 at bank 7"
        );
    }

    #[test]
    fn test_upper_window_switchable() {
        // Spec: CPU $C000–$FFFF is the switchable bank, selected by bits 0–2 of register
        let mut mapper = UxromInvertedMapper::new(
            MapperContext::new_for_test(180, make_128k_prg(), vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0)
                .with_unspecified_prg_ram_size()
                .with_submapper(2),
        );

        // Initially the switchable window should map to bank 0
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 should be bank 0 initially"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "$FFFF should be bank 0 initially"
        );

        // Write selects bank 3 into $C000–$FFFF
        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 should switch to bank 3");
        assert_eq!(mapper.read_prg(0xFFFF), 3, "$FFFF should switch to bank 3");

        // Write via $FFFF selects bank 5
        mapper.write_prg(0xFFFF, 5);
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should switch to bank 5");

        // Write bank 1
        mapper.write_prg(0xC000, 1);
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 should switch to bank 1");
    }

    #[test]
    fn test_bank_select_mask_3_bits() {
        // Spec: bank select uses 3 bits (8 banks for 128KB)
        // Writing 0b0000_1101 (13) should mask to 5 (13 & 7 = 5), selecting bank 5
        // at $C000–$FFFF. The lower window ($8000) stays fixed at bank 0.
        let mut mapper = UxromInvertedMapper::new(
            MapperContext::new_for_test(180, make_128k_prg(), vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0)
                .with_unspecified_prg_ram_size()
                .with_submapper(2),
        );

        // 128KB / 16KB = 8 banks → next_power_of_two(8) - 1 = 7 (3-bit mask)
        // Writing 13 (0b1101) & 7 (0b0111) = 5, so upper window = bank 5
        mapper.write_prg(0x8000, 0b0000_1101);
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "$C000 should be bank 5 when writing 13 (0b1101) to 3-bit masked register (13 & 7 = 5)"
        );
        // Lower window must remain fixed at bank 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must remain bank 0 regardless of register write"
        );
    }

    #[test]
    fn test_registers_snapshot_restore() {
        // Snapshot bank_select and restore it correctly on a fresh instance
        let prg = make_128k_prg();
        let mut original = UxromInvertedMapper::new(
            MapperContext::new_for_test(180, prg.clone(), vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0)
                .with_unspecified_prg_ram_size()
                .with_submapper(2),
        );
        original.write_prg(0x8000, 5);
        original.write_chr(0x0000, 0xAB);

        let snapshot = original.registers_snapshot();
        let chr_snapshot = original.chr_ram_snapshot();

        let mut restored = UxromInvertedMapper::new(
            MapperContext::new_for_test(180, prg, vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0)
                .with_unspecified_prg_ram_size()
                .with_submapper(2),
        );
        restored.restore_registers(&snapshot);
        restored.restore_chr_ram(&chr_snapshot);

        // After restore, upper window should be bank 5, lower still bank 0
        assert_eq!(
            restored.read_prg(0xC000),
            5,
            "Upper window should restore to bank 5"
        );
        assert_eq!(
            restored.read_prg(0x8000),
            0,
            "Lower window must stay fixed at bank 0 after restore"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            0xAB,
            "CHR-RAM should restore correctly"
        );
    }

    #[test]
    fn test_chr_ram_read_write() {
        // CHR-RAM should be available and writable
        let mut mapper = UxromInvertedMapper::new(
            MapperContext::new_for_test(180, make_128k_prg(), vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0)
                .with_unspecified_prg_ram_size(),
        );

        mapper.write_chr(0x0000, 0x55);
        mapper.write_chr(0x1000, 0xAA);
        mapper.write_chr(0x1FFF, 0xFF);

        assert_eq!(mapper.read_chr(0x0000), 0x55);
        assert_eq!(mapper.read_chr(0x1000), 0xAA);
        assert_eq!(mapper.read_chr(0x1FFF), 0xFF);
    }

    #[test]
    fn test_mapper_number() {
        let mapper = UxromInvertedMapper::new(
            MapperContext::new_for_test(180, make_128k_prg(), vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0)
                .with_unspecified_prg_ram_size(),
        );
        assert_eq!(mapper.mapper_number(), 180);
    }
}
