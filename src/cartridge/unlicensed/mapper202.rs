//! Mapper 202 – 150-in-1 pirate multicart
//!
//! Specifications:
//! - Primary source: NesDev wiki mirror:
//!   <https://nesdev-wiki.nes.science/wikipages/INES_Mapper_202.xhtml>
//! - Fallback: Mesen2 `Mapper202.h`
//!   <https://raw.githubusercontent.com/SourMesen/Mesen2/master/Core/NES/Mappers/Unlicensed/Mapper202.h>
//!
//! ## Hardware behavior
//!
//! A write to any address in `$8000–$FFFF` latches address bits into the bank
//! register (the data byte is ignored):
//!
//! | Address bits | Function                                         |
//! |--------------|--------------------------------------------------|
//! | A[3:1]       | Bank register R = (addr >> 1) & 0x07            |
//! | A0           | Nametable mirroring (0: Vertical, 1: Horizontal) |
//! | A0 AND A3    | PRG mode: both set → 32 KB mode; else 16 KB mode |
//!
//! **PRG banking (16 KB pages):**
//! - 16 KB mode: both `$8000–$BFFF` and `$C000–$FFFF` map to page R.
//! - 32 KB mode: `$8000–$BFFF` maps to page R, `$C000–$FFFF` maps to page R+1.
//!
//! **CHR banking:** always bank register R = (addr >> 1) & 0x07 as an
//! 8 KB bank at `$0000–$1FFF`.
//!
//! **Mirroring:** A0=0 → Vertical, A0=1 → Horizontal.
//!
//! No PRG-RAM, no IRQ, no expansion audio.
//! Power-on/reset state: all banks 0, vertical mirroring, 16 KB mode.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 202;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 202 – 150-in-1 pirate multicart.
///
/// See the module-level documentation for hardware details.
pub struct Mapper202 {
    base: BaseMapper,
    /// Bank register R = (addr >> 1) & 0x07.
    bank: u8,
    /// Mirroring: false = Vertical, true = Horizontal (A0 of write address).
    mirroring_h: bool,
    /// PRG mode: true = 32 KB (A0 AND A3 both set), false = 16 KB mirrored.
    prg_mode_32k: bool,
}

impl Mapper202 {
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
            prg_mode_32k: false,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // CHR: always 8 KB from bank R.
        self.base.select_chr_page(0, self.bank as i16);
        // PRG: 16 KB mode → both slots same bank; 32 KB mode → R and R+1.
        if self.prg_mode_32k {
            self.base.select_prg_page(0, self.bank as i16);
            self.base.select_prg_page(1, self.bank as i16 + 1);
        } else {
            self.base.select_prg_page(0, self.bank as i16);
            self.base.select_prg_page(1, self.bank as i16);
        }
        // Mirroring.
        self.base.set_mirroring_hv(self.mirroring_h);
    }
}

