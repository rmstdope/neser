//! Mapper 038 – Crime Busters / 32-in-1 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_038>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.
//!
//! ## Hardware behavior
//!
//! A single write-only register is decoded at `$7000–$7FFF`:
//!
//! | Bits | Function                          |
//! |------|-----------------------------------|
//! | 1–0  | Select 32 KB PRG-ROM bank         |
//! | 3–2  | Select 8 KB CHR-ROM bank          |
//!
//! - PRG: 32 KB switchable bank mapped at `$8000–$FFFF`.
//! - CHR: 8 KB switchable bank mapped at `$0000–$1FFF`.
//! - Mirroring: fixed from the iNES header.
//! - No PRG-RAM, no IRQ, no expansion audio.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 32 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 038 – Crime Busters / 32-in-1 multicart.
///
/// See the module-level documentation for hardware details.
pub struct Mapper38 {
    base: BaseMapper,
    pub(crate) prg_bank: u8,
    pub(crate) chr_bank: u8,
}

impl Mapper38 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
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

impl Mapper for Mapper38 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if !(0x7000..=0x7FFF).contains(&addr) {
            return;
        }
        self.prg_bank = value & 0x03;
        self.chr_bank = (value >> 2) & 0x03;
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
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass via modulo wrapping.
    const PRG_BANKS: usize = 3;
    const CHR_BANKS: usize = 3;

    fn make_mapper() -> Mapper38 {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper38::new(MapperContext::new_for_test(
            38,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_38_is_registered() {
        // Given: valid PRG-ROM and CHR-ROM
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        // When: mapper 38 is created through the factory
        let result = create_mapper(MapperContext::new_for_test(
            38,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        // Then: factory must succeed
        assert!(
            result.is_ok(),
            "Mapper 38 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        // Given: freshly created mapper
        let mapper = make_mapper();
        // Then: $8000 maps to PRG bank 0 (byte value 0)
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must start at PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        // Given: freshly created mapper
        let mut mapper = make_mapper();
        // Then: $0000 maps to CHR bank 0 (byte value 0)
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank 0 at $0000 must be 0 at power-on"
        );
    }

    // ── PRG bank switching via register at $7000-$7FFF ────────────────────────

    #[test]
    fn prg_bank_switches_via_bits_1_0() {
        let mut mapper = make_mapper();
        // bits[1:0] = 0b10 = 2 → PRG bank 2
        mapper.write_prg(0x7000, 0x02);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "PRG bank at $8000 must reflect bits[1:0] of register write"
        );
    }

    #[test]
    fn prg_bank_uses_bits_1_0_not_upper_bits() {
        let mut mapper = make_mapper();
        // bits[3:2]=0b11=3 (CHR), bits[1:0]=0b01=1 (PRG bank 1)
        mapper.write_prg(0x7000, 0x0D); // 0b00001101
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank selection must use bits[1:0] only"
        );
    }

    #[test]
    fn prg_register_responds_to_any_address_in_7000_7fff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0x02); // PRG bank 2 at high end of range
        assert_eq!(mapper.read_prg(0x8000), 2, "Register must respond at $7FFF");
    }

    #[test]
    fn prg_bank_covers_full_32kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7000, 0x02); // PRG bank 2
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG start of window");
        assert_eq!(mapper.read_prg(0xFFFF), 2, "PRG end of window");
    }

    #[test]
    fn prg_write_outside_7000_7fff_is_ignored() {
        let mut mapper = make_mapper();
        // Write to $8000 should be ignored (not a valid register address)
        mapper.write_prg(0x8000, 0x02);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Writes to $8000 must not change PRG bank"
        );
        // Write to $6FFF should also be ignored
        mapper.write_prg(0x6FFF, 0x02);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Writes to $6FFF must not change PRG bank"
        );
    }

    // ── CHR bank switching ────────────────────────────────────────────────────

    #[test]
    fn chr_bank_switches_via_bits_3_2() {
        let mut mapper = make_mapper();
        // bits[3:2] = 0b10 = 2 → CHR bank 2
        mapper.write_prg(0x7000, 0x08); // 0b00001000
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "CHR bank at $0000 must reflect bits[3:2] of register write"
        );
    }

    #[test]
    fn chr_bank_uses_bits_3_2_not_lower_bits() {
        let mut mapper = make_mapper();
        // bits[3:2]=0b01=1 (CHR bank 1), bits[1:0]=0b11=3 (PRG bank 3 → wraps to 0 with 3-bank)
        mapper.write_prg(0x7000, 0x07); // 0b00000111
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "CHR bank selection must use bits[3:2] only"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7000, 0x08); // CHR bank 2 (bits[3:2]=10)
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR start of window");
        assert_eq!(mapper.read_chr(0x1FFF), 2, "CHR end of window");
    }

    #[test]
    fn prg_and_chr_bits_in_same_byte_work_independently() {
        let mut mapper = make_mapper();
        // bits[3:2]=0b01=1 → CHR bank 1; bits[1:0]=0b10=2 → PRG bank 2
        mapper.write_prg(0x7000, 0x06); // 0b00000110
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG bank should be 2");
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR bank should be 1");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_fixed_from_header() {
        let mapper = make_mapper(); // created with Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed from header for mapper 38"
        );
    }

    #[test]
    fn mirroring_not_changed_by_register_writes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7000, 0xFF);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must not change after register write"
        );
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 38 must never assert IRQ");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7000, 0x06); // PRG bank 2, CHR bank 1
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank must be 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 after reset");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // PRG bank 2, CHR bank 1: value = 0b00000110 = 0x06
        mapper.write_prg(0x7000, 0x06);

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

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        // Given: mapper with no CHR-ROM
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let mut mapper = Mapper38::new(MapperContext::new_for_test(
            38,
            prg,
            vec![],
            NametableLayout::Horizontal,
        ));
        // When: writing to CHR space
        mapper.write_chr(0x0100, 0xAB);
        // Then: read back should return the written value
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM is present"
        );
    }
}
