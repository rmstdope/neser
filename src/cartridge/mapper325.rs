//! Mapper 325 – Mali Splash Bomb (MMC3 variant with scrambled addresses and banks)
//!
//! Specifications:
//! - NesDev wiki: No dedicated page (wiki returns 404 for INES Mapper 325 as of 2026).
//! - Mesen2 reference: `MMC3_MaliSB` (`Core/NES/Mappers/Mmc3Variants/MMC3_MaliSB.h`)
//!
//! # Hardware overview
//!
//! Mapper 325 is a clone of the MMC3 (mapper 4) used by the unlicensed game
//! "Mali Splash Bomb". It applies three kinds of hardware scrambling to prevent
//! direct MMC3-clone substitution:
//!
//! 1. **Write-address scrambling** – before the CPU write reaches the MMC3 logic,
//!    the hardware remaps address bit 0:
//!    - For `$8000–$BFFF`: new bit0 = old bit3
//!    - For `$C000–$FFFF`: new bit0 = old bit2 **OR** old bit3
//!
//! 2. **PRG bank scrambling** – after the game writes a PRG bank number, bits 2
//!    and 3 of that number are swapped before it is used to select a physical bank:
//!    `physical_page = (page & 0x03) | ((page & 0x08) >> 1) | ((page & 0x04) << 1)`
//!
//! 3. **CHR bank scrambling** – bits 1 and 5 of every 1 KiB CHR bank number are
//!    swapped:
//!    `physical_bank = (bank & 0xDD) | ((bank & 0x20) >> 4) | ((bank & 0x02) << 4)`
//!
//! All other MMC3 features (IRQ, mirroring, PRG-RAM at `$6000–$7FFF`) are
//! inherited unchanged.
//!
//! # Known limitations
//! - No known functional limitations beyond the scrambling differences documented
//!   above.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities, mapper::MapperContext};

/// Mapper 325 – Mali Splash Bomb MMC3 variant.
///
/// See module-level documentation for the full hardware description.
pub struct Mapper325 {
    pub(crate) mmc3: MMC3Mapper,
}

impl Mapper325 {
    const MAPPER_NUMBER: u16 = 325;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_BANK_SIZE - 1;

    pub fn new(ctx: MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
        }
    }

    /// Scramble the write address before delegating to the MMC3.
    ///
    /// The hardware replaces address bit 0:
    /// - `$8000–$BFFF`: new bit0 = old bit3
    /// - `$C000–$FFFF`: new bit0 = old bit2 | old bit3
    fn scramble_addr(addr: u16) -> u16 {
        if addr >= 0xC000 {
            (addr & 0xFFFE) | ((addr >> 2) & 0x01) | ((addr >> 3) & 0x01)
        } else {
            (addr & 0xFFFE) | ((addr >> 3) & 0x01)
        }
    }

    /// Scramble a raw PRG 8 KiB page number: swap bits 2 and 3.
    fn scramble_prg_page(page: u8) -> u8 {
        (page & 0x03) | ((page & 0x08) >> 1) | ((page & 0x04) << 1)
    }

    /// Scramble a raw CHR 1 KiB bank number: swap bits 1 and 5.
    fn scramble_chr_bank(bank: u8) -> u8 {
        (bank & 0xDD) | ((bank & 0x20) >> 4) | ((bank & 0x02) << 4)
    }
}

