//! Mapper 297 – Multicart (Mapper70/MMC1 hybrid)
//!
//! ## Specifications
//!
//! - Primary source: NesDev wiki
//!   <https://www.nesdev.org/wiki/NES_2.0_Mapper_297>
//!
//! ## Hardware overview
//!
//! A multicart board that presents either Mapper-70 or MMC1 banking depending on
//! an outer register.  The two modes share PRG-ROM and CHR-ROM but address different
//! halves:
//!
//! - **Mapper-70 mode** (`M=0`): A17=0 (lower 128 KiB of PRG). `$8000-$FFFF`
//!   writes latch a value; bits 5:4 select a 16 KiB PRG bank, bits 3:0 select an
//!   8 KiB CHR bank. `$C000-$FFFF` is fixed to bank `(A16<<2)|3` (last within
//!   the A16 half).
//! - **MMC1 mode** (`M=1`): A17=1 (upper 128 KiB of PRG). `$8000-$FFFF` writes
//!   go to the MMC1 serial shift register. All PRG banks are offset +8 and all CHR
//!   banks are offset +16 relative to the MMC1 inner bank selection.
//!
//! Mirroring is **always vertical** (CIRAM A10=PA10) regardless of what either
//! inner mapper writes.
//!
//! ## Outer register
//!
//! Any write whose address satisfies `(addr & 0xE100) == 0xE100` latches the data
//! byte into the outer register:
//!
//! ```text
//! Bit 0 (M): 0 = Mapper-70 mode, 1 = MMC1 mode
//! Bit 1 (A): PRG A16 (used only in Mapper-70 mode)
//! ```
//!
//! ## Known Limitations
//!
//! - No PRG-RAM.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};
use crate::nes::cartridge::mmc1::MMC1Mapper;

/// PRG offset in MMC1 mode: all PRG banks start at bank 8 (A17=1 → upper 128 KiB).
const MMC1_PRG_OFFSET: i16 = 8;
/// CHR offset in MMC1 mode: 16 × 4 KiB pages = 64 KiB into CHR-ROM.
/// In 8 KiB CHR mode the register value is doubled, yielding the same 64 KiB base.
const MMC1_CHR_OFFSET: i16 = 16;

/// Mapper 297 – Mapper70/MMC1 multicart hybrid.
pub struct Mapper297 {
    mmc1: MMC1Mapper,
    /// Outer register controlling which mode and outer bank bits are active.
    outer_reg: u8,
    /// Mapper-70 latch; updated on any `$8000-$FFFF` write in Mapper-70 mode.
    latch_70: u8,
}

impl Mapper297 {
    const MAPPER_NUMBER: u16 = 297;

    pub fn new(ctx: MapperContext) -> Self {
        // Force no PRG-RAM (mapper 297 has none).
        let mut ctx = ctx;
        ctx.prg_ram_banks_8k = 0;
        ctx.prg_ram_size_specified = true;

        let mmc1 = MMC1Mapper::new(ctx);
        let mut mapper = Self {
            mmc1,
            outer_reg: 0,
            latch_70: 0,
        };
        mapper.apply_banks();
        mapper
    }

    fn is_mmc1_mode(&self) -> bool {
        (self.outer_reg & 0x01) != 0
    }

    fn prg_a16(&self) -> i16 {
        ((self.outer_reg >> 1) & 0x01) as i16
    }

    fn apply_banks(&mut self) {
        if self.is_mmc1_mode() {
            self.apply_mmc1_banks();
        } else {
            self.apply_mapper70_banks();
        }
        // Mirroring is always vertical.
        self.mmc1
            .base_mut()
            .set_mirroring(NametableLayout::Vertical);
    }

    fn apply_mapper70_banks(&mut self) {
        let a16 = self.prg_a16();
        let inner_prg = ((self.latch_70 >> 4) & 0x03) as i16;
        let chr_bank = (self.latch_70 & 0x0F) as i16;
        self.mmc1
            .base_mut()
            .select_prg_page(0, (a16 << 2) | inner_prg);
        self.mmc1.base_mut().select_prg_page(1, (a16 << 2) | 3);
        self.mmc1.base_mut().select_chr_page(0, chr_bank * 2);
        self.mmc1.base_mut().select_chr_page(1, chr_bank * 2 + 1);
    }

