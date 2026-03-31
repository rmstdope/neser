//! Mapper 072 - Jaleco JF-17
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_072>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 072 - Jaleco JF-17
///
/// Hardware: Jaleco JF-17 board
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_072>
/// - PRG-ROM: 128 KiB (16 KiB switchable at $8000-$BFFF, last 16 KiB fixed at $C000-$FFFF)
/// - PRG-RAM: None
/// - CHR: 128 KiB ROM (single 8 KiB switchable bank)
/// - Mirroring: Fixed from header (not programmable)
/// - Bus conflicts: YES (written value ANDed with ROM byte at address)
///
/// Register ($8000-$FFFF):
/// - Bit 7 (P): 0→1 transition loads PRG bank from D[2:0]
/// - Bit 6 (C): 0→1 transition loads CHR bank from D[3:0]
/// - Bits [3:0] (D): bank number
///
/// Power-on state: PRG bank 0 at $8000, CHR bank 0, latch = 0.
pub struct Mapper72 {
    base: BaseMapper,
    pub(crate) prg_bank: u8,
    pub(crate) chr_bank: u8,
    pub(crate) latch: u8,
}

impl Mapper72 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let num_prg_banks = ctx.prg_rom.len() / (16 * 1024);
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        base.set_bus_conflicts(true);
        // Slot 1 fixed to last bank
        let last_bank = if num_prg_banks > 0 {
            (num_prg_banks - 1) as i16
        } else {
            0
        };
        base.select_prg_page(1, last_bank);
        Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
            latch: 0,
        }
    }
}

