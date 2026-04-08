//! Mapper 086 – Jaleco JF-13
//!
//! Specifications:
//! - Fallback: Mesen2 `JalecoJf13.h` (primary NesDev source unavailable due to network restriction)
//!
//! Known Limitations:
//! - Audio expansion ($7000–$7FFF writes) is not implemented (hardware detail, no emulator supports it).

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 086 – Jaleco JF-13
///
/// Hardware: Jaleco JF-13 discrete logic board
///
/// Specifications:
/// - Fallback: Mesen2 `JalecoJf13.h`
/// - PRG-ROM: Up to 128 KiB (single 32 KiB switchable bank at $8000–$FFFF)
/// - PRG-RAM: None
/// - CHR: Up to 64 KiB ROM (single 8 KiB switchable bank at $0000–$1FFF)
/// - Mirroring: Fixed from header (not programmable)
/// - Bus conflicts: None
/// - IRQ: None
///
/// Register ($6000–$6FFF):
/// - Bits [5:4]: select 32 KiB PRG bank mapped at $8000
/// - Bits [1:0] and bit[6] (→ bit[2]): select 8 KiB CHR bank mapped at $0000
///
/// Power-on state: PRG bank 0, CHR bank 0.
pub struct JalecoJf13Mapper {
    base: BaseMapper,
    prg_bank: u8,
    chr_bank: u8,
}

impl JalecoJf13Mapper {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_expansion_audio: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
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
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for JalecoJf13Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        // Only respond to $6000–$6FFF; $7000–$7FFF is audio (ignored)
        if !(0x6000..=0x6FFF).contains(&addr) {
            return;
        }
        self.prg_bank = (value & 0x30) >> 4;
        self.chr_bank = (value & 0x03) | ((value >> 4) & 0x04);
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
    const PRG_BANKS: usize = 3; // 3 × 32KB = 96KB
    const CHR_BANKS: usize = 5; // 5 × 8KB  = 40KB

    fn make_mapper() -> JalecoJf13Mapper {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        JalecoJf13Mapper::new(MapperContext::new_for_test(
            86,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_86_is_registered() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            86,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 86 must be registered in the factory"
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

    // --- PRG bank switching ---

    #[test]
    fn prg_bank_switches_via_bits_5_to_4() {
        let mut mapper = make_mapper();
        // bits[5:4] = 0b10 = 2 → PRG bank 2
        mapper.write_prg(0x6000, 0x20);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG bank at $8000 must reflect bits[5:4] of write value"
        );
    }

    #[test]
    fn prg_bank_1_selects_second_bank() {
        let mut mapper = make_mapper();
        // bits[5:4] = 0b01 = 1 → PRG bank 1
        mapper.write_prg(0x6000, 0x10);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank at $8000 must reflect bits[5:4] = 1"
        );
    }

    #[test]
    fn prg_bank_ignores_other_bits() {
        let mut mapper = make_mapper();
        // bits[5:4] = 0b01 = 1; other bits set
        mapper.write_prg(0x6000, 0x1F);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank must only use bits[5:4]"
        );
    }

    #[test]
    fn prg_covers_full_32kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x20); // PRG bank 2
        assert_eq!(mapper.read_prg(0x8000), 2);
        assert_eq!(mapper.read_prg(0xFFFF), 2);
    }

    #[test]
    fn prg_register_only_responds_to_6000_6fff() {
        let mut mapper = make_mapper();
        // Write at $7000 (audio range) must not change PRG bank
        mapper.write_prg(0x7000, 0x20);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Writes to $7000 must not affect PRG bank"
        );
    }

    #[test]
    fn prg_register_does_not_respond_to_8000_ffff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x20);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Writes to $8000 must not affect PRG bank"
        );
    }

    // --- CHR bank switching ---

    #[test]
    fn chr_bank_switches_via_bits_1_to_0() {
        let mut mapper = make_mapper();
        // bits[1:0] = 0b11 = 3 → CHR bank 3
        mapper.write_prg(0x6000, 0x03);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "CHR bank must reflect bits[1:0] of write value"
        );
    }

    #[test]
    fn chr_bank_bit6_maps_to_bit2() {
        let mut mapper = make_mapper();
        // bit[6]=1, bits[1:0]=0 → CHR bank = 0 | (1 << 2) = 4
        mapper.write_prg(0x6000, 0x40);
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "bit[6] of write value must map to bit[2] of CHR bank"
        );
    }

    #[test]
    fn chr_bank_combines_bits_1_0_and_bit6() {
        // Use 8 CHR banks so bank 7 (=3|4) is in range.
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, 8);
        let mut mapper = JalecoJf13Mapper::new(MapperContext::new_for_test(
            86,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        // bit[6]=0, bits[1:0]=0b10=2 → chr_bank = 2
        mapper.write_prg(0x6000, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "CHR bank must use bits[1:0] when bit[6]=0"
        );
        // bit[6]=1, bits[1:0]=0b01=1 → chr_bank = 1 | 4 = 5
        mapper.write_prg(0x6000, 0x41);
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR bank must combine bit[6]→bit[2] with bits[1:0]"
        );
        // bit[6]=1, bits[1:0]=0b11=3 → chr_bank = 3 | 4 = 7
        mapper.write_prg(0x6000, 0x47);
        assert_eq!(
            mapper.read_chr(0x0000),
            7,
            "CHR bank must combine bits[1:0] and bit[6]→bit[2]"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x03); // CHR bank 3
        assert_eq!(mapper.read_chr(0x0000), 3);
        assert_eq!(mapper.read_chr(0x1FFF), 3);
    }

    // --- No mirroring control ---

    #[test]
    fn mirroring_fixed_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed from header for mapper 86"
        );
    }

    #[test]
    fn mirroring_not_changed_by_register_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xFF);
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
        mapper.write_prg(0x6000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 86 must never assert IRQ");
    }

    // --- Snapshot / restore ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // PRG bank 2 (bits[5:4]=0b10), CHR bank 3 (bits[1:0]=0b11)
        mapper.write_prg(0x6000, 0x23);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

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
        mapper.write_prg(0x6000, 0x21); // PRG bank 2, CHR bank 1
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank must be 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 after reset");
    }

    // --- CHR RAM fallback ---

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let mut mapper = JalecoJf13Mapper::new(MapperContext::new_for_test(
            86,
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
