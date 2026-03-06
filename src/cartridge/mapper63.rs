//! Mapper 063 - BMC multi-game bank switching
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_063>
//! - Fallback: Mesen2 `Core/NES/Mappers/Ntdec/Bmc63.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 063 - BMC multi-game bank switching (NTDEC BMC-63 / multi-game compilation)
///
/// Hardware: BMC multi-game compilation board
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_063>
/// - PRG-ROM: Up to 2 MiB (6-bit 32KB bank selector)
/// - CHR: 8 KB CHR-RAM (no CHR banking)
/// - Mirroring: Programmable (H/V)
///
/// Register on any write to `$8000–$FFFF` (data byte encodes control):
///
/// Data: D~[pp pppp M O]
///
///   PRG:
///   - pp pppp (bits 7:2) = 6-bit PRG bank select
///
///   O (bit 0) = PRG banking mode:
///     0: 32 KB at `$8000–$FFFF` (consecutive 16 KB pair)
///     1: 16 KB at `$8000–$BFFF` (selected bank), last 16 KB fixed at `$C000–$FFFF`
///
///   M (bit 1) = Mirroring: 0=Vertical, 1=Horizontal
pub struct Mapper63 {
    base: BaseMapper,
    /// 6-bit PRG bank register (selects 32KB block in mode 0, or 16KB in mode 1)
    pub(crate) prg_bank: u8,
    /// PRG mode: false=32KB NROM-256, true=16KB UNROM-style
    pub(crate) prg_mode: bool,
}

impl Mapper63 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.select_prg_page(1, 1);
        Self {
            base,
            prg_bank: 0,
            prg_mode: false,
        }
    }

    fn update_banks(&mut self) {
        if self.prg_mode {
            self.base.select_prg_page(0, self.prg_bank as i16);
            self.base.select_prg_page(1, -1);
        } else {
            self.base.apply_nrom_prg_banking(self.prg_bank * 2, false);
        }
    }
}

impl Mapper for Mapper63 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            self.prg_bank = (value >> 2) & 0x3F;
            self.prg_mode = (value & 0x01) != 0;
            self.base.set_mirroring_hv((value & 0x02) != 0);
            self.update_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let flags = (self.prg_mode as u8)
            | ((matches!(self.base.mirroring(), NametableLayout::Horizontal) as u8) << 1);
        vec![self.prg_bank, flags]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank = data[0];
            self.prg_mode = (data[1] & 0x01) != 0;
            self.base.set_mirroring_hv((data[1] & 0x02) != 0);
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.prg_mode = false;
        self.base.set_mirroring(NametableLayout::Vertical);
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// 48 banks × 16KB — non-power-of-two avoids modulo false-passes
    const PRG_BANKS: usize = 48;

    fn make_mapper() -> Mapper63 {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        // CHR-RAM: 8KB all zeros
        let chr = vec![0u8; 8 * 1024];
        Mapper63::new(MapperContext::new_for_test(
            63,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_63_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            63,
            banked_data(16 * 1024, PRG_BANKS),
            vec![0u8; 8 * 1024],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 63 must be registered");
    }

    #[test]
    fn power_on_state_reads_bank_0_and_1() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should read bank 0");
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 should read bank 1");
    }

    #[test]
    fn mode0_32kb_bank_select_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 → 16KB page 0");
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 → 16KB page 1");
    }

    #[test]
    fn mode0_32kb_bank_select_bank_1() {
        let mut mapper = make_mapper();
        // value = 0x04: bits[7:2] = 1 → bank 1, mode=0
        mapper.write_prg(0x8000, 0x04);
        assert_eq!(mapper.prg_bank, 1, "prg_bank should be 1");
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 → 16KB page 2");
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 → 16KB page 3");
    }

    #[test]
    fn mode0_32kb_bank_select_bank_5() {
        let mut mapper = make_mapper();
        // value = 0x14: bits[7:2] = 5 → 32KB bank 5 → pages 10 and 11
        mapper.write_prg(0x8000, 0x14);
        assert_eq!(mapper.prg_bank, 5);
        assert_eq!(mapper.read_prg(0x8000), 10, "$8000 → page 10");
        assert_eq!(mapper.read_prg(0xC000), 11, "$C000 → page 11");
    }

    #[test]
    fn mode1_unrom_bank_select_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // bank=0, mode=1
        assert!(mapper.prg_mode, "mode should be UNROM");
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 → page 0");
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000 → last 16KB page"
        );
    }

    #[test]
    fn mode1_unrom_bank_select_bank_3() {
        let mut mapper = make_mapper();
        // value = 0x0D: bits[7:2] = 3 (0x0C) | mode=1 (0x01) = 0x0D, mirror=0
        mapper.write_prg(0x8000, 0x0D);
        assert_eq!(mapper.prg_bank, 3, "prg_bank should be 3");
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 → page 3");
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000 → last 16KB page"
        );
    }

    #[test]
    fn mirroring_vertical_when_bit1_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00); // bit 1 = 0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_horizontal_when_bit1_set() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x02); // bit 1 = 1 → Horizontal, bit 0 = 0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mode1_with_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x03); // bank=0, mode=1, mirror=Horizontal
        assert!(mapper.prg_mode, "mode should be UNROM");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), (PRG_BANKS - 1) as u8);
    }

    #[test]
    fn snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x0F); // some arbitrary state
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(r.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(r.get_mirroring(), mapper.get_mirroring());
    }

    #[test]
    fn reset_restores_bank0_mode0_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF); // some arbitrary state
        mapper.reset();
        assert_eq!(mapper.prg_bank, 0, "prg_bank should be 0 after reset");
        assert!(!mapper.prg_mode, "mode should be 0 after reset");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn write_to_different_addresses_in_range_all_update_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xFFFF, 0x04); // bank=1, mode=0
        assert_eq!(mapper.prg_bank, 1);
        assert_eq!(mapper.read_prg(0x8000), 2);
    }
}
