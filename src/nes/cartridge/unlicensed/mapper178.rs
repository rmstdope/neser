//! Mapper 178 – Waixing FS305 / Nanjing NJ0430 / Jncota / Hengge Dianzi / GameStar
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_178>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Waixing/Waixing178.h`
//!
//! Known Limitations:
//! - Submapper 1 (Infrared Sensor IRQ), Submapper 2 (TMS5220 speech), and
//!   Submapper 3 (Solder Pad) are not implemented.
//!
//! ## Overview
//!
//! Mapper 178 is used by games from Waixing, Nanjing, Jncota, Hengge Dianzi,
//! and GameStar educational computers.  It uses 8 KiB of chip-internal,
//! unbanked CHR-RAM and supports bankable PRG-RAM at `$6000–$7FFF`.
//!
//! ## Memory Map
//!
//! * `CPU $6000–$7FFF`: 8 KiB switchable PRG-RAM bank
//! * `CPU $8000–$FFFF`: 16 or 32 KiB switchable PRG-ROM window
//! * `PPU $0000–$1FFF`: 8 KiB unbanked CHR-RAM
//!
//! ## Registers (CPU `$4800–$4803`)
//!
//! All registers respond to addresses `$4800–$4FFF`; only bits `A1..A0` select
//! the register (i.e., mask `$4803`).
//!
//! ### Mode Register (`$4800`)
//!
//! ```text
//! 7654 3210
//! ---------
//! .... .SSM
//!        +- M: Nametable mirroring  0=Vertical  1=Horizontal
//!      ++-- SS: PRG banking mode
//!             0: NROM-256 (32 KiB) – PRG A14 = CPU A14
//!             1: UNROM (16 KiB switchable; fixed high window when CPU A14=1)
//!             2: NROM-128 (16 KiB mirrored)
//!             3: UNROM with selectable fixed-bank low bit
//! ```
//!
//! ### Low PRG Bank Register (`$4801`)
//!
//! ```text
//! 7654 3210
//! ---------
//! .... .LLL
//!      +++- L[2:0]: PRG A16..A14 (inner bank)
//! ```
//!
//! ### High PRG Bank Register (`$4802`)
//!
//! ```text
//! 7654 3210
//! ---------
//! HHHH HHHH
//! ++++-++++- H[7:0]: PRG A24..A17 (outer bank, provides upper address bits)
//! ```
//!
//! ### PRG-RAM Bank Register (`$4803`)
//!
//! ```text
//! 7654 3210
//! ---------
//! .... ..BB
//!        ++- B[1:0]: PRG-RAM bank select (selects 8 KiB PRG-RAM bank at $6000)
//! ```
//!
//! ## PRG Banking Details
//!
//! Let `inner = reg1 & 0x07` (bits A16..A14 of the low bank register),
//! `outer = reg2` (bits A24..A17 of the high bank register), and
//! `bank(n) = (outer << 3) | n`.
//!
//! | SS | `$8000–$BFFF` | `$C000–$FFFF` |
//! |----|---------------|---------------|
//! | 0  | `bank(inner)` & even (32KB)  | `bank(inner) \| 1` |
//! | 1  | `bank(inner)` | `bank(outer<<3 \| 7)` (last in outer group) |
//! | 2  | `bank(inner)` | `bank(inner)` (mirror) |
//! | 3  | `bank(inner)` | `bank(outer<<3 \| 6 \| inner.bit0)` |
//!
//! PRG-RAM at `$6000–$7FFF` is banked by register `$4803`.  Up to 32 KiB
//! (4 banks) of PRG-RAM is supported.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 178;
const PRG_BANK_SIZE: usize = 16 * 1024;
const PRG_RAM_BANK_SIZE: usize = 8 * 1024;
/// Maximum PRG-RAM: 32 KiB (4 × 8 KiB banks)
const MAX_PRG_RAM_KB: usize = 32;

/// Mapper 178 – Waixing FS305 / Nanjing NJ0430 educational cartridge.
///
/// See the module-level documentation for hardware details.
pub struct Mapper178 {
    base: BaseMapper,
    /// $4800: bits [2:0] = SS (PRG mode) and M (mirroring).
    reg_mode: u8,
    /// $4801: bits [2:0] = inner PRG bank (A16..A14).
    reg_low: u8,
    /// $4802: outer PRG bank (A24..A17).
    reg_high: u8,
    /// $4803: PRG-RAM bank select.
    reg_ram: u8,
}

