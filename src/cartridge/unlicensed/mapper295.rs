//! Mapper 295 – BMC 13-in-1 JY110
//!
//! Specifications:
//! - Primary reference: FCEUX `src/boards/bmc13in1jy110.cpp` (authored by CaH4e3)
//!   (NesDev wiki page not found at time of implementation; Mesen2 lists this mapper as
//!   unimplemented with comment `//13IN1JY110`)
//!
//! # Hardware overview
//!
//! Used by the "13 in 1 JY110" multicart board.
//!
//! - PRG-ROM: Multiple banking modes controlled by `bank_mode` and `bank_value`.
//! - CHR-ROM: 8 KiB, fixed to bank 0. No CHR banking.
//! - Mirroring: Controlled by `$D001` bits 1:0.
//! - IRQ: none
//! - PRG-RAM: none
//! - Bus conflicts: none
//!
//! # Register map
//!
//! | Address     | Register        | Effect                                              |
//! |-------------|-----------------|-----------------------------------------------------|
//! | `$8000`     | `prgb[0]`       | PRG 8 KB sub-bank 0                                |
//! | `$8001`     | `prgb[1]`       | PRG 8 KB sub-bank 1                                |
//! | `$8002`     | `prgb[2]`       | PRG 8 KB sub-bank 2                                |
//! | `$8003`     | `prgb[3]`       | PRG 8 KB sub-bank 3                                |
//! | `$D000`     | `bank_mode`     | Banking mode select (bits 2:0 are significant)     |
//! | `$D001`     | mirroring       | Nametable mirroring (bits 1:0)                     |
//! | `$D002`     | (ignored)       | No effect                                           |
//! | `$D003`     | `bank_value`    | Outer bank index (bits 2:0 are significant)        |
//!
//! All other write addresses are ignored.
//!
//! # PRG banking modes (`bank_mode & 7`)
//!
//! Due to C fall-through behaviour in the FCEUX reference implementation the effective
//! banking for each mode is:
//!
//! | `bank_mode & 7` | Layout  | Description                                                   |
//! |-----------------|---------|---------------------------------------------------------------|
//! | 0               | 32 KB   | Bank = `bank_value & 7`                                       |
//! | 1               | 32 KB   | Bank = `8 + (bank_value & 7)` (same as mode 4 due to fall-through) |
//! | 2               | 4×8 KB  | $8000=`prgb[0]>>2`, $A000=`prgb[1]`, $C000=`prgb[2]`, $E000=last bank |
//! | 3               | 4×8 KB  | $8000=`prgb[0]`, $A000=`prgb[1]`, $C000=`prgb[2]`, $E000=`prgb[3]`   |
//! | 4               | 32 KB   | Bank = `8 + (bank_value & 7)`                                 |
//! | 5               | 4×8 KB  | Same as mode 2 due to fall-through                            |
//! | 6, 7            | (noop)  | Banking unchanged                                             |
//!
//! # Mirroring (`$D001 & 3`)
//!
//! | Value | Mirroring          |
//! |-------|--------------------|
//! | 0     | Horizontal         |
//! | 1     | Vertical           |
//! | 2     | Single-screen lower |
//! | 3     | Single-screen upper |
//!
//! # Power-on / reset state
//!
//! All registers zero: 32 KB at $8000 bank 0, CHR bank 0, Horizontal mirroring.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::ines::NametableLayout;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 295;
const PRG_BANK_SIZE_8K: usize = 8 * 1024;
const CHR_BANK_SIZE_8K: usize = 8 * 1024;

/// Mapper 295 – BMC 13-in-1 JY110 multicart.
pub struct Mapper295 {
    base: BaseMapper,
    prgb: [u8; 4],
    bank_mode: u8,
    bank_value: u8,
    mirroring: NametableLayout,
}

