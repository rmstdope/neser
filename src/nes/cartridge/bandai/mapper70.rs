//! Mapper 070 – Bandai 74161/32
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_070>
//! - Fallback: Mesen2 `Bandai74161_7432.h` (enableMirroringControl = false)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 070 – Bandai 74161/32
///
/// Hardware: Bandai 74161/32 discrete logic board
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_070>
/// - PRG-ROM: Up to 128 KiB (16 KiB switchable at $8000–$BFFF, last 16 KiB fixed at $C000–$FFFF)
/// - PRG-RAM: None
/// - CHR: Up to 128 KiB ROM / 8 KiB RAM (single 8 KiB switchable bank)
/// - Mirroring: Fixed from header (not programmable)
/// - Bus conflicts: None
/// - IRQ: None
///
/// Register ($8000–$FFFF):
/// - Bits [6:4]: select 16 KiB PRG bank mapped at $8000
/// - Bits [3:0]: select 8 KiB CHR bank mapped at $0000
///
/// Power-on state: PRG bank 0 at $8000, CHR bank 0, $C000 fixed to last PRG bank.
pub struct Mapper70 {
    base: BaseMapper,
    pub(crate) prg_bank: u8,
    pub(crate) chr_bank: u8,
}

impl Mapper70 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: if ctx.prg_ram_size_specified && ctx.prg_ram_banks_8k > 0 {
                ctx.prg_ram_banks_8k as usize * 8
            } else {
                0
            },
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        let num_prg_banks = self.base.prg_rom().len() / (16 * 1024);
        let last_bank = if num_prg_banks > 0 {
            (num_prg_banks - 1) as i16
        } else {
            0
        };
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_prg_page(1, last_bank);
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
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }
        self.prg_bank = (value >> 4) & 0x07;
        self.chr_bank = value & 0x0F;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank = data[0];
            self.chr_bank = data[1];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 3; // 3 × 16KB = 48KB
    const CHR_BANKS: usize = 5; // 5 × 8KB  = 40KB

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
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must start at PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank 0 at $0000 must be 0 at power-on"
        );
    }

    #[test]
    fn prg_c000_fixed_to_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000-$FFFF must be fixed to last PRG bank"
        );
    }

    // --- PRG bank switching ---

    #[test]
    fn prg_bank_switches_via_bits_6_to_4() {
        let mut mapper = make_mapper();
        // Write value where bits[6:4] = 0b010 = 2 → PRG bank 2
        mapper.write_prg(0x8000, 0x20); // bits[6:4] = 0b010
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG bank at $8000 must reflect bits[6:4] of write value"
        );
    }

    #[test]
    fn prg_bank_uses_bits_6_to_4_not_lower_bits() {
        let mut mapper = make_mapper();
        // bits[6:4] = 0b001 = 1; bits[3:0] = 0b1111
        mapper.write_prg(0x8000, 0x1F);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank selection must use bits[6:4] only"
        );
    }

    #[test]
    fn prg_c000_stays_fixed_after_register_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x10); // select PRG bank 1
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000-$FFFF must stay fixed after register write"
        );
    }

    #[test]
    fn prg_register_responds_to_any_address_in_8000_ffff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xFFFF, 0x10); // bits[6:4] = 0b001 → bank 1
        assert_eq!(mapper.read_prg(0x8000), 1, "Register must respond at $FFFF");
    }

    // --- CHR bank switching ---

    #[test]
    fn chr_bank_switches_via_bits_3_to_0() {
        let mut mapper = make_mapper();
        // bits[3:0] = 3 → CHR bank 3
        mapper.write_prg(0x8000, 0x03);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bank at $0000 must reflect bits[3:0] of write value"
        );
    }

    #[test]
    fn chr_bank_uses_bits_3_to_0_not_upper_bits() {
        let mut mapper = make_mapper();
        // bits[6:4]=0b010=2, bits[3:0]=0b0001=1
        mapper.write_prg(0x8000, 0x21);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "CHR bank selection must use bits[3:0] only"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x04); // CHR bank 4
        assert_eq!(mapper.read_chr(0x0000), 4);
        assert_eq!(mapper.read_chr(0x1FFF), 4);
    }

    // --- No mirroring control ---

    #[test]
    fn mirroring_fixed_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed from header for mapper 70"
        );
    }

    #[test]
    fn mirroring_not_changed_by_register_writes() {
        let mut mapper = make_mapper();
        // bit 7 set would affect mapper 152; mapper 70 must ignore it
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must not change after register write"
        );
    }

    // --- No IRQ ---

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 70 must never assert IRQ");
    }

    // --- Snapshot / restore ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // Select PRG bank 2 (bits[6:4]=0b010) and CHR bank 3 (bits[3:0]=0b0011)
        mapper.write_prg(0x8000, 0x23);

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
        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored mapper must read same PRG data"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored mapper must read same CHR data"
        );
    }

    // --- Reset ---

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x21); // PRG bank 2, CHR bank 1
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank must be 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 after reset");
    }

    // --- CHR RAM fallback ---

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
