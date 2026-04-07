//! Mapper 151 — VRC1 on VS System
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_151>
//! - VRC1 ASIC: <https://www.nesdev.org/wiki/VRC1>
//!
//! Mapper 151 is an erroneous iNES assignment for the VRC1 ASIC on VS System boards.
//! It is identical to mapper 75 (VRC1) except:
//! - Mirroring is hardwired (typically four-screen on VS System), $9000 bit 0 is ignored
//! - CHR bank registers at $E000/$F000 use the full 8-bit value (no 5th high bit from $9000)
//! - No VS System game using VRC1 has >64KB CHR, so the 5th bit is unnecessary
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

/// Mapper 151 — VRC1 on VS System
///
/// Identical PRG banking to VRC1 (mapper 75): three switchable 8KB banks + fixed last.
/// CHR banking uses full 8-bit registers at $E000/$F000 (no high bit from $9000).
/// Mirroring is hardwired from the ROM header and cannot be changed at runtime.
pub struct Mapper151 {
    base: BaseMapper,
    prg_bank: [u8; 3],
    chr_bank: [u8; 2],
}

impl Mapper151 {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_SIZE: usize = 0x1000; // 4 KiB

    pub fn new(ctx: MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 4,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_bank: [0; 3],
            chr_bank: [0; 2],
        };

        mapper.update_prg_banks();
        mapper.update_chr_banks();
        mapper
    }

    fn update_prg_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank[0] as i16);
        self.base.select_prg_page(1, self.prg_bank[1] as i16);
        self.base.select_prg_page(2, self.prg_bank[2] as i16);
        self.base.select_prg_page(3, -1); // $E000-$FFFF fixed to last bank
    }

    fn update_chr_banks(&mut self) {
        self.base.select_chr_page(0, self.chr_bank[0] as i16);
        self.base.select_chr_page(1, self.chr_bank[1] as i16);
    }
}

impl Mapper for Mapper151 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(0),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr {
            0x8000..=0x8FFF => {
                self.prg_bank[0] = value & 0x0F;
                self.update_prg_banks();
            }
            0x9000..=0x9FFF => {
                // Mapper 151: $9000 is completely ignored.
                // Unlike standard VRC1, mirroring is hardwired and CHR high bits are unused.
            }
            0xA000..=0xAFFF => {
                self.prg_bank[1] = value & 0x0F;
                self.update_prg_banks();
            }
            0xC000..=0xCFFF => {
                self.prg_bank[2] = value & 0x0F;
                self.update_prg_banks();
            }
            0xE000..=0xEFFF => {
                self.chr_bank[0] = value;
                self.update_chr_banks();
            }
            0xF000..=0xFFFF => {
                self.chr_bank[1] = value;
                self.update_chr_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout: [0-2] prg_bank[0..2], [3-4] chr_bank[0..1]
        vec![
            self.prg_bank[0],
            self.prg_bank[1],
            self.prg_bank[2],
            self.chr_bank[0],
            self.chr_bank[1],
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 5 {
            self.prg_bank[0] = data[0];
            self.prg_bank[1] = data[1];
            self.prg_bank[2] = data[2];
            self.chr_bank[0] = data[3];
            self.chr_bank[1] = data[4];
            self.update_prg_banks();
            self.update_chr_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::create_mapper;
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two to prevent modulo-wrapping false-passes
    const PRG_BANKS: usize = 11; // 11 × 8KB = 88KB
    const CHR_BANKS: usize = 13; // 13 × 4KB = 52KB

    fn make_mapper() -> Mapper151 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(4 * 1024, CHR_BANKS);
        Mapper151::new(MapperContext::new_for_test(
            151,
            prg,
            chr,
            NametableLayout::FourScreen,
        ))
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    #[test]
    fn mapper_151_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            151,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(4 * 1024, CHR_BANKS),
            NametableLayout::FourScreen,
        ));
        assert!(
            result.is_ok(),
            "Mapper 151 must be registered in the factory"
        );
    }

    // -----------------------------------------------------------------------
    // Power-on PRG state
    // -----------------------------------------------------------------------

