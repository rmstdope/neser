//! Mapper 189 – TXC / MMC3 variant with external PRG bank register
//!
//! Specifications:
//! - Primary source: NESdev Wiki <https://www.nesdev.org/wiki/INES_Mapper_189>
//! - Reference impl: Mesen2 `Core/NES/Mappers/Txc/MMC3_189.h`
//!
//! ## Overview
//!
//! Mapper 189 is a modified MMC3 used on the TXC board. Everything operates as on
//! the standard MMC3, except that the normal PRG bank registers (R6, R7) are ignored
//! and a new PRG register at `$4120–$7FFF` is used instead.
//!
//! ## Memory Map
//!
//! * `CPU $4120–$7FFF`: PRG bank register (write-only; no PRG-RAM at $6000–$7FFF)
//! * `CPU $8000–$FFFF`: standard MMC3 registers (CHR banking, IRQ, mirroring)
//! * `PPU $0000–$1FFF`: CHR-ROM via standard MMC3 CHR banking
//!
//! ## PRG Register (`$4120–$7FFF`)
//!
//! ```text
//! D~[AAAA BBBB]
//!    |+++ ++++
//!    +----+--- A and B nibbles are OR'd: bank = ((A | B) & 0x07) × 4
//! ```
//!
//! The result `bank` selects the start of a 32 KiB block. All four 8 KiB CPU
//! windows (`$8000`, `$A000`, `$C000`, `$E000`) are locked to pages
//! `bank+0`, `bank+1`, `bank+2`, `bank+3` respectively, overriding the standard
//! MMC3 R6/R7 PRG page selection.
//!
//! ## Power-on / Reset State
//!
//! - `prg_reg = 0`, so all four 8 KiB PRG pages map to block 0.
//! - MMC3 state is reset as normal (CHR banks 0, IRQ disabled, vertical mirroring).
//!
//! ## Known Limitations
//!
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperContext};
use crate::nes::cartridge::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 189;

/// Mapper 189 – TXC / MMC3 variant.
///
/// See the module-level documentation for hardware details.
pub struct Mapper189 {
    inner: MMC3Mapper,
    /// External PRG bank register, written to `$4120–$7FFF`.
    prg_reg: u8,
}

impl Mapper189 {
    pub fn new(ctx: MapperContext) -> Self {
        let inner = MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
            ctx.prg_rom,
            ctx.chr_rom,
            ctx.mirroring,
            false,
            0, // no PRG-RAM
        );
        Self { inner, prg_reg: 0 }
    }

    /// Compute the base 8KB page number from `prg_reg`.
    ///
    /// Low nibble and high nibble are OR'd; the result (3 bits) × 4 = first page
    /// of the 32KB block.
    fn prg_base_page(&self) -> usize {
        (((self.prg_reg | (self.prg_reg >> 4)) & 0x07) as usize) * 4
    }
}

impl Mapper for Mapper189 {
    fn base(&self) -> &BaseMapper {
        self.inner.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.inner.base_mut()
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn capabilities(&self) -> crate::nes::cartridge::mapper::MapperCapabilities {
        self.inner.capabilities()
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.inner)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.inner)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x8000..=0xFFFF => {
                let slot = ((addr - 0x8000) / 0x2000) as usize; // 0..3
                let bank = self.prg_base_page() + slot;
                let offset = (addr & 0x1FFF) as usize;
                self.inner.read_prg_at_bank(bank, offset)
            }
            _ => self.inner.read_prg(addr),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x4120..=0x7FFF).contains(&addr) {
            self.prg_reg = value;
        } else {
            self.inner.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        self.inner.read_chr(ppu_addr)
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        self.inner.write_chr(ppu_addr, value)
    }

    fn irq_pending(&self) -> bool {
        self.inner.irq_pending()
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.inner.registers_snapshot();
        snap.push(self.prg_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 17 {
            let (mmc3_data, ext) = data.split_at(data.len() - 1);
            self.inner.restore_registers(mmc3_data);
            self.prg_reg = ext[0];
        } else {
            // Legacy: MMC3-only snapshot
            self.inner.restore_registers(data);
            self.prg_reg = 0;
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.prg_reg = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::create_mapper;
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_8K: usize = 0x2000;
    const CHR_1K: usize = 0x0400;

    // 64KB PRG (8 × 8KB pages), 8KB CHR (8 × 1KB pages)
    const PRG_BANKS: usize = 8;
    const CHR_BANKS: usize = 8;

    fn make_mapper() -> Mapper189 {
        Mapper189::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_8K, PRG_BANKS),
            banked_data(CHR_1K, CHR_BANKS),
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_189_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_8K, PRG_BANKS),
            banked_data(CHR_1K, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 189 must be creatable via factory");
    }

    #[test]
    fn power_on_maps_all_windows_to_block_0() {
        let mapper = make_mapper();
        // prg_reg=0 → base_page=0 → pages 0,1,2,3
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should be page 0");
        assert_eq!(mapper.read_prg(0xA000), 1, "$A000 should be page 1");
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 should be page 2");
        assert_eq!(mapper.read_prg(0xE000), 3, "$E000 should be page 3");
    }

    #[test]
    fn write_4120_selects_prg_block() {
        let mut mapper = make_mapper();
        // prg_reg=0x10 → (0x10 | 0x01) & 0x07 = 1 → base=4
        mapper.write_prg(0x4120, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should be page 4");
        assert_eq!(mapper.read_prg(0xA000), 5, "$A000 should be page 5");
        assert_eq!(mapper.read_prg(0xC000), 6, "$C000 should be page 6");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 should be page 7");
    }

    #[test]
    fn nibbles_are_ored_for_bank_select() {
        let mut mapper = make_mapper();
        // 0x30 → (0x30 | 0x03) & 0x07 = 0x33 & 0x07 = 3 → base=12; wraps on 8 banks → 12%8=4
        mapper.write_prg(0x4120, 0x30);
        let base = mapper.prg_base_page();
        assert_eq!(base, 12, "base page must be ((0x3 | 0x3) & 7) * 4 = 12");
        // Read wraps: 12%8=4, 13%8=5, 14%8=6, 15%8=7
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xA000), 5);
    }

    #[test]
    fn write_7fff_also_sets_prg_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0x10);
        assert_eq!(mapper.prg_reg, 0x10);
    }

    #[test]
    fn write_below_4120_does_not_set_prg_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x411F, 0x10); // just below range
        assert_eq!(mapper.prg_reg, 0);
    }

    #[test]
    fn reset_clears_prg_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4120, 0x10);
        mapper.reset();
        assert_eq!(mapper.prg_reg, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn snapshot_restore_preserves_prg_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4120, 0x21);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(restored.prg_reg, 0x21);
    }

    #[test]
    fn mmc3_chr_banking_still_works() {
        let mut mapper = make_mapper();
        // Write CHR bank select via MMC3 using R2 (1KB at PPU $1000): $8000=bank_select=2, $8001=3.
        // R2 is a 1KB selector (no bit masking), so PPU $1000 maps to 1KB bank 3 → fill byte 3.
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 0x03);
        assert_eq!(
            mapper.read_chr(0x1000),
            3,
            "CHR banking via MMC3 must still work"
        );
    }

    #[test]
    fn irq_is_disabled_at_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }
}