impl Mapper178 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            max_prg_ram_kb: MAX_PRG_RAM_KB,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        // Force CHR-RAM (chip-internal) and at least 1 PRG-RAM bank.
        let mut ctx = ctx;
        ctx.chr_rom = vec![];
        ctx.chr_ram_size_bytes = Some(8 * 1024);
        // Mapper 178 supports up to 32 KiB (4 banks) of PRG-RAM.
        let banks = ctx.prg_ram_banks_8k.min(4);
        ctx.prg_ram_banks_8k = if banks == 0 { 1 } else { banks };
        ctx.prg_ram_size_specified = true;

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);

        let mut mapper = Self {
            base,
            reg_mode: 0,
            reg_low: 0,
            reg_high: 0,
            reg_ram: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let ss = (self.reg_mode >> 1) & 0x03; // bits [2:1]
        let inner = (self.reg_low & 0x07) as i16;
        let outer = (self.reg_high as i16) << 3;

        match ss {
            1 => {
                // UNROM: slot 0 = switchable, slot 1 = last bank in outer group
                self.base.select_prg_page(0, outer | inner);
                self.base.select_prg_page(1, outer | 7);
            }
            2 => {
                // NROM-128: both slots = same bank (16 KiB mirrored)
                self.base.select_prg_page(0, outer | inner);
                self.base.select_prg_page(1, outer | inner);
            }
            3 => {
                // UNROM with selectable fixed-bank low bit
                self.base.select_prg_page(0, outer | inner);
                self.base.select_prg_page(1, outer | 6 | (inner & 0x01));
            }
            _ => {
                // Mode 0: NROM-256 (32 KiB) – aligned 32 KiB bank
                let bank32 = (outer | inner) & !1;
                self.base.select_prg_page(0, bank32);
                self.base.select_prg_page(1, bank32 | 1);
            }
        }

        // Mirroring: bit 0 of mode register.
        let horizontal = (self.reg_mode & 0x01) != 0;
        self.base.set_mirroring_hv(horizontal);
    }

    /// Compute the banked PRG-RAM byte offset for a CPU address in $6000–$7FFF.
    fn prg_ram_offset(&self, addr: u16) -> usize {
        let bank = (self.reg_ram as usize) & 0x03;
        let page_offset = (addr as usize) - 0x6000;
        let raw_offset = bank * PRG_RAM_BANK_SIZE + page_offset;
        let ram_size = self.base.wram_size();
        if ram_size > 0 {
            raw_offset % ram_size
        } else {
            raw_offset
        }
    }
}

