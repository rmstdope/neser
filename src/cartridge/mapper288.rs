//! Mapper 288 – Gkcx1
//!
//! Specifications:
//! - Primary reference: NesDev wiki (unavailable due to network restriction, Cloudflare 403).
//! - Fallback: Mesen2 `Gkcx1.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Gkcx1.h>
//!
//! # Hardware overview
//!
//! Used by unlicensed cartridges.
//!
//! - PRG-ROM: 32 KiB window at $8000–$FFFF. Bank selected by bits [4:3] of the
//!   write *address* (not the written value).
//! - CHR-ROM: 8 KiB window at $0000–$1FFF. Bank selected by bits [2:0] of the
//!   write *address*.
//! - Mirroring: fixed per iNES header (no runtime control).
//! - Control: any write to $8000–$FFFF uses the address to select banks.
//! - IRQ: none
//! - PRG-RAM: none
//! - Bus conflicts: none
//!
//! # Banking (write to $8000–$FFFF)
//!
//! | Address bits | Effect                                    |
//! |-------------|-------------------------------------------|
//! | [4:3]       | 32 KiB PRG-ROM bank index (`(addr>>3)&3`) |
//! | [2:0]       | 8 KiB CHR-ROM bank index (`addr & 7`)     |
//!
//! # Power-on state
//!
//! PRG bank 0 at $8000–$FFFF; CHR bank 0 at $0000–$1FFF.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 288;
const PRG_BANK_SIZE_BYTES: usize = 32 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;

/// Mapper 288 – Gkcx1
///
/// Banking is encoded in the write address, not the written value:
/// - PRG bank = `(addr >> 3) & 0x03`
/// - CHR bank = `addr & 0x07`
pub struct Mapper288 {
    base: BaseMapper,
    prg_bank: u8,
    chr_bank: u8,
}

impl Mapper288 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
        };
        mapper.apply_state(0, 0);
        mapper
    }

    fn apply_state(&mut self, prg_bank: u8, chr_bank: u8) {
        self.prg_bank = prg_bank;
        self.chr_bank = chr_bank;
        self.base.select_prg_page(0, prg_bank as i16);
        self.base.select_chr_page(0, chr_bank as i16);
    }
}

impl Mapper for Mapper288 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, _value: u8) {
        if addr >= 0x8000 {
            let prg = ((addr >> 3) & 0x03) as u8;
            let chr = (addr & 0x07) as u8;
            self.apply_state(prg, chr);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }
        self.apply_state(data[0], data[1]);
    }

    fn reset(&mut self) {
        self.apply_state(0, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Use non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS_32K: usize = 5; // 5 × 32 KiB
    const CHR_BANKS_8K: usize = 9; // 9 × 8 KiB

    fn make_mapper() -> Mapper288 {
        Mapper288::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_288_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_32K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 288 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_prg_bank_is_0() {
        let mapper = make_mapper();
        // banked_data fills bank N with byte N, so bank 0 == 0x00
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should map to PRG bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "$FFFF should also be in PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank should be 0 at power-on"
        );
    }

    // ── PRG banking: bits [4:3] of the write address ─────────────────────────

    #[test]
    fn prg_bank_selected_by_address_bits_4_3() {
        let mut mapper = make_mapper();
        // addr bits[4:3] = 0b01 => bank 1: addr = 0x8008 (bit 3 set)
        mapper.write_prg(0x8008, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG should be bank 1 when addr bit3=1"
        );

        // addr bits[4:3] = 0b10 => bank 2: addr = 0x8010 (bit 4 set)
        mapper.write_prg(0x8010, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG should be bank 2 when addr bit4=1"
        );

        // addr bits[4:3] = 0b11 => bank 3: addr = 0x8018 (bits 3+4 set)
        mapper.write_prg(0x8018, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG should be bank 3 when addr bits[4:3]=11"
        );
    }

    #[test]
    fn prg_bank_0_when_addr_bits_4_3_are_clear() {
        let mut mapper = make_mapper();
        // Set a non-zero bank first
        mapper.write_prg(0x8008, 0x00); // bank 1
        // Now write with bits[4:3]=00 -> bank 0
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should be bank 0 when addr bits[4:3]=00"
        );
    }

    #[test]
    fn prg_bank_ignores_written_value() {
        let mut mapper = make_mapper();
        // Write different values at same address: bank should not change based on value
        mapper.write_prg(0x8008, 0xFF); // addr bits[4:3]=01 → bank 1, value ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank 1 regardless of written value 0xFF"
        );

        mapper.write_prg(0x8008, 0x00); // same addr, different value → still bank 1
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank 1 regardless of written value 0x00"
        );
    }

    #[test]
    fn prg_32kb_window_covers_8000_to_ffff() {
        let mut mapper = make_mapper();
        // bank 2 => addr bits[4:3]=10 => 0x8010
        mapper.write_prg(0x8010, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 should read bank 2");
        assert_eq!(mapper.read_prg(0xFFFF), 2, "$FFFF should read same bank 2");
    }

    // ── CHR banking: bits [2:0] of the write address ─────────────────────────

    #[test]
    fn chr_bank_selected_by_address_bits_2_0() {
        let mut mapper = make_mapper();
        // addr bits[2:0] = 0b101 => chr bank 5: addr = 0x8005
        mapper.write_prg(0x8005, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR should be bank 5");
    }

    #[test]
    fn chr_bank_0_when_addr_bits_2_0_are_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8007, 0x00); // chr bank 7
        mapper.write_prg(0x8000, 0x00); // chr bank 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 when addr bits[2:0]=000"
        );
    }

    #[test]
    fn chr_bank_independent_of_prg_bank() {
        let mut mapper = make_mapper();
        // addr bits[4:3]=01, bits[2:0]=101 => prg bank 1, chr bank 5
        // addr = 0x8000 | (1<<3) | 5 = 0x800D
        mapper.write_prg(0x800D, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 1, "PRG should be bank 1");
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR should be bank 5");
    }

    #[test]
    fn chr_bank_all_combinations() {
        let mut mapper = make_mapper();
        for chr in 0u8..8 {
            let addr = 0x8000u16 | chr as u16;
            mapper.write_prg(addr, 0x00);
            assert_eq!(
                mapper.read_chr(0x0000),
                chr,
                "CHR bank should be {chr} for addr {addr:#06X}"
            );
        }
    }

    // ── Write register at any $8000–$FFFF ───────────────────────────────────

    #[test]
    fn write_at_ffff_uses_address_for_banking() {
        let mut mapper = make_mapper();
        // $FFFF: bits[4:3] = (0xFFFF >> 3) & 3 = (0x1FFF) & 3 = 3
        // bits[2:0] = 0xFFFF & 7 = 7
        mapper.write_prg(0xFFFF, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG should be bank 3 at $FFFF write"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            7,
            "CHR should be bank 7 at $FFFF write"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_prg_and_chr_banks() {
        let mut mapper = make_mapper();
        // addr 0x800D: prg=1, chr=5
        mapper.write_prg(0x800D, 0x00);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            1,
            "restored PRG bank should be 1"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            5,
            "restored CHR bank should be 5"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 0x00); // prg bank 1
        mapper.restore_registers(&[1]); // too short — must be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "state must be unchanged after short restore"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x801F, 0x00); // prg=3, chr=7
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG should be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 after reset"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(!caps.has_dynamic_mirroring, "no dynamic mirroring");
        assert!(caps.has_chr_banking, "CHR banking");
        assert_eq!(caps.prg_bank_size_kb, 32);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 288 must never assert IRQ");
    }
}
