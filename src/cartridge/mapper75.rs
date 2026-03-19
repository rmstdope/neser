//! Mapper 075 - Konami VRC1
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/VRC1>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::BaseMapper;
use crate::cartridge::NametableLayout;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 075 - Konami VRC1
///
/// Hardware: Konami VRC1 ASIC
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/VRC1>
/// - CPU $8000-$9FFF: 8KB switchable PRG-ROM bank (PRG bank 0)
/// - CPU $A000-$BFFF: 8KB switchable PRG-ROM bank (PRG bank 1)
/// - CPU $C000-$DFFF: 8KB switchable PRG-ROM bank (PRG bank 2)
/// - CPU $E000-$FFFF: 8KB PRG-ROM bank, fixed to last bank
/// - PPU $0000-$0FFF: 4KB switchable CHR bank 0
/// - PPU $1000-$1FFF: 4KB switchable CHR bank 1
/// - Mirroring: Programmable H/V via register
/// - IRQ: None
///
/// Register map:
/// - $8000-$8FFF: PRG bank 0 select [....PPPP]
/// - $9000-$9FFF: [.....FEA]  A=Mirroring (0=V,1=H), E=CHR bank 0 bit 4, F=CHR bank 1 bit 4
/// - $A000-$AFFF: PRG bank 1 select [....PPPP]
/// - $F000-$FFFF: CHR bank 1 low bits [....CCCC]
/// - $C000-$CFFF: PRG bank 2 select [....PPPP]
/// - $E000-$EFFF: CHR bank 0 low bits [....CCCC]
pub struct Mapper75 {
    base: BaseMapper,
    pub(crate) prg_bank: [u8; 3],
    pub(crate) chr_bank_low: [u8; 2],
    pub(crate) chr_bank_high: [u8; 2],
}

impl Mapper75 {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_SIZE: usize = 0x1000; // 4 KiB

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 4,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_bank: [0; 3],
            chr_bank_low: [0; 2],
            chr_bank_high: [0; 2],
        };

        mapper.update_prg_banks();
        mapper.update_chr_banks();
        mapper
    }

    fn update_prg_banks(&mut self) {
        // 4 slots of 8KB cover $8000-$FFFF; slots 0-2 are switchable, slot 3 fixed to last bank
        self.base.select_prg_page(0, self.prg_bank[0] as i16);
        self.base.select_prg_page(1, self.prg_bank[1] as i16);
        self.base.select_prg_page(2, self.prg_bank[2] as i16);
        self.base.select_prg_page(3, -1); // $E000-$FFFF fixed to last bank
    }

    fn update_chr_banks(&mut self) {
        // 5-bit bank number: high bit from chr_bank_high, low 4 bits from chr_bank_low
        let bank0 = ((self.chr_bank_high[0] as i16) << 4) | (self.chr_bank_low[0] as i16);
        let bank1 = ((self.chr_bank_high[1] as i16) << 4) | (self.chr_bank_low[1] as i16);
        self.base.select_chr_page(0, bank0);
        self.base.select_chr_page(1, bank1);
    }
}