    fn apply_mmc1_banks(&mut self) {
        let control = self.mmc1.control();
        let chr0 = self.mmc1.chr_bank_0();
        let chr1 = self.mmc1.chr_bank_1();
        let prg = self.mmc1.prg_bank();

        let prg_mode = (control >> 2) & 0x03;
        match prg_mode {
            0 | 1 => {
                let bank_32k = ((prg & 0x0E) >> 1) as i16;
                self.mmc1
                    .base_mut()
                    .select_prg_page(0, MMC1_PRG_OFFSET + bank_32k * 2);
                self.mmc1
                    .base_mut()
                    .select_prg_page(1, MMC1_PRG_OFFSET + bank_32k * 2 + 1);
            }
            2 => {
                self.mmc1.base_mut().select_prg_page(0, MMC1_PRG_OFFSET);
                self.mmc1
                    .base_mut()
                    .select_prg_page(1, MMC1_PRG_OFFSET + (prg & 0x0F) as i16);
            }
            3 => {
                self.mmc1
                    .base_mut()
                    .select_prg_page(0, MMC1_PRG_OFFSET + (prg & 0x0F) as i16);
                self.mmc1
                    .base_mut()
                    .select_prg_page(1, MMC1_PRG_OFFSET + 0x0F);
            }
            _ => unreachable!(),
        }

        let chr_mode = (control >> 4) & 0x01;
        if chr_mode == 0 {
            let bank = ((chr0 & 0x1E) >> 1) as i16;
            self.mmc1
                .base_mut()
                .select_chr_page(0, MMC1_CHR_OFFSET + bank * 2);
            self.mmc1
                .base_mut()
                .select_chr_page(1, MMC1_CHR_OFFSET + bank * 2 + 1);
        } else {
            self.mmc1
                .base_mut()
                .select_chr_page(0, MMC1_CHR_OFFSET + (chr0 & 0x1F) as i16);
            self.mmc1
                .base_mut()
                .select_chr_page(1, MMC1_CHR_OFFSET + (chr1 & 0x1F) as i16);
        }
    }
}

