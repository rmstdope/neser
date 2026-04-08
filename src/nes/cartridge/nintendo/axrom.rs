//! Mapper 7 - AxROM
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing/board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::nes::cartridge::Mapper;
use crate::nes::cartridge::MapperCapabilities;
use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::common::ChrMemory;
use crate::trace_mapper;

/// Mapper 7 - AxROM (AMROM, ANROM, AN1ROM, AOROM boards)
///
/// Hardware: Simple 32KB PRG banking with programmable one-screen mirroring
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/AxROM>
/// - Variants: <https://www.nesdev.org/wiki/AxROM#Variants>
/// - PRG-ROM: Up to 512KB (16 32KB banks, oversized AxROM)
/// - PRG-RAM: None (some bootleg boards have 8KB)
/// - CHR: 8KB CHR-RAM fixed (no CHR-ROM support)
/// - Mirroring: Programmable one-screen (selectable A or B nametable)
///
/// Common boards: NES-AMROM, NES-ANROM, NES-AOROM
///
/// Notes:
/// - Register at any write to $8000-$FFFF
/// - Bits 0-3: Select 32KB PRG bank (4 bits for up to 512KB)
/// - Bit 4: One-screen mirroring (0 = lower/A, 1 = upper/B)
/// - Used in Battletoads, Marble Madness, Wizards & Warriors
pub struct AxROMMapper {
    base: BaseMapper,
    register: u8,
}

impl AxROMMapper {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let submapper = if ctx.submapper == 0 && ctx.crc32 == 0x41D3_2FD7 {
            2
        } else {
            ctx.submapper
        };
        // AxROM boards have no PRG-RAM; only allocate when explicitly specified.
        let prg_ram_banks_8k = if ctx.prg_ram_size_specified {
            ctx.prg_ram_banks_8k
        } else {
            0
        };
        Self::new_internal(ctx, submapper, prg_ram_banks_8k)
    }

    fn new_internal(
        mut ctx: crate::nes::cartridge::mapper::MapperContext,
        submapper: u8,
        prg_ram_banks_8k: u8,
    ) -> Self {
        // Normalize 16KB ROMs to 32KB for consistent 32KB banking
        if ctx.prg_rom.len() == 0x4000 {
            let mirrored = ctx.prg_rom.clone();
            ctx.prg_rom.extend_from_slice(&mirrored);
        }
        ctx.prg_ram_banks_8k = prg_ram_banks_8k;

        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let bus_conflicts = submapper != 1;
        let mut base = BaseMapper::new(&ctx, capabilities);
        // AxROM uses CHR-RAM regardless of header
        base.set_chr_memory(ChrMemory::new_ram(8192));
        base.configure_prg_banking(32 * 1024);
        base.set_bus_conflicts(bus_conflicts);

        Self { base, register: 0 }
    }
}

