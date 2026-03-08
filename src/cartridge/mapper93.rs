//! Mapper 093 – Sunsoft 74S161/32
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 093 – Sunsoft 74S161/32
///
/// Hardware: Sunsoft 74S161/32 discrete logic board
///
/// Specifications:
/// - Primary: <https://www.nesdev.org/wiki/INES_Mapper_093>
/// - PRG-ROM: 32 KiB fixed at $8000–$FFFF (no switching)
/// - PRG-RAM: None
/// - CHR: Up to 64 KiB ROM (single 8 KiB switchable bank at $0000–$1FFF)
/// - Mirroring: Programmable (bit 0 of register: 0=horizontal, 1=vertical)
/// - Bus conflicts: None
/// - IRQ: None
///
/// Register ($6000–$7FFF, write-only):
/// - Bits [6:4]: CHR bank select (8 KB banks)
/// - Bit 0: Mirroring (0=horizontal, 1=vertical)
///
/// Power-on state: CHR bank 0, mirroring from header.
pub struct Mapper93 {
    base: BaseMapper,
    chr_bank: u8,
    mirroring: NametableLayout,
}

impl Mapper93 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let initial_mirroring = ctx.mirroring;
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
        let mut mapper = Self {
            base,
            chr_bank: 0,
            mirroring: initial_mirroring,
        };
        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        self.base.select_prg_page(0, 0);
        self.base.select_chr_page(0, self.chr_bank as i16);
        self.base.set_mirroring(self.mirroring);
    }
}

impl Mapper for Mapper93 {
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
        self.chr_bank = (value >> 4) & 0x07;
        self.mirroring = if value & 0x01 != 0 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        };
        self.update_banks();
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_bank, self.mirroring.to_snapshot_byte()]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.chr_bank = data[0];
            self.mirroring = NametableLayout::from_snapshot_byte(data[1]);
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.chr_bank = 0;
        // mirroring stays as power-on (header) value on reset
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 1; // 1 × 32 KB = 32 KB (fixed)
    const CHR_BANKS: usize = 5; // 5 × 8 KB = 40 KB

    fn make_mapper() -> Mapper93 {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper93::new(MapperContext::new_for_test(
            93,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // --- Registration ---

    #[test]
    fn mapper_93_is_registered() {
        let prg = banked_data(32 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            93,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 93 must be registered in the factory"
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

    // --- CHR bank switching (bits [6:4] select bank) ---

    #[test]
    fn write_bits_6_4_selects_chr_bank_1() {
        // bits[6:4] = 0b001 (value=0x10) → bank 1
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "Writing $10 (bits[6:4]=1) must select CHR bank 1"
        );
    }

    #[test]
    fn write_bits_6_4_selects_chr_bank_2() {
        // bits[6:4] = 0b010 (value=0x20) → bank 2
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x20);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "Writing $20 (bits[6:4]=2) must select CHR bank 2"
        );
    }

    #[test]
    fn write_bits_6_4_selects_chr_bank_4() {
        // bits[6:4] = 0b100 (value=0x40) → bank 4
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x40);
        assert_eq!(
            mapper.read_chr(0x0000),
            4,
            "Writing $40 (bits[6:4]=4) must select CHR bank 4"
        );
    }

    #[test]
    fn write_0x00_selects_chr_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x40); // select bank 4 first
        mapper.write_prg(0x6000, 0x00); // back to bank 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Writing $00 must select CHR bank 0"
        );
    }

    #[test]
    fn chr_bank_ignores_non_bank_bits() {
        // bits [3:1] and [7] must not affect CHR bank selection
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x8E); // bits[6:4]=0, bit7=1, bits[3:1]=7 → bank 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Non-bank bits must be ignored; bank must be 0"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x20); // CHR bank 2
        assert_eq!(mapper.read_chr(0x0000), 2);
        assert_eq!(mapper.read_chr(0x1FFF), 2);
    }

    #[test]
    fn register_responds_to_any_address_in_6000_7fff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0x10); // bits[6:4]=1 → bank 1
        assert_eq!(mapper.read_chr(0x0000), 1, "Register must respond at $7FFF");
    }

    #[test]
    fn register_does_not_respond_to_8000_ffff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x70); // should be ignored
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Writes to $8000+ must not affect CHR bank"
        );
    }

    // --- Mirroring control (bit 0) ---

    #[test]
    fn mirroring_bit0_0_gives_horizontal() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x00); // bit0=0 → horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit0=0 must select horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_bit0_1_gives_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x01); // bit0=1 → vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit0=1 must select vertical mirroring"
        );
    }

    #[test]
    fn mirroring_toggles_independently_of_chr_bank() {
        let mut mapper = make_mapper();
        // Set CHR bank=2 and mirroring=vertical
        mapper.write_prg(0x6000, 0x21); // bits[6:4]=2, bit0=1
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR bank must be 2");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be vertical"
        );

        // Change CHR bank without changing mirroring
        mapper.write_prg(0x6000, 0x11); // bits[6:4]=1, bit0=1
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR bank must change to 1");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must remain vertical"
        );

        // Clear mirroring without changing CHR bank
        mapper.write_prg(0x6000, 0x10); // bits[6:4]=1, bit0=0
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR bank must remain 1");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must change to horizontal"
        );
    }

    // --- No IRQ ---

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 93 must never assert IRQ");
    }

    // --- Snapshot / restore ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x31); // CHR bank 3, vertical

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored mapper must read same CHR data"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Restored mapper must have same mirroring"
        );
    }

    // --- Reset ---

    #[test]
    fn reset_returns_chr_to_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x30); // CHR bank 3
        mapper.reset();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 after reset");
    }
}
