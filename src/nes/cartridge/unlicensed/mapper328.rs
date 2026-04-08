//! Mapper 328 – RT-01 copy-protection board
//!
//! Specifications:
//! - Fallback: Mesen2 `Rt01.h`
//!   (NesDev wiki unavailable due to network restriction)
//!
//! Known Limitations:
//! - The copy-protection reads at $CE80–$CEFF and $FE80–$FEFF return a
//!   constant 0xFF instead of pseudo-random hardware noise.  Games relying on
//!   value variation across successive reads may not pass the protection check.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 328 – RT-01 board
///
/// Hardware: RT-01 discrete logic (single game known: Thunderbirds)
///
/// Specifications (Mesen2 `Rt01.h`):
/// - PRG-ROM: Fixed 32 KiB at $8000–$FFFF (bank 0 at $8000–$BFFF, bank 1 at $C000–$FFFF)
/// - PRG-RAM: None
/// - CHR: Fixed 8 KiB ROM (banks 0–3 at 2 KiB each across $0000–$1FFF)
/// - Mirroring: Fixed from header (not programmable)
/// - Bus conflicts: None
/// - IRQ: None
///
/// Special reads:
/// - $CE80–$CEFF: returns copy-protection value (0xFF)
/// - $FE80–$FEFF: returns copy-protection value (0xFF)
/// - All other $8000–$FFFF reads: normal ROM data
///
/// Power-on state: PRG slots 0/1 mapped to banks 0/1; CHR slots 0–3 mapped to banks 0–3.
pub struct Mapper328 {
    base: BaseMapper,
}

impl Mapper328 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 2,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(2 * 1024);
        let mut mapper = Self { base };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, 0);
        self.base.select_prg_page(1, 1);
        self.base.select_chr_page(0, 0);
        self.base.select_chr_page(1, 1);
        self.base.select_chr_page(2, 2);
        self.base.select_chr_page(3, 3);
    }

    fn is_protection_addr(addr: u16) -> bool {
        (0xCE80..0xCF00).contains(&addr) || (0xFE80..0xFF00).contains(&addr)
    }
}

