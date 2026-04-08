//! Mapper 306 – Kaiser KS7016
//!
//! Specifications:
//! - Primary: NesDev wiki (no dedicated page found; HTTP 403 or missing).
//! - Fallback: Mesen2 `Core/NES/Mappers/Kaiser/Kaiser7016.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Kaiser/Kaiser7016.h>
//!
//! # Hardware overview
//!
//! Used on Kaiser KS7016 PCBs.
//!
//! - PRG-ROM: 128 KiB (16 × 8 KiB banks).
//!   `$8000–$FFFF` (4 × 8 KiB) is fixed to the last 4 banks at power-on
//!   (banks 0x0C, 0x0D, 0x0E, 0x0F).
//!   `$6000–$7FFF` (8 KiB) is switchable; controlled by the `prg_reg` field,
//!   which starts at bank 8 (0x08) at power-on.
//! - CHR-ROM: 8 KiB, fixed bank 0.
//! - Mirroring: controlled by ROM header (not overridden by mapper hardware).
//! - IRQ: none.
//! - PRG-RAM: none.
//! - Bus conflicts: none.
//!
//! # Register encoding
//!
//! Writes to addresses in `$8000–$FFFF` whose value matches the patterns
//! below update `prg_reg`; other writes in this range have no effect. The
//! register value is encoded in the *address*, not the data byte.
//!
//! ```text
//! mode  = (addr & 0x30) == 0x30
//!
//! addr & 0xD943 == 0xD943:
//!     if mode  → prg_reg = 0x0B
//!     else     → prg_reg = (addr >> 2) & 0x0F
//!
//! addr & 0xD943 == 0xD903:
//!     if mode  → prg_reg = 0x08 | ((addr >> 2) & 0x03)
//!     else     → prg_reg = 0x0B
//! ```
//!
//! # Memory map
//!
//! ```text
//! CPU $6000–$7FFF  PRG-ROM 8 KiB, bank = prg_reg  (switchable)
//! CPU $8000–$9FFF  PRG-ROM 8 KiB, bank 0x0C (fixed)
//! CPU $A000–$BFFF  PRG-ROM 8 KiB, bank 0x0D (fixed)
//! CPU $C000–$DFFF  PRG-ROM 8 KiB, bank 0x0E (fixed)
//! CPU $E000–$FFFF  PRG-ROM 8 KiB, bank 0x0F (fixed)
//! PPU $0000–$1FFF  CHR 8 KiB, bank 0 (fixed)
//! ```
//!
//! # Power-on / reset state
//!
//! - `prg_reg = 8` (bank 0x08 at `$6000–$7FFF`).
//! - `$8000–$FFFF`: fixed to banks 0x0C, 0x0D, 0x0E, 0x0F.
//! - CHR: bank 0 (fixed).
//! - Mirroring: from ROM header.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 306;

/// Mapper 306 – Kaiser KS7016
pub struct Mapper306 {
    base: BaseMapper,
    /// Current 8 KiB PRG-ROM bank mapped at `$6000–$7FFF`.
    prg_reg: u8,
}

impl Mapper306 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024); // 8 KiB pages for $8000–$FFFF
        base.configure_chr_banking(8 * 1024); // 8 KiB single CHR page
        base.configure_prg_6000_banking();

        let mut mapper = Self { base, prg_reg: 8 };
        mapper.apply_state();
        mapper
    }

    fn apply_state(&mut self) {
        // $8000–$FFFF: four fixed 8 KiB slots → banks 0x0C–0x0F
        self.base.select_prg_page(0, 0x0C);
        self.base.select_prg_page(1, 0x0D);
        self.base.select_prg_page(2, 0x0E);
        self.base.select_prg_page(3, 0x0F);
        // $6000–$7FFF: switchable bank
        self.base.select_prg_6000_page(self.prg_reg as i16);
        // CHR fixed to bank 0
        self.base.select_chr_page(0, 0);
    }

    fn decode_write(&mut self, addr: u16) {
        let mode = (addr & 0x30) == 0x30;
        match addr & 0xD943 {
            0xD943 => {
                self.prg_reg = if mode {
                    0x0B
                } else {
                    ((addr >> 2) & 0x0F) as u8
                };
                self.base.select_prg_6000_page(self.prg_reg as i16);
            }
            0xD903 => {
                self.prg_reg = if mode {
                    0x08 | (((addr >> 2) & 0x03) as u8)
                } else {
                    0x0B
                };
                self.base.select_prg_6000_page(self.prg_reg as i16);
            }
            _ => {}
        }
    }
}