    #[test]
    fn power_on_prg_bank0_at_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must start at PRG bank 0");
    }

    #[test]
    fn power_on_prg_bank1_at_a000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xA000), 0, "$A000 must start at PRG bank 0");
    }

    #[test]
    fn power_on_prg_bank2_at_c000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 must start at PRG bank 0");
    }

    #[test]
    fn power_on_prg_e000_is_fixed_to_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000-$FFFF must always map to the last PRG bank"
        );
    }

    // -----------------------------------------------------------------------
    // PRG bank switching
    // -----------------------------------------------------------------------

    #[test]
    fn prg_bank0_select_via_8000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "$8000 must switch to PRG bank 3"
        );
        assert_eq!(mapper.read_prg(0xA000), 0, "$A000 must remain at bank 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 must remain at bank 0");
    }

    #[test]
    fn prg_bank1_select_via_a000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 5);
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "$A000 must switch to PRG bank 5"
        );
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must remain at bank 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 must remain at bank 0");
    }

    #[test]
    fn prg_bank2_select_via_c000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 7);
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 must switch to PRG bank 7"
        );
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must remain at bank 0");
        assert_eq!(mapper.read_prg(0xA000), 0, "$A000 must remain at bank 0");
    }

    #[test]
    fn prg_fixed_bank_at_e000_never_changes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0xA000, 6);
        mapper.write_prg(0xC000, 7);
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 fixed window must not change after PRG bank writes"
        );
    }

    // -----------------------------------------------------------------------
    // CHR bank switching — full 8-bit via $E000/$F000
    // -----------------------------------------------------------------------

    #[test]
    fn chr_bank0_select_via_e000_full_8bit() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 5);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "$0000 must map to CHR bank 5 after writing 5 to $E000"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "$1000 must remain at CHR bank 0"
        );
    }

    #[test]
    fn chr_bank1_select_via_f000_full_8bit() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 8);
        assert_eq!(
            mapper.read_chr(0x1000),
            8,
            "$1000 must map to CHR bank 8 after writing 8 to $F000"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "$0000 must remain at CHR bank 0"
        );
    }

    #[test]
    fn chr_bank_uses_full_value_not_lower_4_bits() {
        // Write a value whose full 8-bit bank number differs from its lower nibble.
        // With 13 CHR banks, 0x1C (28) selects bank 2 when the full value is used,
        // but would select bank 12 if the mapper incorrectly masked to 0x0C.
        // The two outcomes differ, so this proves mapper 151 honors the full 8-bit CHR bank value.
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x1C); // full value = 28, lower nibble = 12
        let expected_full_8bit = (28 % CHR_BANKS) as u8; // 28 % 13 = 2
        let wrong_lower_4bit = (12 % CHR_BANKS) as u8; // 12 % 13 = 12
        assert_ne!(
            expected_full_8bit, wrong_lower_4bit,
            "test setup: values must differ"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            expected_full_8bit,
            "CHR bank must use full 8-bit value, not lower 4 bits"
        );
    }

    // -----------------------------------------------------------------------
    // $9000 does NOT change mirroring
    // -----------------------------------------------------------------------

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::FourScreen,
            "Power-on mirroring must match header (FourScreen)"
        );
    }

    #[test]
    fn writing_9000_does_not_change_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x01); // bit 0 = 1 would be Horizontal on VRC1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::FourScreen,
            "$9000 bit 0 must NOT change mirroring on mapper 151"
        );
    }

    // -----------------------------------------------------------------------
    // $9000 does NOT affect CHR bank selection
    // -----------------------------------------------------------------------

    #[test]
    fn writing_9000_does_not_set_chr_high_bits() {
        let mut mapper = make_mapper();
        // On standard VRC1, $9000 bit 1 sets CHR bank 0 high bit (bit 4)
        // On mapper 151, $9000 should be completely ignored for CHR
        mapper.write_prg(0x9000, 0x02); // would set chr0 high bit on VRC1
        mapper.write_prg(0xE000, 0x00); // chr bank 0 low = 0
        // If $9000 bit 1 were used: bank = (1 << 4) | 0 = 16 → 16 % 13 = 3
        // If $9000 is ignored: bank = 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "$9000 bits must NOT contribute to CHR bank selection on mapper 151"
        );
    }

    // -----------------------------------------------------------------------
    // Snapshot round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn registers_snapshot_round_trips() {
        let mut original = make_mapper();
        original.write_prg(0x8000, 1);
        original.write_prg(0xA000, 2);
        original.write_prg(0xC000, 3);
        original.write_prg(0xE000, 5);
        original.write_prg(0xF000, 7);

        let snap = original.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 1, "PRG bank 0 must be preserved");
        assert_eq!(restored.read_prg(0xA000), 2, "PRG bank 1 must be preserved");
        assert_eq!(restored.read_prg(0xC000), 3, "PRG bank 2 must be preserved");
        assert_eq!(restored.read_chr(0x0000), 5, "CHR bank 0 must be preserved");
        assert_eq!(restored.read_chr(0x1000), 7, "CHR bank 1 must be preserved");
    }
}
