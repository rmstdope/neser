//! Mapper 283 – GS-2004
//!
//! Specifications:
//! - Primary source: NesDev wiki (HTTP 403 at time of implementation)
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Gs2004.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Gs2004.h>
//!
//! # Hardware overview
//!
//! Used by the "GS-2004" board, a simple unlicensed multicart.
//!
//! - PRG-ROM: 4 × 8 KiB windows at `$8000–$FFFF` (32 KiB total), switched in groups.
//!   Any write to `$8000–$FFFF` selects a 32 KiB aligned block: the base bank is
//!   `(value & 0x07) << 2`.  All four 8 KiB pages are selected consecutively starting
//!   from that base bank.
//! - PRG-ROM at `$6000–$7FFF`: fixed to PRG-ROM bank 32 (absolute index 0x20).
//! - CHR-ROM: single fixed 8 KiB page at bank 0 (`$0000–$1FFF`).
//! - Mirroring: fixed by iNES header; no runtime control.
//! - No IRQ, no expansion audio, no PRG-RAM.
//!
//! # Register map
//!
//! | Address range  | Effect                                                  |
//! |----------------|---------------------------------------------------------|
//! | `$8000–$FFFF`  | PRG bank group = `(value & 0x07) << 2` (32 KiB block)  |
//!
//! # PRG banking
//!
//! - `$8000–$9FFF`: base_bank + 0
//! - `$A000–$BFFF`: base_bank + 1
//! - `$C000–$DFFF`: base_bank + 2
//! - `$E000–$FFFF`: base_bank + 3
//!
//! where `base_bank = (value & 0x07) << 2`.
//!
//! # Power-on / reset state
//!
//! - `$6000–$7FFF`: PRG-ROM bank 32 (fixed, never changes).
//! - `$8000–$FFFF`: PRG-ROM banks 28–31 (base_bank = `0x07 << 2 = 28`).
//! - CHR: bank 0.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 283;
const PRG_BANK_SIZE_BYTES: usize = 8 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;

/// Absolute PRG-ROM bank index mapped to `$6000–$7FFF` (always fixed).
const FIXED_6000_BANK: i16 = 0x20;

/// Mapper 283 – GS-2004 multicart.
///
/// Writes to `$8000–$FFFF` select a 32 KiB aligned group of four 8 KiB PRG banks
/// for the `$8000–$FFFF` window.  The `$6000–$7FFF` window is permanently fixed to
/// PRG-ROM bank 32.
pub struct Mapper283 {
    base: BaseMapper,
    /// Bits 2:0 of the last written value, selecting the 32 KiB PRG block.
    prg_group: u8,
}

impl Mapper283 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: false,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_prg_6000_banking();
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self {
            base,
            prg_group: 0x07,
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        let base_bank = (self.prg_group & 0x07) as i16 * 4;
        self.base.select_prg_6000_page(FIXED_6000_BANK);
        for slot in 0..4i16 {
            self.base.select_prg_page(slot as usize, base_bank + slot);
        }
        self.base.select_chr_page(0, 0);
    }
}