impl Mapper for AxROMMapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }

        // Register at $8000-$FFFF
        // Bits 0-3: PRG bank select (4 bits for up to 512KB oversized AxROM)
        // Bit 4: One-screen mirroring (0 = lower, 1 = upper)
        if (0x8000..=0xFFFF).contains(&addr) {
            let register_value = self.base.apply_bus_conflict(addr, value);
            self.register = register_value;
            self.base.select_prg_page(0, (register_value & 0x0F) as i16);
            let mirroring = if (register_value & 0x10) != 0 {
                NametableLayout::SingleScreenUpper
            } else {
                NametableLayout::SingleScreenLower
            };
            self.base.set_mirroring(mirroring);

            trace_mapper!(1;
                "AxROM write ${:04X}: raw=${:02X} effective=${:02X} bank={} mirroring={} conflicts={}",
                addr,
                value,
                register_value,
                register_value & 0x0F,
                if (register_value & 0x10) != 0 { "upper" } else { "lower" },
                self.base.has_bus_conflicts()
            );
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.register]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.register = value;
            self.base.select_prg_page(0, (value & 0x0F) as i16);
            let mirroring = if (value & 0x10) != 0 {
                NametableLayout::SingleScreenUpper
            } else {
                NametableLayout::SingleScreenLower
            };
            self.base.set_mirroring(mirroring);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};

    fn create_axrom_mapper(prg_rom: Vec<u8>, mirroring: NametableLayout) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(7, prg_rom, vec![], mirroring).with_submapper(1))
            .expect("Failed to create AxROM mapper")
    }

    #[test]
    fn test_axrom_256kb_prg_bank_switching() {
        // AxROM with 256KB (8 banks × 32KB)
        let mut prg_rom = vec![0; 256 * 1024];

        // Fill each 32KB bank with its bank number
        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = bank as u8;
            }
        }

        let mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Default bank should be 0
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn test_axrom_bank_select_bits_0_2() {
        // Test bank selection with a 256KB ROM (8 banks, uses bits 0-2)
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 100) as u8;
            }
        }

        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Write to $8000 with different bank values
        mapper.write_prg(0x8000, 0x00); // Bank 0
        assert_eq!(mapper.read_prg(0x8000), 100);

        mapper.write_prg(0x8000, 0x01); // Bank 1
        assert_eq!(mapper.read_prg(0x8000), 101);

        mapper.write_prg(0x8000, 0x07); // Bank 7
        assert_eq!(mapper.read_prg(0x8000), 107);

        // Test that bits above 3 are ignored for bank selection
        mapper.write_prg(0x8000, 0xE2); // 0b11100010 -> bank 2 (bits 4+ ignored)
        assert_eq!(mapper.read_prg(0x8000), 102);
    }

    #[test]
    fn test_axrom_512kb_prg_bank_switching() {
        // Oversized AxROM with 512KB (16 banks × 32KB)
        // NESdev: "Some emulators allow bit 3 to be used to select up to 512 KB"
        let mut prg_rom = vec![0; 512 * 1024];

        for bank in 0..16 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 200) as u8;
            }
        }

        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // All 16 banks should be independently accessible via bits 0-3
        for bank in 0..16u8 {
            mapper.write_prg(0x8000, bank);
            assert_eq!(
                mapper.read_prg(0x8000),
                bank.wrapping_add(200),
                "Bank {} should be accessible with 4-bit select",
                bank
            );
        }
    }

    #[test]
    fn test_axrom_16kb_prg_is_mirrored_to_32kb_window() {
        let mut prg_rom = vec![0; 16 * 1024];
        for (index, byte) in prg_rom.iter_mut().enumerate() {
            *byte = (index & 0xFF) as u8;
        }

        let mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        assert_eq!(mapper.read_prg(0x8000), 0x00);
        assert_eq!(mapper.read_prg(0xBFFF), 0xFF);
        assert_eq!(mapper.read_prg(0xC000), 0x00);
        assert_eq!(mapper.read_prg(0xFFFF), 0xFF);
    }

    #[test]
    fn test_axrom_chr_ram() {
        // AxROM uses 8KB CHR-RAM (no CHR ROM)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Write to CHR-RAM
        mapper.write_chr(0x0000, 0x42);
        mapper.write_chr(0x1FFF, 0x99);

        // Read back
        assert_eq!(mapper.read_chr(0x0000), 0x42);
        assert_eq!(mapper.read_chr(0x1FFF), 0x99);
    }

    #[test]
    fn test_axrom_one_screen_mirroring_lower() {
        // Bit 4 = 0 selects lower nametable (single-screen A)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Write with bit 4 = 0 (lower nametable)
        mapper.write_prg(0x8000, 0x00); // Bits: 0000 0000
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Write with bit 4 = 0 but other bits set
        mapper.write_prg(0x8000, 0x07); // Bits: 0000 0111
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn test_axrom_one_screen_mirroring_upper() {
        // Bit 4 = 1 selects upper nametable (single-screen B)
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Write with bit 4 = 1 (upper nametable)
        mapper.write_prg(0x8000, 0x10); // Bits: 0001 0000
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn test_axrom_registers_and_chr_ram_snapshot_roundtrip() {
        let mut prg_rom = vec![0; 256 * 1024];

        for bank in 0..8 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 1) as u8;
            }
        }

        let mut mapper = create_axrom_mapper(prg_rom.clone(), NametableLayout::Horizontal);

        mapper.write_prg(0x8000, 0x07); // select bank 7
        mapper.write_chr(0x0000, 0x42);
        mapper.write_chr(0x1FFF, 0x99);

        let registers = mapper.registers_snapshot();
        let chr_ram = mapper.chr_ram_snapshot();

        let mut restored = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);
        restored.restore_registers(&registers);
        restored.restore_chr_ram(&chr_ram);

        assert_eq!(restored.read_prg(0x8000), 8);
        assert_eq!(restored.read_chr(0x0000), 0x42);
        assert_eq!(restored.read_chr(0x1FFF), 0x99);
    }

    #[test]
    fn test_axrom_128kb_rom_4_banks() {
        // Test with 128KB ROM (4 banks × 32KB)
        let mut prg_rom = vec![0; 128 * 1024];

        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 50) as u8;
            }
        }

        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Select each of the 4 banks
        for bank in 0..4 {
            mapper.write_prg(0x8000, bank as u8);
            assert_eq!(mapper.read_prg(0x8000), (bank + 50) as u8);
        }

        // Bank numbers wrap (bank 7 % 4 = 3)
        mapper.write_prg(0x8000, 0x07);
        assert_eq!(mapper.read_prg(0x8000), 53); // Bank 3
    }

    #[test]
    fn test_axrom_register_write_any_address() {
        // Writes anywhere in $8000-$FFFF should change the bank
        let mut prg_rom = vec![0; 128 * 1024];

        for bank in 0..4 {
            let start = bank * 32 * 1024;
            let end = start + 32 * 1024;
            for byte in &mut prg_rom[start..end] {
                *byte = (bank + 10) as u8;
            }
        }

        let mut mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        // Write to different addresses in PRG ROM space
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 10);

        mapper.write_prg(0xC000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 11);

        mapper.write_prg(0xFFFF, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 12);
    }

    #[test]
    fn test_axrom_has_no_prg_ram_when_disabled() {
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(0),
        )
        .expect("Failed to create AxROM mapper without PRG-RAM");

        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        assert_eq!(
            mapper.read_prg_open_bus(0x6000, 0x5A),
            0x5A,
            "AxROM should return open bus in $6000-$7FFF"
        );
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, 0xC3),
            0xC3,
            "AxROM should return open bus in $6000-$7FFF"
        );
        assert_eq!(mapper.wram_size(), 0, "AxROM should report no WRAM");
    }

    #[test]
    fn test_axrom_uses_prg_ram_when_present() {
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        )
        .expect("Failed to create AxROM mapper with PRG-RAM");

        mapper.write_prg(0x6000, 0xAA);
        mapper.write_prg(0x7FFF, 0xBB);

        assert_eq!(mapper.read_prg(0x6000), 0xAA);
        assert_eq!(mapper.read_prg(0x7FFF), 0xBB);
        assert_eq!(mapper.wram_size(), 8 * 1024);
    }

    #[test]
    fn test_axrom_open_bus() {
        let prg_rom = vec![0; 128 * 1024];
        let mapper = create_axrom_mapper(prg_rom, NametableLayout::Horizontal);

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0xEE), 0xEE);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0xFF), 0xFF);
    }

    #[test]
    fn test_axrom_submapper_0_applies_bus_conflicts_for_amrom_aorom() {
        // AMROM/AOROM (submapper 0, the default) have AND-type bus conflicts.
        // Writing 0x01 while bank 0 data is 0x00 → 0x01 & 0x00 = 0x00 → bank stays 0.
        let mut prg_rom = vec![0; 64 * 1024];
        for byte in &mut prg_rom[0..32 * 1024] {
            *byte = 0x00;
        }
        for byte in &mut prg_rom[32 * 1024..64 * 1024] {
            *byte = 0x01;
        }

        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_submapper(0),
        )
        .expect("AxROM submapper 0 must be created");

        mapper.write_prg(0x8000, 0x01);

        assert_eq!(
            mapper.read_prg(0x8000),
            0x00,
            "submapper 0 (AMROM/AOROM) must apply bus conflicts and keep bank 0 selected"
        );
    }

    #[test]
    fn test_axrom_submapper_2_applies_bus_conflicts_to_bank_select() {
        let mut prg_rom = vec![0; 64 * 1024];

        for byte in &mut prg_rom[0..32 * 1024] {
            *byte = 0x00;
        }
        for byte in &mut prg_rom[32 * 1024..64 * 1024] {
            *byte = 0x01;
        }

        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_submapper(2),
        )
        .expect("Failed to create AxROM mapper with submapper 2");

        mapper.write_prg(0x8000, 0x01);

        assert_eq!(
            mapper.read_prg(0x8000),
            0x00,
            "submapper 2 should apply bus conflicts and keep bank 0 selected"
        );
    }

    #[test]
    fn test_axrom_submapper_1_disables_bus_conflicts() {
        let mut prg_rom = vec![0; 64 * 1024];

        for byte in &mut prg_rom[0..32 * 1024] {
            *byte = 0x00;
        }
        for byte in &mut prg_rom[32 * 1024..64 * 1024] {
            *byte = 0x01;
        }

        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_submapper(1),
        )
        .expect("Failed to create AxROM mapper with submapper 1");

        mapper.write_prg(0x8000, 0x01);

        assert_eq!(
            mapper.read_prg(0x8000),
            0x01,
            "submapper 1 should not apply bus conflicts and should select bank 1"
        );
    }

    #[test]
    fn test_axrom_no_prg_ram_when_size_unspecified() {
        // AxROM boards have no PRG-RAM. When the ROM header doesn't specify
        // PRG-RAM size (prg_ram_size_specified=false), none should be allocated.
        let prg_rom = vec![0; 128 * 1024];
        let mut mapper = create_mapper(
            MapperContext::new_for_test(7, prg_rom, vec![], NametableLayout::Horizontal)
                .with_unspecified_prg_ram_size(),
        )
        .expect("AxROM mapper must be created");

        mapper.write_prg(0x6000, 0xAA);

        assert_eq!(
            mapper.read_prg_open_bus(0x6000, 0x55),
            0x55,
            "AxROM with unspecified PRG-RAM must return open bus at $6000-$7FFF"
        );
        assert_eq!(
            mapper.wram_size(),
            0,
            "AxROM should report no WRAM when unspecified"
        );
    }
}
