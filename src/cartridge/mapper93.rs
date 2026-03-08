//! Mapper 093 – Sunsoft-2 IC (Sunsoft-3R board, 74S161/32)
//!
//! Specifications:
//! - Primary: NesDev wiki <https://www.nesdev.org/wiki/INES_Mapper_093>
//! - Fallback: Mesen2 `Sunsoft93.h`
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 093 – Sunsoft-2 IC (Sunsoft-3R board)
///
/// Hardware: Sunsoft-2 IC (74S161 + 7432 discrete logic)
///
/// Specifications:
/// - Primary: <https://www.nesdev.org/wiki/INES_Mapper_093>
/// - PRG-ROM: Up to 128 KiB — 16 KiB switchable at $8000–$BFFF + fixed last bank at $C000–$FFFF
/// - PRG-RAM: None
/// - CHR: 8 KiB RAM, enable-gated (E=0: reads return 0/open-bus, writes ignored; E=1: normal)
/// - Mirroring: Fixed from header (not programmable)
/// - Bus conflicts: Yes (register at $8000–$FFFF)
/// - IRQ: None
///
/// Register ($8000–$FFFF, write-only, **bus conflicts**):
/// ```text
/// [.PPP ...E]
///   P = bits 6:4 – 16 KiB PRG bank mapped at $8000–$BFFF
///   E = bit 0    – CHR-RAM enable (0 = disabled, 1 = normal)
/// ```
///
/// Power-on state: PRG bank 0, CHR-RAM disabled.
pub struct Mapper93 {
    base: BaseMapper,
    prg_bank: u8,
    chr_enabled: bool,
}