impl Mapper295 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: false,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_8K);
        base.configure_chr_banking(CHR_BANK_SIZE_8K);

        let mut mapper = Self {
            base,
            prgb: [0; 4],
            bank_mode: 0,
            bank_value: 0,
            mirroring: NametableLayout::Horizontal,
        };
        mapper.apply_state();
        mapper
    }

    fn apply_state(&mut self) {
        self.base.set_mirroring(self.mirroring);
        self.base.select_chr_page(0, 0);

        match self.bank_mode & 7 {
            0 => {
                // 32 KB: bank = bank_value & 7 → two consecutive 8 KB pages
                let bank = ((self.bank_value & 7) * 4) as i16;
                self.base.select_prg_page(0, bank);
                self.base.select_prg_page(1, bank + 1);
                self.base.select_prg_page(2, bank + 2);
                self.base.select_prg_page(3, bank + 3);
            }
            1 | 4 => {
                // 32 KB: bank = 8 + (bank_value & 7)
                let bank = (8 + (self.bank_value & 7) as i16) * 4;
                self.base.select_prg_page(0, bank);
                self.base.select_prg_page(1, bank + 1);
                self.base.select_prg_page(2, bank + 2);
                self.base.select_prg_page(3, bank + 3);
            }
            2 | 5 => {
                // 4×8 KB: prgb[0]>>2, prgb[1], prgb[2], last bank
                self.base.select_prg_page(0, (self.prgb[0] >> 2) as i16);
                self.base.select_prg_page(1, self.prgb[1] as i16);
                self.base.select_prg_page(2, self.prgb[2] as i16);
                self.base.select_prg_page(3, -1); // last bank
            }
            3 => {
                // 4×8 KB: prgb[0], prgb[1], prgb[2], prgb[3]
                self.base.select_prg_page(0, self.prgb[0] as i16);
                self.base.select_prg_page(1, self.prgb[1] as i16);
                self.base.select_prg_page(2, self.prgb[2] as i16);
                self.base.select_prg_page(3, self.prgb[3] as i16);
            }
            _ => {
                // modes 6 and 7: no change to banking
            }
        }
    }
}