impl Mapper for Mapper306 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.try_read_prg_6000(addr).unwrap_or(0),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.try_read_prg_6000(addr).unwrap_or(open_bus),
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
            self.decode_write(addr);
        }
    }

    fn reset(&mut self) {
        self.prg_reg = 8;
        self.apply_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&reg) = data.first() {
            self.prg_reg = reg;
            self.base.select_prg_6000_page(self.prg_reg as i16);
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

    fn create_mapper306(prg_rom: Vec<u8>) -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            306,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ))
        .expect("mapper 306 should be implemented")
    }

    // ── Registration ─────────────────────────────────────────────────────────

    #[test]
    fn mapper_306_is_registered() {
        let prg_rom = banked_data(8192, 16);
        let mapper = create_mapper306(prg_rom);
        assert_eq!(mapper.mapper_number(), 306);
    }

    // ── Power-on: $8000–$FFFF fixed to last 4 banks ──────────────────────────

    /// At power-on, $8000–$FFFF maps to banks 0x0C–0x0F (last 32 KiB of a
    /// 128 KiB PRG-ROM).  Each bank is filled with its index byte.
    #[test]
    fn power_on_8000_ffff_fixed_to_last_four_banks() {
        let prg_rom = banked_data(8192, 16);
        let mapper = create_mapper306(prg_rom);

        // Slot 0: $8000–$9FFF → bank 0x0C (12)
        assert_eq!(mapper.read_prg(0x8000), 12, "$8000 should read bank 12");
        // Slot 1: $A000–$BFFF → bank 0x0D (13)
        assert_eq!(mapper.read_prg(0xA000), 13, "$A000 should read bank 13");
        // Slot 2: $C000–$DFFF → bank 0x0E (14)
        assert_eq!(mapper.read_prg(0xC000), 14, "$C000 should read bank 14");
        // Slot 3: $E000–$FFFF → bank 0x0F (15)
        assert_eq!(mapper.read_prg(0xE000), 15, "$E000 should read bank 15");
        assert_eq!(mapper.read_prg(0xFFFF), 15, "$FFFF should read bank 15");
    }

    // ── Power-on: $6000–$7FFF defaults to bank 8 ─────────────────────────────

    #[test]
    fn power_on_6000_7fff_reads_bank_8() {
        let prg_rom = banked_data(8192, 16);
        let mapper = create_mapper306(prg_rom);

        assert_eq!(mapper.read_prg(0x6000), 8, "$6000 should read bank 8");
        assert_eq!(mapper.read_prg(0x7FFF), 8, "$7FFF should read bank 8");
    }

    // ── $8000–$FFFF stays fixed after register writes ─────────────────────────

    #[test]
    fn fixed_banks_unchanged_after_write() {
        let prg_rom = banked_data(8192, 16);
        let mut mapper = create_mapper306(prg_rom);

        // Trigger a write that changes the $6000 bank
        mapper.write_prg(0xD943, 0);

        // Fixed banks should remain unchanged
        assert_eq!(mapper.read_prg(0x8000), 12, "$8000 must remain bank 12");
        assert_eq!(mapper.read_prg(0xE000), 15, "$E000 must remain bank 15");
    }

    // ── Register write: addr & 0xD943 == 0xD943, mode=false ─────────────────

    /// When `addr & 0xD943 == 0xD943` and `(addr & 0x30) != 0x30`,
    /// `prg_reg = (addr >> 2) & 0x0F`.
    #[test]
    fn write_d943_no_mode_selects_addr_bits() {
        let prg_rom = banked_data(8192, 16);
        let mut mapper = create_mapper306(prg_rom);

        // addr = 0xD943, mode = (0xD943 & 0x30) == 0x30 → 0x00 == 0x30 → false
        // prg_reg = (0xD943 >> 2) & 0x0F = 0x3650 & 0x0F = 0 ... wait
        // Let me compute: 0xD943 >> 2 = 0x3650, & 0x0F = 0
        // So prg_reg = 0
        mapper.write_prg(0xD943, 0xFF);
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "write addr=0xD943 (no mode) → prg_reg=0"
        );
    }

    /// When `addr & 0xD943 == 0xD943` and `(addr & 0x30) == 0x30` (mode=true),
    /// `prg_reg = 0x0B`.
    #[test]
    fn write_d943_mode_true_sets_prg_reg_0x0b() {
        let prg_rom = banked_data(8192, 16);
        let mut mapper = create_mapper306(prg_rom);

        // Need addr where (addr & 0xD943 == 0xD943) AND (addr & 0x30 == 0x30)
        // addr must have bits 0xD943 | 0x30 = 0xD973 set.
        // 0xD973 & 0xD943 = 0xD943 ✓ and 0xD973 & 0x30 = 0x30 ✓
        mapper.write_prg(0xD973, 0);
        assert_eq!(
            mapper.read_prg(0x6000),
            0x0B,
            "write addr=0xD973 (mode) → prg_reg=0x0B (bank 11)"
        );
    }

    // ── Register write: addr & 0xD943 == 0xD903, mode=false ─────────────────

    /// When `addr & 0xD943 == 0xD903` and mode=false, `prg_reg = 0x0B`.
    #[test]
    fn write_d903_no_mode_sets_prg_reg_0x0b() {
        let prg_rom = banked_data(8192, 16);
        let mut mapper = create_mapper306(prg_rom);

        // Need addr where (addr & 0xD943 == 0xD903):
        // 0xD903 & 0xD943 = 0xD903 means bit 6 (0x40) is NOT set in addr,
        // all other bits of 0xD943 are set.
        // addr = 0xD903, mode = (0xD903 & 0x30) == 0x30 → 0x00 == 0x30 → false
        mapper.write_prg(0xD903, 0);
        assert_eq!(
            mapper.read_prg(0x6000),
            0x0B,
            "write addr=0xD903 (no mode) → prg_reg=0x0B"
        );
    }

    /// When `addr & 0xD943 == 0xD903` and mode=true,
    /// `prg_reg = 0x08 | ((addr >> 2) & 0x03)`.
    #[test]
    fn write_d903_mode_true_selects_upper_banks() {
        let prg_rom = banked_data(8192, 16);
        let mut mapper = create_mapper306(prg_rom);

        // Need addr where (addr & 0xD943 == 0xD903) AND (addr & 0x30 == 0x30)
        // addr must have 0xD903 bits set, NOT bit 0x40, AND bits 0x30 set.
        // addr = 0xD933: & 0xD943 = 0xD903 ✓, & 0x30 = 0x30 ✓
        // prg_reg = 0x08 | ((0xD933 >> 2) & 0x03)
        //         = 0x08 | (0x364C & 0x03)
        //         = 0x08 | 0x00 = 0x08
        mapper.write_prg(0xD933, 0);
        assert_eq!(
            mapper.read_prg(0x6000),
            0x08,
            "write addr=0xD933 (mode) → prg_reg=0x08"
        );
    }

    // ── Write with no matching case has no effect ─────────────────────────────

    #[test]
    fn write_unmatched_address_has_no_effect() {
        let prg_rom = banked_data(8192, 16);
        let mut mapper = create_mapper306(prg_rom);

        // addr=0x8000: 0x8000 & 0xD943 = 0x8000, which matches neither 0xD943 nor 0xD903
        mapper.write_prg(0x8000, 0);
        // prg_reg should still be 8 (power-on default)
        assert_eq!(
            mapper.read_prg(0x6000),
            8,
            "unmatched write should not change prg_reg"
        );
    }

    // ── Reset ────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let prg_rom = banked_data(8192, 16);
        let mut mapper = create_mapper306(prg_rom);

        // Change state via a mode write
        mapper.write_prg(0xD973, 0); // prg_reg = 0x0B
        assert_eq!(mapper.read_prg(0x6000), 0x0B);

        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x6000),
            8,
            "after reset, $6000 should read bank 8"
        );
        assert_eq!(
            mapper.read_prg(0x8000),
            12,
            "after reset, $8000 should read bank 12"
        );
    }

    // ── Save-state snapshot / restore ────────────────────────────────────────

    #[test]
    fn snapshot_and_restore_preserves_prg_reg() {
        let prg_rom = banked_data(8192, 16);
        let mut mapper = create_mapper306(prg_rom);

        // Set prg_reg to 0x0B
        mapper.write_prg(0xD973, 0);
        assert_eq!(mapper.read_prg(0x6000), 0x0B);

        let snapshot = mapper.registers_snapshot();

        // Change to a different bank
        mapper.write_prg(0xD943, 0xFF);
        assert_eq!(mapper.read_prg(0x6000), 0);

        mapper.restore_registers(&snapshot);

        assert_eq!(
            mapper.read_prg(0x6000),
            0x0B,
            "restored prg_reg should be 0x0B"
        );
    }

    // ── CHR bank ─────────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_is_fixed_at_zero() {
        let prg_rom = banked_data(8192, 16);
        let chr_rom: Vec<u8> = (0..8192u16).map(|i| (i & 0xFF) as u8).collect();
        let ctx = MapperContext::new_for_test(306, prg_rom, chr_rom, NametableLayout::Vertical);
        let mut mapper = create_mapper(ctx).expect("mapper 306 should be implemented");

        // First byte of CHR-ROM bank 0
        assert_eq!(mapper.read_chr(0x0000), 0x00);
        assert_eq!(mapper.read_chr(0x1000), 0x00);
        // Read near end of CHR window
        assert_eq!(mapper.read_chr(0x1FFF), 0xFF);
    }

    // ── Mirroring from header ─────────────────────────────────────────────────

    #[test]
    fn mirroring_comes_from_header() {
        let prg_rom = banked_data(8192, 16);
        let m_vert = create_mapper(MapperContext::new_for_test(
            306,
            prg_rom.clone(),
            vec![],
            NametableLayout::Vertical,
        ))
        .expect("mapper 306 should be implemented");
        let m_horiz = create_mapper(MapperContext::new_for_test(
            306,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ))
        .expect("mapper 306 should be implemented");

        assert_eq!(
            m_vert.base().mirroring(),
            NametableLayout::Vertical,
            "Vertical header should yield Vertical mirroring"
        );
        assert_eq!(
            m_horiz.base().mirroring(),
            NametableLayout::Horizontal,
            "Horizontal header should yield Horizontal mirroring"
        );
    }
}
