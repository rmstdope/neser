//! Mapper 324 - Farid UNROM-8
//!
//! Specifications:
//! - Mesen2 reference: `FaridUnrom` (`Core/NES/Mappers/Homebrew/FaridUnrom.h`)
//!
//! Known Limitations:
//! - Soft-reset vs hard-reset distinction is not preserved: both reset to power-on
//!   state (`_reg = 0`). On hardware, soft reset preserves outer-bank bits and bit 7.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 324 - Farid UNROM-8 (homebrew enhanced UNROM).
///
/// Hardware summary:
/// - PRG-ROM: up to 1 MiB (64 × 16 KiB banks)
/// - CHR: 8 KiB CHR-RAM (fixed bank 0, no CHR banking)
/// - Mirroring: fixed from header (no programmable mirroring)
/// - IRQ: none
/// - Audio: none
/// - Bus conflicts: yes
///
/// Register (`$8000–$FFFF` write, bus-conflict applied):
/// - Bits 2:0 – inner 16 KiB PRG bank (always writable)
/// - Bit 3    – lock: when set, bits 6:3 cannot be changed
/// - Bits 6:4 – outer 16 KiB PRG block select (latched on bit-7 rising edge when not locked)
/// - Bit 7    – latch trigger / state bit (always written from value)
///
/// Bank mapping:
/// - `$8000–$BFFF`: bank `(inner & 0x07) | ((outer & 0x70) >> 1)`
/// - `$C000–$FFFF`: bank `0x07 | ((outer & 0x70) >> 1)` (fixed last in outer block)
pub struct Mapper324 {
    base: BaseMapper,
    reg: u8,
}

impl Mapper324 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: false,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.set_bus_conflicts(true);

        let mut mapper = Self { base, reg: 0 };
        mapper.apply_reg();
        mapper
    }

    fn apply_reg(&mut self) {
        let outer = self.reg & 0x70;
        let bank0 = (self.reg & 0x07) | (outer >> 1);
        let bank1 = 0x07 | (outer >> 1);
        self.base.select_prg_page(0, bank0 as i16);
        self.base.select_prg_page(1, bank1 as i16);
        self.base.select_chr_page(0, 0);
    }
}