impl Mapper for Mapper75 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(0),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr {
            0x8000..=0x8FFF => {
                self.prg_bank[0] = value & 0x0F;
                self.update_prg_banks();
            }
            0x9000..=0x9FFF => {
                // bit 0 = H/V mirroring (1=H, 0=V)
                // bit 1 = CHR bank 0 high bit (bit 4)
                // bit 2 = CHR bank 1 high bit (bit 4)
                self.base.set_mirroring_hv((value & 0x01) != 0);
                self.chr_bank_high[0] = (value >> 1) & 0x01;
                self.chr_bank_high[1] = (value >> 2) & 0x01;
                self.update_chr_banks();
            }
            0xA000..=0xAFFF => {
                self.prg_bank[1] = value & 0x0F;
                self.update_prg_banks();
            }
            0xC000..=0xCFFF => {
                self.prg_bank[2] = value & 0x0F;
                self.update_prg_banks();
            }
            0xE000..=0xEFFF => {
                self.chr_bank_low[0] = value & 0x0F;
                self.update_chr_banks();
            }
            0xF000..=0xFFFF => {
                self.chr_bank_low[1] = value & 0x0F;
                self.update_chr_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout:
        // [0-2]  prg_bank[0..2]
        // [3-4]  chr_bank_low[0..1]
        // [5-6]  chr_bank_high[0..1]
        // [7]    flags: bit 0 = mirroring (0=Vertical, 1=Horizontal)
        let mirroring_bit = u8::from(self.get_mirroring() == NametableLayout::Horizontal);
        vec![
            self.prg_bank[0],
            self.prg_bank[1],
            self.prg_bank[2],
            self.chr_bank_low[0],
            self.chr_bank_low[1],
            self.chr_bank_high[0],
            self.chr_bank_high[1],
            mirroring_bit,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 8 {
            self.prg_bank[0] = data[0];
            self.prg_bank[1] = data[1];
            self.prg_bank[2] = data[2];
            self.chr_bank_low[0] = data[3];
            self.chr_bank_low[1] = data[4];
            self.chr_bank_high[0] = data[5];
            self.chr_bank_high[1] = data[6];
            self.base.set_mirroring_hv((data[7] & 0x01) != 0);
            self.update_prg_banks();
            self.update_chr_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent modulo-wrapping false-passes
    const PRG_BANKS: usize = 11; // 11 × 8KB = 88KB
    const CHR_BANKS: usize = 11; // 11 × 4KB = 44KB

    fn make_mapper() -> Mapper75 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(4 * 1024, CHR_BANKS);
        Mapper75::new(MapperContext::new_for_test(
            75,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    #[test]
    fn mapper_75_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            75,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(4 * 1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 75 must be registered in the factory"
        );
    }

    // -----------------------------------------------------------------------
    // Power-on state
    // -----------------------------------------------------------------------

    #[test]
    fn power_on_prg_bank0_at_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must start at PRG bank 0");
    }

    #[test]
    fn power_on_prg_bank1_at_a000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xA000), 0, "$A000 must start at PRG bank 0");
    }

