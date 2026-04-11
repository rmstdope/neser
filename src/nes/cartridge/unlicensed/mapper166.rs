//! Mapper 166 – Subor 小霸王中英文学习机 IV / Subor Educational Computer (variant A)
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_166>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Unlicensed/Subor166.h`
//!   (handles both Mapper 166 and 167 via `altMode` flag)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.
//!
//! ## Overview
//!
//! Mapper 166 is used by Subor's Chinese/Russian educational computer cartridges.
//! It is very similar to [Mapper 167](https://www.nesdev.org/wiki/INES_Mapper_167)
//! but with an altered PRG bank swapping order in NROM-256 and UNROM modes:
//! - UNROM (mode 0): fixed bank is `$07` (mapper 167 uses `$20`)
//! - NROM-256 (mode 2/3): even bank → slot 0, odd bank → slot 1 (mapper 167 reverses this)
//!
//! ## Memory Map
//!
//! * `CPU $6000–$7FFF`: unmapped / open bus (no PRG-RAM on this mapper)
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
//!              0: UNROM with fixed bank $07 at $C000–$FFFF
//!              1: Inverted UNROM with fixed bank $1F at $8000–$BFFF
//!              2: NROM-256 (32 KiB) – even→slot0, odd→slot1
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
//! | 0    | switchable   | fixed `$07`  |
//! | 1    | fixed `$1F`  | switchable   |
//! | 2/3  | switchable   | switchable+1 |
//!
//! *Note: Mapper 166 differs from Mapper 167 in modes 0 and 2/3: the UNROM fixed
//! bank is `$07` rather than `$20`, and in NROM-256 mode the even/odd assignment
//! is not swapped.*

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 166;
const PRG_BANK_SIZE: usize = 16 * 1024;

/// Mapper 166 – Subor Educational Computer (variant A).
///
/// See the module-level documentation for hardware details.
pub struct Mapper166 {
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

impl Mapper166 {
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
        // Shift bit 4 (0x10/16) into bit 5 (0x20/32): ×2 for 16 KiB page granularity.
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
                // Mapper 166 does NOT swap slot 0/1 (unlike mapper 167).
                let aligned = (outer | inner) & !1; // align to 32KB boundary
                self.base.select_prg_page(0, aligned); // slot 0 = even bank
                self.base.select_prg_page(1, aligned + 1); // slot 1 = odd bank
            }
            _ => {
                // Mode 0: UNROM – switchable at $8000–$BFFF, fixed $07 at $C000–$FFFF
                self.base.select_prg_page(0, outer | inner);
                self.base.select_prg_page(1, 0x07);
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

impl Mapper for Mapper166 {
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
            self.reg0 = data[0] & 0x11;
            self.reg1 = data[1] & 0x1C;
            self.reg2 = data[2] & 0x1F;
            self.reg3 = data[3] & 0x1F;
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

    // Need enough PRG banks to exercise fixed banks ($07 and $1F).
    const PRG_BANKS: usize = 33; // covers 0..32 (includes 0x20)

    fn make_mapper() -> Mapper166 {
        Mapper166::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    #[test]
    fn mapper_166_is_registered() {
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
            "Mapper 166 must be registered in the factory"
        );
    }

    #[test]
    fn power_on_is_unrom_mode0_with_inner_0() {
        let mapper = make_mapper();
        // Mode 0, all regs 0: inner=0, outer=0
        // slot 0 = bank 0, slot 1 = bank 0x07
        assert_eq!(mapper.read_prg(0x8000), 0, "slot 0 must be bank 0");
        assert_eq!(mapper.read_prg(0xC000), 7, "slot 1 must be fixed bank 7");
    }

    #[test]
    fn unrom_mode0_fixed_bank_is_07_not_20() {
        // This is the key difference from mapper 167 (which uses $20).
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "mapper 166 fixed UNROM bank must be 7 ($07), not 32 ($20)"
        );
    }

    #[test]
    fn unrom_mode0_switchable_bank_at_8000() {
        let mut mapper = make_mapper();
        // Write reg2 = 3, reg3 = 0 → inner = 3
        mapper.write_prg(0xC000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "switchable bank must be inner=3"
        );
        assert_eq!(mapper.read_prg(0xC000), 7, "fixed bank must remain 7");
    }

    #[test]
    fn inverted_unrom_mode1_fixed_bank_at_8000() {
        let mut mapper = make_mapper();
        // Set mode=1 via reg1 bits [3:2]=01 → reg1 = 0x04
        mapper.write_prg(0xA000, 0x04);
        assert_eq!(mapper.read_prg(0x8000), 0x1F, "mode 1: fixed $1F at $8000");
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "mode 1: switchable at $C000 (inner=0)"
        );
    }

    #[test]
    fn nrom256_mode2_even_at_slot0_odd_at_slot1() {
        let mut mapper = make_mapper();
        // Set mode=2 via reg1 bits [3:2]=10 → reg1 = 0x08
        // inner = reg2 ^ reg3. Set reg2=4 → inner=4 (even), aligned=4
        mapper.write_prg(0xA000, 0x08);
        mapper.write_prg(0xC000, 4);
        // slot 0 = aligned = 4 (even), slot 1 = 5 (odd)
        assert_eq!(mapper.read_prg(0x8000), 4, "NROM-256: even bank at slot 0");
        assert_eq!(mapper.read_prg(0xC000), 5, "NROM-256: odd bank at slot 1");
    }

    #[test]
    fn nrom256_mode2_is_not_swapped_unlike_mapper167() {
        // Mapper 166 does NOT swap slot0/slot1 (mapper 167 does).
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x08); // mode 2
        mapper.write_prg(0xC000, 0); // inner=0 → aligned=0
        // Expect slot0=bank0(even), slot1=bank1(odd) — NOT swapped
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    #[test]
    fn reg2_xor_reg3_selects_inner_bank() {
        let mut mapper = make_mapper();
        // reg2=5, reg3=3 → inner = 5^3 = 6
        mapper.write_prg(0xC000, 5);
        mapper.write_prg(0xE000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "inner bank must be reg2 XOR reg3"
        );
    }

    #[test]
    fn mirroring_bit0_of_reg0_controls_nametable() {
        let mut mapper = make_mapper();
        // N=0 → Horizontal
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        // N=1 → Vertical
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 0x08); // mode 2
        mapper.write_prg(0xC000, 5);
        mapper.write_prg(0xE000, 3);
        mapper.reset();
        // After reset: mode 0, all regs 0 → slot0=bank0, slot1=bank7
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    #[test]
    fn snapshot_restore_round_trips_registers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // reg0: N=1 (vertical)
        mapper.write_prg(0xA000, 0x08); // reg1: mode 2
        mapper.write_prg(0xC000, 5); // reg2: 5
        mapper.write_prg(0xE000, 3); // reg3: 3
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // inner = 5^3=6, mode=2, aligned=6
        assert_eq!(
            restored.read_prg(0x8000),
            6,
            "restored slot0 must be bank 6"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            7,
            "restored slot1 must be bank 7"
        );
    }
}