impl Mapper for Mapper324 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x8000..=0xFFFF).contains(&addr) {
            let effective = self.base.apply_bus_conflict(addr, value);
            let locked = (self.reg & 0x08) != 0;
            if !locked && (self.reg & 0x80) == 0 && (effective & 0x80) != 0 {
                // Rising edge of bit 7: latch outer bits 6:3
                self.reg = (self.reg & 0x87) | (effective & 0x78);
            }
            self.reg = (self.reg & 0x78) | (effective & 0x87);
            self.apply_reg();
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&r) = data.first() {
            self.reg = r;
            self.apply_reg();
        }
    }

    fn reset(&mut self) {
        self.reg = 0;
        self.apply_reg();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    const NUM_BANKS: usize = 48; // Non-power-of-2 to catch wrap-around bugs

    /// Build a 48-bank × 16 KiB PRG ROM where:
    /// - All bytes are 0xFF (so bus conflicts pass write values through unchanged)
    /// - Offset 0x100 within each bank stores the bank index (for bank identification)
    fn make_prg_rom() -> Vec<u8> {
        let bank_size = 16 * 1024;
        let mut rom = vec![0xFF_u8; bank_size * NUM_BANKS];
        for bank in 0..NUM_BANKS {
            rom[bank * bank_size + 0x100] = bank as u8;
        }
        rom
    }

    fn make_mapper() -> Mapper324 {
        Mapper324::new(MapperContext::new_for_test(
            324,
            make_prg_rom(),
            vec![],
            NametableLayout::Vertical,
        ))
    }

    fn read_bank(mapper: &Mapper324, window: u16) -> u8 {
        // Read offset 0x100 within the window to get the bank index
        mapper.read_prg(window + 0x100)
    }

    // --- Registration ---

    #[test]
    fn mapper_324_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            324,
            make_prg_rom(),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 324 must be registered in the factory"
        );
    }

    // --- Power-on state ---

    #[test]
    fn power_on_lower_window_is_bank0() {
        // Given: mapper just created (reg=0)
        let mapper = make_mapper();
        // Then: $8000-$BFFF maps to bank 0
        assert_eq!(read_bank(&mapper, 0x8000), 0);
    }

    #[test]
    fn power_on_upper_window_is_bank7() {
        // Given: mapper just created (reg=0, outer=0 → bank1 = 0x07|0x00 = 7)
        let mapper = make_mapper();
        // Then: $C000-$FFFF maps to bank 7 (last bank in outer block 0)
        assert_eq!(read_bank(&mapper, 0xC000), 7);
    }

    // --- Inner bank switching (bits 2:0) ---

    #[test]
    fn inner_bank_bits_select_lower_prg_window() {
        // Given: outer block = 0, inner bank = 3
        let mut mapper = make_mapper();
        // When: write value 0x03 (no bit 7, no lock, inner=3)
        mapper.write_prg(0x8000, 0x03);
        // Then: lower window reads bank 3
        assert_eq!(read_bank(&mapper, 0x8000), 3);
    }

    #[test]
    fn inner_bank_bits_only_affect_lower_window() {
        // Given: outer block = 0, inner bank = 5
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05);
        // Lower window = bank 5, upper window = bank 7 (always last in block 0)
        assert_eq!(read_bank(&mapper, 0x8000), 5);
        assert_eq!(read_bank(&mapper, 0xC000), 7);
    }

    // --- Outer bank latching (bits 6:4) ---

    #[test]
    fn outer_bank_latched_on_bit7_rising_edge() {
        // Given: reg bit 7 = 0 (power-on), outer block = 0
        let mut mapper = make_mapper();
        // When: write 0x8A = 0b1000_1010 → bit 7 set (rising edge), outer = bits 6:4 = 0b000
        //   bits 6:4 of 0x8A = 0b000 → outer stays 0
        //   inner (bits 2:0) of 0x8A = 0b010 = 2
        // Latch: reg = (0 & 0x87) | (0x8A & 0x78) = 0x00 | 0x08 = 0x08
        // Then: reg = (0x08 & 0x78) | (0x8A & 0x87) = 0x08 | 0x82 = 0x8A
        // outer = 0x8A & 0x70 = 0x00 → bank0 = 0x02, bank1 = 0x07
        mapper.write_prg(0x8000, 0x8A);
        assert_eq!(read_bank(&mapper, 0x8000), 2, "lower: inner bits 2:0 = 2");
        assert_eq!(
            read_bank(&mapper, 0xC000),
            7,
            "upper: last bank in outer block 0"
        );
    }

    #[test]
    fn outer_bank_bits_set_via_latch() {
        // Given: reg = 0 (bit 7 = 0)
        let mut mapper = make_mapper();
        // When: write 0x93 = 0b1001_0011 → bit 7 = 1 (rising edge), bits 6:3 = 0b0010 = 2 (bit 4)
        //   bits 6:3 of 0x93 = (0x93 & 0x78) = 0b0001_0000 = 0x10
        //   latch: reg = (0 & 0x87) | 0x10 = 0x10
        //   update: reg = (0x10 & 0x78) | (0x93 & 0x87) = 0x10 | 0x83 = 0x93
        //   outer = 0x93 & 0x70 = 0x10 → outer >> 1 = 0x08
        //   bank0 = (0x93 & 0x07) | 0x08 = 3 | 8 = 11
        //   bank1 = 0x07 | 0x08 = 15
        mapper.write_prg(0x8000, 0x93);
        assert_eq!(
            read_bank(&mapper, 0x8000),
            11,
            "outer block 1, inner 3 → bank 11"
        );
        assert_eq!(
            read_bank(&mapper, 0xC000),
            15,
            "outer block 1, fixed last → bank 15"
        );
    }

    #[test]
    fn outer_bank_not_re_latched_if_bit7_already_set() {
        // Given: first write latches outer block 1 and sets bit 7
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x93); // outer block 1, bit7=1
        // When: second write also has bit 7 = 1 → no rising edge, no latch
        // Write 0xA6 = 0b1010_0110 → bits 6:3 = 0b0100 = 0x20
        // locked? reg & 0x08 = 0x93 & 0x08 = 0 → not locked
        // reg bit 7 = 1, value bit 7 = 1 → NO rising edge, no latch
        // update: reg = (0x93 & 0x78) | (0xA6 & 0x87) = 0x10 | 0x86 = 0x96
        //   outer = 0x10, outer >> 1 = 0x08
        //   bank0 = 6 | 8 = 14, bank1 = 7 | 8 = 15
        mapper.write_prg(0x8000, 0xA6);
        assert_eq!(
            read_bank(&mapper, 0x8000),
            14,
            "outer remains block 1, inner now 6"
        );
        assert_eq!(
            read_bank(&mapper, 0xC000),
            15,
            "outer block 1 fixed last still 15"
        );
    }

    // --- Lock bit (bit 3) ---

    #[test]
    fn lock_bit_prevents_outer_bank_update() {
        // Given: latch outer block 2, with lock bit also set
        // Write 0x98 = 0b1001_1000 → bit 7=1 (rising edge), bits 6:3 = 0b0011 = 0x18
        //   (bit 3 = lock = 1, outer bits 6:4 = 0b001 → outer = 0x10... wait
        //   0x18 & 0x78 = 0x18 → bits: bit4=1, bit3=1 → outer bits 6:4 = 001 → outer=0x10
        //   After latch: reg = (0 & 0x87) | 0x18 = 0x18
        //   After update: reg = (0x18 & 0x78) | (0x98 & 0x87) = 0x18 | 0x88 = 0x98
        //   outer = 0x98 & 0x70 = 0x10, lock = 0x98 & 0x08 = 0x08 (locked!)
        //   bank0 = (0x98 & 0x07) | (0x10>>1) = 0 | 8 = 8, bank1 = 7 | 8 = 15
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x98); // outer block 1 + lock bit + inner=0
        assert_eq!(
            read_bank(&mapper, 0x8000),
            8,
            "initial: outer block 1, inner 0"
        );
        assert_eq!(
            read_bank(&mapper, 0xC000),
            15,
            "initial: outer block 1, last=15"
        );

        // When: attempt to latch different outer block (bit 7 goes 0→1 again after clearing)
        // Clear bit 7 first: write 0x01 (bit7=0) → no latch, update reg bits 7,2:0
        // locked=1 → no latch, update: reg = (0x98 & 0x78) | (0x01 & 0x87) = 0x18 | 0x01 = 0x19
        mapper.write_prg(0x8000, 0x01);
        // Now try to latch with bit 7 rising edge: write 0xE5 = 0b1110_0101
        // locked = (0x19 & 0x08) = 0x08 → still locked!
        // No latch, update: reg = (0x19 & 0x78) | (0xE5 & 0x87) = 0x18 | 0x85 = 0x9D
        // outer = 0x18 (bit 4 = 1 → outer block 1, same as before)
        // bank0 = (0x9D & 0x07) | 0x08 = 5 | 8 = 13, bank1 = 7 | 8 = 15
        mapper.write_prg(0x8000, 0xE5);
        assert_eq!(
            read_bank(&mapper, 0x8000),
            13,
            "locked: outer block unchanged at 1, inner=5 → bank 13"
        );
        assert_eq!(
            read_bank(&mapper, 0xC000),
            15,
            "locked: outer block stays at 1, last=15"
        );
    }

    // --- Reset behavior ---

    #[test]
    fn reset_clears_register_and_restores_power_on_state() {
        // Given: mapper with outer block 2 active
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x92); // bit7=1 (rising), outer bits 6:3 = 0b0010, inner = 2
        // outer = 0x92 & 0x70 = 0x10 → block 1, bank0 = 2|8 = 10, bank1 = 7|8 = 15
        assert_ne!(read_bank(&mapper, 0x8000), 0); // verify state was changed

        // When: hard reset
        mapper.reset();

        // Then: back to power-on state (bank 0 lower, bank 7 upper)
        assert_eq!(read_bank(&mapper, 0x8000), 0);
        assert_eq!(read_bank(&mapper, 0xC000), 7);
    }

    // --- Bus conflicts ---

    #[test]
    fn bus_conflicts_applied_to_register_writes() {
        // Given: mapper with PRG ROM all-zeros (bus conflict zeros the write value)
        let bank_size = 16 * 1024;
        let prg_rom = vec![0x00_u8; bank_size * 8]; // all zeros
        let mut mapper = Mapper324::new(MapperContext::new_for_test(
            324,
            prg_rom,
            vec![],
            NametableLayout::Vertical,
        ));
        // When: write bank 5 (0x05), but ROM byte is 0x00 → effective = 5 & 0 = 0
        mapper.write_prg(0x8000, 0x05);
        // Then: bank 0 selected (bus conflict forced selection to 0)
        assert_eq!(mapper.read_prg(0x8000), 0x00);
    }

    #[test]
    fn bus_conflicts_pass_through_with_ff_rom() {
        // Given: mapper with PRG ROM all 0xFF (bus conflict passthrough)
        let mut mapper = make_mapper(); // uses 0xFF ROM
        // When: write bank 5
        mapper.write_prg(0x8000, 0x05);
        // Then: inner bank 5 selected (bus conflict 0x05 & 0xFF = 0x05)
        assert_eq!(read_bank(&mapper, 0x8000), 5);
    }

    // --- CHR-RAM ---

    #[test]
    fn chr_is_8kb_ram_readable_and_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_chr(0x1FFF, 0xCD);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCD);
    }

    // --- Snapshot / restore ---

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x83); // latch outer block 0, bit7=1, inner=3
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(read_bank(&restored, 0x8000), read_bank(&mapper, 0x8000));
        assert_eq!(read_bank(&restored, 0xC000), read_bank(&mapper, 0xC000));
    }

    // --- Upper window always last bank in outer block ---

    #[test]
    fn upper_window_tracks_outer_block_not_inner() {
        // Given: outer block 2 selected
        // Write 0xA0 = 0b1010_0000 → bit7=1 (rising), bits 6:3 = 0b0100 = 0x20
        //   outer = 0xA0 & 0x70 = 0x20 → outer >> 1 = 0x10
        //   bank1 = 0x07 | 0x10 = 0x17 = 23
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xA0); // outer block 2, inner=0
        // Then: upper window = bank 23 (last in outer block 2)
        assert_eq!(
            read_bank(&mapper, 0xC000),
            23,
            "upper: outer block 2, last = 23"
        );
        // When: change inner bank only (no bit 7 rising edge from 1)
        // write 0x07 → bit7=0, update: reg = (reg & 0x78) | (0x07 & 0x87)
        // reg = 0xA0 after first write; update: reg = (0xA0 & 0x78) | (0x07 & 0x87) = 0x20 | 0x07 = 0x27
        // outer = 0x27 & 0x70 = 0x20, bank1 = 7 | 0x10 = 23 (unchanged)
        mapper.write_prg(0x8000, 0x07);
        assert_eq!(
            read_bank(&mapper, 0xC000),
            23,
            "upper: outer block unchanged, still 23"
        );
        // Lower window: bank = (0x27 & 0x07) | 0x10 = 7 | 16 = 23
        assert_eq!(
            read_bank(&mapper, 0x8000),
            23,
            "lower: outer block 2, inner=7 → bank 23"
        );
    }
}
