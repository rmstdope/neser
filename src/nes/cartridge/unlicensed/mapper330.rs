//! Mapper 330 – unknown bootleg board
//!
//! Specifications:
//! - NesDev wiki: unavailable due to network restriction (Cloudflare 403).
//! - Fallback: Mesen2 `MapperFactory.cpp` – `//330` (not implemented; no board name recorded).
//! - FCEUX: no implementation found.
//! - NES 2.0 database: one known ROM – 三国志 III꞉ 覇王の大陸 (Romance of the Three Kingdoms III
//!   bootleg port).  Header specifies 256 KB PRG-ROM, 256 KB CHR-ROM, 8 KB PRG-NVRAM
//!   (battery-backed), fixed horizontal mirroring.
//!
//! Known Limitations:
//! - No authoritative specification has been found for this board in any publicly
//!   accessible source. This implementation is a minimal stub that allows ROMs to
//!   instantiate without panicking.
//! - PRG and CHR banking behavior is unknown; both are treated as fixed (last 32 KB of
//!   PRG-ROM mapped at $8000–$FFFF; first 8 KB of CHR-ROM mapped at $0000–$1FFF).
//! - Mirroring is fixed horizontal as specified in the NES 2.0 database entry.
//! - IRQ behavior is unknown; no IRQ is generated.
//! - PRG-NVRAM (battery-backed save RAM at $6000–$7FFF) is supported via BaseMapper.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 330 – unknown bootleg board (minimal stub)
///
/// Hardware: unknown discrete logic board (single known game: Romance of the Three
/// Kingdoms III bootleg NES port)
///
/// This is a minimal stub implementation. The board's specification is not available
/// from any accessible source. The implementation maps the full PRG-ROM and CHR at
/// fixed addresses with no banking, which allows ROMs to instantiate without errors.
///
/// Power-on state: last 32 KB of PRG mapped from $8000–$FFFF; first 8 KB of CHR
/// from $0000–$1FFF; PRG-NVRAM at $6000–$7FFF.
pub struct Mapper330 {
    base: BaseMapper,
}

impl Mapper330 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 8,
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

impl Mapper for Mapper330 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(0),
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.base.try_write_prg_ram(addr, value);
        }
        // No known write-effect registers for ROM banking on this board.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 5; // 5 × 32 KB = 160 KB
    const CHR_BANKS: usize = 7; // 7 × 8 KB = 56 KB

    fn make_mapper() -> Mapper330 {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper330::new(
            MapperContext::new_for_test(330, prg, chr, NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        )
    }

    // --- Registration ---

    #[test]
    fn mapper_330_is_registered() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(
            MapperContext::new_for_test(330, prg, chr, NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        );
        assert!(
            result.is_ok(),
            "Mapper 330 must be registered in the factory"
        );
    }

    // --- Power-on PRG state: last bank fixed at $8000-$FFFF ---

    #[test]
    fn power_on_prg_8000_reads_last_bank_first_byte() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        // Last bank marker is PRG_BANKS - 1
        let expected = prg[(PRG_BANKS - 1) * 32 * 1024];
        let mapper = Mapper330::new(
            MapperContext::new_for_test(330, prg, chr, NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        );
        assert_eq!(
            mapper.read_prg(0x8000),
            expected,
            "$8000 must map to last PRG bank at power-on"
        );
    }

    #[test]
    fn power_on_prg_ffff_reads_last_bank_last_byte() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        // Last byte of last bank
        let expected = prg[PRG_BANKS * 32 * 1024 - 1];
        let mapper = Mapper330::new(
            MapperContext::new_for_test(330, prg, chr, NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            expected,
            "$FFFF must read last byte of last PRG bank"
        );
    }

    // --- Power-on CHR state: bank 0 at $0000-$1FFF ---

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
        let expected = chr[0x1FFF];
        let mut mapper = Mapper330::new(
            MapperContext::new_for_test(330, prg, chr, NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        );
        assert_eq!(
            mapper.read_chr(0x1FFF),
            expected,
            "$1FFF must read last byte of CHR bank 0"
        );
    }

    // --- PRG-NVRAM at $6000-$7FFF ---

    #[test]
    fn prg_nvram_read_write_at_6000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "$6000 PRG-NVRAM must be readable after write"
        );
    }

    #[test]
    fn prg_nvram_read_write_at_7fff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0xCD);
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0xCD,
            "$7FFF PRG-NVRAM must be readable after write"
        );
    }

    // --- ROM banking is fixed (writes to $8000-$FFFF have no effect) ---

    #[test]
    fn writes_to_prg_rom_space_do_not_change_prg() {
        let mut mapper = make_mapper();
        let initial = mapper.read_prg(0x8000);
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            initial,
            "PRG must remain at last bank after writes to ROM space"
        );
    }

    // --- Mirroring: fixed horizontal ---

    #[test]
    fn mirroring_is_horizontal() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed horizontal"
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
        assert!(!mapper.irq_pending(), "Mapper 330 must never assert IRQ");
    }

    // --- Snapshot / restore (no register state) ---

    #[test]
    fn registers_snapshot_is_empty() {
        let mapper = make_mapper();
        assert!(
            mapper.registers_snapshot().is_empty(),
            "Mapper 330 has no registers; snapshot must be empty"
        );
    }

    #[test]
    fn restore_registers_with_empty_data_is_noop() {
        let mut mapper = make_mapper();
        let expected = mapper.read_prg(0x8000);
        mapper.restore_registers(&[]);
        assert_eq!(mapper.read_prg(0x8000), expected);
    }

    // --- Reset (no effect on fixed mapper) ---

    #[test]
    fn reset_leaves_prg_at_last_bank() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let expected = prg[(PRG_BANKS - 1) * 32 * 1024];
        let mut mapper = Mapper330::new(
            MapperContext::new_for_test(330, prg, chr, NametableLayout::Horizontal)
                .with_prg_ram_banks(1),
        );
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            expected,
            "PRG must remain at last bank after reset"
        );
    }
}