impl Mapper for Mapper72 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }
        let effective = self.base.apply_bus_conflict(addr, value);

        let p_rising = (self.latch & 0x80) == 0 && (effective & 0x80) != 0;
        let c_rising = (self.latch & 0x40) == 0 && (effective & 0x40) != 0;

        if p_rising {
            self.prg_bank = effective & 0x07;
            self.base.select_prg_page(0, self.prg_bank as i16);
        }
        if c_rising {
            self.chr_bank = effective & 0x0F;
            self.base.select_chr_page(0, self.chr_bank as i16);
        }
        self.latch = effective;
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank, self.latch]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 3 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            self.latch = data[2];
            self.base.select_prg_page(0, self.prg_bank as i16);
            self.base.select_chr_page(0, self.chr_bank as i16);
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.latch = 0;
        self.base.select_prg_page(0, 0);
        self.base.select_chr_page(0, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping
    const PRG_BANKS: usize = 3; // 3 × 16KB = 48KB
    const CHR_BANKS: usize = 5; // 5 × 8KB  = 40KB

    /// Build a Mapper72 whose PRG ROM is filled with 0xFF so bus-conflict AND is transparent.
    fn make_mapper() -> Mapper72 {
        let prg = vec![0xFFu8; PRG_BANKS * 16 * 1024];
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper72::new(MapperContext::new_for_test(
            72,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // Helper: write a register value; since PRG ROM is 0xFF the effective value
    // equals the written value (no bus-conflict masking).
    fn write(mapper: &mut Mapper72, value: u8) {
        mapper.write_prg(0x8000, value);
    }

    // --- Registration ---

    #[test]
    fn mapper_72_is_registered() {
        let prg = vec![0xFFu8; PRG_BANKS * 16 * 1024];
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            72,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 72 must be registered in the factory"
        );
    }

    // --- Power-on state ---

    #[test]
    fn power_on_prg_bank_is_0() {
        // PRG ROM is 0xFF; we need distinguishable banks → use banked_data + no-conflict
        // Make a fresh mapper with banked PRG (0xFF fill loses bank identity), so we
        // test indirectly via the latch/bank fields.
        let mapper = make_mapper();
        assert_eq!(mapper.prg_bank, 0, "PRG bank must default to 0 at power-on");
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        // CHR is banked data: bank N filled with byte N. Bank 0 → 0x00.
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank must default to 0 at power-on"
        );
    }

    #[test]
    fn prg_c000_is_fixed_to_last_bank() {
        // Build a mapper with banked PRG (0xFF would make all banks identical).
        // Use a distinct fill so we can verify the last bank.
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let mapper = Mapper72::new(MapperContext::new_for_test(
            72,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        // Last bank index = PRG_BANKS - 1 = 2; banked_data fills it with 2.
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000-$FFFF must be fixed to the last PRG bank"
        );
    }

    // --- PRG bank switching (latch / rising-edge detection) ---

    #[test]
    fn prg_bank_switches_on_p_bit_rising_edge() {
        // Build mapper with banked PRG so bank reads are distinguishable, but ROM
        // bytes at $8000 must be 0xFF for the bus-conflict AND to be transparent.
        // Since banked_data fills bank N with byte N (not 0xFF), use 0xFF PRG for
        // the write; verify bank switch via the prg_bank field directly.
        let mut mapper = make_mapper();
        // Reset latch to 0
        write(&mut mapper, 0x00); // latch = 0
        // Write 0x82: P=1, D=2 → rising edge on P → load PRG bank 2
        write(&mut mapper, 0x82);
        assert_eq!(
            mapper.prg_bank, 2,
            "PRG bank must switch to 2 on P-bit rising edge"
        );
    }

    #[test]
    fn prg_bank_does_not_switch_on_p_bit_already_high() {
        let mut mapper = make_mapper();
        write(&mut mapper, 0x00);
        write(&mut mapper, 0x82); // rising edge → bank 2
        // Write 0x83: P still 1 (no rising edge) → bank must stay at 2 not switch to 3
        write(&mut mapper, 0x83);
        assert_eq!(
            mapper.prg_bank, 2,
            "PRG bank must NOT switch when P bit was already high"
        );
    }

    #[test]
    fn chr_bank_switches_on_c_bit_rising_edge() {
        let mut mapper = make_mapper();
        write(&mut mapper, 0x00); // latch = 0
        // Write 0x43: C=1, D=3 → rising edge on C → load CHR bank 3
        write(&mut mapper, 0x43);
        assert_eq!(
            mapper.chr_bank, 3,
            "CHR bank must switch to 3 on C-bit rising edge"
        );
    }

    #[test]
    fn chr_bank_does_not_switch_on_c_bit_already_high() {
        let mut mapper = make_mapper();
        write(&mut mapper, 0x00);
        write(&mut mapper, 0x43); // rising edge → bank 3
        // Write 0x45: C still 1 (no rising edge) → CHR bank stays at 3 not 5
        write(&mut mapper, 0x45);
        assert_eq!(
            mapper.chr_bank, 3,
            "CHR bank must NOT switch when C bit was already high"
        );
    }

    #[test]
    fn both_banks_switch_simultaneously_with_pc_bits_set() {
        let mut mapper = make_mapper();
        write(&mut mapper, 0x00); // latch = 0
        // Write 0xC5: P=1, C=1, D=5 → both rising edges → PRG bank 5%8=5, CHR bank 5
        // But with only 3 PRG banks, PRG bank 5%3=2. Verify with field values.
        write(&mut mapper, 0xC5);
        // D[2:0] = 5, D[3:0] = 5
        assert_eq!(mapper.prg_bank, 5, "PRG bank field must be set to 5");
        assert_eq!(mapper.chr_bank, 5, "CHR bank field must be set to 5");
    }

    // --- Snapshot / restore ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        write(&mut mapper, 0x00);
        write(&mut mapper, 0x82); // prg_bank = 2
        write(&mut mapper, 0x00);
        write(&mut mapper, 0x43); // chr_bank = 3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.prg_bank, mapper.prg_bank,
            "Snapshot must preserve PRG bank"
        );
        assert_eq!(
            restored.chr_bank, mapper.chr_bank,
            "Snapshot must preserve CHR bank"
        );
        assert_eq!(restored.latch, mapper.latch, "Snapshot must preserve latch");
    }

    // --- CHR RAM fallback ---

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let prg = vec![0xFFu8; PRG_BANKS * 16 * 1024];
        let mut mapper = Mapper72::new(MapperContext::new_for_test(
            72,
            prg,
            vec![],
            NametableLayout::Horizontal,
        ));
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM is present"
        );
    }
}
