//! Mapper 305 – Kaiser KS7031
//!
//! Specifications:
//! - Primary: NesDev wiki (unavailable, HTTP 403).
//! - Fallback: Mesen2 `Core/NES/Mappers/Kaiser/Kaiser7031.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Kaiser/Kaiser7031.h>
//!
//! # Hardware overview
//!
//! Used for a Kaiser KS7031 cartridge.
//!
//! - PRG-ROM: accessed via 2 KiB pages.  `$8000–$FFFF` is fixed to the last
//!   16 × 2 KiB pages in reversed order (power-on / reset state).
//!   `$6000–$7FFF` is divided into four independently-switchable 2 KiB windows
//!   controlled by four registers.
//! - CHR-ROM/RAM: 8 KiB, bank 0 (fixed / unbanked).
//! - Mirroring: fixed Vertical.
//! - IRQ: none.
//! - PRG-RAM: none.
//! - Bus conflicts: none.
//!
//! # Register map
//!
//! Writes to `$8000–$FFFF` select a register and update the `$6000–$7FFF` window:
//!
//! | Address range   | Register | `$6000` window it controls |
//! |-----------------|----------|-----------------------------|
//! | `$8000–$87FF`   | regs[0]  | `$6000–$67FF`               |
//! | `$8800–$8FFF`   | regs[1]  | `$6800–$6FFF`               |
//! | `$9000–$97FF`   | regs[2]  | `$7000–$77FF`               |
//! | `$9800–$9FFF`   | regs[3]  | `$7800–$7FFF`               |
//! | `$A000–$A7FF`   | regs[0]  | (mirrors of above)          |
//! | …               | …        | …                           |
//!
//! Register index: `(addr >> 11) & 0x03`
//! Register value: PRG-ROM 2 KiB bank number mapped into the corresponding window.
//!
//! # Memory map
//!
//! ```text
//! CPU $6000–$67FF  PRG-ROM 2 KiB, bank = regs[0]
//! CPU $6800–$6FFF  PRG-ROM 2 KiB, bank = regs[1]
//! CPU $7000–$77FF  PRG-ROM 2 KiB, bank = regs[2]
//! CPU $7800–$7FFF  PRG-ROM 2 KiB, bank = regs[3]
//! CPU $8000–$FFFF  PRG-ROM 16 × 2 KiB, fixed (reversed init order)
//! PPU $0000–$1FFF  CHR 8 KiB bank 0 (fixed)
//! ```
//!
//! # Power-on / reset state
//!
//! - regs[0..NUM_REGS] = 0 (all `$6000–$7FFF` windows point to PRG bank 0).
//! - `$8000–$FFFF`: slot i → bank (15 − i) for i in 0..16.
//! - Mirroring: Vertical.
//! - CHR: bank 0 (fixed).

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 305;
const PRG_PAGE_SIZE: usize = 0x800; // 2 KiB
const PRG_SLOTS: usize = 16; // 16 × 2 KiB for $8000–$FFFF
const NUM_REGS: usize = 4; // four 2 KiB windows at $6000–$7FFF

/// Mapper 305 – Kaiser KS7031
pub struct Mapper305 {
    base: BaseMapper,
    /// Four 2-KiB bank selectors for the `$6000–$7FFF` region.
    regs: [u8; NUM_REGS],
}

impl Mapper305 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 2,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_PAGE_SIZE);

        let mut mapper = Self {
            base,
            regs: [0; NUM_REGS],
        };
        mapper.set_power_on_state();
        mapper
    }

    fn set_power_on_state(&mut self) {
        // $8000–$FFFF: 16 slots, slot i → bank (15 − i)
        for i in 0..PRG_SLOTS {
            self.base.select_prg_page(i, 15 - i as i16);
        }
        // regs already zeroed, so $6000–$7FFF all point to bank 0
        // KS7031 always uses fixed Vertical mirroring regardless of ROM header.
        self.base.set_mirroring(NametableLayout::Vertical);
    }

    /// Read a byte from the `$6000–$7FFF` PRG-ROM windows.
    fn read_prg_6000_window(&self, addr: u16) -> u8 {
        debug_assert!((0x6000..=0x7FFF).contains(&addr));
        let slot = ((addr - 0x6000) >> 11) as usize; // yields slot index 0..=3 for the four 2 KiB windows
        let bank = self.regs[slot] as usize;
        let offset = (addr as usize - 0x6000) & (PRG_PAGE_SIZE - 1);
        let prg = self.base.prg_rom();
        prg[(bank * PRG_PAGE_SIZE + offset) % prg.len()]
    }
}

