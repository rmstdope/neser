//! Mapper 167 – Subor 小霸王中英文学习机 IV / Subor Educational Computer
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_167>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Unlicensed/Subor166.h`
//!   (handles both Mapper 166 and 167 via `altMode` flag)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.
//!
//! ## Overview
//!
//! Mapper 167 is used by Subor's Chinese/Russian educational computer cartridges.
//! It is very similar to [Mapper 166](https://www.nesdev.org/wiki/INES_Mapper_166)
//! but with an altered PRG bank swapping order in NROM-256 and UNROM modes.
//!
//! ## Memory Map
//!
//! * `CPU $6000–$7FFF`: 8 KiB unbanked CHR-RAM (not PRG-RAM)
//! * `CPU $8000–$BFFF`: 16 KiB switchable or fixed PRG-ROM window
//! * `CPU $C000–$FFFF`: 16 KiB switchable or fixed PRG-ROM window
//! * `PPU $0000–$1FFF`: 8 KiB unbanked CHR-RAM
//!
//! ## Registers
//!
//! There are four write-only registers.  The entire `$8000–$FFFF` range is
//! decoded, with only address bits `A14..A13` distinguishing the four registers.
//!
//! ### Register 0 – `$8000–$9FFF` (address mask `$E000`)
//!
//! ```text
//! D~[...F ...N]
//!        +---- N (bit 0): Nametable arrangement  0=Horizontal, 1=Vertical
//!   +--------- F (bit 4): PRG A19 (outer bank bit) XOR'd with 'f' from reg 1
//! ```
//!
//! ### Register 1 – `$A000–$BFFF` (address mask `$E000`)
//!
//! ```text
//! D~[...f MM..]
//!          +++ MM (bits 3..2): PRG-ROM banking mode
//!              0: UNROM with fixed bank $20 at $C000–$FFFF  (Mapper 167)
//!              1: Inverted UNROM with fixed bank $1F at $8000–$BFFF
//!              2: NROM-256 (32 KiB) – CPU A14 drives PRG A14
//!              3: same as mode 2
//!    +----- f (bit 4): PRG A19 XOR'd with 'F' from reg 0
//! ```
//!
//! ### Register 2 – `$C000–$DFFF` (address mask `$E000`)
//!
//! ```text
//! D~[...E DCBA]
//!   +-++++---- EDCBA (bits 4..0): switchable window bank bits,
//!              XOR'd with the corresponding bits from Register 3
//! ```
//!
//! ### Register 3 – `$E000–$FFFF` (address mask `$E000`)
//!
//! ```text
//! D~[...e dcba]
//!   +-++++---- edcba (bits 4..0): XOR mask for Register 2
//! ```
//!
//! The final bank number for the switchable window is:
//! `inner = reg2 XOR reg3` (5 bits: A18..A14).
//! The PRG A19 bit = `(reg0.F XOR reg1.f)`.
//! So the full 6-bit bank number = `(outer << 5) | inner` where
//! `outer = ((reg0 ^ reg1) >> 4) & 0x01`.
//!
//! ## Banking Modes
//!
//! | Mode | `$8000–$BFFF` | `$C000–$FFFF` |
//! |------|--------------|--------------|
//! | 0    | switchable   | fixed `$20`  |
//! | 1    | fixed `$1F`  | switchable   |
//! | 2/3  | switchable+1 | switchable   |
//!
//! *Note: Mapper 167 differs from Mapper 166 in modes 0 and 2/3: the even/odd
//! bank assignment within NROM-256 is swapped, and the UNROM fixed bank is `$20`
//! rather than `$07`.*

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 167;
const PRG_BANK_SIZE: usize = 16 * 1024;

/// Mapper 167 – Subor Educational Computer.
///
/// See the module-level documentation for hardware details.
pub struct Mapper167 {
    base: BaseMapper,
    /// Register 0 – written to $8000–$9FFF; bits [4,0] used (F, N).
    reg0: u8,
    /// Register 1 – written to $A000–$BFFF; bits [4,3,2] used (f, MM).
    reg1: u8,
    /// Register 2 – written to $C000–$DFFF; bits [4:0] used (EDCBA).
    reg2: u8,
    /// Register 3 – written to $E000–$FFFF; bits [4:0] used (edcba).
    reg3: u8,
}