impl Mapper for Mapper295 {
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
        match addr {
            0x8000..=0x8003 => {
                self.prgb[(addr & 3) as usize] = value;
            }
            0xD000 => {
                self.bank_mode = value;
            }
            0xD001 => {
                self.mirroring = match value & 3 {
                    0 => NametableLayout::Horizontal,
                    1 => NametableLayout::Vertical,
                    2 => NametableLayout::SingleScreenLower,
                    _ => NametableLayout::SingleScreenUpper,
                };
            }
            0xD002 => {} // ignored
            0xD003 => {
                self.bank_value = value;
            }
            _ => return,
        }
        self.apply_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.prgb[0],
            self.prgb[1],
            self.prgb[2],
            self.prgb[3],
            self.bank_mode,
            self.bank_value,
            match &self.mirroring {
                NametableLayout::Horizontal => 0,
                NametableLayout::Vertical => 1,
                NametableLayout::SingleScreenLower => 2,
                NametableLayout::SingleScreenUpper => 3,
                _ => 0,
            },
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 7 {
            return;
        }
        self.prgb[0] = data[0];
        self.prgb[1] = data[1];
        self.prgb[2] = data[2];
        self.prgb[3] = data[3];
        self.bank_mode = data[4];
        self.bank_value = data[5];
        self.mirroring = match data[6] & 3 {
            0 => NametableLayout::Horizontal,
            1 => NametableLayout::Vertical,
            2 => NametableLayout::SingleScreenLower,
            _ => NametableLayout::SingleScreenUpper,
        };
        self.apply_state();
    }

    fn reset(&mut self) {
        self.prgb = [0; 4];
        self.bank_mode = 0;
        self.bank_value = 0;
        self.mirroring = NametableLayout::Horizontal;
        self.apply_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 16 banks × 8 KB = 128 KB PRG-ROM; 1 bank × 8 KB = 8 KB CHR-ROM
    const PRG_BANKS_8K: usize = 16;
    const CHR_BANKS_8K: usize = 1;

    fn make_mapper() -> Mapper295 {
        Mapper295::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE_8K, PRG_BANKS_8K),
                banked_data(CHR_BANK_SIZE_8K, CHR_BANKS_8K),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_295_is_registered_in_factory() {
        let result = create_mapper(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE_8K, PRG_BANKS_8K),
                banked_data(CHR_BANK_SIZE_8K, CHR_BANKS_8K),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(result.is_ok(), "Mapper 295 must be registered in factory");
    }

    // ── Power-on state ───────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_is_32kb_bank_0() {
        let mapper = make_mapper();
        // mode 0, bank_value 0 → 32KB bank 0: 4 × 8KB pages 0-3
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 in 8KB page 0");
        assert_eq!(mapper.read_prg(0xE000), 3, "$E000 in 8KB page 3 of block 0");
        assert_eq!(mapper.read_prg(0xFFFF), 3, "$FFFF in 8KB page 3 of block 0");
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x1FFF), 0);
    }

    #[test]
    fn power_on_mirroring_is_horizontal() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── PRG banking – mode 0 (32 KB, bank = bank_value & 7) ─────────────────

    #[test]
    fn mode0_bank_value_selects_32kb_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x00); // mode 0
        mapper.write_prg(0xD003, 0x02); // bank_value = 2 → 32KB bank 2 → 8KB pages 8-11
        // All 4 × 8KB pages should read bank index corresponding to their position in bank 2
        assert_eq!(
            mapper.read_prg(0x8000),
            8 % PRG_BANKS_8K as u8,
            "page 0 = 8"
        );
        assert_eq!(
            mapper.read_prg(0xA000),
            9 % PRG_BANKS_8K as u8,
            "page 1 = 9"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            10 % PRG_BANKS_8K as u8,
            "page 2 = 10"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            11 % PRG_BANKS_8K as u8,
            "page 3 = 11"
        );
    }

    #[test]
    fn mode0_bank_value_masked_to_3_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x00); // mode 0
        mapper.write_prg(0xD003, 0xFF); // bank_value & 7 = 7 → 8KB pages 28-31
        assert_eq!(mapper.read_prg(0x8000), 28 % PRG_BANKS_8K as u8, "page 0");
    }

    // ── PRG banking – mode 1/4 (32 KB, bank = 8 + bank_value & 7) ───────────

    #[test]
    fn mode1_selects_32kb_upper_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x01); // mode 1
        mapper.write_prg(0xD003, 0x01); // bank_value = 1 → 8 + 1 = 9 → 8KB pages 36-39
        assert_eq!(mapper.read_prg(0x8000), 36 % PRG_BANKS_8K as u8, "page 0");
        assert_eq!(mapper.read_prg(0xE000), 39 % PRG_BANKS_8K as u8, "page 3");
    }

    #[test]
    fn mode4_selects_32kb_upper_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x04); // mode 4
        mapper.write_prg(0xD003, 0x01); // bank_value = 1 → 8 + 1 = 9 → same as mode 1
        assert_eq!(mapper.read_prg(0x8000), 36 % PRG_BANKS_8K as u8, "page 0");
        assert_eq!(mapper.read_prg(0xE000), 39 % PRG_BANKS_8K as u8, "page 3");
    }

    #[test]
    fn mode1_and_mode4_produce_same_result() {
        let mut m1 = make_mapper();
        let mut m4 = make_mapper();
        m1.write_prg(0xD000, 0x01);
        m1.write_prg(0xD003, 0x03);
        m4.write_prg(0xD000, 0x04);
        m4.write_prg(0xD003, 0x03);
        assert_eq!(m1.read_prg(0x8000), m4.read_prg(0x8000), "mode 1 == mode 4");
        assert_eq!(m1.read_prg(0xE000), m4.read_prg(0xE000), "mode 1 == mode 4");
    }

    // ── PRG banking – mode 2/5 (4×8 KB, prgb[0]>>2, prgb[1], prgb[2], last) ─

    #[test]
    fn mode2_4x8kb_banking_from_prgb_regs() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x08); // prgb[0] = 8 → prgb[0]>>2 = 2
        mapper.write_prg(0x8001, 0x05); // prgb[1] = 5
        mapper.write_prg(0x8002, 0x06); // prgb[2] = 6
        mapper.write_prg(0x8003, 0x03); // prgb[3] = 3 (ignored in mode 2)
        mapper.write_prg(0xD000, 0x02); // mode 2
        assert_eq!(mapper.read_prg(0x8000), 2, "$8000 = prgb[0]>>2 = 2");
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 = prgb[1] = 5");
        assert_eq!(mapper.read_prg(0xC000), 6, "$C000 = prgb[2] = 6");
        // $E000 = last bank (bank index 15 in a 16-bank ROM)
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS_8K - 1) as u8,
            "$E000 = last bank"
        );
    }

    #[test]
    fn mode5_produces_same_result_as_mode2() {
        let mut m2 = make_mapper();
        let mut m5 = make_mapper();
        for addr in [0x8000u16, 0x8001, 0x8002, 0x8003] {
            m2.write_prg(addr, (addr & 0x0F) as u8 + 1);
            m5.write_prg(addr, (addr & 0x0F) as u8 + 1);
        }
        m2.write_prg(0xD000, 0x02);
        m5.write_prg(0xD000, 0x05);
        for base_addr in [0x8000u16, 0xA000, 0xC000, 0xE000] {
            assert_eq!(
                m2.read_prg(base_addr),
                m5.read_prg(base_addr),
                "mode 2 == mode 5 at ${:04X}",
                base_addr
            );
        }
    }

    // ── PRG banking – mode 3 (4×8 KB, all prgb registers) ──────────────────

    #[test]
    fn mode3_4x8kb_all_prgb_registers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // prgb[0] = 1
        mapper.write_prg(0x8001, 0x03); // prgb[1] = 3
        mapper.write_prg(0x8002, 0x05); // prgb[2] = 5
        mapper.write_prg(0x8003, 0x07); // prgb[3] = 7
        mapper.write_prg(0xD000, 0x03); // mode 3
        assert_eq!(mapper.read_prg(0x8000), 1, "$8000 = prgb[0] = 1");
        assert_eq!(mapper.read_prg(0xA000), 3, "$A000 = prgb[1] = 3");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 = prgb[2] = 5");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 = prgb[3] = 7");
    }

    // ── PRG register writes ──────────────────────────────────────────────────

    #[test]
    fn prgb_regs_8000_to_8003_each_select_sub_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x03); // mode 3 to use all prgb
        for i in 0u16..4 {
            mapper.write_prg(0x8000 + i, i as u8 + 1);
        }
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 3);
        assert_eq!(mapper.read_prg(0xE000), 4);
    }

    #[test]
    fn d002_write_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD000, 0x00); // mode 0
        mapper.write_prg(0xD003, 0x01); // bank_value = 1
        mapper.write_prg(0xD002, 0xFF); // should be ignored
        // banking should not change
        assert_eq!(mapper.read_prg(0x8000), 4 % PRG_BANKS_8K as u8);
    }

    // ── Modes 6 and 7 ────────────────────────────────────────────────────────

    #[test]
    fn modes_6_and_7_do_not_change_banking() {
        let mut mapper = make_mapper();
        // Set up mode 0 with known banking
        mapper.write_prg(0xD000, 0x00);
        mapper.write_prg(0xD003, 0x02); // bank 2 → pages 8-11
        let page_before = mapper.read_prg(0x8000);

        // Switching to mode 6 should leave banking unchanged
        mapper.write_prg(0xD000, 0x06);
        assert_eq!(
            mapper.read_prg(0x8000),
            page_before,
            "mode 6: no banking change"
        );

        mapper.write_prg(0xD000, 0x07);
        assert_eq!(
            mapper.read_prg(0x8000),
            page_before,
            "mode 7: no banking change"
        );
    }

    // ── CHR banking ──────────────────────────────────────────────────────────

    #[test]
    fn chr_is_always_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0xD000, 0x03);
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 at $0000");
        assert_eq!(mapper.read_chr(0x1FFF), 0, "CHR bank 0 at $1FFF");
    }

    // ── Mirroring ────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_d001_value_0_is_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x01); // set vertical first
        mapper.write_prg(0xD001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_d001_value_1_is_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn mirroring_d001_value_2_is_single_screen_lower() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x02);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    #[test]
    fn mirroring_d001_value_3_is_single_screen_upper() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x03);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);
    }

    #[test]
    fn mirroring_upper_bits_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0xFE); // bits[1:0] = 2 → SingleScreenLower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05);
        mapper.write_prg(0x8001, 0x06);
        mapper.write_prg(0xD000, 0x03);
        mapper.write_prg(0xD001, 0x01);
        mapper.write_prg(0xD003, 0x07);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "after reset page 0 = bank 0");
        assert_eq!(mapper.read_chr(0x0000), 0, "after reset CHR = bank 0");
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_all_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // prgb[0] = 1
        mapper.write_prg(0x8001, 0x02); // prgb[1] = 2
        mapper.write_prg(0x8002, 0x03); // prgb[2] = 3
        mapper.write_prg(0x8003, 0x04); // prgb[3] = 4
        mapper.write_prg(0xD000, 0x03); // mode 3
        mapper.write_prg(0xD001, 0x01); // Vertical
        mapper.write_prg(0xD003, 0x05); // bank_value = 5
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 1);
        assert_eq!(restored.read_prg(0xA000), 2);
        assert_eq!(restored.read_prg(0xC000), 3);
        assert_eq!(restored.read_prg(0xE000), 4);
        assert_eq!(restored.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xD001, 0x01); // Vertical
        mapper.restore_registers(&[0, 0, 0, 0, 0, 0]); // only 6 bytes, needs 7
        // mirroring should be unchanged
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(caps.has_dynamic_mirroring, "dynamic mirroring");
        assert!(!caps.has_chr_banking, "no CHR banking");
        assert_eq!(caps.prg_bank_size_kb, 8, "8 KB PRG banks");
        assert_eq!(caps.chr_bank_size_kb, 8, "8 KB CHR bank");
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        mapper.write_prg(0xD000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 295 must never assert IRQ");
    }
}
