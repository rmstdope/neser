//! Mapper 212 – BMC Super HiK 300-in-1 pirate multicart
//!
//! Specifications:
//! - Primary source: NesDev wiki (403 restricted); archive mirror confirmed:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_212.xhtml>
//!   Also called "BMC Super HiK 300-in-1" (Nestopia name).
//! - Fallback: Mesen2 `Mapper212.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Unlicensed/Mapper212.h>
//!
//! ## Hardware behavior
//!
//! A write to any address in `$8000–$FFFF` latches address bits into the bank
//! register (the data byte is discarded):
//!
//! | Address bits | Function                                       |
//! |--------------|------------------------------------------------|
//! | A[2:0]       | Combined PRG and CHR bank number (BBb)         |
//! | A3           | Nametable mirroring (0: Vertical, 1: Horizontal)|
//! | A14          | Banking style (0: 16 KB NROM-128, 1: 32 KB)   |
//!
//! **PRG banking:**
//! - A14=0: `BBb` selects a 16 KB PRG bank mapped identically to both
//!   `$8000–$BFFF` and `$C000–$FFFF` (NROM-128 style).
//! - A14=1: `BB` (addr bits [2:1]) selects a 32 KB PRG bank at `$8000–$FFFF`.
//!
//! **CHR banking:** always `BBb` (addr bits [2:0]) as an 8 KB bank at
//! `$0000–$1FFF`, regardless of banking style.
//!
//! **Read `$6000–$7FFF`:** When `(addr & 0xE010) == 0x6000`, the read returns
//! the open-bus value OR-ed with `0x80` (bit 7 set). All other bits are open bus.
//!
//! No PRG-RAM, no IRQ, no expansion audio.
//! Power-on/reset state: equivalent to writing to `$8000` (all banks 0,
//! vertical mirroring, 16 KB mode).

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 212;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 212 – BMC Super HiK 300-in-1 pirate multicart.
///
/// See the module-level documentation for hardware details.
pub struct Mapper212 {
    base: BaseMapper,
    /// Latched address bits [2:0]: combined PRG/CHR bank (BBb).
    bank: u8,
    /// Latched address bit 3: mirroring (false=Vertical, true=Horizontal).
    mirroring_h: bool,
    /// Latched address bit 14: banking style (false=16KB NROM-128, true=32KB).
    banking_32k: bool,
}

impl Mapper212 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
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
            banking_32k: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // PRG banking: NROM-128 (16KB mirrored) or NROM-256 (32KB).
        self.base
            .apply_nrom_prg_banking(self.bank, !self.banking_32k);
        // CHR: always 8KB from BBb.
        self.base.select_chr_page(0, self.bank as i16);
        // Mirroring.
        self.base.set_mirroring_hv(self.mirroring_h);
    }
}

