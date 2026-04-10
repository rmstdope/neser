//! Mapper 184 - Sunsoft-1
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 184 - Sunsoft-1
///
/// Hardware: Sunsoft-1 chip (also describable as discrete logic).
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_184>
/// - PRG-ROM: Fixed 32 KB bank at $8000–$FFFF (no PRG switching)
/// - CHR-ROM: 8 KB (two switchable 4 KB pages)
/// - Mirroring: Fixed from cartridge header
///
/// Register (write to $6000–$7FFF):
///
/// ```text
/// 7 bit 0
/// ---------
/// $6000-7FFF: .HHH .LLL
///              |||   |||
///              +++   +++-- L: select 4 KB CHR bank at $0000 (range 0–7)
///              +---------- H: select 4 KB CHR bank at $1000 (range 4–7; MSB always set in hardware)
/// ```
///
/// Note: The most significant bit of H is always set in hardware, so H is always in range 4–7.
/// Note: No SRAM — $6000–$7FFF is exclusively register space.
pub struct Sunsoft1Mapper {
    base: BaseMapper,
    reg: u8,
}

impl Sunsoft1Mapper {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 4,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_chr_banking(4 * 1024);
        let mut mapper = Self { base, reg: 0 };
        mapper.apply_register(0);
        mapper
    }

    fn apply_register(&mut self, value: u8) {
        self.reg = value;
        let l = (value & 0x07) as i16;
        // MSB of H (bit 6 of value) is always set in hardware; range is 4–7.
        let h = ((value >> 4) & 0x07 | 0x04) as i16;
        self.base.select_chr_page(0, l);
        self.base.select_chr_page(1, h);
    }
}

impl Mapper for Sunsoft1Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.apply_register(value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&value) = data.first() {
            self.apply_register(value);
        }
    }

    fn reset(&mut self) {
        self.apply_register(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    fn create_mapper184(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            184,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ))
    }

    #[test]
    fn mapper184_instantiates_via_factory() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        assert!(
            create_mapper184(prg_rom, chr_rom).is_ok(),
            "Mapper 184 must be creatable via the factory"
        );
    }

    #[test]
    fn prg_is_fixed_at_bank_zero() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mapper = create_mapper184(prg_rom, chr_rom).unwrap();
        // PRG is always bank 0 (single 32KB bank)
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should read PRG bank 0");
        assert_eq!(mapper.read_prg(0xFFFF), 0, "$FFFF should read PRG bank 0");
    }

    #[test]
    fn prg_write_above_8000_is_ignored() {
        // Writes to $8000-$FFFF should not affect PRG banking
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mut mapper = create_mapper184(prg_rom, chr_rom).unwrap();
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG bank should remain 0 after write above $8000"
        );
    }

    #[test]
    fn chr_l_bits_select_4kb_at_0000() {
        // L = bits 2-0 of register, selects 4KB CHR bank at $0000
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8); // 8 banks of 4KB
        let mut mapper = create_mapper184(prg_rom, chr_rom).unwrap();

        // Write register with L=5 (bit pattern: ..0... 101)
        mapper.write_prg(0x6000, 0b0000_0101); // L=5, H=0 (forced to 4)
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "$0000 should be CHR bank 5 (L=5)"
        );
        assert_eq!(
            mapper.read_chr(0x0FFF),
            5,
            "$0FFF should still be CHR bank 5 (end of 4KB page)"
        );
    }

    #[test]
    fn chr_h_bits_select_4kb_at_1000_with_forced_msb() {
        // H = bits 6-4 of register, MSB of H is always set (bit 6 forced = range 4-7)
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mut mapper = create_mapper184(prg_rom, chr_rom).unwrap();

        // Write register with H bits = 0b010 (bits 6-4 = 0b010, so H raw = 2, forced H = 2|4 = 6)
        mapper.write_prg(0x6000, 0b0010_0000); // H raw = 2 (bit 5 set, bit 6 clear), L=0
        assert_eq!(
            mapper.read_chr(0x1000),
            6,
            "$1000 should be CHR bank 6 (H=2, forced MSB gives 2|4=6)"
        );
    }

    #[test]
    fn chr_h_msb_forced_even_when_zero() {
        // When H raw = 0 (bits 6-4 all clear), forced H = 0|4 = 4
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mut mapper = create_mapper184(prg_rom, chr_rom).unwrap();

        mapper.write_prg(0x6000, 0b0000_0000); // H=0, L=0; forced H = 4
        assert_eq!(
            mapper.read_chr(0x1000),
            4,
            "$1000 should be CHR bank 4 when H raw=0 (MSB forced: 0|4=4)"
        );
    }

    #[test]
    fn chr_register_writable_from_any_6000_7fff_address() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mut mapper = create_mapper184(prg_rom, chr_rom).unwrap();

        mapper.write_prg(0x7FFF, 0b0000_0011); // L=3
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "Write to $7FFF should update CHR L bank"
        );
    }

    #[test]
    fn chr_l_and_h_are_independent() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mut mapper = create_mapper184(prg_rom, chr_rom).unwrap();

        // L=2, H bits=3 (forced H = 3|4 = 7)
        mapper.write_prg(0x6000, 0b0011_0010); // H raw=3 (bits 6-4=0b011), L=2
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "$0000 should be CHR bank 2 (L=2)"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            7,
            "$1000 should be CHR bank 7 (H=3|4=7)"
        );
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mut mapper = create_mapper184(prg_rom, chr_rom).unwrap();

        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring should be fixed from header"
        );
        // Writing register should not change mirroring
        mapper.write_prg(0x6000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring should not change after register write"
        );
    }

    #[test]
    fn no_prg_ram_at_6000_7fff() {
        // $6000-$7FFF is register space (write-only); reads should return open bus
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mapper = create_mapper184(prg_rom, chr_rom).unwrap();
        let open_bus = 0xA5;
        // No PRG-RAM means reads at $6000-$7FFF should preserve the open-bus value.
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, open_bus),
            open_bus,
            "No PRG-RAM should be present; $6000 read should return the open-bus value"
        );
    }

    #[test]
    fn snapshot_restore_preserves_chr_banks() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mut mapper = create_mapper184(prg_rom.clone(), chr_rom.clone()).unwrap();

        // L=3, H raw=1 (bits 6-4=0b001), forced H = 1|4=5
        mapper.write_prg(0x6000, 0b0001_0011);
        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper184(prg_rom, chr_rom).unwrap();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_chr(0x0000),
            3,
            "CHR bank 3 (L=3) should be restored at $0000"
        );
        assert_eq!(
            restored.read_chr(0x1000),
            5,
            "CHR bank 5 (H=1|4=5) should be restored at $1000"
        );
    }

    #[test]
    fn reset_restores_initial_state() {
        let prg_rom = banked_data(32 * 1024, 1);
        let chr_rom = banked_data(4 * 1024, 8);
        let mut mapper = create_mapper184(prg_rom, chr_rom).unwrap();

        mapper.write_prg(0x6000, 0b0111_0111); // Set some non-default value
        mapper.reset();

        // After reset: L=0 → $0000 = bank 0; H=0|4=4 → $1000 = bank 4
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Reset should restore CHR L bank to 0"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            4,
            "Reset should restore CHR H bank to 4 (forced MSB)"
        );
    }
}
