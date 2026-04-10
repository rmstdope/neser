//! Mapper 267 – 8-in-1 JY-119 (NES 2.0)
//!
//! ## Specification
//!
//! Primary source: NesDev Wiki – NES 2.0 Mapper 267
//! (<https://www.nesdev.org/wiki/NES_2.0_Mapper_267>)
//! Backup source:
//! <https://nesdev-wiki.nes.science/wikipages/NES_2_0_Mapper_267.xhtml>
//! Supplemental fallback: libretro-FCEUmm `src/boards/267.c`
//!
//! Used by the "8-in-1 JY-119" multicart board.
//!
//! The mapper is a standard MMC3 core with a single outer-bank register
//! written at CPU $6000–$7FFF.  The outer register selects one 256 KiB PRG /
//! 128 KiB CHR outer bank that is combined with the inner MMC3 bank number.
//!
//! ## Outer Bank Register ($6000 write, mask unknown)
//!
//! ```text
//! 7654 3210
//! ---- ----
//! UUB. .BB.
//! ||+---++-- Outer-bank bits (bits 5, 2, 1 of the byte)
//! ++-------- Unknown / ignored in emulation
//! ```
//!
//! * Reset / power-on value: `$00` (required for multicart menu to appear).
//! * Bit 7 is a lock flag: once set, further writes to this register are
//!   ignored (the lock does not release until a hard reset).
//! * No PRG-RAM is present at $6000–$7FFF; reads return open bus.
//!
//! ## PRG Banking
//!
//! Standard MMC3 inner banks, limited to 5 bits (256 KiB per game).
//!
//! ```text
//! outer_bank = ((reg & 0x20) >> 2) | (reg & 0x06)
//! prg_bank   = (outer_bank << 4) | (mmc3_inner & 0x1F)
//! ```
//!
//! ## CHR Banking
//!
//! Standard MMC3 inner banks, limited to 7 bits (128 KiB per game).
//!
//! ```text
//! chr_bank   = (outer_bank << 6) | (mmc3_inner & 0x7F)
//! ```
//!
//! ## Mirroring / IRQ
//! Standard MMC3 H/V mirroring ($A000) and scanline IRQ ($C000/$C001/$E000/$E001).
//!
//! ## Known limitations
//! The two "unknown" bits (6 and 7) are treated as not affecting banking,
//! consistent with both the NesDev note ("the cart seems to run just fine when
//! ignoring the two unknown bits") and the FCEUmm reference implementation.

use crate::nes::cartridge::Mapper;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::nintendo::mmc3::MMC3Mapper;

/// Mapper 267 – 8-in-1 JY-119
pub struct Mapper267 {
    mmc3: MMC3Mapper,
    /// The single outer-bank register, written at $6000–$7FFF.
    ex_reg: u8,
    /// Set by `initialize_ram` (hard reset); consumed by `reset()` to decide
    /// whether to clear `ex_reg`.  Soft resets must leave the lock bit intact.
    hard_reset_pending: bool,
}

impl Mapper267 {
    const MAPPER_NUMBER: u16 = 267;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode_and_prg_ram_banks(
                ctx.prg_rom,
                ctx.chr_rom,
                ctx.mirroring,
                false, // standard MMC3 IRQ
                0,     // no PRG-RAM; $6000–$7FFF is the outer-bank register
            ),
            ex_reg: 0,
            hard_reset_pending: false,
        }
    }

    /// Compute the outer-bank field from the external register.
    ///
    /// `outer_bank = ((reg & 0x20) >> 2) | (reg & 0x06)`
    fn outer_bank(&self) -> usize {
        ((self.ex_reg & 0x20) >> 2) as usize | (self.ex_reg & 0x06) as usize
    }

    /// Combine the MMC3 inner PRG bank with the outer field.
    ///
    /// Inner is limited to 5 bits (256 KiB per game).
    fn apply_prg_outer(&self, inner: usize) -> usize {
        (self.outer_bank() << 4) | (inner & 0x1F)
    }

    /// Combine the MMC3 inner CHR bank with the outer field.
    ///
    /// Inner is limited to 7 bits (128 KiB per game).
    fn apply_chr_outer(&self, inner: usize) -> usize {
        (self.outer_bank() << 6) | (inner & 0x7F)
    }

    /// Returns `true` when the lock bit (bit 7) is set; writes to the outer
    /// register must be suppressed.
    fn is_locked(&self) -> bool {
        self.ex_reg & 0x80 != 0
    }
}

