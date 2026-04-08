//! Mapper 216 – RCM Group address-selected bank switcher
//!
//! Specifications:
//! - Primary source: NesDev wiki — <https://www.nesdev.org/wiki/INES_Mapper_216>
//!   (page exists but has minimal content; full behavior from Mesen2 below)
//! - Fallback source: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper216.h`
//!
//! # Hardware overview
//!
//! Used by Russian games published by RCM Group, including *Bonza*, *Videopoker Bonza*,
//! and *Magic Jewelry II*. Games requiring Dendy timings may not display correctly on
//! standard NTSC/PAL hardware.
//!
//! - PRG-ROM: one switchable 32 KiB window ($8000–$FFFF), page selected by bit 0 of
//!   the write address.
//! - CHR-ROM: one switchable 8 KiB window ($0000–$1FFF), page selected by bits 1–3 of
//!   the write address.
//! - Mirroring: fixed from the cartridge header; no dynamic mirroring control.
//! - IRQ: none.
//! - PRG-RAM: none.
//! - Bus conflicts: none.
//!
//! # Register writes
//!
//! Writes to $5000 **or** anywhere in $8000–$FFFF trigger bank selection.  The **address**
//! (not the data byte) encodes the desired banks:
//!
//! | Address bits | Field    | Selects         |
//! |--------------|----------|-----------------|
//! | bit 0        | PRG bank | 32 KiB PRG page |
//! | bits 3–1     | CHR bank | 8 KiB CHR page  |
//!
//! Examples:
//! - Write to $8000 → PRG bank 0, CHR bank 0  (power-on state)
//! - Write to $8001 → PRG bank 1, CHR bank 0
//! - Write to $8002 → PRG bank 0, CHR bank 1
//! - Write to $800F → PRG bank 1, CHR bank 7
//!
//! # Register reads
//!
//! Reads from $5000 return 0 (used by *Videopoker Bonza*).
//! Reads from $8000–$FFFF return PRG-ROM content as normal.
//!
//! # Power-on / reset state
//!
//! PRG bank 0, CHR bank 0 (equivalent to a write to $8000).

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 216;
const PRG_BANK_SIZE_BYTES: usize = 32 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;

/// Mapper 216 – RCM Group address-selected bank switcher.
///
/// Bank selection is encoded in the **write address** bits, not the data byte:
/// - `prg_bank`: bit 0 of address.
/// - `chr_bank`: bits 1–3 of address.
pub struct Mapper216 {
    base: BaseMapper,
    prg_bank: u8,
    chr_bank: u8,
}

impl Mapper216 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self {
            base,
            prg_bank: 0,
            chr_bank: 0,
        };
        mapper.apply_banking();
        mapper
    }

    fn apply_banking(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_chr_page(0, self.chr_bank as i16);
    }

    fn bank_select_from_addr(&mut self, addr: u16) {
        self.prg_bank = (addr & 0x01) as u8;
        self.chr_bank = ((addr & 0x0E) >> 1) as u8;
        self.apply_banking();
    }
}

impl Mapper for Mapper216 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr {
            0x5000 | 0x8000..=0xFFFF => self.bank_select_from_addr(addr),
            _ => {}
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if addr == 0x5000 {
            return 0;
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.chr_bank = 0;
        self.apply_banking();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_bank, self.chr_bank]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_bank = data[0] & 0x01;
            self.chr_bank = data[1] & 0x07;
            self.apply_banking();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 2;
    const CHR_BANKS: usize = 8;