impl Mapper for Mapper178 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.read_prg_ram_at_offset(self.prg_ram_offset(addr)),
            _ => self.base.read_prg_rom(addr),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                let offset = self.prg_ram_offset(addr);
                self.base.write_prg_ram_at_offset(offset, value);
            }
            0x4800..=0x4FFF => match addr & 0x03 {
                0 => {
                    self.reg_mode = value & 0x07;
                    self.update_banks();
                }
                1 => {
                    self.reg_low = value & 0x07;
                    self.update_banks();
                }
                2 => {
                    self.reg_high = value;
                    self.update_banks();
                }
                3 => {
                    self.reg_ram = value;
                }
                _ => unreachable!(),
            },
            _ => {}
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        // $4800-$4FFF: registers are write-only; reads return open bus.
        if (0x4800..=0x4FFF).contains(&addr) {
            return open_bus;
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg_mode, self.reg_low, self.reg_high, self.reg_ram]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 4 {
            self.reg_mode = data[0];
            self.reg_low = data[1];
            self.reg_high = data[2];
            self.reg_ram = data[3];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.reg_mode = 0;
        self.reg_low = 0;
        self.reg_high = 0;
        self.reg_ram = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to avoid modulo-wrapping false positives.
    // Need enough banks to include bank 7 (fixed in UNROM mode).
    const PRG_BANKS: usize = 11;
    const PRG_RAM_BANKS: u8 = 3;

    fn make_mapper() -> Mapper178 {
        Mapper178::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                banked_data(8 * 1024, 1),
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(PRG_RAM_BANKS),
        )
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_178_is_registered() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(1),
        );
        assert!(
            result.is_ok(),
            "Mapper 178 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_mode0_32k_bank0() {
        let mapper = make_mapper();
        // SS=0 → NROM-256 (32KB aligned): slot 0 = bank 0, slot 1 = bank 1
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 = bank 0 at power-on");
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 = bank 1 at power-on");
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Mode 0 – NROM-256 (32 KiB) ───────────────────────────────────────────

    #[test]
    fn mode0_nrom256_inner_2_gives_banks_2_3() {
        let mut mapper = make_mapper();
        // SS=0, inner=2
        mapper.write_prg(0x4801, 2); // inner=2
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 = bank 2");
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 = bank 3");
    }

    #[test]
    fn mode0_nrom256_odd_inner_aligns_to_even() {
        let mut mapper = make_mapper();
        // inner=3, aligned to 2 → banks 2 and 3
        mapper.write_prg(0x4801, 3); // inner=3 → bank32=2
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 = aligned bank 2");
        assert_eq!(mapper.read_prg(0xC000), 3, "$C000 = bank 3");
    }

    // ── Mode 1 – UNROM ────────────────────────────────────────────────────────

    #[test]
    fn mode1_unrom_slot0_switchable_slot1_fixed() {
        let mut mapper = make_mapper();
        // SS=1 → 0x4800 bits[2:1]=01 → value=0x02
        mapper.write_prg(0x4800, 0x02); // SS=1
        mapper.write_prg(0x4801, 3); // inner=3
        assert_eq!(mapper.read_prg(0x8000), 3, "$8000 = switchable bank 3");
        // Fixed = outer|7 = 0|7 = 7
        assert_eq!(
            mapper.read_prg(0xC000),
            (7 % PRG_BANKS) as u8,
            "$C000 = fixed last bank"
        );
    }

    #[test]
    fn mode1_high_reg_provides_outer_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4800, 0x02); // SS=1
        // outer=1 → outer<<3=8; inner=0 → slot 0 = 8, slot 1 = 15
        mapper.write_prg(0x4802, 1); // outer=1
        assert_eq!(
            mapper.read_prg(0x8000),
            (8 % PRG_BANKS) as u8,
            "$8000 with outer=1"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            (15 % PRG_BANKS) as u8,
            "$C000 fixed = outer|7 with outer=1"
        );
    }

    // ── Mode 2 – NROM-128 (16 KiB mirrored) ──────────────────────────────────

    #[test]
    fn mode2_nrom128_both_slots_same() {
        let mut mapper = make_mapper();
        // SS=2 → 0x4800 bits[2:1]=10 → value=0x04
        mapper.write_prg(0x4800, 0x04); // SS=2
        mapper.write_prg(0x4801, 4); // inner=4
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 = bank 4");
        assert_eq!(mapper.read_prg(0xC000), 4, "$C000 mirrors bank 4");
    }

    // ── Mode 3 – UNROM with selectable fixed bit ──────────────────────────────

    #[test]
    fn mode3_slot0_switchable_slot1_selectable_even() {
        let mut mapper = make_mapper();
        // SS=3 → 0x4800 bits[2:1]=11 → value=0x06
        mapper.write_prg(0x4800, 0x06); // SS=3
        mapper.write_prg(0x4801, 2); // inner=2 (even)
        // slot 1 = outer|6|(inner&1) = 0|6|0 = 6
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xC000), (6 % PRG_BANKS) as u8);
    }

    #[test]
    fn mode3_slot1_odd_selectable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4800, 0x06); // SS=3
        mapper.write_prg(0x4801, 3); // inner=3 (odd)
        // slot 1 = outer|6|(inner&1) = 0|6|1 = 7
        assert_eq!(mapper.read_prg(0xC000), (7 % PRG_BANKS) as u8);
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mode_reg_bit0_1_selects_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4800, 0x01); // M=1 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mode_reg_bit0_0_selects_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4800, 0x00); // M=0 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── CHR-RAM ────────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0100, 0xCD);
        assert_eq!(mapper.read_chr(0x0100), 0xCD);
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4800, 0x06); // SS=3
        mapper.write_prg(0x4801, 5);
        mapper.write_prg(0x4802, 2);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 = bank 0 after reset");
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 = bank 1 after reset");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4800, 0x02); // SS=1
        mapper.write_prg(0x4801, 3);
        mapper.write_prg(0x4802, 0);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG $8000 must match after restore"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "PRG $C000 must match after restore"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Mirroring must match after restore"
        );
    }

    // ── Banked PRG-RAM ────────────────────────────────────────────────────────

    #[test]
    fn prg_ram_bank0_readable_writable() {
        let mut mapper = make_mapper();
        // Default bank = 0
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }

    #[test]
    fn prg_ram_bank_switching() {
        let mut mapper = make_mapper();
        // Write distinct values into bank 0 and bank 1
        mapper.write_prg(0x4803, 0); // select bank 0
        mapper.write_prg(0x6000, 0x11);
        mapper.write_prg(0x4803, 1); // select bank 1
        mapper.write_prg(0x6000, 0x22);
        // Bank 1 should still read 0x22
        assert_eq!(mapper.read_prg(0x6000), 0x22);
        // Switch back to bank 0 → should read 0x11
        mapper.write_prg(0x4803, 0);
        assert_eq!(mapper.read_prg(0x6000), 0x11);
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4800, 0);
        assert!(!mapper.irq_pending(), "Mapper 178 must not assert IRQ");
    }

    #[test]
    fn prg_ram_present_even_when_header_omits_size() {
        let mut mapper = Mapper178::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(1)
            .with_unspecified_prg_ram_size(),
        );
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB);
    }
}
