//! Mapper 134 — MMC3-clone multicart (T4A54A / WX-KB4K / BS-5652)
//!
//! Specifications:
//! - NESdev wiki: <https://www.nesdev.org/wiki/INES_Mapper_134>
//!
//! This mapper wraps the standard MMC3 with an extra outer-bank register at
//! `$6001` (address mask `$E003`) that extends the addressable PRG and CHR
//! ROM beyond the 256 KiB / 256 KiB limit of the base MMC3:
//!
//! | Bit  | Function                                       |
//! |------|------------------------------------------------|
//! | `1`  | PRG outer bank bit — shifts PRG bank by 32 (×256 KiB) |
//! | `5`  | CHR outer bank bit — shifts CHR 1 KiB bank by 256 (×256 KiB) |
//!
//! All standard MMC3 registers (`$8000-$FFFF`, mask `$E001`) are fully
//! functional.  The MMC3-compatible scanline IRQ is inherited unchanged.
//!
//! The NESdev wiki also documents a Mode register (`$6000`) with CNROM, NROM,
//! register-lock, and solder-pad bits, and additional outer bits for A18/A19.
//! Those modes are not implemented here; only the outer-bank bits of `$6001`
//! needed by the known ROM files are supported.
//!
//! Known limitations:
//! - CNROM CHR banking mode (`$6000` bit 2) is not implemented.
//! - NROM PRG mode (`$6001` bit 7) is not implemented.
//! - Register lock (`$6000` bit 7) is not implemented.
//! - PRG A19 / CHR A19 (`$6000` bits 4/5) are not implemented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::nes::cartridge::nintendo::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 134;

/// Mapper 134 — MMC3 multicart with one outer-bank register at `$6001`.
pub struct Mapper134 {
    mmc3: MMC3Mapper,
    /// Outer-bank register (written at `$6001`, mask `$E003`).
    outer: u8,
}

impl Mapper134 {
    const PRG_BANK_MASK: usize = 0x2000 - 1; // 8 KiB
    const CHR_1K_BANK_MASK: usize = 0x0400 - 1; // 1 KiB

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            outer: 0,
        }
    }

    /// Adjust an MMC3 inner 8 KiB PRG bank index with the outer bank bit.
    ///
    /// `outer` bit 1 shifts the bank by 32 (i.e. selects the second 256 KiB
    /// PRG block), matching Mesen's `MMC3_134` implementation.
    fn prg_bank_adjusted(&self, inner: usize) -> usize {
        let outer_prg = ((self.outer & 0x02) as usize) << 4; // bit 1 → bit 5
        (inner & 0x1F) | outer_prg
    }

    /// Adjust an MMC3 inner 1 KiB CHR bank index with the outer bank bit.
    ///
    /// `outer` bit 5 shifts the bank by 256 (i.e. selects the second 256 KiB
    /// CHR block), matching Mesen's `MMC3_134` implementation.
    fn chr_bank_adjusted(&self, inner: usize) -> usize {
        let outer_chr = ((self.outer & 0x20) as usize) << 3; // bit 5 → bit 8
        (inner & 0xFF) | outer_chr
    }
}

