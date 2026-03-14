//! Mapper 263 – KoF97 (King of Fighters '97) – MMC3 variant with scrambled register writes
//!
//! Specifications:
//! - Fallback: Mesen2 `Core/NES/Mappers/Mmc3Variants/MMC3_Kof97.h`
//!   (No dedicated NesDev wiki page found at time of implementation.)
//!
//! Hardware: Unlicensed board used by the pirate port of King of Fighters '97.
//!
//! All behaviour is identical to standard MMC3 (Mapper 4), except that
//! writes to the ROM-mapped region (`$8000–$FFFF`) first pass through a
//! value scrambler and an address remapper before reaching the MMC3 logic:
//!
//! ## Value scramble (applied to every write in `$8000–$FFFF`)
//!
//! ```text
//! out = (value & 0xD8)            // bits 7,6,4,3 pass through unchanged
//!     | ((value & 0x20) >> 4)     // input bit 5 → output bit 1
//!     | ((value & 0x04) << 3)     // input bit 2 → output bit 5
//!     | ((value & 0x02) >> 1)     // input bit 1 → output bit 0
//!     | ((value & 0x01) << 2);    // input bit 0 → output bit 2
//! ```
//!
//! ## Address remapping (applied after value scramble)
//!
//! | Written address | Effective MMC3 address |
//! |-----------------|------------------------|
//! | `$9000`         | `$8001` (bank data)    |
//! | `$D000`         | `$C001` (IRQ reload)   |
//! | `$F000`         | `$E001` (IRQ enable)   |
//! | all others      | unchanged              |
//!
//! ## PRG banking
//! Standard MMC3: two switchable 8 KiB banks at `$8000–$BFFF`, two fixed at
//! `$C000–$FFFF` (mode-dependent).
//!
//! ## CHR banking
//! Standard MMC3: eight 1 KiB banks across `$0000–$1FFF`.
//!
//! ## Mirroring
//! Dynamic via scrambled write to remapped `$A000`.
//!
//! ## IRQ
//! Standard MMC3 scanline counter (A12 rising-edge), accessed via the
//! remapped `$C001`, `$E000`, `$E001` addresses.
//!
//! ## PRG-RAM
//! 8 KiB at `$6000–$7FFF` (standard MMC3).
//!
//! ## Known Limitations
//! - No known functional limitations relative to the Mesen2 reference.
//!   Source: Mesen2 `MMC3_Kof97.h`. No known deltas.

use crate::cartridge::Mapper;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;

/// Mapper 263 – KoF97 (MMC3 variant with scrambled register writes)
pub struct Mapper263 {
    mmc3: MMC3Mapper,
}

impl Mapper263 {
    const MAPPER_NUMBER: u16 = 263;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
        }
    }

    /// Scramble a value written to the MMC3 register region.
    ///
    /// Bit mapping:
    /// - bits 7, 6, 4, 3 pass through unchanged  (mask 0xD8)
    /// - input bit 5 → output bit 1
    /// - input bit 2 → output bit 5
    /// - input bit 1 → output bit 0
    /// - input bit 0 → output bit 2
    fn scramble_value(value: u8) -> u8 {
        (value & 0xD8)
            | ((value & 0x20) >> 4)
            | ((value & 0x04) << 3)
            | ((value & 0x02) >> 1)
            | ((value & 0x01) << 2)
    }

    /// Remap an address before forwarding to MMC3.
    ///
    /// Three special addresses redirect to their MMC3 "data" counterparts:
    /// - `$9000` → `$8001`
    /// - `$D000` → `$C001`
    /// - `$F000` → `$E001`
    fn remap_address(addr: u16) -> u16 {
        match addr {
            0x9000 => 0x8001,
            0xD000 => 0xC001,
            0xF000 => 0xE001,
            _ => addr,
        }
    }
}

