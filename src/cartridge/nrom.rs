//! Mapper 0 - NROM
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::base_mapper::BaseMapper;

/// Mapper 0 - NROM
///
/// Hardware: Nintendo's simplest cartridge board with no bank switching
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/NROM>
/// - PRG-ROM: 16KB or 32KB fixed (16KB mirrored at $C000-$FFFF)
/// - PRG-RAM: 2KB or 4KB at $6000-$7FFF (Family BASIC only), or none
/// - CHR: 8KB fixed (ROM or RAM)
/// - Mirroring: Fixed horizontal or vertical (solder pads)
///
/// Common boards: NES-NROM-128, NES-NROM-256, HVC-NROM-128, HVC-NROM-256
///
/// Notes:
/// - This is the baseline mapper implementation
/// - Used in early NES games like Super Mario Bros., Ice Climber, Excitebike
/// - Some NROM boards have no PRG-RAM (depends on board variant)
pub struct NROMMapper {
    base: BaseMapper,
}

impl NROMMapper {
    /// Create a new NROM mapper
    /// If chr_rom is empty, 8KB of CHR-RAM is allocated
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: false,
            has_chr_banking: false,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: if ctx.prg_ram_size_specified && ctx.prg_ram_banks_8k > 0 {
                ctx.prg_ram_banks_8k as usize * 8
            } else {
                0
            },
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            trainer_jsr: false,
            ..Default::default()
        };
        Self {
            base: BaseMapper::new(&ctx, capabilities),
        }
    }
}

impl Mapper for NROMMapper {
    fn base(&self) -> Option<&BaseMapper> {
        Some(&self.base)
    }

    fn base_mut(&mut self) -> Option<&mut BaseMapper> {
        Some(&mut self.base)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF (only if present)
        if let Some(value) = self.base.try_read_prg_ram(addr) {
            return value;
        }
        // PRG ROM at $8000-$FFFF; % len naturally mirrors 16KB ROM to $C000-$FFFF
        self.base.read_prg_rom_fixed(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // PRG-RAM at $6000-$7FFF (only if present)
        self.base.try_write_prg_ram(addr, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::MapperContext;

    #[test]
    fn test_nrom_32kb_prg_rom_read() {
        // Create a 32KB PRG ROM
        let mut prg_rom = vec![0; 0x8000]; // 32KB
        prg_rom[0x0000] = 0xAA; // First byte at $8000
        prg_rom[0x4000] = 0xBB; // First byte at $C000
        prg_rom[0x7FFF] = 0xCC; // Last byte at $FFFF

        let mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            prg_rom,
            vec![0; 8192],
            NametableLayout::Horizontal,
        ));

        // Test reading from different PRG addresses
        assert_eq!(mapper.read_prg(0x8000), 0xAA);
        assert_eq!(mapper.read_prg(0xC000), 0xBB);
        assert_eq!(mapper.read_prg(0xFFFF), 0xCC);
    }

    #[test]
    fn test_nrom_16kb_prg_rom_mirroring() {
        // Create a 16KB PRG ROM
        let mut prg_rom = vec![0; 0x4000]; // 16KB
        prg_rom[0x0000] = 0xAA; // First byte
        prg_rom[0x3FFF] = 0xBB; // Last byte

        let mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            prg_rom,
            vec![0; 8192],
            NametableLayout::Horizontal,
        ));

        // Test reading from $8000-$BFFF (first 16KB)
        assert_eq!(mapper.read_prg(0x8000), 0xAA);
        assert_eq!(mapper.read_prg(0xBFFF), 0xBB);