impl Mapper for Mapper305 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.read_prg_6000_window(addr),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.read_prg_6000_window(addr),
            _ => self
                .base
                .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr >= 0x8000 {
            let slot = ((addr >> 11) & 0x03) as usize;
            self.regs[slot] = value;
        }
    }

    fn reset(&mut self) {
        self.regs = [0; NUM_REGS];
        self.set_power_on_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.regs.to_vec()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= NUM_REGS {
            self.regs.copy_from_slice(&data[..NUM_REGS]);
        }
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;
    fn create_mapper305(prg_rom: Vec<u8>) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            305,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ))
        .expect("mapper 305 should be implemented")
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_305_is_registered() {
        let prg_rom = banked_data(2048, 16);
        let mapper = create_mapper305(prg_rom);
        assert_eq!(mapper.mapper_number(), 305);
    }

    // ── Power-on: $8000–$FFFF reversed fixed banks ────────────────────────────

    /// Slot 0 at $8000 maps to bank 15 (last 2 KiB), slot 15 at $FFFF to bank 0.
    #[test]
    fn power_on_8000_ffff_mapped_in_reversed_order() {
        // Use 16 × 2 KiB banks; bank N is filled with byte N.
        let prg_rom = banked_data(2048, 16);
        let mapper = create_mapper305(prg_rom);

        // Slot 0 ($8000–$87FF) → bank 15
        assert_eq!(mapper.read_prg(0x8000), 15, "slot 0 should read bank 15");
        // Slot 1 ($8800–$8FFF) → bank 14
        assert_eq!(mapper.read_prg(0x8800), 14, "slot 1 should read bank 14");
        // Slot 7 ($8000 + 7*0x800) → bank 8
        assert_eq!(
            mapper.read_prg(0x8000 + 7 * 0x800),
            8,
            "slot 7 should read bank 8"
        );
        // Slot 15 ($F800–$FFFF) → bank 0
        assert_eq!(mapper.read_prg(0xF800), 0, "slot 15 should read bank 0");
    }

    /// $8000–$FFFF does not change after a register write.
    #[test]
    fn power_on_8000_ffff_fixed_after_register_write() {
        let prg_rom = banked_data(2048, 16);
        let mut mapper = create_mapper305(prg_rom);

        // Write to regs[0] via $8000
        mapper.write_prg(0x8000, 5);

        // $8000 slot should still map to bank 15 (fixed)
        assert_eq!(
            mapper.read_prg(0x8000),
            15,
            "$8000–$FFFF region must remain fixed even after register writes"
        );
    }

    // ── Power-on: $6000–$7FFF windows default to bank 0 ─────────────────────

    #[test]
    fn power_on_6000_windows_read_from_bank_0() {
        // 16 banks; bank 0 is all zeroes
        let prg_rom = banked_data(2048, 16);
        let mapper = create_mapper305(prg_rom);

        // All four 2 KiB windows start at regs[0..4] = 0 → bank 0
        assert_eq!(mapper.read_prg(0x6000), 0, "$6000 should read bank 0");
        assert_eq!(mapper.read_prg(0x67FF), 0, "$67FF should read bank 0");
        assert_eq!(mapper.read_prg(0x6800), 0, "$6800 should read bank 0");
        assert_eq!(mapper.read_prg(0x7000), 0, "$7000 should read bank 0");
        assert_eq!(mapper.read_prg(0x7800), 0, "$7800 should read bank 0");
        assert_eq!(mapper.read_prg(0x7FFF), 0, "$7FFF should read bank 0");
    }

    // ── Register writes at $8000–$9FFF ────────────────────────────────────────

    /// Write to $8000–$87FF → regs[0] → $6000–$67FF window.
    #[test]
    fn write_8000_87ff_selects_6000_window_bank() {
        // Use 24 banks (not a power of two) to avoid modulo false passes.
        let prg_rom = banked_data(2048, 24);
        let mut mapper = create_mapper305(prg_rom);

        mapper.write_prg(0x8000, 7);
        assert_eq!(
            mapper.read_prg(0x6000),
            7,
            "$6000 should reflect bank 7 after regs[0]=7"
        );
        assert_eq!(mapper.read_prg(0x67FF), 7, "$67FF should also be in bank 7");
    }

    /// Write to $8800–$8FFF → regs[1] → $6800–$6FFF window.
    #[test]
    fn write_8800_8fff_selects_6800_window_bank() {
        let prg_rom = banked_data(2048, 24);
        let mut mapper = create_mapper305(prg_rom);

        mapper.write_prg(0x8800, 11);
        assert_eq!(
            mapper.read_prg(0x6800),
            11,
            "$6800 should reflect bank 11 after regs[1]=11"
        );
    }

    /// Write to $9000–$97FF → regs[2] → $7000–$77FF window.
    #[test]
    fn write_9000_97ff_selects_7000_window_bank() {
        let prg_rom = banked_data(2048, 24);
        let mut mapper = create_mapper305(prg_rom);

        mapper.write_prg(0x9000, 5);
        assert_eq!(
            mapper.read_prg(0x7000),
            5,
            "$7000 should reflect bank 5 after regs[2]=5"
        );
    }

    /// Write to $9800–$9FFF → regs[3] → $7800–$7FFF window.
    #[test]
    fn write_9800_9fff_selects_7800_window_bank() {
        let prg_rom = banked_data(2048, 24);
        let mut mapper = create_mapper305(prg_rom);

        mapper.write_prg(0x9800, 3);
        assert_eq!(
            mapper.read_prg(0x7800),
            3,
            "$7800 should reflect bank 3 after regs[3]=3"
        );
    }

    /// Register writes mirror every $2000 (addr bits 12:11 determine register).
    #[test]
    fn register_writes_mirror_every_2000() {
        let prg_rom = banked_data(2048, 24);
        let mut mapper = create_mapper305(prg_rom);

        // $A000–$A7FF mirrors to regs[0] (same as $8000–$87FF)
        mapper.write_prg(0xA000, 9);
        assert_eq!(
            mapper.read_prg(0x6000),
            9,
            "$A000 write should alias to regs[0]"
        );
    }

    // ── Independent windows ───────────────────────────────────────────────────

    /// The four windows are independently switchable.
    #[test]
    fn four_windows_are_independent() {
        let prg_rom = banked_data(2048, 24);
        let mut mapper = create_mapper305(prg_rom);

        mapper.write_prg(0x8000, 1); // regs[0] → $6000–$67FF = bank 1
        mapper.write_prg(0x8800, 3); // regs[1] → $6800–$6FFF = bank 3
        mapper.write_prg(0x9000, 5); // regs[2] → $7000–$77FF = bank 5
        mapper.write_prg(0x9800, 7); // regs[3] → $7800–$7FFF = bank 7

        assert_eq!(mapper.read_prg(0x6000), 1, "window 0 should be bank 1");
        assert_eq!(mapper.read_prg(0x6800), 3, "window 1 should be bank 3");
        assert_eq!(mapper.read_prg(0x7000), 5, "window 2 should be bank 5");
        assert_eq!(mapper.read_prg(0x7800), 7, "window 3 should be bank 7");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mapper_305_uses_vertical_mirroring() {
        let prg_rom = banked_data(2048, 16);
        // Deliberately pass Horizontal header mirroring to verify the mapper overrides it.
        let mapper = create_mapper(MapperContext::new_for_test(
            305,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ))
        .expect("mapper 305 should be implemented");
        assert_eq!(
            mapper.base().mirroring(),
            NametableLayout::Vertical,
            "mapper 305 should enforce fixed Vertical mirroring regardless of ROM header"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let prg_rom = banked_data(2048, 24);
        let mut mapper = create_mapper305(prg_rom);

        mapper.write_prg(0x8000, 10); // regs[0] = 10
        mapper.write_prg(0x8800, 11); // regs[1] = 11
        mapper.write_prg(0x9000, 12); // regs[2] = 12
        mapper.write_prg(0x9800, 13); // regs[3] = 13

        assert_eq!(mapper.read_prg(0x6000), 10);

        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "after reset, $6000 should read bank 0"
        );
        assert_eq!(
            mapper.read_prg(0x6800),
            0,
            "after reset, $6800 should read bank 0"
        );
        assert_eq!(
            mapper.read_prg(0x7000),
            0,
            "after reset, $7000 should read bank 0"
        );
        assert_eq!(
            mapper.read_prg(0x7800),
            0,
            "after reset, $7800 should read bank 0"
        );
    }

    // ── Save-state snapshot / restore ─────────────────────────────────────────

    #[test]
    fn snapshot_and_restore_preserves_registers() {
        let prg_rom = banked_data(2048, 24);
        let mut mapper = create_mapper305(prg_rom);

        mapper.write_prg(0x8000, 2);
        mapper.write_prg(0x8800, 4);
        mapper.write_prg(0x9000, 6);
        mapper.write_prg(0x9800, 8);

        let snapshot = mapper.registers_snapshot();

        // Overwrite
        mapper.write_prg(0x8000, 0);
        mapper.write_prg(0x8800, 0);
        mapper.write_prg(0x9000, 0);
        mapper.write_prg(0x9800, 0);

        mapper.restore_registers(&snapshot);

        assert_eq!(
            mapper.read_prg(0x6000),
            2,
            "restored window 0 should be bank 2"
        );
        assert_eq!(
            mapper.read_prg(0x6800),
            4,
            "restored window 1 should be bank 4"
        );
        assert_eq!(
            mapper.read_prg(0x7000),
            6,
            "restored window 2 should be bank 6"
        );
        assert_eq!(
            mapper.read_prg(0x7800),
            8,
            "restored window 3 should be bank 8"
        );
    }
}