impl Mapper for Mapper267 {
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

    fn read_prg(&self, addr: u16) -> u8 {
        // $6000–$7FFF: outer-bank register range – no PRG-RAM.
        // `read_prg` returns 0; open-bus callers should use `read_prg_open_bus`.
        if (0x6000..=0x7FFF).contains(&addr) {
            return 0;
        }
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let inner = self.mmc3.mapped_prg_bank(addr);
        let bank = self.apply_prg_outer(inner);
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        // $6000–$7FFF is unmapped (no PRG-RAM); return the CPU open-bus value.
        if (0x6000..=0x7FFF).contains(&addr) {
            return open_bus;
        }
        self.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            // Outer-bank register write; ignored if locked.
            if !self.is_locked() {
                self.ex_reg = value;
            }
        } else {
            // All other addresses forwarded to MMC3 (banking, mirroring, IRQ).
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let inner = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_outer(inner);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let inner = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_outer(inner);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        vec![]
    }

    fn load_wram_snapshot(&mut self, _data: &[u8]) {}

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.ex_reg);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let mmc3_snap_len = self.mmc3.registers_snapshot().len();
        if data.len() == mmc3_snap_len + 1 {
            // New-format snapshot: last byte is ex_reg, rest goes to MMC3.
            let (mmc3_part, ex_part) = data.split_at(data.len() - 1);
            self.mmc3.restore_registers(mmc3_part);
            self.ex_reg = ex_part[0];
        } else {
            // Older / truncated snapshot without ex_reg; pass it all to MMC3.
            self.mmc3.restore_registers(data);
            self.ex_reg = 0;
        }
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
        self.hard_reset_pending = true;
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        if self.hard_reset_pending {
            self.ex_reg = 0;
            self.hard_reset_pending = false;
        }
        // Soft reset: leave ex_reg (and the lock bit) intact.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-2 bank counts to avoid modulo-wrap false passes.
    const PRG_8K_BANKS: usize = 9; // 9 × 8 KiB = 72 KiB PRG-ROM
    const CHR_1K_BANKS: usize = 17; // 17 × 1 KiB = 17 KiB CHR-ROM

    fn make_mapper() -> Mapper267 {
        Mapper267::new(
            MapperContext::new_for_test(
                267,
                banked_data(8 * 1024, PRG_8K_BANKS),
                banked_data(1024, CHR_1K_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        )
    }

    // -------------------------------------------------------------------------
    // Factory registration
    // -------------------------------------------------------------------------

    #[test]
    fn mapper_267_is_registered_in_factory() {
        let result = create_mapper(
            MapperContext::new_for_test(
                267,
                banked_data(8 * 1024, PRG_8K_BANKS),
                banked_data(1024, CHR_1K_BANKS),
                NametableLayout::Horizontal,
            )
            .with_prg_ram_banks(0),
        );
        assert!(result.is_ok(), "Mapper 267 must be registered in factory");
    }

    // -------------------------------------------------------------------------
    // Power-on / reset state
    // -------------------------------------------------------------------------

    #[test]
    fn power_on_outer_reg_is_zero() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.ex_reg, 0,
            "outer-bank register must be 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_acts_like_mmc3() {
        let mut mapper = make_mapper();
        // Select R6 (PRG bank at $8000) and set it to inner page 3.
        mapper.write_prg(0x8000, 0x06); // bank_select → R6
        mapper.write_prg(0x8001, 3); // R6 = 3
        // With outer_bank=0: prg_bank = (0 << 4) | (3 & 0x1F) = 3
        // banked_data fills bank N with value N; 3 % 9 = 3
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG must use inner MMC3 bank directly when outer_bank = 0"
        );
    }

    #[test]
    fn reset_clears_outer_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x06); // set outer bits
        assert_ne!(mapper.ex_reg, 0);
        mapper.initialize_ram(crate::nes::console::RamInitMode::Zero);
        mapper.reset();
        assert_eq!(
            mapper.ex_reg, 0,
            "outer-bank register must be 0 after hard reset"
        );
    }

    #[test]
    fn soft_reset_does_not_clear_outer_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x06); // set outer bits
        assert_ne!(mapper.ex_reg, 0);
        mapper.reset(); // soft reset only
        assert_eq!(
            mapper.ex_reg, 0x06,
            "outer-bank register must be preserved across soft reset"
        );
    }

    // -------------------------------------------------------------------------
    // Outer-bank register writes
    // -------------------------------------------------------------------------

    #[test]
    fn write_to_6000_updates_outer_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x06);
        assert_eq!(mapper.ex_reg, 0x06);
    }

    #[test]
    fn write_to_7fff_updates_outer_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0x02);
        assert_eq!(mapper.ex_reg, 0x02);
    }

    #[test]
    fn read_from_6000_returns_open_bus_no_prg_ram() {
        let mapper = make_mapper();
        let open_bus = 0xA5;
        // No PRG-RAM; reads from $6000–$7FFF must preserve the CPU open-bus value.
        assert_eq!(mapper.read_prg_open_bus(0x6000, open_bus), open_bus);
    }

    // -------------------------------------------------------------------------
    // Lock bit (bit 7)
    // -------------------------------------------------------------------------

    #[test]
    fn lock_bit_prevents_further_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x86); // bit7=1 (lock) + outer bits 0x06
        assert_eq!(mapper.ex_reg, 0x86, "first write (lock+outer) must succeed");

        mapper.write_prg(0x6000, 0x02); // attempt to change register
        assert_eq!(mapper.ex_reg, 0x86, "write after lock must be ignored");
    }

    #[test]
    fn reset_clears_lock() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x86); // lock
        mapper.initialize_ram(crate::nes::console::RamInitMode::Zero);
        mapper.reset();
        mapper.write_prg(0x6000, 0x04); // should work now
        assert_eq!(mapper.ex_reg, 0x04, "write after hard reset must succeed");
    }

    // -------------------------------------------------------------------------
    // PRG banking – outer-bank field
    // -------------------------------------------------------------------------

    #[test]
    fn prg_outer_bank_from_bits_2_and_1() {
        // reg = 0x06: bits 2 and 1 set → outer_bank = ((0 >> 2) | 0x06) = 6
        // prg_bank = (6 << 4) | inner = 0x60 | inner
        // inner = 0 → bank = 0x60 = 96; 96 % 9 = 6
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x06);

        mapper.write_prg(0x8000, 0x06); // select R6
        mapper.write_prg(0x8001, 0); // inner = 0

        let expected = (0x60usize) % PRG_8K_BANKS; // 96 % 9 = 6
        assert_eq!(
            mapper.read_prg(0x8000) as usize,
            expected,
            "outer_bank from bits 2:1 must shift PRG bank correctly"
        );
    }

    #[test]
    fn prg_outer_bank_from_bit_5() {
        // reg = 0x20: bit 5 set → outer_bank = ((0x20 >> 2) | 0) = 8
        // prg_bank = (8 << 4) | 0 = 0x80 = 128; 128 % 9 = 2
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x20);

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0);

        let expected = 0x80usize % PRG_8K_BANKS; // 128 % 9 = 2
        assert_eq!(
            mapper.read_prg(0x8000) as usize,
            expected,
            "outer_bank from bit 5 must shift PRG bank correctly"
        );
    }

    #[test]
    fn prg_inner_and_outer_combine_correctly() {
        // reg = 0x02: outer_bank = 2; inner = 3
        // bank = (2 << 4) | 3 = 0x23 = 35; 35 % 9 = 8
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x02);

        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 3);

        let expected = 0x23usize % PRG_8K_BANKS; // 35 % 9 = 8
        assert_eq!(
            mapper.read_prg(0x8000) as usize,
            expected,
            "PRG outer and inner must combine correctly"
        );
    }

    #[test]
    fn prg_inner_not_polluted_by_outer_bank() {
        // With reg=0x02 (outer_bank=2) and inner=1:
        // bank = (2<<4)|1 = 0x21 = 33; 33 % 9 = 6
        // Then with reg=0x00 (outer_bank=0) and same inner:
        // bank = 0 | 1 = 1; 1 % 9 = 1
        // Verifies that outer bits don't bleed into inner.
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 1);

        mapper.write_prg(0x6000, 0x00);
        let bank_no_outer = mapper.read_prg(0x8000);

        mapper.write_prg(0x6000, 0x02);
        let bank_with_outer = mapper.read_prg(0x8000);

        assert_ne!(
            bank_no_outer, bank_with_outer,
            "changing outer bank must change PRG read result"
        );
        assert_eq!(
            bank_no_outer, 1,
            "without outer bank: bank 1 must read value 1"
        );
        assert_eq!(
            bank_with_outer as usize,
            0x21usize % PRG_8K_BANKS,
            "with outer bank 2, bank = 0x21 = 33; 33 % 9 = 6"
        );
    }

    // -------------------------------------------------------------------------
    // CHR banking – outer-bank field
    // -------------------------------------------------------------------------

    #[test]
    fn chr_power_on_acts_like_mmc3() {
        let mut mapper = make_mapper();
        // Select R0 (2-KiB CHR pair at $0000) = inner page 3
        mapper.write_prg(0x8000, 0x00); // bank_select → R0
        mapper.write_prg(0x8001, 4); // R0 = 4 (maps $0000–$07FF as pages 4,5)
        // outer_bank = 0 → chr_bank = 4 & 0x7F = 4; 4 % 17 = 4
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "CHR must use inner MMC3 bank directly when outer_bank = 0"
        );
    }

    #[test]
    fn chr_outer_bank_from_bit_1() {
        // reg = 0x02: outer_bank = 2 → chr_bank = (2 << 6) | inner = 0x80 | inner
        // inner = 0 → bank = 0x80 = 128; 128 % 17 = 9
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x02);
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0);

        let expected = 0x80usize % CHR_1K_BANKS; // 128 % 17 = 9
        assert_eq!(
            mapper.read_chr(0x0000) as usize,
            expected,
            "outer_bank from reg bit 1 must shift CHR bank correctly"
        );
    }

    #[test]
    fn chr_inner_masked_to_7_bits_with_outer() {
        // Verify outer bank bits don't overlap with inner: with outer_bank=2
        // and inner=0 (R0=0), bank = (2<<6)|0 = 128; 128%17=9.
        // With inner=0 and outer=0: bank=0; 0%17=0.
        // Confirms the outer and inner are OR-combined without overlap.
        let mut mapper = make_mapper();

        // outer=0: inner=0 → bank=0 → read 0
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 0);
        mapper.write_prg(0x6000, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 0);

        // outer=2: inner=0 → bank=128 → 128%17=9 → read 9
        mapper.write_prg(0x6000, 0x02);
        let expected = 0x80usize % CHR_1K_BANKS; // 128 % 17 = 9
        assert_eq!(
            mapper.read_chr(0x0000) as usize,
            expected,
            "outer bank must set upper CHR bank bits"
        );
    }

    #[test]
    fn chr_inner_and_outer_combine_correctly() {
        // reg = 0x04: outer_bank = ((0 >> 2) | 0x04) = 4
        // R0 = 5; MMC3 uses even-aligned 2KB banks: r0 = 5 & 0xFE = 4.
        // At $0000: bank_1k = r0 = 4; inner = 4 % 17 = 4
        // apply_chr_outer(4) = (4 << 6) | (4 & 0x7F) = 256 + 4 = 260
        // 260 % 17 = 260 - 15*17 = 5
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x04);
        mapper.write_prg(0x8000, 0x00);
        mapper.write_prg(0x8001, 5);

        // r0 = 5 & 0xFE = 4; inner = 4 % 17 = 4
        // bank = (4 << 6) | 4 = 260; 260 % 17 = 5
        let expected = ((4usize << 6) | 4) % CHR_1K_BANKS;
        assert_eq!(
            mapper.read_chr(0x0000) as usize,
            expected,
            "CHR outer and inner must combine correctly"
        );
    }

    // -------------------------------------------------------------------------
    // Save state
    // -------------------------------------------------------------------------

    #[test]
    fn registers_snapshot_includes_ex_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x06);
        let snap = mapper.registers_snapshot();
        assert_eq!(
            *snap.last().unwrap(),
            0x06,
            "snapshot must preserve ex_reg as last byte"
        );
    }

    #[test]
    fn restore_registers_restores_ex_reg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x04);
        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);
        assert_eq!(
            mapper2.ex_reg, 0x04,
            "restore_registers must restore ex_reg"
        );
    }
}
