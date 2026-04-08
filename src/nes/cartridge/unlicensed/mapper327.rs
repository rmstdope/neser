//! Mapper 327 – 10-24-C-A1 (unknown board)
//!
//! Specifications:
//! - NesDev wiki: unavailable due to network restriction (Cloudflare 403).
//! - Fallback: Mesen2 `MapperFactory.cpp` – `case 327: break; //10-24-C-A1`
//!   (not implemented; only the board name is recorded).
//! - FCEUX: no implementation found.
//!
//! Known Limitations:
//! - No authoritative specification has been found for this board in any publicly
//!   accessible source. This implementation is a minimal stub that allows ROMs
//!   to instantiate without panicking.
//! - PRG and CHR banking behavior is unknown; both are treated as fixed.
//! - Mirroring control behavior is unknown; header value is preserved.
//! - IRQ behavior is unknown; no IRQ is generated.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 327 – 10-24-C-A1 board (minimal stub)
///
/// Hardware: 10-24-C-A1 discrete logic board (game(s) unknown)
///
/// This is a minimal stub implementation. The board's specification is not available
/// from any accessible source. The implementation maps the full PRG-ROM and CHR at
/// fixed addresses with no banking, which allows ROMs to instantiate without errors.
///
/// Power-on state: PRG mapped from $8000–$FFFF; CHR from $0000–$1FFF.
pub struct Mapper327 {
    base: BaseMapper,
}

impl Mapper327 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self { base };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, 0);
        self.base.select_chr_page(0, 0);
    }
}

impl Mapper for Mapper327 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, _addr: u16, _value: u8) {
        // No known write-effect registers for this board.
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
    const PRG_BANKS: usize = 3; // 3 × 32KB = 96KB
    const CHR_BANKS: usize = 5; // 5 × 8KB = 40KB

    fn make_mapper() -> Mapper327 {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper327::new(MapperContext::new_for_test(
            327,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_327_is_registered() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            327,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 327 must be registered in the factory"
        );
    }

    // --- Power-on PRG state ---

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
    fn power_on_prg_ffff_reads_bank_0_last_byte() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        // Last byte of bank 0 is at offset 32767 = 0x7FFF
        let expected = prg[0x7FFF];
        let mapper = Mapper327::new(MapperContext::new_for_test(
            327,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert_eq!(
            mapper.read_prg(0xFFFF),
            expected,
            "$FFFF must read last byte of PRG bank 0"
        );
    }

    // --- Power-on CHR state ---

    #[test]
    fn power_on_chr_0000_reads_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "$0000 must map to CHR bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_1fff_reads_bank_0_last_byte() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        // Last byte of CHR bank 0 is at offset 8191 = 0x1FFF
        let expected = chr[0x1FFF];
        let mut mapper = Mapper327::new(MapperContext::new_for_test(
            327,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert_eq!(
            mapper.read_chr(0x1FFF),
            expected,
            "$1FFF must read last byte of CHR bank 0"
        );
    }

    // --- No PRG banking (writes have no effect) ---

    #[test]
    fn writes_to_prg_space_do_not_change_prg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG must remain bank 0 after writes"
        );
    }

    // --- Mirroring: fixed from header ---

    #[test]
    fn mirroring_fixed_from_header_horizontal() {
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
        assert!(!mapper.irq_pending(), "Mapper 327 must never assert IRQ");
    }

    // --- Snapshot / restore (no state) ---

    #[test]
    fn registers_snapshot_is_empty() {
        let mapper = make_mapper();
        assert!(
            mapper.registers_snapshot().is_empty(),
            "Mapper 327 has no registers; snapshot must be empty"
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
