//! Mapper 087 – Jaleco/Konami CHR-only banking
//!
//! Specifications:
//! - Fallback: Mesen2 `JalecoJfxx.h` (`_orderedBits = false` path)
//!   (NesDev wiki unavailable due to network restriction)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 087 – Jaleco/Konami CHR-only banking
///
/// Hardware: Jaleco/Konami discrete logic board
///
/// Specifications:
/// - Fallback: Mesen2 `JalecoJfxx.h` (`_orderedBits = false`)
/// - PRG-ROM: Up to 32 KiB fixed at $8000–$FFFF (no switching)
/// - PRG-RAM: None
/// - CHR: Up to 32 KiB ROM (single 8 KiB switchable bank at $0000–$1FFF)
/// - Mirroring: Fixed from header (not programmable)
/// - Bus conflicts: None
/// - IRQ: None
///
/// Register ($6000–$7FFF, write-only):
/// - Bit 0 → CHR A13 (bit 1 of bank number)
/// - Bit 1 → CHR A14 (bit 0 of bank number)
/// - Bank = ((value & 0x01) << 1) | ((value & 0x02) >> 1)
///
/// Power-on state: CHR bank 0.
pub struct Mapper87 {
    base: BaseMapper,
    chr_bank: u8,
}

impl Mapper87 {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self { base, chr_bank: 0 };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, 0);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper87 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !(0x6000..=0x7FFF).contains(&addr) {
            return;
        }
        // Bits are swapped: bit 0 → CHR A13 (bank bit 1), bit 1 → CHR A14 (bank bit 0)
        self.chr_bank = ((value & 0x01) << 1) | ((value & 0x02) >> 1);
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if !data.is_empty() {
            self.chr_bank = data[0];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.chr_bank = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 1; // 1 × 32KB = 32KB (fixed)
    const CHR_BANKS: usize = 5; // 5 × 8KB = 40KB

    fn make_mapper() -> Mapper87 {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper87::new(MapperContext::new_for_test(
            87,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_87_is_registered() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            87,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 87 must be registered in the factory"
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

    // --- PRG is fixed (no switching) ---

    #[test]
    fn prg_covers_full_32kb_window() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xFFFF), 0);
    }

    #[test]
    fn prg_not_changed_by_register_write() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG must remain fixed after register write"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0,
            "PRG must remain fixed after register write"
        );
    }

    // --- CHR bank switching (bits swapped per hardware spec) ---

    #[test]
    fn write_0x01_selects_chr_bank_2() {
        // bit0=1, bit1=0 → bank = (1<<1)|(0>>1) = 2
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x01);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "Writing $01 must select CHR bank 2 (bit swap: bit0→A13)"
        );
    }

    #[test]
    fn write_0x02_selects_chr_bank_1() {
        // bit0=0, bit1=1 → bank = (0<<1)|(2>>1) = 1
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x02);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "Writing $02 must select CHR bank 1 (bit swap: bit1→A14)"
        );
    }

    #[test]
    fn write_0x03_selects_chr_bank_3() {
        // bit0=1, bit1=1 → bank = (1<<1)|(2>>1) = 3
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x03);
        assert_eq!(
            mapper.read_chr(0x0000),
            3,
            "Writing $03 must select CHR bank 3"
        );
    }

    #[test]
    fn write_0x00_selects_chr_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x03); // select bank 3 first
        mapper.write_prg(0x6000, 0x00); // back to bank 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Writing $00 must select CHR bank 0"
        );
    }

    #[test]
    fn chr_bank_ignores_upper_bits() {
        // Upper bits (7:2) must not affect CHR bank selection
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xFC); // bits[1:0]=0, upper bits set → bank 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Upper bits of write value must be ignored"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x03); // CHR bank 3
        assert_eq!(mapper.read_chr(0x0000), 3);
        assert_eq!(mapper.read_chr(0x1FFF), 3);
    }

    #[test]
    fn register_responds_to_any_address_in_6000_7fff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0x02); // bits[1:0]=0b10 → bank 1
        assert_eq!(mapper.read_chr(0x0000), 1, "Register must respond at $7FFF");
    }

    #[test]
    fn register_does_not_respond_to_8000_ffff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x03); // should be ignored
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Writes to $8000+ must not affect CHR bank"
        );
    }

    // --- No mirroring control ---

    #[test]
    fn mirroring_fixed_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed from header for mapper 87"
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
        assert!(!mapper.irq_pending(), "Mapper 87 must never assert IRQ");
    }

    // --- Snapshot / restore ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x03); // CHR bank 3

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

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
        mapper.write_prg(0x6000, 0x03); // CHR bank 3
        mapper.reset();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 after reset");
    }

    // --- CHR RAM fallback ---

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let mut mapper = Mapper87::new(MapperContext::new_for_test(
            87,
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
