//! Mapper 309 – UNL-LH51 (Ai Senshi Nicol FDS conversion)
//!
//! Specifications:
//! - Primary source: NesDev wiki
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_309>
//!
//! # Hardware overview
//!
//! Used by Whirlwind Manu's cartridge conversion of the Famicom Disk System game
//! 愛戦士ニコル (*Ai Senshi Nicol*, cartridge code LH51). UNIF board name: UNL-LH51.
//!
//! # Memory map
//!
//! | CPU range      | Size    | Contents                             |
//! |----------------|---------|--------------------------------------|
//! | `$6000–$7FFF`  | 8 KiB   | Unbanked PRG-RAM                     |
//! | `$8000–$9FFF`  | 8 KiB   | Switchable PRG-ROM bank (0–15)       |
//! | `$A000–$BFFF`  | 8 KiB   | Fixed PRG-ROM bank #13               |
//! | `$C000–$DFFF`  | 8 KiB   | Fixed PRG-ROM bank #14               |
//! | `$E000–$FFFF`  | 8 KiB   | Fixed PRG-ROM bank #15               |
//! | PPU `$0000–$1FFF` | 8 KiB | Unbanked CHR-RAM                  |
//!
//! # Registers
//!
//! ## PRG-ROM bank (`$8000–$9FFF`, mask probably `$E000`)
//!
//! ```text
//! D~7654 3210
//!   ---------
//!   .... PPPP
//!        ++++- Select 8 KiB PRG-ROM bank at CPU $8000–$9FFF
//! ```
//!
//! ## Mirroring (`$E000–$FFFF`, mask probably `$E000`)
//!
//! ```text
//! D~7654 3210
//!   ---------
//!   .... M...
//!        +---- Nametable mirroring: 0=Vertical, 1=Horizontal
//! ```
//!
//! # Notes
//!
//! This particular conversion has broken sound even on real hardware. Other
//! conversions of the same game that run under iNES Mapper 042 do not have
//! this problem. An emulator could provide the FDS expansion sound channel
//! since the conversion retains all writes to the FDS sound registers,
//! though this may not be worthwhile given the broken sound routine.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 309;
const PRG_BANK_SIZE: usize = 8 * 1024;

/// Mapper 309 – UNL-LH51 (Ai Senshi Nicol FDS conversion)
pub struct Mapper309 {
    base: BaseMapper,
    /// 4-bit switchable PRG-ROM bank at `$8000–$9FFF`.
    prg_bank: u8,
    /// Mirroring control bit: `false` = Vertical, `true` = Horizontal.
    mirroring_h: bool,
}

impl Mapper309 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: false,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        // CHR-RAM is unbanked; no configure_chr_banking needed.

        let mut mapper = Self {
            base,
            prg_bank: 0,
            mirroring_h: false,
        };
        mapper.apply_banks();
        mapper
    }

    fn apply_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        // Fixed banks: #13, #14, #15
        self.base.select_prg_page(1, 13);
        self.base.select_prg_page(2, 14);
        self.base.select_prg_page(3, 15);
        self.base.set_mirroring_hv(self.mirroring_h);
    }
}

impl Mapper for Mapper309 {
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
            0x8000 => {
                self.prg_bank = value & 0x0F;
                self.apply_banks();
            }
            0xE000 => {
                self.mirroring_h = value & 0x08 != 0;
                self.apply_banks();
            }
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.mirroring_h = false;
        self.apply_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.mirroring_h as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 2 {
            return;
        }
        self.prg_bank = data[0] & 0x0F;
        self.mirroring_h = data[1] & 0x01 != 0;
        self.apply_banks();
    }

    fn wram_size(&self) -> usize {
        self.base.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.base.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.base.load_wram_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.base.initialize_ram(mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 16 × 8 KiB = 128 KiB PRG (banks 0–15)
    const PRG_BANKS: usize = 16;

    fn make_mapper() -> Mapper309 {
        Mapper309::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                banked_data(PRG_BANK_SIZE, PRG_BANKS),
                vec![],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(1),
        )
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_309_is_registered_in_factory() {
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
            "Mapper 309 must be registered in the factory"
        );
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_slot0_maps_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 → bank 0");
        assert_eq!(mapper.read_prg(0x9FFF), 0, "$9FFF → bank 0");
    }

    #[test]
    fn power_on_fixed_banks_13_14_15() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xA000), 13, "$A000 → bank 13");
        assert_eq!(mapper.read_prg(0xC000), 14, "$C000 → bank 14");
        assert_eq!(mapper.read_prg(0xE000), 15, "$E000 → bank 15");
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── PRG bank switching ────────────────────────────────────────────────────

    #[test]
    fn write_to_8000_switches_slot0_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 5, "slot0 → bank 5");
    }

    #[test]
    fn write_to_9000_also_switches_slot0_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x07);
        assert_eq!(mapper.read_prg(0x8000), 7, "write to $9000 also works");
    }

    #[test]
    fn prg_bank_register_uses_lower_4_bits_only() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF); // only 0x0F = 15
        assert_eq!(mapper.read_prg(0x8000), 15, "bank = 0xFF & 0x0F = 15");
    }

    #[test]
    fn fixed_banks_unaffected_by_prg_bank_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x0A);
        assert_eq!(mapper.read_prg(0xA000), 13, "bank 13 still fixed");
        assert_eq!(mapper.read_prg(0xC000), 14, "bank 14 still fixed");
        assert_eq!(mapper.read_prg(0xE000), 15, "bank 15 still fixed");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn write_to_e000_with_bit3_set_selects_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x08);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn write_to_e000_with_bit3_clear_selects_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 0x08); // set H
        mapper.write_prg(0xE000, 0x00); // clear to V
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn write_to_f000_also_affects_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 0x08);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── CHR-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_readable_and_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0x42);
        assert_eq!(
            mapper.read_chr(0x0000),
            0x42,
            "CHR-RAM read/write roundtrip"
        );
    }

    // ── PRG-RAM at $6000-$7FFF ────────────────────────────────────────────────

    #[test]
    fn prg_ram_is_readable_and_writable_at_6000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(mapper.read_prg(0x6000), 0xAB, "PRG-RAM at $6000");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_bank_0_and_fixed_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x09);
        mapper.write_prg(0xE000, 0x08);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "bank 0 after reset");
        assert_eq!(
            mapper.read_prg(0xA000),
            13,
            "bank 13 still fixed after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "V mirroring after reset"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x07);
        mapper.write_prg(0xE000, 0x08);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 7, "restored bank 7");
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Horizontal,
            "restored mirroring"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05);
        mapper.restore_registers(&[0x00]); // need 2 bytes
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "state unchanged on short restore"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(
            !caps.has_chr_banking,
            "no CHR banking (CHR-RAM is unbanked)"
        );
        assert!(caps.has_dynamic_mirroring, "dynamic mirroring");
        assert!(!caps.has_irq, "no IRQ");
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 8);
    }
}
