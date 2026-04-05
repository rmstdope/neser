//! Mapper 214 – Super Gun 20-in-1 pirate multicart
//!
//! Specifications:
//! - Primary source: NesDev wiki (403 restricted); archive mirror confirmed:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_214.xhtml>
//!   Description: "iNES Mapper 214 is used for the Super Gun 20-in-1 pirate multicart."
//! - Fallback source: Mesen2 `Mapper214.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Unlicensed/Mapper214.h>
//!
//! ## Hardware behavior
//!
//! A write to any address in `$8000–$FFFF` latches the address bus into the
//! bank register (the data byte is discarded):
//!
//! | Address bits | Function                    |
//! |--------------|-----------------------------|
//! | A[1:0]       | Select 8 KB CHR-ROM bank    |
//! | A[3:2]       | Select 16 KB PRG-ROM bank   |
//!
//! - PRG: two 16 KB slots (`$8000–$BFFF` and `$C000–$FFFF`) both wired to the
//!   **same** bank (NROM-128 style mirroring).
//! - CHR: single 8 KB bank mapped to `$0000–$1FFF`.
//! - Mirroring: fixed from the iNES header; no dynamic mirroring control.
//! - No PRG-RAM, no IRQ, no expansion audio.
//! - Power-on/reset: CHR bank 0, PRG bank 0 (equivalent to writing to $8000).

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 214;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 214 – Super Gun 20-in-1 pirate multicart.
///
/// See the module-level documentation for hardware details.
pub struct Mapper214 {
    base: BaseMapper,
    prg_bank: u8,
    chr_bank: u8,
}

impl Mapper214 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // Both PRG 16KB slots point to the same bank (NROM-128 style).
        self.base.apply_nrom_prg_banking(self.prg_bank, true);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper214 {
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
        let _ = value; // data bus ignored; bank selection is from address lines
        if addr >= 0x8000 {
            self.chr_bank = (addr & 0x03) as u8;
            self.prg_bank = ((addr >> 2) & 0x03) as u8;
            self.update_banks();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank = data[0] & 0x03;
            self.chr_bank = data[1] & 0x03;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to expose modulo-wrapping bugs.
    const PRG_BANKS: usize = 3;
    const CHR_BANKS: usize = 3;

    fn make_mapper() -> Mapper214 {
        Mapper214::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_214_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 214 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_mirrors_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "$C000 must also map to PRG bank 0 at power-on (NROM-128 mirror)"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR $0000 must map to bank 0 at power-on"
        );
    }

    // ── PRG bank switching (via address bits A[3:2]) ──────────────────────────

    #[test]
    fn prg_bank_selected_by_address_bits_3_2() {
        let mut mapper = make_mapper();
        // A[3:2] = 0b10 = 2, A[1:0] = 0b00 = 0 → addr low byte = 0b1000 = 0x08
        mapper.write_prg(0x8008, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG bank must be selected by address bits A[3:2]"
        );
    }

    #[test]
    fn both_prg_slots_mirror_same_bank() {
        let mut mapper = make_mapper();
        // A[3:2] = 0b01 = 1 → addr 0x8004
        mapper.write_prg(0x8004, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$8000 slot must reflect PRG bank 1"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 slot must mirror same PRG bank 1"
        );
    }

    #[test]
    fn prg_bank_selection_uses_only_bits_3_2() {
        let mut mapper = make_mapper();
        // A[3:2] = 0b01 = 1, upper bits set → should still be bank 1
        mapper.write_prg(0xFF04, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank must use only address bits A[3:2]"
        );
    }

    #[test]
    fn data_byte_is_ignored_for_prg_bank() {
        let mut mapper = make_mapper();
        // A[3:2]=0b10=2, write different data bytes - bank should still be 2
        mapper.write_prg(0x8008, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 2);
        mapper.write_prg(0x8008, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 2, "Data byte must be ignored");
    }

    #[test]
    fn write_below_8000_does_not_change_prg_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Writes below $8000 must not affect PRG bank"
        );
    }

    #[test]
    fn prg_bank_covers_full_16kb_window_8000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 0); // PRG bank 2
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG start of $8000 window");
        assert_eq!(mapper.read_prg(0xBFFF), 2, "PRG end of $8000 window");
    }

    #[test]
    fn prg_bank_covers_full_16kb_window_c000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 0); // PRG bank 2
        assert_eq!(mapper.read_prg(0xC000), 2, "PRG start of $C000 window");
        assert_eq!(mapper.read_prg(0xFFFF), 2, "PRG end of $C000 window");
    }

    // ── CHR bank switching (via address bits A[1:0]) ──────────────────────────

    #[test]
    fn chr_bank_selected_by_address_bits_1_0() {
        let mut mapper = make_mapper();
        // A[1:0] = 0b10 = 2 → addr 0x8002
        mapper.write_prg(0x8002, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "CHR bank must be selected by address bits A[1:0]"
        );
    }

    #[test]
    fn chr_bank_selection_uses_only_bits_1_0() {
        let mut mapper = make_mapper();
        // A[1:0] = 0b01 = 1, upper bits noise
        mapper.write_prg(0xFF01, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "CHR bank must use only address bits A[1:0]"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8002, 0); // CHR bank 2
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR start of window");
        assert_eq!(mapper.read_chr(0x1FFF), 2, "CHR end of window");
    }

    #[test]
    fn prg_and_chr_banks_decode_independently() {
        let mut mapper = make_mapper();
        // A[3:2]=0b01=1 (PRG bank 1), A[1:0]=0b10=2 (CHR bank 2) → addr bits = 0b0110 = 0x06
        mapper.write_prg(0x8006, 0);
        assert_eq!(mapper.read_prg(0x8000), 1, "PRG bank must be 1");
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR bank must be 2");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mapper = make_mapper(); // created with Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed from header"
        );
    }

    #[test]
    fn mirroring_not_changed_by_register_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xFFFF, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must not change after register write"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 214 must never assert IRQ");
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "Must have CHR banking");
        assert!(!caps.has_irq, "Must not have IRQ");
        assert!(!caps.has_expansion_audio, "Must not have expansion audio");
        assert!(
            !caps.has_dynamic_mirroring,
            "Must not have dynamic mirroring"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8006, 0); // PRG bank 1, CHR bank 2
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank must be 0 after reset");
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "PRG $C000 bank must be 0 after reset"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 after reset");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // PRG bank 1, CHR bank 2: addr bits = 0b0110 = 0x06
        mapper.write_prg(0x8006, 0);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.prg_bank, mapper.prg_bank,
            "Snapshot must preserve PRG bank"
        );
        assert_eq!(
            restored.chr_bank, mapper.chr_bank,
            "Snapshot must preserve CHR bank"
        );
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored mapper must read same PRG data"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored mapper must read same CHR data"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper214::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ));
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM is present"
        );
    }
}
