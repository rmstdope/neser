//! Mapper 070 - Bandai 74161/32
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_070>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 070 - Bandai 74161/32
///
/// Hardware: Bandai 74161/32 board
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_070>
/// - PRG-ROM: up to 128 KiB (16 KiB switchable at `$8000-$BFFF`, last 16 KiB fixed at `$C000-$FFFF`)
/// - PRG-RAM: None
/// - CHR: 8 KiB ROM (single switchable bank at `$0000-$1FFF`)
/// - Mirroring: Fixed from iNES header (not programmable)
/// - Bus conflicts: None
///
/// Register (`$8000-$FFFF`):
/// - Bits \[6:4\] (P): select 16 KiB PRG bank mapped at `$8000-$BFFF`
/// - Bits \[3:0\] (C): select 8 KiB CHR bank mapped at `$0000-$1FFF`
///
/// Power-on state: PRG bank 0 at `$8000`, last PRG bank fixed at `$C000`, CHR bank 0.
pub struct Mapper70 {
    base: BaseMapper,
    pub(crate) prg_bank: u8,
    pub(crate) chr_bank: u8,
}

impl Mapper70 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
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
        let last_bank = if num_prg_banks > 0 {
            (num_prg_banks - 1) as i16
        } else {
            0
        };
        // Slot 1 fixed to last bank
        base.select_prg_page(1, last_bank);
        Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
        }
    }

    fn apply_banks(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper70 {
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
        self.prg_bank = (value >> 4) & 0x07;
        self.chr_bank = value & 0x0F;
        self.apply_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            self.apply_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.apply_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 3; // 3 × 16 KiB = 48 KiB
    const CHR_BANKS: usize = 5; // 5 × 8 KiB = 40 KiB

    fn make_mapper() -> Mapper70 {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper70::new(MapperContext::new_for_test(
            70,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_70_is_registered() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            70,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 70 must be registered in the factory"
        );
    }

    // --- Power-on state ---

    #[test]
    fn power_on_prg_bank_0_is_switchable_and_reads_bank_0() {
        let mapper = make_mapper();
        // banked_data fills bank N with byte N; bank 0 → byte 0x00.
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000-$BFFF must be mapped to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_fixed_to_last_bank() {
        let mapper = make_mapper();
        // Last bank index = PRG_BANKS - 1 = 2; banked_data fills it with 2.
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000-$FFFF must be fixed to the last PRG bank at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank must default to 0 at power-on"
        );
    }

    // --- PRG bank switching ---

    #[test]
    fn prg_bank_selected_by_bits_6_to_4() {
        let mut mapper = make_mapper();
        // Write 0x20: bits[6:4] = 0b010 = 2, bits[3:0] = 0 → PRG bank 2.
        mapper.write_prg(0x8000, 0x20);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG bank must switch to bank selected by bits [6:4]"
        );
    }

    #[test]
    fn prg_bank_switch_does_not_affect_fixed_c000_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x10); // switch $8000 bank to 1
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000-$FFFF must remain fixed to last bank after PRG bank switch"
        );
    }

    #[test]
    fn prg_bank_switch_is_accepted_across_full_register_range() {
        let mut mapper = make_mapper();
        // $FFFF is also a valid write address.
        mapper.write_prg(0xFFFF, 0x20);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "Writes to $FFFF must also select PRG bank"
        );
    }

    // --- CHR bank switching ---

    #[test]
    fn chr_bank_selected_by_bits_3_to_0() {
        let mut mapper = make_mapper();
        // Write 0x03: bits[3:0] = 3 → CHR bank 3.
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bank must switch to bank selected by bits [3:0]"
        );
    }

    #[test]
    fn prg_and_chr_banks_switch_simultaneously() {
        let mut mapper = make_mapper();
        // Write 0x12: bits[6:4] = 1 → PRG bank 1; bits[3:0] = 2 → CHR bank 2.
        mapper.write_prg(0x8000, 0x12);
        assert_eq!(mapper.read_prg(0x8000), 1, "PRG bank must be 1");
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR bank must be 2");
    }

    // --- Write ignored outside $8000-$FFFF ---

    #[test]
    fn write_below_8000_is_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0x12);
        assert_eq!(mapper.prg_bank, 0, "Write below $8000 must be ignored");
        assert_eq!(mapper.chr_bank, 0, "Write below $8000 must be ignored");
    }

    // --- No IRQ ---

    #[test]
    fn mapper_70_has_no_irq_capability() {
        let mapper = make_mapper();
        assert!(
            !mapper.capabilities().has_irq,
            "Mapper 70 must not advertise IRQ capability"
        );
    }

    // --- Snapshot / restore ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x12); // prg_bank=1, chr_bank=2
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
        // Verify memory mapping was actually restored.
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored PRG read must match"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored CHR read must match"
        );
    }

    // --- CHR-RAM fallback ---

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let mut mapper = Mapper70::new(MapperContext::new_for_test(
            70,
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
