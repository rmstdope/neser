//! Mapper 316 – unknown board (minimal stub)
//!
//! ## Specifications
//!
//! - NesDev wiki: unavailable due to network restriction (Cloudflare 403).
//!   URL: <https://www.nesdev.org/wiki/INES_Mapper_316>
//! - Fallback: Mesen2 `MapperFactory.cpp` – `//316-318` (range comment; no
//!   board name recorded, not implemented).
//! - FCEUX: no implementation found for mapper 316.
//! - NES 2.0 database (nes20db.xml, 2021-12-25): no known ROM entries use
//!   mapper 316.
//!
//! ## Known Limitations
//!
//! No authoritative specification has been found for this board in any publicly
//! accessible source. This implementation is a minimal stub that allows ROMs
//! using mapper 316 to instantiate without panicking.
//!
//! - PRG and CHR banking behavior is unknown; treated as fixed (last 32 KiB of
//!   PRG-ROM at $8000–$FFFF; first 8 KiB of CHR at $0000–$1FFF).
//! - Mirroring control behavior is unknown; header value is preserved.
//! - IRQ behavior is unknown; no IRQ is generated.
//! - No PRG-RAM is provisioned (none observed in database entries).

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 316;

/// Mapper 316 – unknown board (minimal stub)
///
/// Hardware: unknown board (no documented games or PCB name found).
///
/// This is a minimal stub implementation. The board's specification is not
/// available from any accessible source. The implementation maps the full
/// PRG-ROM and CHR at fixed addresses with no banking, which allows ROMs to
/// instantiate without errors.
///
/// Power-on state: last 32 KiB of PRG mapped from $8000–$FFFF; first 8 KiB of
/// CHR from $0000–$1FFF.
pub struct Mapper316 {
    base: BaseMapper,
}

impl Mapper316 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
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
        let last_bank = self.base.prg_bank_count().saturating_sub(1) as i16;
        self.base.select_prg_page(0, last_bank);
        self.base.select_chr_page(0, 0);
    }
}

impl Mapper for Mapper316 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
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
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 3; // 3 × 32 KiB = 96 KiB
    const CHR_BANKS: usize = 5; // 5 × 8 KiB  = 40 KiB
    const PRG_BANK_SIZE: usize = 32 * 1024;
    const CHR_BANK_SIZE: usize = 8 * 1024;

    fn make_mapper() -> Mapper316 {
        Mapper316::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_316_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 316 must be registered in factory");
    }

    // ── Power-on PRG state ────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_maps_last_bank_at_8000() {
        let mapper = make_mapper();
        // banked_data fills bank N with byte value N.
        // With 3 banks (0,1,2), last bank = 2.
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 must read from the last PRG bank at power-on"
        );
    }

    #[test]
    fn power_on_prg_ffff_reads_last_bank() {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let last_byte = prg[PRG_BANK_SIZE * (PRG_BANKS - 1) + PRG_BANK_SIZE - 1];
        let mapper = Mapper316::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg,
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert_eq!(
            mapper.read_prg(0xFFFF),
            last_byte,
            "$FFFF must read last byte of last PRG bank"
        );
    }

    // ── Power-on CHR state ────────────────────────────────────────────────────

    #[test]
    fn power_on_chr_maps_first_bank_at_0000() {
        let mut mapper = make_mapper();
        // banked_data fills bank 0 with 0.
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "$0000 must map to CHR bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_1fff_reads_bank_0() {
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        let last_chr_byte = chr[CHR_BANK_SIZE - 1]; // last byte of bank 0 = 0
        let mut mapper = Mapper316::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            chr,
            NametableLayout::Horizontal,
        ));
        assert_eq!(
            mapper.read_chr(0x1FFF),
            last_chr_byte,
            "$1FFF must read last byte of CHR bank 0"
        );
    }

    // ── No banking (writes have no effect) ───────────────────────────────────

    #[test]
    fn writes_to_prg_space_do_not_change_banking() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG banking must remain fixed after writes"
        );
    }

    // ── Mirroring: fixed from header ──────────────────────────────────────────

    #[test]
    fn mirroring_is_fixed_from_header_horizontal() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be preserved from ROM header"
        );
    }

    #[test]
    fn mirroring_is_fixed_from_header_vertical() {
        let mapper = Mapper316::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Vertical mirroring from header must be preserved"
        );
    }

    #[test]
    fn mirroring_is_not_changed_by_prg_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Writes must not change mirroring"
        );
    }

    // ── No IRQ ───────────────────────────────────────────────────────────────

    #[test]
    fn irq_is_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 316 must never assert IRQ");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_is_empty() {
        let mapper = make_mapper();
        assert!(
            mapper.registers_snapshot().is_empty(),
            "Mapper 316 has no registers; snapshot must be empty"
        );
    }

    #[test]
    fn restore_registers_with_empty_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.restore_registers(&[]);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "restore_registers must not change fixed PRG mapping"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_leaves_prg_at_last_bank() {
        let mut mapper = make_mapper();
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG must remain at last bank after reset"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_report_no_irq_no_chr_banking_no_dynamic_mirroring() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq, "no IRQ expected");
        assert!(!caps.has_expansion_audio, "no expansion audio expected");
        assert!(!caps.has_dynamic_mirroring, "no dynamic mirroring expected");
        assert!(!caps.has_chr_banking, "no CHR banking expected");
        assert_eq!(caps.prg_bank_size_kb, 32);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
