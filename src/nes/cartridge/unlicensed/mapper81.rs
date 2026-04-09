//! Mapper 81 – NTDEC N715021 (Super Gun)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_081>
//!
//! Known Limitations:
//! - Only one known game uses this mapper: Super Gun (NTDEC).
//!
//! ## Hardware behavior (unusual)
//!
//! The mapper latches the **four lowest address bits** of a write to $8000–$FFFF;
//! the write *data byte* is ignored entirely.
//!
//! Address bits [3:2] select the 16 KB PRG bank for CPU $8000–$BFFF.
//! Address bits [1:0] select the 8 KB CHR bank for PPU $0000–$1FFF.
//! CPU $C000–$FFFF is always fixed to the last 16 KB PRG bank.
//! Mirroring is fixed from the cartridge header (soldered H or V pads).
//!
//! ## Memory map
//!
//! | CPU address  | Description                                |
//! |-------------|---------------------------------------------|
//! | $8000–$BFFF | Switchable 16 KB PRG bank (bits [3:2] of write address) |
//! | $C000–$FFFF | Fixed to last 16 KB PRG bank                |
//!
//! | PPU address  | Description                                |
//! |-------------|---------------------------------------------|
//! | $0000–$1FFF | Switchable 8 KB CHR bank (bits [1:0] of write address)  |

use crate::nes::cartridge::{BaseMapper, Mapper, MapperCapabilities};

const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 81 – NTDEC N715021
///
/// Specifications: <https://www.nesdev.org/wiki/INES_Mapper_081>
pub struct Mapper81 {
    base: BaseMapper,
    /// Selected 16 KB PRG bank at $8000–$BFFF (address bits [3:2]).
    prg_bank: u8,
    /// Selected 8 KB CHR bank at $0000–$1FFF (address bits [1:0]).
    chr_bank: u8,
}

impl Mapper81 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 16,
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
        let last_bank = (self.base.prg_rom().len() / PRG_BANK_SIZE).saturating_sub(1) as i16;
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_prg_page(1, last_bank);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }
}