    #[test]
    fn power_on_prg_bank2_at_c000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 must start at PRG bank 0");
    }

    #[test]
    fn power_on_prg_e000_is_fixed_to_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000-$FFFF must always map to the last PRG bank"
        );
    }

    // -----------------------------------------------------------------------
    // PRG bank switching
    // -----------------------------------------------------------------------

    #[test]
    fn prg_bank0_select_via_8000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 1);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$8000 must switch to PRG bank 1 after writing 1 to $8000"
        );
        // Other windows must not change
        assert_eq!(mapper.read_prg(0xA000), 0, "$A000 must remain at bank 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 must remain at bank 0");
    }

    #[test]
    fn prg_bank1_select_via_a000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 2);
        assert_eq!(
            mapper.read_prg(0xA000),
            2,
            "$A000 must switch to PRG bank 2 after writing 2 to $A000"
        );
        // Other windows must not change
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must remain at bank 0");
        assert_eq!(mapper.read_prg(0xC000), 0, "$C000 must remain at bank 0");
    }

    #[test]
    fn prg_bank2_select_via_c000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 3);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must switch to PRG bank 3 after writing 3 to $C000"
        );
        // Other windows must not change
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must remain at bank 0");
        assert_eq!(mapper.read_prg(0xA000), 0, "$A000 must remain at bank 0");
    }

    #[test]
    fn prg_fixed_bank_at_e000_never_changes() {
        let mut mapper = make_mapper();
        // Switch all switchable banks
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0xA000, 6);
        mapper.write_prg(0xC000, 7);
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 fixed window must not change after PRG bank writes"
        );
    }

    // -----------------------------------------------------------------------
    // CHR bank switching
    // -----------------------------------------------------------------------

    #[test]
    fn chr_bank0_low_bits_via_e000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 3);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "$0000-$0FFF must map to CHR bank 3 after writing 3 to $E000"
        );
        assert_eq!(
            mapper.read_chr(0x1000),
            0,
            "$1000-$1FFF must remain at CHR bank 0"
        );
    }

    #[test]
    fn chr_bank1_low_bits_via_f000_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 4);
        assert_eq!(
            mapper.read_chr(0x1000),
            4,
            "$1000-$1FFF must map to CHR bank 4 after writing 4 to $F000"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "$0000-$0FFF must remain at CHR bank 0"
        );
    }

    #[test]
    fn chr_bank0_high_bit_via_9000_bit1() {
        let mut mapper = make_mapper();
        // Set CHR bank 0 high bit (bit 1 of $9000) → CHR bank = 0b10000 = 16
        // But with only 11 CHR banks, bank 16 % 11 = 5. Use low bits=0, high bit=1.
        // To keep things simple: set high bit only (no low bits), use 11 banks.
        // 16 % 11 = 5, but we test the high-bit effect by using low bits = 0:
        // expected effective bank = 1 * 16 + 0 = 16 → 16 % 11 = 5
        // Let's use a simpler approach: set high bit via $9000 and verify via chr_bank_high field.
        mapper.write_prg(0x9000, 0x02); // bit 1 = CHR bank 0 high bit
        assert_eq!(
            mapper.chr_bank_high[0], 1,
            "CHR bank 0 high bit must be set after writing bit 1 of $9000"
        );
    }

    #[test]
    fn chr_bank1_high_bit_via_9000_bit2() {
        let mut mapper = make_mapper();
        // bit 2 of $9000 = CHR bank 1 high bit
        mapper.write_prg(0x9000, 0x04); // bit 2 = CHR bank 1 high bit
        assert_eq!(
            mapper.chr_bank_high[1], 1,
            "CHR bank 1 high bit must be set after writing bit 2 of $9000"
        );
    }

    #[test]
    fn chr_bank0_uses_combined_high_and_low_bits() {
        // Use exactly 3 CHR banks (each 4KB) to test 5-bit bank selection.
        // With high bit=0, low=1: bank 1
        // With high bit=1, low=0: bank 16 → 16%3 = 1 (ambiguous!)
        // Use CHR_BANKS=11: high=1,low=1 → bank 17 → 17%11 = 6
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(4 * 1024, CHR_BANKS);
        let mut mapper = Mapper75::new(MapperContext::new_for_test(
            75,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        // Set CHR bank 0 low = 1, high = 1 → effective = 0b10001 = 17 → 17 % 11 = 6
        mapper.write_prg(0x9000, 0x02); // high bit = 1
        mapper.write_prg(0xE000, 0x01); // low bits = 1
        assert_eq!(
            mapper.read_chr(0x0000),
            17 % CHR_BANKS as u8,
            "$0000 must reflect combined CHR bank 0 high+low bits"
        );
    }

    #[test]
    fn chr_bank1_uses_combined_high_and_low_bits() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(4 * 1024, CHR_BANKS);
        let mut mapper = Mapper75::new(MapperContext::new_for_test(
            75,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        // Set CHR bank 1 low = 2, high = 1 → effective = 0b10010 = 18 → 18 % 11 = 7
        mapper.write_prg(0x9000, 0x04); // high bit for bank 1 = 1
        mapper.write_prg(0xF000, 0x02); // low bits = 2
        assert_eq!(
            mapper.read_chr(0x1000),
            18 % CHR_BANKS as u8,
            "$1000 must reflect combined CHR bank 1 high+low bits"
        );
    }

    // -----------------------------------------------------------------------
    // Mirroring
    // -----------------------------------------------------------------------

    #[test]
    fn mirroring_defaults_to_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Power-on mirroring must match header (Vertical)"
        );
    }

    #[test]
    fn mirroring_switches_to_horizontal_via_9000_bit0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x01); // bit 0 = 1 → Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "$9000 bit 0 = 1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_switches_to_vertical_when_bit0_clear() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x01); // set Horizontal
        mapper.write_prg(0x9000, 0x00); // clear → Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "$9000 bit 0 = 0 must select Vertical mirroring"
        );
    }

    // -----------------------------------------------------------------------
    // Snapshot round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn registers_snapshot_round_trips() {
        let mut original = make_mapper();
        original.write_prg(0x8000, 1); // prg_bank[0] = 1
        original.write_prg(0xA000, 2); // prg_bank[1] = 2
        original.write_prg(0xC000, 3); // prg_bank[2] = 3
        original.write_prg(0xE000, 5); // chr_bank_low[0] = 5
        original.write_prg(0xF000, 7); // chr_bank_low[1] = 7
        original.write_prg(0x9000, 0x07); // mirroring=H, chr0_high=1, chr1_high=1

        let snap = original.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.prg_bank, original.prg_bank,
            "PRG banks must be preserved"
        );
        assert_eq!(
            restored.chr_bank_low, original.chr_bank_low,
            "CHR low bits must be preserved"
        );
        assert_eq!(
            restored.chr_bank_high, original.chr_bank_high,
            "CHR high bits must be preserved"
        );
        assert_eq!(
            restored.get_mirroring(),
            original.get_mirroring(),
            "Mirroring must be preserved"
        );
    }
}