impl Mapper167 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        let mut mapper = Self {
            base,
            reg0: 0,
            reg1: 0,
            reg2: 0,
            reg3: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // Outer bank bit: PRG A19 = F XOR f (bit 4 of reg0 XOR bit 4 of reg1).
        let outer_bit = ((self.reg0 ^ self.reg1) & 0x10) as i16;
        // Shift to become the A19 bit: outer_bit represents 32 decimal when set.
        let outer = outer_bit << 1; // bit 4 → bit 5 position (×2 for 16KB pages)

        // Inner bank: 5-bit value reg2 XOR reg3.
        let inner = (self.reg2 ^ self.reg3) as i16 & 0x1F;

        let mode = (self.reg1 >> 2) & 0x03;

        match mode {
            1 => {
                // Inverted UNROM: fixed 0x1F at $8000–$BFFF, switchable at $C000–$FFFF
                self.base.select_prg_page(0, 0x1F);
                self.base.select_prg_page(1, outer | inner);
            }
            2 | 3 => {
                // NROM-256 (32 KiB): CPU A14 → PRG A14
                // Mapper 167 swaps slot 0/1 relative to mapper 166.
                let aligned = (outer | inner) & !1; // align to 32KB boundary
                self.base.select_prg_page(0, aligned + 1); // slot 0 = odd bank
                self.base.select_prg_page(1, aligned); // slot 1 = even bank
            }
            _ => {
                // Mode 0: UNROM – switchable at $8000–$BFFF, fixed 0x20 at $C000–$FFFF
                self.base.select_prg_page(0, outer | inner);
                self.base.select_prg_page(1, 0x20);
            }
        }

        // Nametable: reg0 bit 0 = N; 0=Horizontal, 1=Vertical.
        let vertical = (self.reg0 & 0x01) != 0;
        if vertical {
            self.base.set_mirroring(NametableLayout::Vertical);
        } else {
            self.base.set_mirroring(NametableLayout::Horizontal);
        }
    }
}