impl Mapper for Mapper202 {
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
        self.bank = ((addr >> 1) & 0x07) as u8;
        self.mirroring_h = (addr & 0x0001) != 0;
        // 32 KB mode when both A0 and A3 are set.
        self.prg_mode_32k = (addr & 0x0009) == 0x0009;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Pack into a single byte: bits [2:0] = bank, bit 3 = mirroring_h, bit 4 = prg_mode_32k.
        let byte = self.bank
            | if self.mirroring_h { 0x08 } else { 0 }
            | if self.prg_mode_32k { 0x10 } else { 0 };
        vec![byte]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&byte) = data.first() {
            self.bank = byte & 0x07;
            self.mirroring_h = (byte & 0x08) != 0;
            self.prg_mode_32k = (byte & 0x10) != 0;
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.bank = 0;
        self.mirroring_h = false;
        self.prg_mode_32k = false;
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

    fn make_mapper() -> Mapper202 {
        Mapper202::new(
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
    fn mapper_202_is_registered() {
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
            "Mapper 202 must be registered in the factory"
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
            "$C000 must also map to PRG bank 0 at power-on (16 KB mirror)"
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
            "Power-on mirroring must be Vertical (A0=0)"
        );
    }

    // ── 16 KB PRG banking (A0=0 OR A3=0, so not both set) ────────────────────

    #[test]
    fn write_8002_selects_16kb_bank_1_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8002: A[3:1]=001 → R=1, A0=0 → 16 KB mode
        mapper.write_prg(0x8002, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 must reflect PRG bank 1");
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must mirror PRG bank 1 in 16 KB mode"
        );
    }

    #[test]
    fn write_8004_selects_16kb_bank_2_mirrored() {
        let mut mapper = make_mapper();
        // addr=0x8004: A[3:1]=010 → R=2, A0=0 → 16 KB mode
        mapper.write_prg(0x8004, 0);
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 window must be bank 2");
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "$C000 window must be bank 2 (mirror)"
        );
    }

    #[test]
    fn write_8008_a3_only_is_still_16kb_mode() {
        let mut mapper = make_mapper();
        // addr=0x8008: A3=1, A0=0 → (addr & 0x0009) = 0x0008 ≠ 0x0009 → 16 KB mode
        mapper.write_prg(0x8008, 0);
        // R = (0x8008 >> 1) & 0x07 = 0x04 & 0x07 = 4
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 must be bank 4");
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 must mirror bank 4 (16 KB)"
        );
    }

    // ── 32 KB PRG banking (A0=1 AND A3=1) ────────────────────────────────────

    #[test]
    fn write_8009_selects_32kb_mode_bank_0_and_1() {
        let mut mapper = make_mapper();
        // addr=0x8009: A0=1, A3=1 → 32 KB mode; R = (0x8009 >> 1) & 0x07 = 4
        mapper.write_prg(0x8009, 0);
        // R = (0x8009 >> 1) & 0x07 = (0x4004) & 0x07 = 4
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 must be bank 4 in 32 KB mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0, // bank 5 mod 5 = 0
            "$C000 must be bank 5 (4+1) mod 5 = 0 in 32 KB mode"
        );
    }

    #[test]
    fn write_8003_a0_only_is_still_16kb_mode() {
        let mut mapper = make_mapper();
        // addr=0x8003: A0=1, A3=0 → (addr & 0x0009) = 0x0001 ≠ 0x0009 → 16 KB mode
        mapper.write_prg(0x8003, 0);
        // R = (0x8003 >> 1) & 0x07 = 1
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 must be bank 1");
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "$C000 must mirror bank 1 (16 KB)"
        );
    }

    #[test]
    fn write_800b_selects_32kb_mode() {
        let mut mapper = make_mapper();
        // addr=0x800B: binary 0000_1011 → A0=1, A1=1, A3=1 → 32 KB mode
        // R = (0x800B >> 1) & 0x07 = 5 & 0x07 = 5 mod 5 = 0 → wraps
        mapper.write_prg(0x800B, 0);
        // R raw = 5, mod 5 = 0 for first slot
        // second slot = 6, mod 5 = 1
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must be bank 5 mod 5 = 0");
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 must be bank 6 mod 5 = 1");
    }

    // ── CHR banking ────────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_selected_by_addr_bits_3_1() {
        let mut mapper = make_mapper();
        // addr=0x8002: R = (0x8002 >> 1) & 0x07 = 1 → CHR bank 1
        mapper.write_prg(0x8002, 0);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "CHR bank must be R from address bits A[3:1]"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        // addr=0x8004: R=2 → CHR bank 2
        mapper.write_prg(0x8004, 0);
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR start of window");
        assert_eq!(mapper.read_chr(0x1FFF), 2, "CHR end of window");
    }

    #[test]
    fn chr_bank_independent_of_prg_mode() {
        let mut mapper = make_mapper();
        // 32 KB mode, R=2
        mapper.write_prg(0x8009, 0); // addr 0x8009: R=(0x8009>>1)&0x07=4
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "CHR bank must be R regardless of PRG mode"
        );
    }

    #[test]
    fn data_byte_is_ignored() {
        let mut mapper = make_mapper();
        // Same address, different data values → same bank result
        mapper.write_prg(0x8006, 0x00); // R=3
        assert_eq!(mapper.read_prg(0x8000), 3);
        mapper.write_prg(0x8006, 0xFF);
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
    fn a0_0_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0); // A0=0
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "A0=0 must select Vertical mirroring"
        );
    }

    #[test]
    fn a0_1_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0); // A0=1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "A0=1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_changes_independently_of_bank() {
        let mut mapper = make_mapper();
        // Set bank 3, horizontal
        mapper.write_prg(0x8007, 0); // A[3:1]=011 → R=3, A0=1 → H
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        assert_eq!(mapper.read_prg(0x8000), 3);
        // Change to vertical, same bank
        mapper.write_prg(0x8006, 0); // A[3:1]=011 → R=3, A0=0 → V
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 202 must never assert IRQ");
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
        mapper.write_prg(0x800F, 0); // bank 7, 32 KB mode, horizontal
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG $8000 must be bank 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "PRG $C000 must be bank 0 after reset (16 KB mirror)"
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
    fn registers_snapshot_round_trips_16kb_mode() {
        let mut mapper = make_mapper();
        // addr=0x8005: A[3:1]=010 → R=2, A0=1 → H mirror, A3=0 → 16 KB mode
        mapper.write_prg(0x8005, 0);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.bank, mapper.bank, "Snapshot must preserve bank");
        assert_eq!(
            restored.mirroring_h, mapper.mirroring_h,
            "Snapshot must preserve mirroring"
        );
        assert_eq!(
            restored.prg_mode_32k, mapper.prg_mode_32k,
            "Snapshot must preserve PRG mode (16 KB)"
        );
        assert!(!restored.prg_mode_32k, "PRG mode must be 16 KB");
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

    #[test]
    fn registers_snapshot_round_trips_32kb_mode() {
        let mut mapper = make_mapper();
        // addr=0x8009: A0=1, A3=1 → 32 KB mode; R = (0x8009 >> 1) & 0x07 = 4
        // $8000–$BFFF = bank 4; $C000–$FFFF = bank 5 mod 5 = 0
        mapper.write_prg(0x8009, 0);

        assert!(mapper.prg_mode_32k, "PRG mode must be 32 KB");
        assert_eq!(mapper.bank, 4, "Bank register must be 4");

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.bank, mapper.bank, "Snapshot must preserve bank");
        assert_eq!(
            restored.mirroring_h, mapper.mirroring_h,
            "Snapshot must preserve mirroring"
        );
        assert_eq!(
            restored.prg_mode_32k, mapper.prg_mode_32k,
            "Snapshot must preserve PRG mode (32 KB)"
        );
        assert!(restored.prg_mode_32k, "Restored PRG mode must be 32 KB");
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored $8000 must map to bank R"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored $C000 must map to bank R+1"
        );
        // Verify the $C000 bank is R+1 = 5, which wraps to 0 (mod PRG_BANKS=5), distinct from $8000 = bank 4
        assert_ne!(
            restored.read_prg(0x8000),
            restored.read_prg(0xC000),
            "$8000 and $C000 must map to different banks in 32 KB mode"
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
        let mut mapper = Mapper202::new(
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