impl Mapper for Mapper297 {
    fn base(&self) -> &BaseMapper {
        self.mmc1.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.mmc1.base_mut()
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 4,
            ..Default::default()
        }
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.mmc1.read_prg(addr)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => open_bus, // no PRG-RAM
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !matches!(addr, 0x8000..=0xFFFF) {
            return;
        }
        // Outer register trigger: (addr & 0xE100) == 0xE100.
        if (addr & 0xE100) == 0xE100 {
            self.outer_reg = value;
            self.apply_banks();
            return;
        }
        if self.is_mmc1_mode() {
            self.mmc1.write_prg(addr, value);
        } else {
            self.latch_70 = value;
        }
        self.apply_banks();
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.mmc1.read_chr(addr)
    }

    fn cpu_cycle(&mut self) {
        self.mmc1.cpu_cycle();
    }

    fn get_mirroring(&self) -> NametableLayout {
        NametableLayout::Vertical
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = vec![self.outer_reg, self.latch_70];
        snap.extend(self.mmc1.registers_snapshot());
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.outer_reg = data[0];
            self.latch_70 = data[1];
            self.mmc1.restore_registers(&data[2..]);
            self.apply_banks();
        }
    }

    fn reset(&mut self) {
        self.mmc1.reset();
        self.outer_reg = 0;
        self.latch_70 = 0;
        self.apply_banks();
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.mmc1.initialize_ram(mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    // PRG: 16 × 16 KiB = 256 KiB (banks 0-7 = lower/mapper70, 8-15 = upper/MMC1)
    // CHR: 32 × 4 KiB = 128 KiB (banks 0-15 = lower, 16-31 = upper/MMC1)
    const PRG_16K_BANKS: usize = 16; // exactly 256 KiB
    const CHR_4K_BANKS: usize = 32; // exactly 128 KiB

    fn make_mapper() -> Mapper297 {
        let prg = banked_data(16 * 1024, PRG_16K_BANKS);
        let chr = banked_data(4 * 1024, CHR_4K_BANKS);
        Mapper297::new(MapperContext::new_for_test(
            Mapper297::MAPPER_NUMBER,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ────────────────────────────────────────────────

    #[test]
    fn mapper_297_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            Mapper297::MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_16K_BANKS),
            banked_data(4 * 1024, CHR_4K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 297 must be registered in the factory"
        );
    }

    // ── Mirroring always vertical ────────────────────────────────────

    #[test]
    fn mirroring_is_always_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must always be vertical"
        );
    }

    // ── Mapper-70 mode (power-on) ────────────────────────────────────

    #[test]
    fn power_on_mapper70_mode_prg_c000_is_bank_3() {
        let mapper = make_mapper();
        // A16=0, latch=0 → PRG $C000 fixed = (0<<2)|3 = bank 3.
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "Power-on: $C000 must be PRG bank 3 (fixed in A16=0 half)"
        );
    }

    #[test]
    fn mapper70_mode_latch_selects_prg_bank() {
        let mut mapper = make_mapper();
        // bits 5:4 of latch select PRG inner bank, A16=0 → bank = (0<<2)|inner
        // Write latch = 0x20 → bits 5:4 = 0b10 = 2 → PRG $8000 = bank 2.
        mapper.write_prg(0x8000, 0x20);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "Mapper-70 latch bits 5:4 must select PRG bank at $8000"
        );
    }

    #[test]
    fn mapper70_mode_with_a16_set_uses_upper_half() {
        let mut mapper = make_mapper();
        // Write outer register: A16=1, M=0 (mapper70 mode).
        // Address where (addr & 0xE100) == 0xE100, value = 0x02 (bit 1 = A16=1).
        mapper.write_prg(0xE100, 0x02);
        // With A16=1 and latch=0: PRG $C000 fixed = (1<<2)|3 = bank 7.
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "A16=1 must fix $C000 to bank 7 in mapper-70 mode"
        );
    }

    #[test]
    fn mapper70_mode_latch_selects_chr_bank() {
        let mut mapper = make_mapper();
        // Write latch = 0x03 → CHR = bits 3:0 = 3 → 8KiB bank 3 = 4KiB banks 6+7.
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(
            mapper.read_chr(0x0000),
            6,
            "Mapper-70 latch bits 3:0 must select CHR 8KiB bank (4KiB page 0)"
        );
    }

    // ── MMC1 mode ───────────────────────────────────────────────────

    #[test]
    fn mmc1_mode_enabled_by_outer_register_bit0() {
        let mut mapper = make_mapper();
        // Write outer register with M=1 at an address that triggers it.
        mapper.write_prg(0xE100, 0x01);
        // In MMC1 mode, $8000 PRG starts at bank 8 (PRG_OFFSET=8).
        // After reset, MMC1 prg_mode=3 → $8000 switchable, $C000 fixed to last of outer.
        // With no MMC1 writes, prg_bank=0 → $8000 = bank 8+0 = bank 8.
        assert_eq!(
            mapper.read_prg(0x8000),
            8,
            "MMC1 mode: PRG $8000 must start at bank 8 (offset +8)"
        );
    }

    #[test]
    fn mmc1_mode_mirroring_stays_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE100, 0x01); // enable MMC1 mode
        // MMC1 normally sets mirroring from control register; mapper 297 must override.
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must stay vertical even in MMC1 mode"
        );
    }

    // ── Outer register detection ─────────────────────────────────────

    #[test]
    fn outer_register_triggered_by_e100_mask() {
        let mut mapper = make_mapper();
        // Regular write at $8000 (not outer): latch_70 should update.
        mapper.write_prg(0x8000, 0x30); // bits 5:4 = 3 → PRG bank 3
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Regular write must update latch"
        );
        // Now write to an address that DOES trigger outer register.
        mapper.write_prg(0xE100, 0x02); // A16=1
        // Outer register updated; now check mapper70 mode with A16=1, latch=0x30
        // inner_prg = (0x30 >> 4) & 0x03 = 3, PRG $8000 = (1<<2)|3 = 7.
        assert_eq!(
            mapper.read_prg(0x8000),
            7,
            "Outer register write must update outer_reg without changing latch"
        );
    }

    // ── Reset ───────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE100, 0x02);
        mapper.write_prg(0x8000, 0x30);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "After reset, $C000 must be bank 3 (power-on state)"
        );
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }
}