impl Mapper93 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        // Upper 16 KB window ($C000–$FFFF) is fixed to the last bank.
        base.select_prg_page(1, -1);
        base.set_bus_conflicts(true);
        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_enabled: false,
        };
        mapper.update_prg();
        mapper
    }

    fn update_prg(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
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
        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }
        let effective = self.base.apply_bus_conflict(addr, value);
        self.prg_bank = (effective >> 4) & 0x07;
        self.chr_enabled = (effective & 0x01) != 0;
        self.update_prg();
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        if !self.chr_enabled {
            return 0; // open bus when CHR-RAM disabled
        }
        self.base.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.chr_enabled {
            return; // writes ignored when CHR-RAM disabled
        }
        self.base.write_chr(addr, value);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, u8::from(self.chr_enabled)]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some(&bank) = data.first() {
            self.prg_bank = bank;
            self.update_prg();
        }
        if let Some(&enabled) = data.get(1) {
            self.chr_enabled = enabled != 0;
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_enabled = false;
        self.update_prg();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    // Non-power-of-two PRG bank count prevents false-pass modulo wrapping.
    // Mapper 93 supports up to 8 banks (3 bits); use 5 banks here.
    const PRG_BANKS: usize = 5; // 5 × 16 KiB = 80 KiB

    /// Build a 5-bank × 16 KiB PRG ROM where:
    /// - All bytes are 0xFF (so bus conflicts pass write values through unchanged)
    /// - Offset 0x100 within each bank stores the bank index (for bank identification)
    fn make_prg_rom() -> Vec<u8> {
        let bank_size = 16 * 1024;
        let mut rom = vec![0xFF_u8; bank_size * PRG_BANKS];
        for bank in 0..PRG_BANKS {
            rom[bank * bank_size + 0x100] = bank as u8;
        }
        rom
    }

    fn make_mapper() -> Mapper93 {
        Mapper93::new(MapperContext::new_for_test(
            93,
            make_prg_rom(),
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    /// Read bank index from a 16 KiB window by sampling offset 0x100.
    fn read_prg_bank(mapper: &Mapper93, window_base: u16) -> u8 {
        mapper.read_prg(window_base + 0x100)
    }

    // --- Registration ---

    #[test]
    fn mapper_93_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            93,
            make_prg_rom(),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 93 must be registered in the factory"
        );
    }

    // --- Power-on state ---

    #[test]
    fn power_on_lower_window_is_bank0() {
        let mapper = make_mapper();
        assert_eq!(
            read_prg_bank(&mapper, 0x8000),
            0,
            "$8000–$BFFF must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_upper_window_is_last_bank() {
        let mapper = make_mapper();
        let last = (PRG_BANKS - 1) as u8;
        assert_eq!(
            read_prg_bank(&mapper, 0xC000),
            last,
            "$C000–$FFFF must be fixed to the last PRG bank at power-on"
        );
    }

    #[test]
    fn power_on_chr_ram_is_disabled() {
        // Before any register write, CHR-RAM E=0: reads return 0.
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0xAB); // write should be ignored
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR-RAM reads must return 0 (open bus) at power-on (E=0)"
        );
    }

    // --- PRG bank switching (bits 6:4) ---

    #[test]
    fn prg_bank_1_selected_by_bits_6_4() {
        // bits[6:4] = 0b001 → bank 1; bit0=1 (chr enable)
        // Write 0x11 = 0001_0001 → effective = 0x11 & 0xFF = 0x11 → prg_bank=1
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x11);
        assert_eq!(
            read_prg_bank(&mapper, 0x8000),
            1,
            "Writing 0x11 must select PRG bank 1 at $8000–$BFFF"
        );
    }

    #[test]
    fn prg_bank_3_selected_by_bits_6_4() {
        // bits[6:4] = 0b011 → bank 3; bit0=1
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x31);
        assert_eq!(
            read_prg_bank(&mapper, 0x8000),
            3,
            "Writing 0x31 must select PRG bank 3"
        );
    }

    #[test]
    fn prg_upper_window_stays_fixed_after_bank_switch() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x31); // PRG bank 3
        let last = (PRG_BANKS - 1) as u8;
        assert_eq!(
            read_prg_bank(&mapper, 0xC000),
            last,
            "$C000–$FFFF must remain fixed to the last bank after a bank switch"
        );
    }

    #[test]
    fn prg_bank_covers_full_16kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x31); // select bank 3
        // Both ends of the lower window map to bank 3
        assert_eq!(mapper.read_prg(0x8000 + 0x100), 3);
        assert_eq!(mapper.read_prg(0xBFFF), 0xFF); // non-index byte = 0xFF fill
    }

    #[test]
    fn prg_register_only_responds_to_8000_ffff() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7FFF, 0x31); // outside register range — must be ignored
        assert_eq!(
            read_prg_bank(&mapper, 0x8000),
            0,
            "Write below $8000 must not affect PRG bank"
        );
    }

    // --- CHR-RAM enable/disable (bit 0) ---

    #[test]
    fn chr_ram_enabled_when_bit0_is_1() {
        let mut mapper = make_mapper();
        // Enable CHR-RAM: write 0x01 (prg_bank=0, chr_enable=1)
        mapper.write_prg(0x8000, 0x01);
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0000),
            0xAB,
            "CHR-RAM must be readable and writable when E=1"
        );
    }

    #[test]
    fn chr_ram_disabled_when_bit0_is_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // enable
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_prg(0x8000, 0x00); // disable (bit0=0, bus: 0x00&0xFF=0x00)
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR-RAM reads must return 0 after disabling E bit"
        );
    }

    #[test]
    fn chr_writes_ignored_when_disabled() {
        let mut mapper = make_mapper();
        // Write to CHR while disabled
        mapper.write_chr(0x0100, 0x55);
        // Enable and verify write was not stored
        mapper.write_prg(0x8000, 0x01);
        assert_eq!(
            mapper.read_chr(0x0100),
            0,
            "CHR-RAM write while disabled must be discarded"
        );
    }

    #[test]
    fn chr_ram_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x01); // enable
        mapper.write_chr(0x0000, 0x11);
        mapper.write_chr(0x1FFF, 0x22);
        assert_eq!(mapper.read_chr(0x0000), 0x11);
        assert_eq!(mapper.read_chr(0x1FFF), 0x22);
    }

    // --- Bus conflicts ---

    #[test]
    fn bus_conflicts_apply_to_register_writes() {
        // Use a PRG ROM with bank 0 bytes = 0x10 at write address.
        // Writing 0x50 (prg_bank=5 without conflict) → effective = 0x50 & 0x10 = 0x10 → bank 1.
        let bank_size = 16 * 1024;
        let mut prg_rom = vec![0xFF_u8; bank_size * PRG_BANKS];
        // Set ROM[$8000] (offset 0 in bank 0) to 0x10
        prg_rom[0] = 0x10;
        // Embed bank markers at offset 0x100
        for bank in 0..PRG_BANKS {
            prg_rom[bank * bank_size + 0x100] = bank as u8;
        }
        let mut mapper = Mapper93::new(MapperContext::new_for_test(
            93,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ));
        // Without bus conflict: 0x50 >> 4 & 7 = 5 → bank 5.
        // With bus conflict: (0x50 & 0x10) >> 4 & 7 = 0x10 >> 4 & 7 = 1 → bank 1.
        mapper.write_prg(0x8000, 0x50);
        assert_eq!(
            read_prg_bank(&mapper, 0x8000),
            1,
            "Bus conflict must AND written value with PRG-ROM byte"
        );
    }

    // --- Mirroring ---

    #[test]
    fn mirroring_fixed_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed from header"
        );
    }

    #[test]
    fn mirroring_not_changed_by_register_writes() {
        let mut mapper = make_mapper();
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
        assert!(!mapper.irq_pending(), "Mapper 93 must never assert IRQ");
    }

    // --- Snapshot / restore ---

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x31); // PRG bank 3, CHR enabled

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            read_prg_bank(&restored, 0x8000),
            3,
            "Restored mapper must map PRG bank 3"
        );
        // Write to CHR-RAM to confirm it is enabled after restore
        restored.write_chr(0x0000, 0xDE);
        assert_eq!(
            restored.read_chr(0x0000),
            0xDE,
            "Restored mapper must have CHR-RAM enabled"
        );
    }

    #[test]
    fn registers_snapshot_preserves_chr_disabled_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x20); // PRG bank 2, CHR disabled

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_chr(0x0000),
            0,
            "Restored mapper must have CHR-RAM disabled when E=0 was snapshotted"
        );
    }

    // --- Reset ---

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x31); // PRG bank 3, CHR enabled
        mapper.write_chr(0x0000, 0x42);

        mapper.reset();

        assert_eq!(
            read_prg_bank(&mapper, 0x8000),
            0,
            "PRG bank must be 0 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR-RAM must return 0 (disabled) after reset"
        );
    }
}