impl Mapper for Mapper81 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    /// Any write to $8000–$FFFF latches the four lowest **address** bits.
    /// The data byte is ignored.
    ///
    /// - Address bits [3:2] → 16 KB PRG bank at $8000–$BFFF
    /// - Address bits [1:0] → 8 KB CHR bank at $0000–$1FFF
    fn write_prg(&mut self, addr: u16, _value: u8) {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return;
        }
        self.prg_bank = (addr >> 2) as u8 & 0x03;
        self.chr_bank = addr as u8 & 0x03;
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

    // Use non-power-of-two bank counts to prevent false-pass via modulo wrapping.
    // PRG: 3 × 16KB = 48 KB; CHR: 3 × 8KB = 24 KB.
    const PRG_BANKS: usize = 3;
    const CHR_BANKS: usize = 3;

    fn make_mapper() -> Mapper81 {
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper81::new(MapperContext::new_for_test(
            81,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_81_is_registered() {
        // Given: valid 48 KB PRG-ROM and 24 KB CHR-ROM
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        // When: mapper 81 is created through the factory
        let result = create_mapper(MapperContext::new_for_test(
            81,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));
        // Then: factory must succeed
        assert!(
            result.is_ok(),
            "Mapper 81 must be registered in the factory"
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
    fn power_on_prg_c000_fixed_to_last_bank() {
        // Given: freshly created mapper with PRG_BANKS=3, last bank = 2
        let mapper = make_mapper();
        // Then: $C000 maps to PRG bank 2 (byte value 2)
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000–$FFFF must be fixed to last PRG bank at power-on"
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

    // ── PRG bank switching ────────────────────────────────────────────────────

    /// The bank is encoded in bits [3:2] of the *write address*, not the data byte.
    #[test]
    fn prg_bank_selected_by_address_bits_3_2() {
        let mut mapper = make_mapper();

        // Write to address $8000 (bits [3:2] = 0b00): PRG bank 0
        mapper.write_prg(0x8000, 0xFF); // data byte must be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Address $8000 bits[3:2]=0 must select PRG bank 0"
        );

        // Write to address $8004 (bits [3:2] = 0b01): PRG bank 1
        mapper.write_prg(0x8004, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Address $8004 bits[3:2]=1 must select PRG bank 1"
        );

        // Write to address $8008 (bits [3:2] = 0b10): PRG bank 2
        mapper.write_prg(0x8008, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "Address $8008 bits[3:2]=2 must select PRG bank 2"
        );
    }

    #[test]
    fn prg_data_byte_is_ignored_for_bank_selection() {
        let mut mapper = make_mapper();
        // Write to $8004 (PRG bank 1), using data byte 0x00 (which would be bank 0 if data used)
        mapper.write_prg(0x8004, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Data byte must be ignored; PRG bank from address bits[3:2]"
        );
    }

    #[test]
    fn prg_c000_stays_fixed_after_prg_bank_change() {
        let mut mapper = make_mapper();
        // Switch PRG bank at $8000 to bank 1
        mapper.write_prg(0x8004, 0xFF); // bits[3:2]=1 → PRG bank 1
        // $C000 must still be the last bank (bank 2)
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000–$FFFF must remain fixed to last bank after PRG switch"
        );
    }

    #[test]
    fn prg_register_responds_to_any_address_in_8000_ffff() {
        let mut mapper = make_mapper();
        // Write to $C004 (bits[3:2] = 0b01 → PRG bank 1)
        mapper.write_prg(0xC004, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Register must respond to writes anywhere in $8000–$FFFF"
        );
    }

    // ── CHR bank switching ────────────────────────────────────────────────────

    /// The CHR bank is encoded in bits [1:0] of the *write address*, not the data byte.
    #[test]
    fn chr_bank_selected_by_address_bits_1_0() {
        let mut mapper = make_mapper();

        // Address $8000 (bits[1:0]=0b00): CHR bank 0
        mapper.write_prg(0x8000, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Address $8000 bits[1:0]=0 must select CHR bank 0"
        );

        // Address $8001 (bits[1:0]=0b01): CHR bank 1
        mapper.write_prg(0x8001, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "Address $8001 bits[1:0]=1 must select CHR bank 1"
        );

        // Address $8002 (bits[1:0]=0b10): CHR bank 2
        mapper.write_prg(0x8002, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            2,
            "Address $8002 bits[1:0]=2 must select CHR bank 2"
        );
    }

    #[test]
    fn chr_data_byte_is_ignored_for_bank_selection() {
        let mut mapper = make_mapper();
        // Write to $8001 (CHR bank 1) using data byte 0x00 (which would pick bank 0 if data used)
        mapper.write_prg(0x8001, 0x00);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "Data byte must be ignored; CHR bank from address bits[1:0]"
        );
    }

    #[test]
    fn chr_bank_covers_full_8kb_window() {
        let mut mapper = make_mapper();
        // CHR bank 1: all bytes in that 8KB page have value 1
        mapper.write_prg(0x8001, 0xFF); // bits[1:0]=1 → CHR bank 1
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR start of window");
        assert_eq!(mapper.read_chr(0x1FFF), 1, "CHR end of window");
    }

    #[test]
    fn prg_and_chr_bits_in_same_address_work_independently() {
        let mut mapper = make_mapper();
        // Address $8005 (bits[3:2]=0b01 → PRG 1; bits[1:0]=0b01 → CHR 1)
        mapper.write_prg(0x8005, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 1, "PRG bank should be 1");
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR bank should be 1");

        // Address $800A (bits[3:2]=0b10 → PRG 2; bits[1:0]=0b10 → CHR 2)
        mapper.write_prg(0x800A, 0xFF);
        assert_eq!(mapper.read_prg(0x8000), 2, "PRG bank should be 2");
        assert_eq!(mapper.read_chr(0x0000), 2, "CHR bank should be 2");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_fixed_from_header() {
        let mapper = make_mapper(); // created with Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring must be fixed from header for mapper 81"
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

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 81 must never assert IRQ");
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8005, 0xFF); // PRG bank 1, CHR bank 1
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "PRG bank must be 0 after reset");
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank must be 0 after reset");
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 1) as u8,
            "$C000–$FFFF must be fixed to last bank after reset"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        // PRG bank 1, CHR bank 2
        mapper.write_prg(0x8006, 0xFF); // bits[3:2]=1 → PRG 1; bits[1:0]=2 → CHR 2

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored mapper must have same PRG mapping"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored mapper must have same CHR mapping"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            mapper.read_prg(0xC000),
            "Restored mapper must have same fixed PRG window"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        // Given: mapper with no CHR-ROM
        let prg = banked_data(16 * 1024, PRG_BANKS);
        let mut mapper = Mapper81::new(MapperContext::new_for_test(
            81,
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