    fn make_mapper() -> Mapper216 {
        Mapper216::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_216_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(
            result.is_ok(),
            "Mapper 216 must be registered in the factory"
        );
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_selects_prg_bank_0_and_chr_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG bank 0 byte 0 == bank index 0"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank 0 byte 0 == bank index 0"
        );
    }

    // ── PRG bank switching via write address ──────────────────────────────────

    #[test]
    fn write_to_odd_8000_address_selects_prg_bank_1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG bank 1 byte 0 == bank index 1"
        );
    }

    #[test]
    fn write_to_even_8000_address_selects_prg_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0x00); // switch to bank 1 first
        mapper.write_prg(0x8000, 0xFF); // even address → back to bank 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Even address resets PRG to bank 0"
        );
    }

    #[test]
    fn write_data_byte_is_ignored_for_bank_selection() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8001, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Data byte must not affect PRG bank"
        );
        mapper.write_prg(0x8001, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Data byte 0x00 must not affect PRG bank"
        );
    }

    // ── CHR bank switching via write address ──────────────────────────────────

    #[test]
    fn write_address_bits_1_to_3_select_chr_bank() {
        let mut mapper = make_mapper();
        // addr & 0x0E >> 1:  addr=0x8002 → CHR bank 1
        mapper.write_prg(0x8002, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 1, "addr=0x8002 → CHR bank 1");
    }

    #[test]
    fn write_address_selects_chr_bank_7() {
        let mut mapper = make_mapper();
        // addr=0x800E → bits 3:1 = 0b111 → CHR bank 7
        mapper.write_prg(0x800E, 0x00);
        assert_eq!(mapper.read_chr(0x0000), 7, "addr=0x800E → CHR bank 7");
    }

    #[test]
    fn write_address_0x800f_selects_prg_bank_1_chr_bank_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800F, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 1, "addr=0x800F → PRG bank 1");
        assert_eq!(mapper.read_chr(0x0000), 7, "addr=0x800F → CHR bank 7");
    }

    // ── $5000 register ────────────────────────────────────────────────────────

    #[test]
    fn write_to_5000_selects_prg_bank_0_chr_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800F, 0x00); // set prg=1, chr=7
        mapper.write_prg(0x5000, 0x00); // $5000 → addr bit0=0, bits3:1=0 → prg=0, chr=0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$5000 write resets PRG to bank 0"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "$5000 write resets CHR to bank 0"
        );
    }

    #[test]
    fn read_from_5000_returns_zero() {
        let mapper = make_mapper();
        // read_prg_open_bus must return 0 for $5000 regardless of open_bus value
        assert_eq!(
            mapper.read_prg_open_bus(0x5000, 0xFF),
            0,
            "reads from $5000 must return 0"
        );
    }

    // ── Writes outside $5000 and $8000–$FFFF are ignored ─────────────────────

    #[test]
    fn writes_outside_register_range_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800F, 0x00); // prg=1, chr=7
        mapper.write_prg(0x6000, 0x00); // $6000 — no PRG-RAM, must be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$6000 write must not change banks"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            7,
            "$6000 write must not change banks"
        );
        mapper.write_prg(0x4020, 0x00); // $4020 — must be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "$4020 write must not change banks"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_prg_bank_0_and_chr_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800F, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_chr(0x0000), 7);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "reset must restore PRG bank 0");
        assert_eq!(mapper.read_chr(0x0000), 0, "reset must restore CHR bank 0");
    }

    // ── Save state ────────────────────────────────────────────────────────────

    #[test]
    fn snapshot_and_restore_preserve_bank_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x800F, 0x00); // prg=1, chr=7
        let snap = mapper.registers_snapshot();
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        mapper.restore_registers(&snap);
        assert_eq!(mapper.read_prg(0x8000), 1, "restore must reload PRG bank");
        assert_eq!(mapper.read_chr(0x0000), 7, "restore must reload CHR bank");
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn reports_expected_capabilities() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_chr_banking, "CHR banking must be enabled");
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(
            !caps.has_dynamic_mirroring,
            "mirroring is fixed from header"
        );
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let vert = Mapper216::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert_eq!(vert.get_mirroring(), NametableLayout::Vertical);

        let horiz = Mapper216::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert_eq!(horiz.get_mirroring(), NametableLayout::Horizontal);
    }
}
