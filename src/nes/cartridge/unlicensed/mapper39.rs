//! Mapper 39 - BMC-STUDYNGAME (Study and Game 32-in-1)
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::nes::cartridge::mapper_templates::DualBank32Mapper;

/// Mapper 39 - BMC-STUDYNGAME / Study and Game 32-in-1
///
/// Hardware: Simple unlicensed multicart mapper (BMC-STUDYNGAME board)
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_039>
/// - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper39.h`
/// - PRG-ROM: Up to 256 × 32KB banks
/// - PRG-RAM: None
/// - CHR: 8KB fixed (ROM or RAM, always bank 0)
/// - Mirroring: Fixed horizontal or vertical (solder pads)
///
/// Register map:
/// - Any write to $8000–$FFFF selects the 32KB PRG bank (full byte value)
/// - CHR bank is always fixed at 0
///
/// Notes:
/// - Used in the "Study and Game 32-in-1" multicart (Chinese release)
/// - No bus conflicts on any known board
/// - On reset, PRG bank is restored to 0
///
/// Implementation:
/// - Uses `DualBank32Mapper` template: PRG bits 7-0 (mask 0xFF, shift 0), CHR fixed at 0
pub type Mapper39 = DualBank32Mapper<0xFF, 0, 0, 0, false, 39>;

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_mapper39(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(39, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn test_mapper39_initial_prg_bank_is_zero() {
        // Mapper 39: Power-on PRG bank should be 0 (32KB window at $8000-$FFFF).
        // Use 3 banks (non-power-of-two) so that a wrong bank index won't wrap
        // to coincidentally match bank 0.
        let prg_rom = banked_data(32 * 1024, 3);
        let chr_rom = vec![0u8; 8 * 1024];

        let mapper = create_mapper39(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 39 should be supported");

        assert_eq!(mapper.read_prg(0x8000), 0, "initial PRG bank should be 0");
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "initial PRG bank should be 0 at end"
        );
    }

    #[test]
    fn test_mapper39_prg_bank_switching_via_write() {
        // Write to $8000-$FFFF selects the 32KB PRG bank.
        // Use 3 banks (non-power-of-two) to prevent modulo wrapping false pass.
        let prg_rom = banked_data(32 * 1024, 3);
        let chr_rom = vec![0u8; 8 * 1024];

        let mut mapper = create_mapper39(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 39 should be supported");

        // Select bank 1
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_prg(0x8000), 1, "PRG bank 1 should be mapped");
        assert_eq!(
            mapper.read_prg(0xFFFF),
            1,
            "PRG bank 1 at end should be mapped"
        );

        // Select bank 2
        mapper.write_prg(0xFFFF, 2);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG bank 2 should be mapped");
        assert_eq!(
            mapper.read_prg(0xFFFF),
            2,
            "PRG bank 2 at end should be mapped"
        );
    }

    #[test]
    fn test_mapper39_write_anywhere_in_prg_range_switches_bank() {
        // Any write to $8000-$FFFF should switch the PRG bank.
        let prg_rom = banked_data(32 * 1024, 3);
        let chr_rom = vec![0u8; 8 * 1024];

        let mut mapper = create_mapper39(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 39 should be supported");

        // Write at $C000 (middle of PRG range)
        mapper.write_prg(0xC000, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "write at $C000 should switch bank"
        );
    }

    #[test]
    fn test_mapper39_chr_is_fixed_at_bank_zero() {
        // CHR is always fixed at bank 0; PRG writes must not affect CHR bank.
        let prg_rom = banked_data(32 * 1024, 3);
        let chr_rom = banked_data(8 * 1024, 3);

        let mut mapper = create_mapper39(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 39 should be supported");

        // After switching PRG bank, CHR should still show bank 0 data
        mapper.write_prg(0x8000, 2);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be fixed at bank 0 regardless of PRG bank write"
        );
        assert_eq!(
            mapper.read_chr(0x1FFF),
            0,
            "CHR end should be fixed at bank 0 regardless of PRG bank write"
        );
    }

    #[test]
    fn test_mapper39_mirroring_is_fixed_from_header() {
        // Mapper 39 uses fixed mirroring from the cartridge header.
        let prg_rom = vec![0u8; 32 * 1024];
        let chr_rom = vec![0u8; 8 * 1024];

        let mut mapper =
            create_mapper39(prg_rom.clone(), chr_rom.clone(), NametableLayout::Vertical)
                .expect("Mapper 39 should be supported");

        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        // PRG writes should not change mirroring
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should not change after PRG bank write"
        );

        let mapper_h = create_mapper39(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 39 should be supported");
        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn test_mapper39_registers_snapshot_restore() {
        // registers_snapshot() / restore_registers() must round-trip the bank selection.
        let prg_rom = banked_data(32 * 1024, 3);
        let chr_rom = vec![0u8; 8 * 1024];

        let mut mapper = create_mapper39(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        )
        .expect("Mapper 39 should be supported");

        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_prg(0x8000), 2);

        let snapshot = mapper.registers_snapshot();

        let mut restored = create_mapper39(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 39 should be supported");
        restored.restore_registers(&snapshot);

        assert_eq!(
            restored.read_prg(0x8000),
            2,
            "restored mapper should select PRG bank 2"
        );
    }
}