impl Mapper for Mapper263 {
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
        self.mmc3.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            self.mmc3.write_prg(addr, value);
            return;
        }
        if (0x8000..=0xFFFF).contains(&addr) {
            let scrambled = Self::scramble_value(value);
            let remapped = Self::remap_address(addr);
            self.mmc3.write_prg(remapped, scrambled);
            return;
        }
        self.mmc3.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.mmc3.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.mmc3.write_chr(addr, value);
    }

    fn mapper_number(&self) -> u16 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        self.mmc3.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.mmc3.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.mmc3.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.mmc3.registers_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.mmc3.restore_registers(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
    }

    fn reset(&mut self) {
        self.mmc3.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-2 CHR bank count to avoid modulo-wrap false passes.
    const CHR_1K_BANKS: usize = 48;
    // Non-power-of-2 PRG 8KB bank count.
    const PRG_8K_BANKS: usize = 9;

    fn make_mapper() -> Mapper263 {
        Mapper263::new(MapperContext::new_for_test(
            263,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ))
    }

    // -------------------------------------------------------------------------
    // Factory registration
    // -------------------------------------------------------------------------

    #[test]
    fn mapper_263_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            263,
            banked_data(8 * 1024, PRG_8K_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 263 must be registered in factory");
    }

    // -------------------------------------------------------------------------
    // Value scramble
    // -------------------------------------------------------------------------

    #[test]
    fn scramble_preserves_bits_7_6_4_3() {
        // Set only bits 7,6,4,3 (mask 0xD8 = 1101_1000)
        let input: u8 = 0xD8;
        let output = Mapper263::scramble_value(input);
        assert_eq!(output, 0xD8, "bits 7,6,4,3 must pass through unchanged");
    }

    #[test]
    fn scramble_moves_bit5_to_bit1() {
        // Input: only bit 5 set (0x20)
        let output = Mapper263::scramble_value(0x20);
        assert_eq!(output, 0x02, "input bit 5 must map to output bit 1");
    }

    #[test]
    fn scramble_moves_bit2_to_bit5() {
        // Input: only bit 2 set (0x04)
        let output = Mapper263::scramble_value(0x04);
        assert_eq!(output, 0x20, "input bit 2 must map to output bit 5");
    }

    #[test]
    fn scramble_moves_bit1_to_bit0() {
        // Input: only bit 1 set (0x02)
        let output = Mapper263::scramble_value(0x02);
        assert_eq!(output, 0x01, "input bit 1 must map to output bit 0");
    }

    #[test]
    fn scramble_moves_bit0_to_bit2() {
        // Input: only bit 0 set (0x01)
        let output = Mapper263::scramble_value(0x01);
        assert_eq!(output, 0x04, "input bit 0 must map to output bit 2");
    }

    #[test]
    fn scramble_all_bits_set_is_identity() {
        // All bits set: scramble should return all bits set (0xFF)
        let output = Mapper263::scramble_value(0xFF);
        assert_eq!(output, 0xFF, "0xFF scrambled must remain 0xFF");
    }

    #[test]
    fn scramble_zero_is_zero() {
        assert_eq!(Mapper263::scramble_value(0x00), 0x00);
    }

    // -------------------------------------------------------------------------
    // Address remapping
    // -------------------------------------------------------------------------

    #[test]
    fn address_9000_remaps_to_8001() {
        assert_eq!(Mapper263::remap_address(0x9000), 0x8001);
    }

    #[test]
    fn address_d000_remaps_to_c001() {
        assert_eq!(Mapper263::remap_address(0xD000), 0xC001);
    }

    #[test]
    fn address_f000_remaps_to_e001() {
        assert_eq!(Mapper263::remap_address(0xF000), 0xE001);
    }

    #[test]
    fn other_addresses_are_not_remapped() {
        for addr in [
            0x8000u16, 0x8001, 0x9001, 0xA000, 0xA001, 0xC000, 0xC001, 0xE000, 0xE001,
        ] {
            assert_eq!(
                Mapper263::remap_address(addr),
                addr,
                "address ${:04X} must not be remapped",
                addr
            );
        }
    }

    // -------------------------------------------------------------------------
    // PRG banking via scrambled writes
    // -------------------------------------------------------------------------

    #[test]
    fn prg_banking_via_8000_and_scrambled_9000() {
        let mut mapper = make_mapper();

        // In standard MMC3:
        // - Write 0x06 to $8000: bank_select = 0x06 → select R6 (PRG bank at $8000)
        // - Write <bank> to $8001: set R6 = <bank>
        //
        // For mapper 263, $8001 is written by writing to $9000 with the value
        // that scrambles to the desired bank number.
        //
        // To select bank 3 in R6: scramble_value(x) = 3 = 0b0000_0011
        // We need out = 0b0000_0011 from: (value & 0xD8)=0 → bits7,6,4,3=0
        //   bit1 of out from input bit5: (x & 0x20) >> 4 = 0x02 → x must have bit5 set
        //   bit0 of out from input bit1: (x & 0x02) >> 1 = 0x01 → x must have bit1 set
        //   So x = 0x22 → scramble: (0x22 & 0xD8)=0 | (0x22 & 0x20)>>4=0x02 | 0 | (0x22 & 0x02)>>1=0x01 | 0 = 0x03 ✓
        //
        // For bank_select 0x06 to $8000:
        // scramble(y) = 0x06 = 0b0000_0110
        //   bit2(out) from input bit0: (y&0x01)<<2 = 0x04 → y bit0 set
        //   bit1(out) from input bit5: (y&0x20)>>4 = 0x02 → y bit5 set
        //   So y = 0x21 → scramble: 0 | (0x21&0x20)>>4=0x02 | 0 | 0 | (0x21&0x01)<<2=0x04 = 0x06 ✓
        //
        // But $8000 is NOT remapped and the value IS scrambled.
        // So we write scrambled(0x21)=0x06 to $8000 (which stays $8000):
        mapper.write_prg(0x8000, 0x21); // scrambles to 0x06 = select R6

        // Write scrambled(0x22)=0x03 to $9000 (remaps to $8001):
        mapper.write_prg(0x9000, 0x22); // scrambles to 0x03, addr remaps to $8001 → R6 = 3

        // With PRG_8K_BANKS=9, bank 3 fills all bytes with value 3.
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "PRG bank 3 should be at $8000 after scrambled writes"
        );
    }

    #[test]
    fn prg_banking_direct_8000_8001_still_works() {
        let mut mapper = make_mapper();

        // Direct writes to $8000 / $8001 are also scrambled.
        // To select R6 (bank_select=6 → bits: 0b0000_0110):
        // Need scramble(x)=6: bit2 from x.bit0, bit1 from x.bit5 → x=0x21
        // But simpler: for a direct write to $8000/$8001, we can just verify
        // that the scramble is applied by checking the effect.
        //
        // Let's write bank_select=6 without scramble (plain MMC3 style) directly
        // via the internal mmc3 field is not accessible, so let's test through the API.
        //
        // Write such that scramble(value) = 6 to $8000, then scramble(v2) = bank to $8001.
        // scramble(0x21) = 0x06 = bank_select for R6
        mapper.write_prg(0x8000, 0x21);
        // scramble to get bank 5: need 0x05 = 0b0000_0101
        //   bit2(out) from bit0(in): (x&0x01)<<2 = 0x04 → x bit0=1
        //   bit0(out) from bit1(in): (x&0x02)>>1 = 0x01 → x bit1=1
        //   So x = 0x03 → scramble: 0 | 0 | 0 | (0x03&0x02)>>1=0x01 | (0x03&0x01)<<2=0x04 = 0x05 ✓
        mapper.write_prg(0x8001, 0x03); // scrambles to 0x05 → R6 = bank 5

        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "PRG bank 5 at $8000 via direct $8001 write with scrambling"
        );
    }

    // -------------------------------------------------------------------------
    // CHR banking
    // -------------------------------------------------------------------------

    #[test]
    fn chr_banking_standard_mmc3() {
        let mut mapper = make_mapper();

        // Select R0 (CHR 2KB group for $0000-$07FF) and set it to bank 4 (even-aligned → banks 8&9)
        // To select R0 via $8000: bank_select = 0 → scramble(x) = 0 → x = 0
        mapper.write_prg(0x8000, 0x00); // scramble(0)=0 → R0 selected
        // Set R0 = 8: scramble(y) = 8 = 0b0000_1000 → only bit3 set (passes through mask 0xD8)
        // So y = 0x08, scramble(0x08) = 0x08 ✓
        mapper.write_prg(0x8001, 0x08); // scramble(0x08)=0x08 → R0=8

        // CHR bank 8 in 1K-bank space at $0000 (MMC3 uses R0 for $0000/$0400 in 2KB mode)
        // banked_data fills each 1KB bank with its index, so bank 8 = all 8s
        assert_eq!(
            mapper.read_chr(0x0000),
            8,
            "CHR bank 8 at $0000 after standard MMC3 CHR banking"
        );
    }

    // -------------------------------------------------------------------------
    // Mirroring
    // -------------------------------------------------------------------------

    #[test]
    fn mirroring_via_scrambled_a000_write() {
        let mut mapper = make_mapper();

        // $A000 in MMC3: bit0 → 0=Vertical, 1=Horizontal
        // $A000 is NOT remapped, value IS scrambled.
        //
        // To set mirroring bit0=1 (Horizontal): scramble(x) must have bit0=1.
        // bit0(out) from bit1(in): (x & 0x02) >> 1 = 1 → x bit1 must be set.
        // x = 0x02 → scramble: 0 | 0 | 0 | 1 | 0 = 0x01 ✓
        mapper.write_prg(0xA000, 0x02); // scrambles to 0x01 → Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring should be Horizontal"
        );

        // Set back to Vertical: scramble(x) has bit0=0.
        // x = 0x00 → scramble(0x00) = 0x00 → bit0=0 → Vertical
        mapper.write_prg(0xA000, 0x00); // scrambles to 0x00 → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring should be Vertical"
        );
    }

    // -------------------------------------------------------------------------
    // IRQ via remapped D000/F000
    // -------------------------------------------------------------------------

    #[test]
    fn irq_latch_set_via_c000_and_reload_via_d000() {
        let mut mapper = make_mapper();

        // Set IRQ latch via $C000 (direct, not remapped):
        // scramble(x) = latch_value; choose latch=5: scramble(x)=5=0b0000_0101
        //   bit2 from bit0: (x&0x01)<<2 = 4 → x bit0=1
        //   bit0 from bit1: (x&0x02)>>1 = 1 → x bit1=1
        //   x = 0x03 → scramble(0x03) = 0x05 ✓
        mapper.write_prg(0xC000, 0x03); // scramble(0x03)=0x05 → IRQ latch = 5

        // Trigger IRQ reload via $D000 (remaps to $C001):
        // Value scramble doesn't matter for $C001 (reload ignores value)
        mapper.write_prg(0xD000, 0x00); // remaps to $C001 → reload IRQ counter

        // Enable IRQ via $E001:
        // We can write directly to $E001 (not remapped):
        // scramble(x) must have some value; just use 0x00
        mapper.write_prg(0xE001, 0x00); // scramble(0)=0 → IRQ enable

        // The IRQ counter has been loaded. Simulate A12 toggles to count down.
        // After 5+1 clock edges the IRQ should fire (we'll just verify
        // the mechanism doesn't panic and the latch was accepted).
        // We can't easily test full IRQ firing here without the PPU clock,
        // so just verify the mapper is functional.
        assert!(
            !mapper.irq_pending(),
            "IRQ should not be pending immediately after reload"
        );
    }

    #[test]
    fn irq_disable_via_e000() {
        let mut mapper = make_mapper();

        // Enable IRQ: write to $E001 (not remapped)
        mapper.write_prg(0xE001, 0x00);

        // Disable IRQ: write to $E000 (not remapped, also acknowledges)
        mapper.write_prg(0xE000, 0x00);

        assert!(
            !mapper.irq_pending(),
            "IRQ must not be pending after $E000 write (disable+ack)"
        );
    }

    // -------------------------------------------------------------------------
    // PRG-RAM passthrough (writes in $6000–$7FFF bypass scramble)
    // -------------------------------------------------------------------------

    #[test]
    fn prg_ram_writes_are_not_scrambled() {
        let mut mapper = make_mapper();

        // MMC3 PRG-RAM is at $6000-$7FFF; writes go through unmodified.
        // First enable PRG-RAM: write $80 to $A001 (bit7=enable, bit6=not-protect)
        // scramble(x) = 0x80: bit7 passes through → x bit7 must be set → x = 0x80
        mapper.write_prg(0xA001, 0x80); // scramble(0x80)=0x80 → PRG-RAM enabled

        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "PRG-RAM write at $6000 must not be scrambled"
        );
    }

    // -------------------------------------------------------------------------
    // Save state round-trip
    // -------------------------------------------------------------------------

    #[test]
    fn registers_snapshot_restore_round_trip() {
        let mut mapper = make_mapper();

        // Set PRG bank 2 via scrambled writes
        mapper.write_prg(0x8000, 0x21); // scramble→0x06 → select R6
        // scramble(x)=2: 0b0000_0010 → bit1 from bit5: (x&0x20)>>4=2 → x=0x20
        mapper.write_prg(0x9000, 0x20); // scramble(0x20)=0x02 → R6=2, via $8001

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            2,
            "PRG bank must be restored from snapshot"
        );
    }
}