impl Mapper for Mapper134 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let raw = self.mmc3.mapped_prg_bank(addr);
        let bank = self.prg_bank_adjusted(raw);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if (0x8000..=0xFFFF).contains(&addr) {
            return self.read_prg(addr);
        }
        open_bus
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (addr & 0xE003) == 0x6001 {
            self.outer = value;
        } else {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let raw = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.chr_bank_adjusted(raw);
        let offset = (addr as usize) & Self::CHR_1K_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let raw = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.chr_bank_adjusted(raw);
        let offset = (addr as usize) & Self::CHR_1K_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.outer);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&outer, mmc3_data)) = data.split_last() {
            self.outer = outer;
            self.mmc3.restore_registers(mmc3_data);
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        self.mmc3.ppu_address_changed(addr);
    }

    fn irq_pending(&self) -> bool {
        self.mmc3.irq_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 512 KiB PRG (64 × 8 KiB) and 512 KiB CHR (512 × 1 KiB) to exercise
    // both the inner (256 KiB) and outer (×2) bank ranges.
    const PRG_BANKS_8K: usize = 64;

    fn make_mapper() -> Mapper134 {
        Mapper134::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS_8K),
            chr_two_blocks(),
            NametableLayout::Vertical,
        ))
    }

    /// 512 × 1KiB CHR-ROM split into two distinct 256KiB blocks.
    /// First block (banks 0-255): each bank starts with 0x00.
    /// Second block (banks 256-511): each bank starts with 0x42.
    /// This lets tests distinguish accesses to the two outer-bank regions
    /// even though bank 256 % 256 == 0 (which would cause a false-pass with
    /// `banked_data`).
    fn chr_two_blocks() -> Vec<u8> {
        let bank_size = 1024usize;
        let mut chr = vec![0u8; 512 * bank_size];
        for bank in 256..512 {
            chr[bank * bank_size] = 0x42;
        }
        chr
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_134_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS_8K),
            chr_two_blocks(),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 134 must be creatable via factory");
    }

    // ── MMC3 inner PRG banking (without outer bit) ────────────────────────────

    #[test]
    fn mmc3_prg_banking_works_in_first_block() {
        let mut mapper = make_mapper();
        // MMC3 bank register 6 → $8000-$9FFF, bank register 7 → $A000-$BFFF
        // Write 5 to bank slot 6 ($8000 window)
        mapper.write_prg(0x8000, 0x86); // command: select register 6
        mapper.write_prg(0x8001, 5); // bank value = 5
        assert_eq!(mapper.read_prg(0x8000), 5, "inner PRG bank 5 at $8000");
    }

    // ── Outer PRG bank bit ────────────────────────────────────────────────────

    #[test]
    fn outer_reg_bit1_shifts_prg_to_second_256k_block() {
        let mut mapper = make_mapper();
        // Select inner PRG bank 0 for $8000 window
        mapper.write_prg(0x8000, 0x86);
        mapper.write_prg(0x8001, 0);
        assert_eq!(mapper.read_prg(0x8000), 0, "inner bank 0 → PRG bank 0");

        // Set outer bit 1: bank becomes 0 | (1 << 5) = 32
        mapper.write_prg(0x6001, 0x02);
        assert_eq!(
            mapper.read_prg(0x8000),
            32,
            "outer bit 1 shifts PRG bank by 32"
        );
    }

    #[test]
    fn outer_prg_bit_combines_with_inner_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x86);
        mapper.write_prg(0x8001, 3); // inner bank 3
        mapper.write_prg(0x6001, 0x02); // outer bit 1 set → shift 32
        assert_eq!(mapper.read_prg(0x8000), 35, "inner 3 + outer 32 = bank 35");
    }

    // ── Outer CHR bank bit ────────────────────────────────────────────────────

    #[test]
    fn outer_reg_bit5_shifts_chr_to_second_256k_block() {
        let mut mapper = make_mapper();
        // MMC3 CHR register 0 → 2 KiB at PPU $0000; default inner bank = 0
        mapper.write_prg(0x8000, 0x80); // command: select CHR register 0
        mapper.write_prg(0x8001, 0); // inner CHR bank 0 (maps 2 × 1 KiB)
        assert_eq!(
            mapper.read_chr(0x0000),
            0x00,
            "inner CHR bank 0 → first block byte"
        );

        // Set outer bit 5: adjusted bank = 0 | 256 = 256 (second block → 0x42)
        mapper.write_prg(0x6001, 0x20);
        assert_eq!(
            mapper.read_chr(0x0000),
            0x42,
            "outer bit 5 shifts CHR to second block"
        );
    }

    // ── Outer register address mask $E003 ─────────────────────────────────────

    #[test]
    fn write_to_6001_activates_outer_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x86);
        mapper.write_prg(0x8001, 0);

        mapper.write_prg(0x6001, 0x02); // outer PRG bit set
        assert_eq!(mapper.read_prg(0x8000), 32, "outer register at $6001");
    }

    #[test]
    fn write_to_6000_does_not_change_outer_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x86);
        mapper.write_prg(0x8001, 0);
        mapper.write_prg(0x6001, 0x02); // outer PRG bit
        mapper.write_prg(0x6000, 0xFF); // mode register — ignored in this impl
        // outer register should still have 0x02
        assert_eq!(
            mapper.read_prg(0x8000),
            32,
            "$6000 write must not touch outer register"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_outer_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x86);
        mapper.write_prg(0x8001, 0);
        mapper.write_prg(0x6001, 0x02);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(
            restored.read_prg(0x8000),
            32,
            "outer register must be restored"
        );
    }

    // ── IRQ inherited from MMC3 ───────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_startup() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "no IRQ at power-on");
    }
}