impl Mapper for Mapper283 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            return;
        }
        self.prg_group = value & 0x07;
        let base_bank = self.prg_group as i16 * 4;
        for slot in 0..4i16 {
            self.base.select_prg_page(slot as usize, base_bank + slot);
        }
    }

    fn reset(&mut self) {
        self.prg_group = 0x07;
        self.apply_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_group]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        self.prg_group = data[0] & 0x07;
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 35 banks (non-power-of-two) so that bank 32 is a real distinct bank.
    const PRG_BANKS: usize = 35;
    const CHR_BANKS: usize = 7;

    fn make_mapper() -> Mapper283 {
        Mapper283::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS),
                banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_283_is_registered_in_factory() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS),
                banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 283 must be registered in the factory"
        );
    }

    // ── Power-on state ───────────────────────────────────────────────────────

    #[test]
    fn power_on_8000_ffff_maps_to_banks_28_31() {
        let mapper = make_mapper();
        // base_bank = (0x07 & 0x07) << 2 = 28
        assert_eq!(mapper.read_prg(0x8000), 28, "$8000 should map to bank 28");
        assert_eq!(mapper.read_prg(0xA000), 29, "$A000 should map to bank 29");
        assert_eq!(mapper.read_prg(0xC000), 30, "$C000 should map to bank 30");
        assert_eq!(mapper.read_prg(0xE000), 31, "$E000 should map to bank 31");
    }

    #[test]
    fn power_on_6000_7fff_maps_to_bank_32() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            32,
            "$6000 should map to bank 32 (fixed)"
        );
        assert_eq!(
            mapper.read_prg(0x7FFF),
            32,
            "$7FFF should map to bank 32 (fixed)"
        );
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR $0000 should be bank 0");
        assert_eq!(mapper.read_chr(0x1FFF), 0, "CHR $1FFF should be bank 0");
    }

    #[test]
    fn power_on_mirroring_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring should be inherited from the iNES header"
        );
    }

    // ── PRG bank-switching via $8000–$FFFF writes ────────────────────────────

    #[test]
    fn write_selects_32kb_block_via_bits_2_0() {
        let mut mapper = make_mapper();
        // value = 0 → base_bank = 0 → pages 0,1,2,3
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xA000), 1);
        assert_eq!(mapper.read_prg(0xC000), 2);
        assert_eq!(mapper.read_prg(0xE000), 3);
    }

    #[test]
    fn write_group_1_maps_banks_4_to_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x01);
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xA000), 5);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);
    }

    #[test]
    fn write_group_3_maps_banks_12_to_15() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 12);
        assert_eq!(mapper.read_prg(0xA000), 13);
        assert_eq!(mapper.read_prg(0xC000), 14);
        assert_eq!(mapper.read_prg(0xE000), 15);
    }

    #[test]
    fn write_masks_upper_bits_of_value() {
        let mut mapper = make_mapper();
        // Only bits 2:0 matter; 0xF8 & 0x07 = 0 → group 0 → banks 0-3
        mapper.write_prg(0x8000, 0xF8);
        assert_eq!(mapper.read_prg(0x8000), 0, "upper bits must be masked");
        // 0xFF & 0x07 = 7 → group 7 → banks 28-31
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 28, "0xFF & 0x07 = 7 → bank 28");
    }

    #[test]
    fn write_to_any_address_in_8000_ffff_is_valid() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x02);
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "write to $A000 selects bank group"
        );
        mapper.write_prg(0xFFFF, 0x01);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "write to $FFFF selects bank group"
        );
    }

    // ── $6000-$7FFF is always fixed (unaffected by PRG writes) ──────────────

    #[test]
    fn bank_at_6000_remains_fixed_after_prg_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00); // change $8000 group
        assert_eq!(
            mapper.read_prg(0x6000),
            32 % PRG_BANKS as u8,
            "$6000 must remain fixed to bank 32 regardless of PRG writes"
        );
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(
            mapper.read_prg(0x6000),
            32 % PRG_BANKS as u8,
            "$6000 must still be bank 32 after group-change"
        );
    }

    // ── CHR is always bank 0 ─────────────────────────────────────────────────

    #[test]
    fn chr_stays_at_bank_0_after_prg_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must stay at bank 0");
        assert_eq!(mapper.read_chr(0x1FFF), 0, "CHR must stay at bank 0");
    }

    // ── Reset behavior ───────────────────────────────────────────────────────

    #[test]
    fn reset_restores_banks_28_31_at_8000_ffff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00); // change to group 0
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            28,
            "reset should restore bank 28 at $8000"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            29,
            "reset should restore bank 29 at $A000"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            30,
            "reset should restore bank 30 at $C000"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "reset should restore bank 31 at $E000"
        );
    }

    #[test]
    fn reset_preserves_fixed_6000_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x02);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x6000),
            32 % PRG_BANKS as u8,
            "reset must preserve $6000 fixed at bank 32"
        );
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_prg_group() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05); // group 5 → banks 20-23
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 20, "restored $8000 = bank 20");
        assert_eq!(restored.read_prg(0xA000), 21, "restored $A000 = bank 21");
        assert_eq!(restored.read_prg(0xC000), 22, "restored $C000 = bank 22");
        assert_eq!(restored.read_prg(0xE000), 23, "restored $E000 = bank 23");
    }

    #[test]
    fn restore_with_empty_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x03); // group 3
        mapper.restore_registers(&[]);
        assert_eq!(
            mapper.read_prg(0x8000),
            12,
            "state unchanged after empty restore data"
        );
    }

    #[test]
    fn restore_masks_upper_bits_of_snapshot_byte() {
        let mut mapper = make_mapper();
        mapper.restore_registers(&[0xFF]);
        // 0xFF & 0x07 = 7 → group 7 → bank 28
        assert_eq!(mapper.read_prg(0x8000), 28, "restore must mask upper bits");
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(!caps.has_dynamic_mirroring, "no dynamic mirroring");
        assert!(!caps.has_chr_banking, "CHR is fixed, no CHR banking");
        assert_eq!(caps.prg_bank_size_kb, 8, "8 KB PRG banks");
        assert_eq!(caps.chr_bank_size_kb, 8, "8 KB CHR bank");
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 283 must never assert IRQ");
    }
}
