//! Mapper 028 - Action 53 (homebrew multicart)
//!
//! Specifications:
//! - Fallback: Mesen2 `Core/NES/Mappers/Homebrew/Action53.h`
//! - NesDev wiki: <https://www.nesdev.org/wiki/INES_Mapper_028>
//!
//! Hardware: Action 53 multicart board
//!
//! Register select ($5000–$5FFF write): `selected_reg = ((value & 0x80) >> 6) | (value & 0x01)`
//! Selects which of 4 registers is the target for the next $8000–$FFFF write.
//!
//! Registers (written via $8000–$FFFF):
//! - `regs[0]` (R:CHR)   – bits 1:0 = 8 KB CHR bank
//! - `regs[1]` (R:PRG)   – bits 3:0 = inner PRG bank
//! - `regs[2]` (R:MODE)  – bits 1:0 = mirroring, bit 2 = slotSelect,
//!   bit 3 = prgSize, bits 5:4 = gameSize
//! - `regs[3]` (R:OUTER) – outer PRG bank (32 KB units; outerPrgSelect = regs[3] << 1)
//!
//! `mirroring_bit` is updated on every $8000–$FFFF write:
//! - regs[0] or regs[1] write: `mirroring_bit = (value >> 4) & 1`
//! - regs[2] write:            `mirroring_bit = value & 1`
//!
//! Mirroring:
//! - `regs[2] & 0x03` bits 1:0: if bit 1 is clear, `mirroring_bit` selects single-screen
//!   (0 = lower / ScreenA, 1 = upper / ScreenB).  If bit 1 is set: 2 = Vertical, 3 = Horizontal.
//!
//! PRG banking (16 KB pages):
//! - 32 KB mode (`prgSize=0`): both pages form a 32 KB block selected by inner and outer.
//! - 16 KB mode (`prgSize=1`): one page is switchable, the other is fixed to the outer edge.
//!
//! CHR: 8 KB switchable window; CHR-ROM or CHR-RAM (32 KB when no CHR-ROM).
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::common::ChrMemory;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const CHR_RAM_SIZE: usize = 32 * 1024;

/// Mapper 028 – Action 53 multicart.
pub struct Action53Mapper {
    base: BaseMapper,
    selected_reg: u8,
    regs: [u8; 4],
    mirroring_bit: u8,
    hard_reset_pending: bool,
}

impl Action53Mapper {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: ctx.prg_ram_banks_8k as usize * 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        if ctx.chr_rom.is_empty() {
            base.set_chr_memory(ChrMemory::new_ram(CHR_RAM_SIZE));
        }
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);

        let mut mapper = Self {
            base,
            selected_reg: 0,
            regs: [0; 4],
            mirroring_bit: 0,
            hard_reset_pending: false,
        };
        mapper.update_banks();
        mapper.apply_power_on_mapping();
        mapper
    }

    fn apply_power_on_mapping(&mut self) {
        // NESdev: "At power on, the last 16 KiB of the ROM is mapped into $C000-$FFFF."
        self.base.select_prg_page(1, -1);
    }

    fn update_banks(&mut self) {
        // CHR
        self.base.select_chr_page(0, (self.regs[0] & 0x03) as i16);

        // Mirroring
        let mir_bits = self.regs[2] & 0x03;
        let mirroring = if (mir_bits & 0x02) == 0 {
            if self.mirroring_bit == 0 {
                NametableLayout::SingleScreenLower
            } else {
                NametableLayout::SingleScreenUpper
            }
        } else if mir_bits == 2 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        };
        self.base.set_mirroring(mirroring);

        // PRG
        let outer_prg = (self.regs[3] as u16) << 1;
        let prg_select = (self.regs[1] & 0x0F) as u16;
        let slot_select = ((self.regs[2] >> 2) & 0x01) as u16;
        let prg_size = (self.regs[2] >> 3) & 0x01;
        let game_size = ((self.regs[2] >> 4) & 0x03) as usize;

        const OUTER_AND: [u16; 4] = [0x1FE, 0x1FC, 0x1F8, 0x1F0];
        const INNER_AND: [u16; 4] = [0x01, 0x03, 0x07, 0x0F];
        let omask = OUTER_AND[game_size];
        let imask = INNER_AND[game_size];

        if prg_size != 0 {
            // 16 KB mode
            let var_page = if slot_select != 0 { 0u8 } else { 1u8 };
            let fix_page = 1 - var_page;
            let var_bank = (outer_prg & omask) | (prg_select & imask);
            let fix_bank = (outer_prg & 0x1FE) | slot_select;
            self.base.select_prg_page(var_page.into(), var_bank as i16);
            self.base.select_prg_page(fix_page.into(), fix_bank as i16);
        } else {
            // 32 KB mode – inner selects a 32 KB block
            let ps = prg_select << 1;
            let page0 = (outer_prg & omask) | (ps & imask);
            let page1 = (outer_prg & omask) | ((ps | 0x01) & imask);
            self.base.select_prg_page(0, page0 as i16);
            self.base.select_prg_page(1, page1 as i16);
        }
    }
}