impl Mapper for Mapper167 {
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
        match addr & 0xE000 {
            0x8000 => self.reg0 = value & 0x11, // keep bits [4,0]
            0xA000 => self.reg1 = value & 0x1C, // keep bits [4,3,2]
            0xC000 => self.reg2 = value & 0x1F, // keep bits [4:0]
            0xE000 => self.reg3 = value & 0x1F, // keep bits [4:0]
            _ => return,
        }
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg0, self.reg1, self.reg2, self.reg3]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.reg0 = data[0];
            self.reg1 = data[1];
            self.reg2 = data[2];
            self.reg3 = data[3];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.reg0 = 0;
        self.reg1 = 0;
        self.reg2 = 0;
        self.reg3 = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Use a non-power-of-two number of banks to catch modulo-wrapping issues.
    // PRG is 16KB pages; we need enough banks to exercise bank 0x20+ (fixed high bank).
    const PRG_BANKS: usize = 37; // covers 0..36 (0x00..0x24), includes 0x20

    fn make_mapper() -> Mapper167 {
        Mapper167::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_167_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(
            result.is_ok(),
            "Mapper 167 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_mode_0_unrom_slot0_bank0_slot1_fixed_0x20() {
        let mapper = make_mapper();
        // all regs = 0 → mode 0 (UNROM), inner=0, outer=0 → slot 0 = 0, slot 1 = 0x20
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 slot must be bank 0 at power-on"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            (0x20usize % PRG_BANKS) as u8,
            "$C000 slot must be fixed bank 0x20 at power-on"
        );
    }

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_mapper();
        // reg0.N = 0 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── Mode 0 – UNROM ($8000 switchable, $C000 fixed 0x20) ──────────────────

    #[test]
    fn mode0_switchable_slot0_inner_bank_3() {
        let mut mapper = make_mapper();
        // reg2=3, reg3=0 → inner = 3; outer=0 → slot 0 = 3
        mapper.write_prg(0xC000, 3); // reg2
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 slot = inner bank 3");
        assert_eq!(
            mapper.read_prg(0xC000),
            (0x20usize % PRG_BANKS) as u8,
            "$C000 fixed at 0x20"
        );
    }

    #[test]
    fn mode0_inner_xor_applies() {
        let mut mapper = make_mapper();
        // reg2=5, reg3=3 → inner = 5 XOR 3 = 6
        mapper.write_prg(0xC000, 5); // reg2
        mapper.write_prg(0xE000, 3); // reg3
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "XOR of reg2 and reg3 must give inner bank 6"
        );
    }

    // ── Mode 1 – Inverted UNROM ($8000 fixed 0x1F, $C000 switchable) ──────────

    #[test]
    fn mode1_slot0_fixed_0x1f() {
        let mut mapper = make_mapper();
        // reg1 with MM=1 → bits[3:2]=01 → value = 0b00000100 = 0x04
        mapper.write_prg(0xA000, 0x04); // reg1: MM=1
        assert_eq!(
            mapper.read_prg(0x8000),
            (0x1Fusize % PRG_BANKS) as u8,
            "$8000 slot fixed at 0x1F in mode 1"
        );
    }

    #[test]
    fn mode1_slot1_switchable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x04); // MM=1
        mapper.write_prg(0xC000, 4); // inner = 4
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 slot switchable in mode 1"
        );
    }

    // ── Mode 2/3 – NROM-256 (32 KiB, swapped banks) ──────────────────────────

    #[test]
    fn mode2_nrom256_slot0_is_odd_bank_slot1_is_even() {
        let mut mapper = make_mapper();
        // MM=2 → bits[3:2]=10 → value = 0b00001000 = 0x08
        mapper.write_prg(0xA000, 0x08); // reg1: MM=2
        mapper.write_prg(0xC000, 0); // inner = 0 → aligned bank 0
        // Mapper 167 (altMode): slot 0 = bank+1 = 1, slot 1 = bank = 0
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Mapper 167 NROM-256: $8000 slot = inner+1 (odd)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "Mapper 167 NROM-256: $C000 slot = inner (even)"
        );
    }

    #[test]
    fn mode2_nrom256_inner_4_gives_banks_5_and_4() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x08); // MM=2
        mapper.write_prg(0xC000, 4); // inner=4 → aligned=4
        assert_eq!(mapper.read_prg(0x8000), 5); // odd
        assert_eq!(mapper.read_prg(0xC000), 4); // even
    }

    #[test]
    fn mode3_same_as_mode2() {
        let mut mapper = make_mapper();
        // MM=3 → bits[3:2]=11 → 0x0C
        mapper.write_prg(0xA000, 0x0C); // MM=3
        mapper.write_prg(0xC000, 2); // inner=2 → aligned=2
        assert_eq!(mapper.read_prg(0x8000), 3);
        assert_eq!(mapper.read_prg(0xC000), 2);
    }

    // ── Outer bank (PRG A19) via F and f XOR ─────────────────────────────────

    #[test]
    fn outer_bank_bit_applies_when_f_xor_f_set() {
        let mut mapper = make_mapper();
        // reg0 F=1 (bit 4 set): 0x10
        mapper.write_prg(0x8000, 0x10); // reg0: F=1
        // reg1 f=0 (bit 4 clear): keep MM=0 → outer = 1 XOR 0 = 1 → outer_bank=32
        // slot 0 (mode 0) = outer | inner = 32 | 0 = 32 = 0x20
        assert_eq!(
            mapper.read_prg(0x8000),
            (0x20usize % PRG_BANKS) as u8,
            "Outer bank bit shifts PRG bank by 32"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn reg0_bit0_1_selects_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // reg0: N=1 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn reg0_bit0_0_selects_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01);
        mapper.write_prg(0x8000, 0x00); // reg0: N=0 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x11);
        mapper.write_prg(0xA000, 0x08);
        mapper.write_prg(0xC000, 7);
        mapper.write_prg(0xE000, 3);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(
            mapper.read_prg(0xC000),
            (0x20usize % PRG_BANKS) as u8,
            "$C000 must return to fixed 0x20 after reset"
        );
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x11);
        mapper.write_prg(0xA000, 0x04);
        mapper.write_prg(0xC000, 0x0A);
        mapper.write_prg(0xE000, 0x05);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Snapshot PRG $8000 must match"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Snapshot PRG $C000 must match"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Snapshot mirroring must match"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0);
        assert!(!mapper.irq_pending(), "Mapper 167 must never assert IRQ");
    }

    // ── CHR-RAM ────────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable (no CHR-ROM present)"
        );
    }
}