impl Mapper for Mapper212 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        // $6000-$7FFF with A[14,13,4]=011,0 → return open_bus | 0x80.
        if (addr & 0xE010) == 0x6000 {
            return open_bus | 0x80;
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < 0x8000 {
            return;
        }
        // Data byte is ignored; bank selection comes from address lines.
        let _ = value;
        self.bank = (addr & 0x07) as u8;
        self.mirroring_h = (addr & 0x0008) != 0;
        self.banking_32k = (addr & 0x4000) != 0;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Pack the three fields into a single byte:
        // bits [2:0] = bank, bit 3 = mirroring_h, bit 4 = banking_32k.
        let byte = self.bank
            | if self.mirroring_h { 0x08 } else { 0 }
            | if self.banking_32k { 0x10 } else { 0 };
        vec![byte]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&byte) = data.first() {
            self.bank = byte & 0x07;
            self.mirroring_h = (byte & 0x08) != 0;
            self.banking_32k = (byte & 0x10) != 0;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.bank = 0;
        self.mirroring_h = false;
        self.banking_32k = false;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts prevent modulo-wrapping false positives.
    const PRG_BANKS: usize = 5;
    const CHR_BANKS: usize = 5;

    fn make_mapper() -> Mapper212 {
        Mapper212::new(
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
    fn mapper_212_is_registered() {
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
            "Mapper 212 must be registered in the factory"
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
    fn power_on_prg_c000_mirrors_bank_0_in_16kb_mode() {
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

    // ── 16KB PRG banking (A14=0, addr $8000-$BFFF) ───────────────────────────

    #[test]
    fn write_8000_selects_16kb_prg_bank_nrom128() {
        let mut mapper = make_mapper();
        // A14=0, A[2:0]=001 → bank 1
        mapper.write_prg(0x8001, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 must reflect PRG bank 1");
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must mirror PRG bank 1 in 16KB mode"
        );
    }

    #[test]
    fn write_8000_16kb_mode_both_slots_same_bank() {
        let mut mapper = make_mapper();
        // A14=0, A[2:0]=010 → bank 2
        mapper.write_prg(0x8002, 0);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 window must be bank 2");
        assert_eq!(
            mapper.read_prg(0xBFFF),
            2,
            "$BFFF window end must be bank 2"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "$C000 window must be bank 2 (mirror)"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            2,
            "$FFFF window end must be bank 2 (mirror)"
        );
    }

    #[test]
    fn write_bfff_selects_16kb_prg_bank() {
        let mut mapper = make_mapper();
        // A14=0 (addr $BFFF), A[2:0]=011 → bank 3
        mapper.write_prg(0xBFFF & !0x07 | 0x003, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Write to $BFFF area must select 16KB bank 3"
        );
    }

    // ── 32KB PRG banking (A14=1, addr $C000-$FFFF) ───────────────────────────

    #[test]
    fn write_c000_selects_32kb_prg_bank() {
        let mut mapper = make_mapper();
        // A14=1, A[2:1]=00 (even pair = bank 0/1)
        mapper.write_prg(0xC000, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must be bank 0 of 32KB pair"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must be bank 1 of 32KB pair"
        );
    }

    #[test]
    fn write_c002_selects_32kb_prg_bank_2_3() {
        let mut mapper = make_mapper();
        // A14=1, A[2:0]=010 → 32KB pair: banks 2 and 3
        mapper.write_prg(0xC002, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 must be bank 2 of 32KB pair"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must be bank 3 of 32KB pair"
        );
    }

    #[test]
    fn write_c003_selects_32kb_prg_bank_even_pair() {
        let mut mapper = make_mapper();
        // A14=1, A[2:0]=011 → even pair is (A[2:1]=01 → 0b10 = 2), so banks 2/3
        mapper.write_prg(0xC003, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 must be even bank of A[2:1]=01 pair (bank 2)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must be odd bank of A[2:1]=01 pair (bank 3)"
        );
    }

    #[test]
    fn write_ffff_selects_32kb_prg_bank() {
        let mut mapper = make_mapper();
        // A14=1 (addr $FFFF), A[2:0]=111 → even pair = A[2:1]=11 = 6, banks 6/7 (wraps mod 5)
        mapper.write_prg(0xFFFF, 0);
        // bank 6 mod 5 = 1, bank 7 mod 5 = 2 (modulo wrap in 5-bank PRG)
        let bank_6_mod_5 = 6 % PRG_BANKS;
        let bank_7_mod_5 = 7 % PRG_BANKS;
        assert_eq!(
            mapper.read_prg(0x8000),
            bank_6_mod_5 as u8,
            "$8000 must be bank 6 mod 5"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            bank_7_mod_5 as u8,
            "$C000 must be bank 7 mod 5"
        );
    }

    // ── CHR banking ────────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_selected_by_address_bits_2_0() {
        let mut mapper = make_mapper();
        // A[2:0]=001 → CHR bank 1
        mapper.write_prg(0x8001, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "CHR bank must be selected by address bits A[2:0]"
        );
    }

    #[test]
    fn chr_bank_independent_of_a14() {
        let mut mapper = make_mapper();
        // A14=1 (32KB mode), A[2:0]=011 → CHR bank 3
        mapper.write_prg(0xC003, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bank must use BBb regardless of A14"
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
    fn data_byte_is_ignored() {
        let mut mapper = make_mapper();
        // Same address, different data values → same bank
        mapper.write_prg(0x8003, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 3);
        mapper.write_prg(0x8003, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 3, "Data byte must be ignored");
    }

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
        mapper.write_prg(0x8000, 0); // A3=0
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "A3=0 must select Vertical mirroring"
        );
    }

    #[test]
    fn a3_1_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 0); // A3=1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "A3=1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_changes_independently_of_bank() {
        let mut mapper = make_mapper();
        // Set bank 3, horizontal
        mapper.write_prg(0x800B, 0); // A[2:0]=011, A3=1
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(mapper.read_prg(0x8000), 3);
        // Change to vertical, same bank
        mapper.write_prg(0x8003, 0); // A[2:0]=011, A3=0
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    // ── Read $6000: bit 7 set when (addr & 0xE010) == 0x6000 ─────────────────

    #[test]
    fn read_6000_returns_bit7_set() {
        let mapper = make_mapper();
        let result = mapper.read_prg_open_bus(0x6000, 0x00);
        assert_eq!(result & 0x80, 0x80, "Read $6000 must have bit 7 set");
    }

    #[test]
    fn read_6000_ors_open_bus_with_0x80() {
        let mapper = make_mapper();
        let result = mapper.read_prg_open_bus(0x6000, 0x55);
        assert_eq!(result, 0xD5, "Read $6000 must return open_bus | 0x80");
    }

    #[test]
    fn read_6010_bit4_set_does_not_set_bit7() {
        let mapper = make_mapper();
        // $6010 & $E010 = $6010 != $6000 → no bit 7 override
        let result = mapper.read_prg_open_bus(0x6010, 0x00);
        assert_eq!(
            result & 0x80,
            0x00,
            "Read $6010 (A4=1) must NOT have bit 7 set"
        );
    }

    #[test]
    fn read_6001_no_a4_sets_bit7() {
        let mapper = make_mapper();
        // $6001 & $E010 = $6000 → bit 7 override applies
        let result = mapper.read_prg_open_bus(0x6001, 0x00);
        assert_eq!(result & 0x80, 0x80, "Read $6001 (A4=0) must have bit 7 set");
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 212 must never assert IRQ");
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
        mapper.write_prg(0xC00F, 0); // bank 7 (wraps), 32KB mode, horizontal
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG $8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "PRG $C000 must be bank 0 after reset (NROM-128)"
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
        // 32KB mode, bank 3, horizontal mirroring
        mapper.write_prg(0xC00B, 0); // A14=1, A[2:0]=011, A3=1

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.bank, mapper.bank, "Snapshot must preserve bank");
        assert_eq!(
            restored.mirroring_h, mapper.mirroring_h,
            "Snapshot must preserve mirroring"
        );
        assert_eq!(
            restored.banking_32k, mapper.banking_32k,
            "Snapshot must preserve banking style"
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
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Restored mapper must have same mirroring"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let mut mapper = Mapper212::new(
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