impl Mapper for Action53Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                self.base.try_write_prg_ram(addr, value);
            }
            0x5000..=0x5FFF => {
                self.selected_reg = ((value & 0x80) >> 6) | (value & 0x01);
            }
            0x8000..=0xFFFF => {
                let r = self.selected_reg as usize;
                if r <= 1 {
                    self.mirroring_bit = (value >> 4) & 0x01;
                } else if r == 2 {
                    self.mirroring_bit = value & 0x01;
                }
                self.regs[r] = value;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.regs.to_vec();
        snap.push(self.selected_reg);
        snap.push(self.mirroring_bit);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 6 {
            self.regs.copy_from_slice(&data[0..4]);
            self.selected_reg = data[4];
            self.mirroring_bit = data[5];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        // NESdev: "The mapper state is unchanged on reset."
        if self.hard_reset_pending {
            self.hard_reset_pending = false;
            self.selected_reg = 0;
            self.regs = [0; 4];
            self.mirroring_bit = 0;
            self.update_banks();
            self.apply_power_on_mapping();
        }
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.base.initialize_ram(mode);
        self.hard_reset_pending = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    /// 32 banks × 16 KB = 512 KB PRG-ROM; no CHR-ROM (uses CHR-RAM).
    fn make_mapper() -> Action53Mapper {
        Action53Mapper::new(MapperContext::new_for_test(
            28,
            banked_data(16 * 1024, 32),
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    // ── Registration ──────────────────────────────────────────────────────

    #[test]
    fn mapper_28_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            28,
            banked_data(16 * 1024, 32),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "mapper 28 must be available in factory");
    }

    // ── Power-on state (NESdev: last 16 KiB in $C000-$FFFF) ─────────────

    #[test]
    fn power_on_maps_last_page_to_c000() {
        // NESdev spec: "At power on, the last 16 KiB of the ROM is mapped
        // into $C000-$FFFF."
        // 32 banks × 16 KB = banks 0..31; last bank = 31.
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "page 0 → bank 0");
        assert_eq!(mapper.read_prg(0xC000), 31, "page 1 → last bank");
    }

    #[test]
    fn default_mirroring_is_single_screen_lower() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    // ── Register selection ────────────────────────────────────────────────

    #[test]
    fn register_select_encoding() {
        // selected_reg = ((value & 0x80) >> 6) | (value & 0x01)
        // 0x00 → 0, 0x01 → 1, 0x80 → 2, 0x81 → 3
        let mut mapper = make_mapper();

        // Select reg 2 (0x80) and write MODE = 0x0B (prgSize=1, slotSelect=0, gameSize=0)
        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x08); // regs[2] = 0x08: prgSize=1
        // In 16KB mode slotSelect=0: page0 fixed=bank0, page1 variable=bank0
        assert_eq!(mapper.read_prg(0x8000), 0, "page 0 fixed = bank 0");

        // Select reg 1 (0x01) and write PRG inner = 1
        mapper.write_prg(0x5000, 0x01); // select reg 1
        mapper.write_prg(0x8000, 0x01); // regs[1] = 1, innerBank=1
        assert_eq!(mapper.read_prg(0xC000), 1, "page 1 variable = bank 1");
    }

    // ── CHR banking ───────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_32kb() {
        let mapper = make_mapper();
        assert_eq!(mapper.chr_ram_snapshot().len(), CHR_RAM_SIZE);
    }

    #[test]
    fn chr_bank_switching() {
        let mut mapper = make_mapper();

        // Write to CHR bank 2
        mapper.write_prg(0x5000, 0x00); // select reg 0
        mapper.write_prg(0x8000, 0x02); // regs[0] = 2, CHR bank = 2
        mapper.write_chr(0x0100, 0xAB);

        // Switch to bank 0 – data should not be visible there
        mapper.write_prg(0x5000, 0x00);
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(
            mapper.read_chr(0x0100),
            0x00,
            "bank 0 should not contain data written to bank 2"
        );

        // Switch back to bank 2 – data should be present
        mapper.write_prg(0x5000, 0x00);
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(mapper.read_chr(0x0100), 0xAB, "bank 2 data must persist");
    }

    // ── PRG 32 KB mode ───────────────────────────────────────────────────

    #[test]
    fn prg_32kb_mode_inner_bank_selection() {
        // gameSize=0 (32KB window), prgSize=0 (32KB), inner selects 32KB block
        // innerAnd=0x01, ps = inner << 1
        // page0 = (outer & 0x1FE) | (ps & 0x01)
        // page1 = (outer & 0x1FE) | ((ps|1) & 0x01)
        let mut mapper = make_mapper();
        // prgSize=0 is default; regs[2]=0 already

        // inner=1: ps=2, page0=(0&0x1FE)|(2&0x01)=0, page1=(0&0x1FE)|(3&0x01)=1
        mapper.write_prg(0x5000, 0x01); // select reg 1
        mapper.write_prg(0x8000, 0x01); // inner=1
        assert_eq!(mapper.read_prg(0x8000), 0, "32KB inner=1: page0=bank0");
        assert_eq!(mapper.read_prg(0xC000), 1, "32KB inner=1: page1=bank1");

        // inner=2: ps=4, gameSize=0 innerAnd=0x01 → ps&0x01=0, (ps|1)&0x01=1
        // page0=bank0, page1=bank1 (same as above due to 1-bit inner mask)
        // Use gameSize=3 for wider inner range
        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x30); // gameSize=3 (bits5:4=11), prgSize=0
        mapper.write_prg(0x5000, 0x01); // select reg 1
        mapper.write_prg(0x8000, 0x06); // inner=6, ps=12
        // outerAndMask=0x1F0, innerAndMask=0x0F
        // page0 = (0 & 0x1F0) | (12 & 0x0F) = 12
        // page1 = (0 & 0x1F0) | (13 & 0x0F) = 13
        assert_eq!(
            mapper.read_prg(0x8000),
            12,
            "32KB gameSize3 inner=6: page0=bank12"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            13,
            "32KB gameSize3 inner=6: page1=bank13"
        );
    }

    // ── PRG 16 KB mode, slotSelect=0 ─────────────────────────────────────

    #[test]
    fn prg_16kb_mode_slot0_page1_switchable() {
        // prgSize=1, slotSelect=0 → page1 is variable, page0 is fixed
        // fixed page0 = (outer & 0x1FE) | 0 = even bank of outer window
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x38); // gameSize=3, prgSize=1, slotSelect=0
        // outer=0 → fixed page0 = 0, variable page1

        mapper.write_prg(0x5000, 0x01); // select reg 1
        mapper.write_prg(0x8000, 0x05); // inner=5
        // var bank = (0 & 0x1F0) | (5 & 0x0F) = 5
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "page0 fixed = bank0 (even outer)"
        );
        assert_eq!(mapper.read_prg(0xC000), 5, "page1 variable = bank5");

        mapper.write_prg(0x5000, 0x01);
        mapper.write_prg(0x8000, 0x09); // inner=9
        assert_eq!(mapper.read_prg(0xC000), 9, "page1 follows inner=9");
    }

    // ── PRG 16 KB mode, slotSelect=1 ─────────────────────────────────────

    #[test]
    fn prg_16kb_mode_slot1_page0_switchable() {
        // prgSize=1, slotSelect=1 → page0 is variable, page1 is fixed
        // fixed page1 = (outer & 0x1FE) | 1 = odd bank of outer window
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x3C); // gameSize=3, prgSize=1, slotSelect=1
        // outer=0 → fixed page1 = 1

        mapper.write_prg(0x5000, 0x01); // select reg 1
        mapper.write_prg(0x8000, 0x07); // inner=7
        // var bank = (0 & 0x1F0) | (7 & 0x0F) = 7
        assert_eq!(mapper.read_prg(0x8000), 7, "page0 variable = bank7");
        assert_eq!(
            mapper.read_prg(0xC000),
            1,
            "page1 fixed = bank1 (odd outer)"
        );
    }

    // ── Outer bank register ───────────────────────────────────────────────

    #[test]
    fn prg_outer_bank_selection() {
        // outerPrgSelect = regs[3] << 1; for gameSize=3, outerAndMask=0x1F0
        // Outer controls bits [8:4] of page index (256 KB blocks).
        // regs[3]=8 → outerPrgSelect=16; 16 & 0x1F0 = 16.
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x38); // gameSize=3, prgSize=1, slotSelect=0

        mapper.write_prg(0x5000, 0x81); // select reg 3
        mapper.write_prg(0x8000, 0x08); // outer=8 → outerPrgSelect=16

        mapper.write_prg(0x5000, 0x01); // select reg 1
        mapper.write_prg(0x8000, 0x05); // inner=5
        // fixed page0 = (16 & 0x1FE) | 0 = 16
        // var bank = (16 & 0x1F0) | (5 & 0x0F) = 16 | 5 = 21
        assert_eq!(
            mapper.read_prg(0x8000),
            16,
            "page0 fixed = bank16 (outer shift)"
        );
        assert_eq!(mapper.read_prg(0xC000), 21, "page1 variable = bank21");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────

    #[test]
    fn mirroring_vertical_and_horizontal() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x02); // bits 1:0 = 2 → Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x5000, 0x80);
        mapper.write_prg(0x8000, 0x03); // bits 1:0 = 3 → Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mirroring_single_screen_via_mirroring_bit() {
        let mut mapper = make_mapper();

        // Set regs[2] bits 1:0 = 0 (single-screen controlled by mirroring_bit)
        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x00); // bits1:0=0, mirroring_bit = 0 & 1 = 0 → lower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        // Write reg 0 with bit 4 set → mirroring_bit = 1 → upper
        mapper.write_prg(0x5000, 0x00); // select reg 0
        mapper.write_prg(0x8000, 0x10); // mirroring_bit = (0x10>>4)&1 = 1
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        // Regs[2] still 0 (bits1:0=0) so mirroring_bit is used
        // Write reg 1 with bit 4 clear → mirroring_bit = 0 → lower
        mapper.write_prg(0x5000, 0x01); // select reg 1
        mapper.write_prg(0x8000, 0x00); // mirroring_bit = 0 → lower
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    // ── Reset (NESdev: "mapper state is unchanged on reset") ────────────

    #[test]
    fn soft_reset_preserves_mapper_state() {
        // NESdev spec: "The mapper state is unchanged on reset."
        let mut mapper = make_mapper();

        // Change state: set mode with horizontal mirroring
        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x3B); // gameSize=3, prgSize=1, slotSelect=0, H-mirror

        let prg_8000_before = mapper.read_prg(0x8000);
        let prg_c000_before = mapper.read_prg(0xC000);
        let mirroring_before = mapper.get_mirroring();

        // Soft reset should NOT change state
        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x8000),
            prg_8000_before,
            "soft reset must preserve $8000"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            prg_c000_before,
            "soft reset must preserve $C000"
        );
        assert_eq!(
            mapper.get_mirroring(),
            mirroring_before,
            "soft reset must preserve mirroring"
        );
    }

    #[test]
    fn hard_reset_restores_power_on_state() {
        use crate::nes::console::RamInitMode;

        let mut mapper = make_mapper();

        // Change state away from power-on defaults
        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x3B); // some mode
        mapper.write_prg(0x5000, 0x01); // select reg 1
        mapper.write_prg(0x8000, 0x07); // inner = 7

        // Hard reset = initialize_ram + reset
        mapper.initialize_ram(RamInitMode::Zero);
        mapper.reset();

        assert_eq!(mapper.read_prg(0x8000), 0, "hard reset: page 0 → bank 0");
        assert_eq!(
            mapper.read_prg(0xC000),
            31,
            "hard reset: page 1 → last bank"
        );
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x80); // select reg 2
        mapper.write_prg(0x8000, 0x3B); // mode
        mapper.write_prg(0x5000, 0x01); // select reg 1
        mapper.write_prg(0x8000, 0x07); // inner = 7
        mapper.write_prg(0x5000, 0x81); // select reg 3
        mapper.write_prg(0x8000, 0x02); // outer = 2

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(restored.get_mirroring(), mapper.get_mirroring());
    }
}
