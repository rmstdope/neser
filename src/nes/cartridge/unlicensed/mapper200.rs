//! Mapper 200 – 1993 Super 50-in-1 multicart (MG109 board)
//!
//! Specifications:
//! - Primary source: NesDev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_200.xhtml>
//! - Fallback: Mesen2 `Mapper200.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Unlicensed/Mapper200.h>
//!
//! ## Hardware behavior
//!
//! A write to any address in `$8000–$FFFF` latches address bits into the bank
//! register (the data byte is ignored, address lines only):
//!
//! ```text
//! A~[1... .... .... bBBB]
//!                   |+++- PRG A16..A14, CHR A15..A13 (bank select, bits [2:0])
//!                   +---- Nametable mirroring:
//!                          0: Vertical
//!                          1: Horizontal
//! ```
//!
//! **PRG banking (16 KB pages):**
//! - Both `$8000–$BFFF` and `$C000–$FFFF` map to the same 16 KB bank (NROM-128 style).
//!
//! **CHR banking:** always 8 KB bank `BBB` at `$0000–$1FFF`.
//!
//! **Mirroring:** A3=0 → Vertical, A3=1 → Horizontal.
//!
//! No PRG-RAM, no IRQ, no expansion audio.
//! Power-on/reset state: bank 0, vertical mirroring.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 200;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 200 – 1993 Super 50-in-1 multicart.
///
/// See the module-level documentation for hardware details.
pub struct Mapper200 {
    base: BaseMapper,
    /// Bank register = (addr & 0x07): bits [2:0] of write address.
    bank: u8,
    /// Mirroring: false = Vertical, true = Horizontal (A3 of write address).
    mirroring_h: bool,
}

impl Mapper200 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
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
            bank: 0,
            mirroring_h: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // PRG: both 16 KB slots map to the same bank (NROM-128 mirrored).
        self.base.select_prg_page(0, self.bank as i16);
        self.base.select_prg_page(1, self.bank as i16);
        // CHR: 8 KB from the same bank index.
        self.base.select_chr_page(0, self.bank as i16);
        // Mirroring.
        self.base.set_mirroring_hv(self.mirroring_h);
    }
}

impl Mapper for Mapper200 {
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
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr < 0x8000 {
            return;
        }
        // Data byte is ignored; banking is determined by address lines.
        let _ = value;
        self.bank = (addr & 0x0007) as u8;
        // A3=0 → Vertical, A3=1 → Horizontal (per NesDev spec).
        self.mirroring_h = (addr & 0x0008) != 0;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // byte0: bits [2:0] = bank, bit 3 = mirroring_h.
        vec![self.bank | if self.mirroring_h { 0x08 } else { 0 }]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&byte) = data.first() {
            self.bank = byte & 0x07;
            self.mirroring_h = (byte & 0x08) != 0;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.bank = 0;
        self.mirroring_h = false;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts prevent modulo-wrapping false positives.
    const PRG_BANKS: usize = 5;
    const CHR_BANKS: usize = 5;

    fn make_mapper() -> Mapper200 {
        Mapper200::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_200_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(CHR_BANK_SIZE, CHR_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 200 must be registered in the factory"
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

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring must be Vertical (A3=0)"
        );
    }

    // ── PRG banking ───────────────────────────────────────────────────────────

    #[test]
    fn write_8001_selects_bank_1_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8001: bits[2:0]=1, A3=0 → bank 1, Vertical
        mapper.write_prg(0x8001, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 must reflect PRG bank 1");
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must mirror PRG bank 1 (NROM-128)"
        );
    }

    #[test]
    fn write_8002_selects_bank_2_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8002: bits[2:0]=2, A3=0 → bank 2, Vertical
        mapper.write_prg(0x8002, 0);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 window must be bank 2");
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "$C000 window must be bank 2 (mirror)"
        );
    }

    #[test]
    fn write_8007_selects_bank_7_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8007: bits[2:0]=7 → bank 7 (wraps to 7 mod 5 = 2 for 5-bank ROM)
        mapper.write_prg(0x8007, 0);
        // bank = 7, mod 5 = 2
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 must be bank 7 mod 5 = 2");
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "$C000 must mirror bank 7 mod 5 = 2"
        );
    }

    #[test]
    fn prg_8000_and_c000_always_same_bank() {
        let mut mapper = make_mapper();
        for bank in 0..=4u8 {
            mapper.write_prg(0x8000 | bank as u16, 0);
            assert_eq!(
                mapper.read_prg(0x8000),
                mapper.read_prg(0xC000),
                "$8000 and $C000 must always map to the same bank (NROM-128)"
            );
        }
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_matches_prg_bank() {
        let mut mapper = make_mapper();
        // addr=0x8003: bits[2:0]=3 → bank 3
        mapper.write_prg(0x8003, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bank must match the PRG bank index"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        // addr=0x8004: bits[2:0]=4 → bank 4
        mapper.write_prg(0x8004, 0);
        assert_eq!(mapper.read_chr(0x0000), 4, "CHR start of window");
        assert_eq!(mapper.read_chr(0x1FFF), 4, "CHR end of window");
    }

    // ── Data byte ignored ─────────────────────────────────────────────────────

    #[test]
    fn data_byte_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8003, 0x00); // bank = 3
        assert_eq!(mapper.read_prg(0x8000), 3);
        mapper.write_prg(0x8003, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 3, "Data byte must be ignored");
    }

    // ── Writes below $8000 ignored ────────────────────────────────────────────

    #[test]
    fn write_below_8000_does_not_change_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Writes below $8000 must not affect PRG bank"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Writes below $8000 must not affect CHR bank"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn a3_0_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        // addr=0x8002: A3=0 → Vertical
        mapper.write_prg(0x8002, 0);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "A3=0 must select Vertical mirroring"
        );
    }

    #[test]
    fn a3_1_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        // addr=0x8008: A3=1 → Horizontal (bits[2:0]=0, bank=0)
        mapper.write_prg(0x8008, 0);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "A3=1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_and_bank_change_together() {
        let mut mapper = make_mapper();
        // addr=0x800B: A3=1, bits[2:0]=3 → bank 3, Horizontal
        mapper.write_prg(0x800B, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(mapper.read_prg(0x8000), 3);
        // addr=0x8003: A3=0, bits[2:0]=3 → bank 3, Vertical
        mapper.write_prg(0x8003, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 200 must never assert IRQ");
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "Must have CHR banking");
        assert!(caps.has_dynamic_mirroring, "Must have dynamic mirroring");
        assert!(!caps.has_irq, "Must not have IRQ");
        assert!(!caps.has_expansion_audio, "Must not have expansion audio");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        // Set some non-zero state
        mapper.write_prg(0x800F, 0); // bank 7, horizontal
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG $8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "PRG $C000 must be bank 0 after reset"
        );
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR must be bank 0 after reset");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be Vertical after reset"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // addr=0x800B: A3=1 → Horizontal, bits[2:0]=3 → bank 3
        mapper.write_prg(0x800B, 0);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.bank, mapper.bank, "Snapshot must preserve bank");
        assert_eq!(
            restored.mirroring_h, mapper.mirroring_h,
            "Snapshot must preserve mirroring"
        );
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored mapper must read same PRG data at $8000"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored mapper must read same PRG data at $C000"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored mapper must read same CHR data"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Restored mapper must have same mirroring"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper200::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM is present"
        );
    }
}