        // Test reading from $C000-$FFFF (mirrored second 16KB)
        assert_eq!(mapper.read_prg(0xC000), 0xAA); // Should mirror to $8000
        assert_eq!(mapper.read_prg(0xFFFF), 0xBB); // Should mirror to $BFFF
    }

    #[test]
    fn test_nrom_chr_rom_read() {
        // Create 8KB CHR ROM
        let mut chr_rom = vec![0; 8192];
        chr_rom[0x0000] = 0x11;
        chr_rom[0x0FFF] = 0x22;
        chr_rom[0x1000] = 0x33;
        chr_rom[0x1FFF] = 0x44;

        let mut mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Test reading from CHR ROM
        assert_eq!(mapper.read_chr(0x0000), 0x11);
        assert_eq!(mapper.read_chr(0x0FFF), 0x22);
        assert_eq!(mapper.read_chr(0x1000), 0x33);
        assert_eq!(mapper.read_chr(0x1FFF), 0x44);
    }

    #[test]
    fn test_nrom_chr_ram_write_and_read() {
        // Create mapper with CHR-RAM (empty CHR ROM)
        let mut mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![],
            NametableLayout::Horizontal,
        ));

        // Initially should read 0
        assert_eq!(mapper.read_chr(0x0000), 0x00);

        // Write to CHR-RAM
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1000, 0xBB);
        mapper.write_chr(0x1FFF, 0xCC);

        // Read back the values
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x1000), 0xBB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCC);
    }

    #[test]
    fn test_nrom_chr_rom_write_ignored() {
        // Create mapper with CHR ROM (not RAM)
        let chr_rom = vec![0x55; 8192];
        let mut mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            chr_rom,
            NametableLayout::Horizontal,
        ));

        // Try to write to CHR ROM (should be ignored)
        mapper.write_chr(0x0000, 0xAA);

        // Should still read original value
        assert_eq!(mapper.read_chr(0x0000), 0x55);
    }

    #[test]
    fn test_nrom_prg_write_ignored() {
        // NROM has no PRG-RAM or mapper registers
        let prg_rom = vec![0xAA; 0x8000];
        let mut mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            prg_rom,
            vec![0; 8192],
            NametableLayout::Horizontal,
        ));

        // Try to write to PRG space (should be ignored)
        mapper.write_prg(0x8000, 0xBB);

        // Should still read original value
        assert_eq!(mapper.read_prg(0x8000), 0xAA);
    }

    #[test]
    fn test_nrom_mirroring_modes() {
        let mapper_h = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);

        let mapper_v = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Vertical,
        ));
        assert_eq!(mapper_v.get_mirroring(), NametableLayout::Vertical);

        let mapper_4 = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::FourScreen,
        ));
        assert_eq!(mapper_4.get_mirroring(), NametableLayout::FourScreen);
    }

    #[test]
    fn test_nrom_ppu_address_changed_noop() {
        // NROM doesn't care about PPU address changes (no IRQ, no banking)
        let mut mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![0; 8192],
            NametableLayout::Horizontal,
        ));

        // Should not panic or change behavior
        mapper.ppu_address_changed(0x0000);
        mapper.ppu_address_changed(0x1000);
        mapper.ppu_address_changed(0x1FFF);
    }

    #[test]
    fn test_nrom_chr_ram_snapshot_restores_contents() {
        let mut mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![],
            NametableLayout::Horizontal,
        ));

        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x1FFF, 0xBB);

        let chr = mapper.chr_ram_snapshot();

        let mut restored = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 0x8000],
            vec![],
            NametableLayout::Horizontal,
        ));
        restored.restore_chr_ram(&chr);

        assert_eq!(restored.read_chr(0x0000), 0xAA);
        assert_eq!(restored.read_chr(0x1FFF), 0xBB);
    }

    #[test]
    fn test_nrom_open_bus() {
        let mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 32 * 1024],
            vec![0; 8192],
            NametableLayout::Horizontal,
        ));

        // Test various open-bus scenarios
        assert_eq!(mapper.read_prg_open_bus(0x0000, 0x12), 0x12);
        assert_eq!(mapper.read_prg_open_bus(0x1000, 0x34), 0x34);
        assert_eq!(mapper.read_prg_open_bus(0x2000, 0x56), 0x56);
        assert_eq!(mapper.read_prg_open_bus(0x3000, 0x78), 0x78);
        assert_eq!(mapper.read_prg_open_bus(0x4000, 0x9A), 0x9A);
        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xBC), 0xBC);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xDE), 0xDE);
    }

    #[test]
    fn test_nrom_mapped_regions_dont_return_open_bus() {
        let prg_rom = vec![0xAB; 32 * 1024];
        let chr_rom = vec![0; 8 * 1024];
        let mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ));

        let open_bus = 0x42;

        // PRG-ROM region ($8000-$FFFF) should return ROM data, not open-bus
        let rom_result = mapper.read_prg_open_bus(0x8000, open_bus);
        assert_eq!(
            rom_result, 0xAB,
            "PRG-ROM region should return ROM data, not open-bus"
        );

        let rom_result2 = mapper.read_prg_open_bus(0xC000, open_bus);
        assert_eq!(
            rom_result2, 0xAB,
            "PRG-ROM region should return ROM data, not open-bus"
        );
    }

    #[test]
    fn test_nrom_open_bus_boundary_at_6000() {
        let mapper = NROMMapper::new(MapperContext::new_for_test(
            0,
            vec![0; 32 * 1024],
            vec![0; 8192],
            NametableLayout::Horizontal,
        ));
        let open_bus = 0x55;

        // $5FFF should return open-bus
        assert_eq!(
            mapper.read_prg_open_bus(0x5FFF, open_bus),
            open_bus,
            "$5FFF should return open-bus"
        );

        // $6000 might return different value (PRG-RAM or 0)
        // We just verify it doesn't panic
        let _ = mapper.read_prg_open_bus(0x6000, open_bus);
    }

    // --- Issue #344: PRG-RAM absence via metadata ---

    #[test]
    fn test_nrom_no_prg_ram_read_returns_zero() {
        // Given: NROM with no PRG-RAM (prg_ram_banks_8k = 0)
        let mapper = NROMMapper::new(
            MapperContext::new_for_test(
                0,
                vec![0; 0x8000],
                vec![0; 8192],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );

        // When: reading from $6000-$7FFF
        // Then: return 0 (no RAM present)
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "no PRG-RAM: read at $6000 should return 0"
        );
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0,
            "no PRG-RAM: read at $7FFF should return 0"
        );
        assert_eq!(
            mapper.read_prg(0x6800),
            0,
            "no PRG-RAM: read at $6800 should return 0"
        );
    }

    #[test]
    fn test_nrom_no_prg_ram_write_ignored() {
        // Given: NROM with no PRG-RAM
        let mut mapper = NROMMapper::new(
            MapperContext::new_for_test(
                0,
                vec![0; 0x8000],
                vec![0; 8192],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );

        // When: writing to $6000-$7FFF
        mapper.write_prg(0x6000, 0xAB);
        mapper.write_prg(0x7FFF, 0xCD);

        // Then: reads still return 0 (writes ignored)
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "no PRG-RAM: write should be ignored"
        );
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0,
            "no PRG-RAM: write should be ignored"
        );
    }

    #[test]
    fn test_nrom_no_prg_ram_open_bus_returns_open_bus() {
        // Given: NROM with no PRG-RAM
        let mapper = NROMMapper::new(
            MapperContext::new_for_test(
                0,
                vec![0; 0x8000],
                vec![0; 8192],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        let open_bus = 0x42;

        // When: reading $6000-$7FFF with open_bus value
        // Then: return open_bus (no RAM present)
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus),
            open_bus,
            "no PRG-RAM: $6000 should return open-bus"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, open_bus),
            open_bus,
            "no PRG-RAM: $7FFF should return open-bus"
        );
    }

    #[test]
    fn test_nrom_with_prg_ram_read_write_works() {
        // Given: NROM with PRG-RAM present (prg_ram_banks_8k = 1)
        let mut mapper = NROMMapper::new(
            MapperContext::new_for_test(
                0,
                vec![0; 0x8000],
                vec![0; 8192],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(1),
        );

        // When: writing and reading back
        mapper.write_prg(0x6000, 0x77);
        mapper.write_prg(0x7FFF, 0x88);

        // Then: values are preserved
        assert_eq!(
            mapper.read_prg(0x6000),
            0x77,
            "PRG-RAM present: write/read should work"
        );
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0x88,
            "PRG-RAM present: write/read should work"
        );
    }
}