impl Mapper for Mapper325 {
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
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg(addr),
            0x8000..=0xFFFF => {
                let raw_page = self.mmc3.raw_prg_8k_page_number(addr);
                let scrambled = Self::scramble_prg_page(raw_page);
                let offset = (addr as usize) & Self::PRG_BANK_MASK;
                self.mmc3.read_prg_at_bank(scrambled as usize, offset)
            }
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.mmc3.read_prg_open_bus(addr, open_bus),
            0x8000..=0xFFFF => self.read_prg(addr),
            _ => open_bus,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.mmc3.write_prg(addr, value);
            return;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            self.mmc3.write_prg(Self::scramble_addr(addr), value);
        }
    }

    fn read_chr(&mut self, ppu_addr: u16) -> u8 {
        let raw_bank = self.mmc3.raw_chr_1k_bank(ppu_addr) as u8;
        let scrambled = Self::scramble_chr_bank(raw_bank);
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(scrambled as usize, offset)
    }

    fn write_chr(&mut self, ppu_addr: u16, value: u8) {
        let raw_bank = self.mmc3.raw_chr_1k_bank(ppu_addr) as u8;
        let scrambled = Self::scramble_chr_bank(raw_bank);
        let count = self.mmc3.chr_bank_count_1k();
        if count == 0 {
            return;
        }
        let bank = (scrambled as usize) % count;
        let offset = (ppu_addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        8 * 1024
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.mmc3.registers_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.mmc3.restore_registers(data);
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// 8 PRG 8 KiB banks = 64 KiB PRG (the known ROM size for Mali Splash Bomb).
    /// Non-power-of-2 would be 12; we stay at 8 since PRG scrambling of 0xFF/0xFE
    /// gives 0x0F/0x06, which only resolve to fixed-last/second-to-last for powers of 2
    /// up to 8.
    const PRG_BANKS: usize = 8;

    /// 64 CHR 1 KiB banks = 64 KiB CHR.
    const CHR_BANKS: usize = 64;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        create_mapper(MapperContext::new_for_test(
            325,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 325 must be registered")
    }

    /// Write the MMC3 bank-select register via the scrambled game address.
    ///
    /// The game writes $8000 (→ scrambled $8000, MMC3 bank-select even) and
    /// $8008 (→ scrambled $8009, MMC3 bank-data odd) to set a register.
    fn write_mmc3_reg(mapper: &mut Box<dyn Mapper>, reg: u8, value: u8) {
        // $8000 → bank_select address (bit0=0), $8008 → bank_data address (bit0=1 after scramble)
        mapper.write_prg(0x8000, reg);
        mapper.write_prg(0x8008, value);
    }

    // =========================================================================
    // Factory registration
    // =========================================================================

    #[test]
    fn mapper_325_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            325,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 325 must be registered in the factory"
        );
    }

    // =========================================================================
    // Address scrambling – write path
    // =========================================================================

    #[test]
    fn bank_select_reaches_mmc3_at_game_addr_8000() {
        // Given: mapper at power-on (PRG mode 0, R6=0 → $8000 maps to bank 0)
        let mut mapper = make_mapper();

        // When: game writes bank-select (0x06 = R6) to $8000 → scrambled $8000 (even=bank-select)
        //       then bank-data (value 5) to $8008 → scrambled $8009 (odd=bank-data)
        write_mmc3_reg(&mut mapper, 0x06, 5);

        // Then: $8000 now maps to scramble_prg(5)=9, 9%8=1 → bank 1 (value 1)
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$8000 must read from scrambled PRG bank (R6=5 → physical bank 1)"
        );
    }

    #[test]
    fn irq_latch_reaches_mmc3_at_game_addr_c000() {
        // Given: mapper at power-on
        let mut mapper = make_mapper();

        // When: game writes IRQ latch = 2 to $C000
        //       scramble($C000): bit2=0, bit3=0 → new bit0 = 0 → $C000 (MMC3 IRQ latch)
        mapper.write_prg(0xC000, 2);

        // Then: trigger reload and enable so we can observe the IRQ fires after 2 edges
        // IRQ reload: $C004 → scrambled: bit2=1,bit3=0 → new bit0=1 → $C005 (MMC3 reload)
        mapper.write_prg(0xC004, 0);
        // IRQ enable: $E008 → scrambled: bit2=0,bit3=1 → new bit0=1 → $E009 (MMC3 enable)
        mapper.write_prg(0xE008, 0);

        // Fire 3 A12 rising edges (latch=2: edge1 loads counter=2, edge2→1, edge3→0 → fires)
        for _ in 0..3 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }

        assert!(
            mapper.irq_pending(),
            "IRQ must fire after latch=2 and 2 A12 rising edges"
        );
    }

    // =========================================================================
    // PRG bank scrambling
    // =========================================================================

    #[test]
    fn prg_reg_r6_selects_scrambled_bank() {
        // Given: 8-bank PRG, bank N = all bytes N
        let mut mapper = make_mapper();

        // R6=5: scramble_prg(5) = (5&3)|((5&8)>>1)|((5&4)<<1) = 1|0|8 = 9, 9%8=1
        write_mmc3_reg(&mut mapper, 0x06, 5);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "R6=5 → physical bank 1 (value 1)"
        );
    }

    #[test]
    fn prg_reg_r6_unscrambled_would_differ() {
        // Verify the test above is meaningful: without scrambling, R6=5 → bank 5 (value 5).
        // This test asserts the scrambled result (1) is different from the unscrambled (5).
        let mut mapper = make_mapper();
        write_mmc3_reg(&mut mapper, 0x06, 5);
        let actual = mapper.read_prg(0x8000);
        assert_ne!(
            actual, 5,
            "Without scrambling R6=5 would yield bank 5 – scrambling must change the result"
        );
        assert_eq!(actual, 1, "Scrambled: R6=5 → bank 1");
    }

    #[test]
    fn prg_reg_r7_selects_scrambled_bank_at_a000() {
        // R7=4: scramble_prg(4)=(4&3)|((4&8)>>1)|((4&4)<<1)=0|0|8=8, 8%8=0
        let mut mapper = make_mapper();
        write_mmc3_reg(&mut mapper, 0x07, 4);
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "R7=4 → physical bank 0 (value 0)"
        );
    }

    #[test]
    fn prg_swap_bits2_and_3() {
        // R6=4 (bit2=1, bit3=0) ↔ R6=8 (bit2=0, bit3=1) → after swap: 8%8=0 vs 4%8=4
        let mut mapper = make_mapper();
        write_mmc3_reg(&mut mapper, 0x06, 4);
        assert_eq!(
            mapper.read_prg(0x8000),
            0, // scramble_prg(4)=8, 8%8=0
            "R6=4 (bit2) → scramble swaps to bit3 → bank 8%8=0"
        );
        write_mmc3_reg(&mut mapper, 0x06, 8);
        assert_eq!(
            mapper.read_prg(0x8000),
            4, // scramble_prg(8)=4, 4%8=4
            "R6=8 (bit3) → scramble swaps to bit2 → bank 4"
        );
    }

    // =========================================================================
    // Fixed banks
    // =========================================================================

    #[test]
    fn power_on_fixed_last_bank_at_e000() {
        // PRG mode 0 (default): $E000 = fixed last → scramble_prg(0xFF)=0x0F, 0x0F%8=7
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            7,
            "$E000 must read from the last PRG bank (7)"
        );
    }

    #[test]
    fn power_on_fixed_second_to_last_bank_at_c000() {
        // PRG mode 0: $C000 = fixed second-to-last → scramble_prg(0xFE)=0x06, 0x06%8=6
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "$C000 must read from the second-to-last PRG bank (6)"
        );
    }

    // =========================================================================
    // CHR bank scrambling
    // =========================================================================

    #[test]
    fn chr_reg_r2_selects_scrambled_bank() {
        // Given: 64-bank CHR, bank N = all bytes N
        let mut mapper = make_mapper();

        // Set CHR mode 0 (default), R2 controls PPU $1000 in mode 0.
        // Write R2=0x02: game addr $8000 (bank-select=0x02 → R2), $8008 (data=0x02)
        write_mmc3_reg(&mut mapper, 0x02, 0x02);

        // scramble_chr(0x02): (0x02&0xDD)|(0x02&0x20)>>4|(0x02&0x02)<<4
        //   = 0x00 | 0 | 0x20 = 0x20 = 32
        assert_eq!(
            mapper.read_chr(0x1000),
            32,
            "R2=0x02 → CHR bank 32 (scramble: bit1→bit5 → 0x20)"
        );
    }

    #[test]
    fn chr_reg_r2_unscrambled_would_differ() {
        // Without scrambling, R2=0x02 → CHR bank 2 (value 2).
        // With scrambling, R2=0x02 → CHR bank 32 (value 32).
        let mut mapper = make_mapper();
        write_mmc3_reg(&mut mapper, 0x02, 0x02);
        let actual = mapper.read_chr(0x1000);
        assert_ne!(
            actual, 2,
            "Without scrambling R2=0x02 would yield bank 2 (value 2)"
        );
        assert_eq!(actual, 32, "Scrambled: R2=0x02 → bank 32");
    }

    #[test]
    fn chr_swap_bits1_and_5() {
        // R0=0x20 (bit5=1, bit1=0) → scramble: bit5→bit1 → 0x02 → bank 2
        // R0=0x02 (bit5=0, bit1=1) → scramble: bit1→bit5 → 0x20=32 → bank 32
        let mut mapper = make_mapper();

        // R0 controls PPU $0000 in mode 0.
        write_mmc3_reg(&mut mapper, 0x00, 0x20);
        assert_eq!(
            mapper.read_chr(0x0000),
            2, // scramble_chr(0x20): bit5→bit1 → 0x02 → 2%64=2
            "R0=0x20 (bit5) → scramble to bit1 → bank 2"
        );

        write_mmc3_reg(&mut mapper, 0x00, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            32, // scramble_chr(0x02): bit1→bit5 → 0x20 → 32%64=32
            "R0=0x02 (bit1) → scramble to bit5 → bank 32"
        );
    }

    // =========================================================================
    // Mirroring (MMC3 delegation)
    // =========================================================================

    #[test]
    fn mirroring_controlled_by_mmc3_a000() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring must match construction argument"
        );

        // MMC3 $A000 (even) controls mirroring. Game addr $A000 → scrambled $A000 ✓
        mapper.write_prg(0xA000, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Writing $A000 bit0=1 must switch to Horizontal"
        );
    }

    // =========================================================================
    // IRQ (MMC3 delegation)
    // =========================================================================

    #[test]
    fn irq_fires_via_mmc3_delegation() {
        let mut mapper = make_mapper();

        // IRQ latch=1: game $C000 → scrambled $C000 (MMC3 IRQ latch, even)
        mapper.write_prg(0xC000, 1);
        // IRQ reload: game $C004 → scrambled $C005 (MMC3 IRQ reload, odd)
        mapper.write_prg(0xC004, 0);
        // IRQ enable: game $E008 → scrambled $E009 (MMC3 IRQ enable, odd)
        mapper.write_prg(0xE008, 0);

        // Two A12 rising edges trigger the IRQ (latch=1 → counter=1, 1 clock → fires)
        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }

        assert!(mapper.irq_pending(), "MMC3 IRQ must fire via mapper 325");
    }

    // =========================================================================
    // Snapshot / restore
    // =========================================================================

    #[test]
    fn registers_snapshot_round_trips_prg() {
        let mut mapper = make_mapper();
        write_mmc3_reg(&mut mapper, 0x06, 5); // R6=5 → bank 1 via scrambling

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        assert_eq!(
            mapper2.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "PRG bank state must survive snapshot round-trip"
        );
    }

    #[test]
    fn registers_snapshot_round_trips_chr() {
        let mut mapper = make_mapper();
        write_mmc3_reg(&mut mapper, 0x02, 0x02); // R2=0x02 → CHR bank 32

        let snap = mapper.registers_snapshot();

        let mut mapper2 = make_mapper();
        mapper2.restore_registers(&snap);

        assert_eq!(
            mapper2.read_chr(0x1000),
            mapper.read_chr(0x1000),
            "CHR bank state must survive snapshot round-trip"
        );
    }
}