impl Mapper for Mapper328 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if (0x8000..=0xFFFF).contains(&addr) && Self::is_protection_addr(addr) {
            // Copy-protection: hardware returns 0xF2 | (rand & 0x0D).
            // Constant bits (0xF2) are always set; variable bits 0,2,3 are noise.
            // We return 0xFF (all bits set) as a compatible constant.
            return 0xFF;
        }
        match addr {
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, _addr: u16, _value: u8) {
        // No write-effect registers on RT-01.
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![]
    }

    fn restore_registers(&mut self, _data: &[u8]) {}

    fn reset(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 3; // 3 × 16KB = 48KB
    const CHR_BANKS: usize = 5; // 5 × 2KB = 10KB

    fn make_mapper() -> Mapper328 {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(2 * 1024, CHR_BANKS);
        Mapper328::new(MapperContext::new_for_test(
            328,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_328_is_registered() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(2 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            328,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 328 must be registered in the factory"
        );
    }

    // --- Power-on state: PRG ---

    #[test]
    fn power_on_prg_8000_reads_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_reads_bank_1() {
        let mapper = make_mapper();
        // Second 16KB window is fixed to bank 1.
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must map to PRG bank 1 at power-on"
        );
    }

    #[test]
    fn power_on_prg_lower_window_bank_0_upper_window_bank_1() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(2 * 1024, CHR_BANKS);
        // Bank 0 byte at offset 0 = 0; bank 1 byte at offset 0 = 1.
        // Lower window ($8000-$BFFF) must return bank 0 data, upper ($C000-$FFFF) bank 1.
        let mapper = Mapper328::new(MapperContext::new_for_test(
            328,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert_eq!(mapper.read_prg(0x8000), 0, "Lower window must be bank 0");
        assert_eq!(mapper.read_prg(0xC001), 1, "Upper window must be bank 1");
    }

    // --- Power-on state: CHR ---

    #[test]
    fn power_on_chr_pages_at_respective_banks() {
        let mut mapper = make_mapper();
        // Each 2KB page maps to its own bank: page 0→bank 0, page 1→bank 1, etc.
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR page 0 must be bank 0");
        assert_eq!(mapper.read_chr(0x0800), 1, "CHR page 1 must be bank 1");
        assert_eq!(mapper.read_chr(0x1000), 2, "CHR page 2 must be bank 2");
        assert_eq!(mapper.read_chr(0x1800), 3, "CHR page 3 must be bank 3");
    }

    // --- Copy-protection reads ---

    #[test]
    fn reads_from_ce80_cfff_return_protection_value() {
        let mapper = make_mapper();
        let value = mapper.read_prg(0xCE80);
        assert_eq!(
            value & 0xF2,
            0xF2,
            "Protection read at $CE80 must have bits 0xF2 set"
        );
    }

    #[test]
    fn reads_from_ceff_return_protection_value() {
        let mapper = make_mapper();
        let value = mapper.read_prg(0xCEFF);
        assert_eq!(
            value & 0xF2,
            0xF2,
            "Protection read at $CEFF must have bits 0xF2 set"
        );
    }

    #[test]
    fn reads_from_fe80_feff_return_protection_value() {
        let mapper = make_mapper();
        let value = mapper.read_prg(0xFE80);
        assert_eq!(
            value & 0xF2,
            0xF2,
            "Protection read at $FE80 must have bits 0xF2 set"
        );
    }

    #[test]
    fn reads_from_feff_return_protection_value() {
        let mapper = make_mapper();
        let value = mapper.read_prg(0xFEFF);
        assert_eq!(
            value & 0xF2,
            0xF2,
            "Protection read at $FEFF must have bits 0xF2 set"
        );
    }

    #[test]
    fn reads_just_outside_ce80_range_return_rom_data() {
        // $CE7F is just below the protection range; falls in the upper (bank 1) window.
        let mapper = make_mapper();
        let value = mapper.read_prg(0xCE7F);
        assert_eq!(
            value, 1,
            "$CE7F must return normal ROM data (bank 1), not the protection value"
        );
    }

    #[test]
    fn reads_just_outside_cf00_range_return_rom_data() {
        // $CF00 is just above the protection range; falls in the upper (bank 1) window.
        let mapper = make_mapper();
        let value = mapper.read_prg(0xCF00);
        assert_eq!(
            value, 1,
            "$CF00 must return normal ROM data (bank 1), not the protection value"
        );
    }

    #[test]
    fn reads_just_outside_fe80_range_return_rom_data() {
        let mapper = make_mapper();
        let value = mapper.read_prg(0xFE7F);
        assert_eq!(
            value, 1,
            "$FE7F must return normal ROM data (bank 1), not the protection value"
        );
    }

    #[test]
    fn reads_just_outside_ff00_range_return_rom_data() {
        let mapper = make_mapper();
        let value = mapper.read_prg(0xFF00);
        assert_eq!(
            value, 1,
            "$FF00 must return normal ROM data (bank 1), not the protection value"
        );
    }

    // --- No PRG banking (writes have no effect) ---

    #[test]
    fn writes_to_8000_ffff_do_not_change_prg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG must remain bank 0 after writes"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "PRG upper window must remain bank 1 after writes"
        );
    }

    // --- Mirroring: fixed from header ---

    #[test]
    fn mirroring_fixed_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed from header"
        );
    }

    #[test]
    fn mirroring_not_changed_by_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must not change after writes"
        );
    }

    // --- No IRQ ---

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 328 must never assert IRQ");
    }

    // --- Snapshot / restore (no state) ---

    #[test]
    fn registers_snapshot_is_empty() {
        let mapper = make_mapper();
        assert!(
            mapper.registers_snapshot().is_empty(),
            "Mapper 328 has no registers; snapshot must be empty"
        );
    }

    #[test]
    fn restore_registers_with_empty_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.restore_registers(&[]);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    // --- Reset (no effect on fixed mapper) ---

    #[test]
    fn reset_leaves_prg_at_bank_0() {
        let mut mapper = make_mapper();
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG must remain bank 0 after reset"
        );
    }
}
