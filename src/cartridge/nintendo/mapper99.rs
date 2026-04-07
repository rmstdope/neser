//! Mapper 099 — VS System
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_099>
//! - VS System: <https://www.nesdev.org/wiki/VS._System>
//!
//! A simple CNROM-like mapper used by VS System arcade games (e.g., Vs. Super Mario Bros.).
//! CHR ROM bank selection is controlled by bit 2 of CPU writes to $4016 (the 2A03's OUT2 pin),
//! not by writes to the PRG address space.
//!
//! For the 40KB PRG variant (Vs. Gumshoe), OUT2 additionally switches the 8KB PRG bank
//! at $8000-$9FFF.
//!
//! Known Limitations:
//! - VS DualSystem RaidOnBungelingBay PRG variant is not implemented (requires dual-console).
//! - Undersize ROM open-bus behavior is not implemented (no known game relies on it).

use crate::cartridge::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

/// Mapper 099 — VS System
///
/// - PRG: 4 × 8KB banks, normally fixed. 40KB variant switches $8000-$9FFF via OUT2.
/// - CHR: 8KB bank selected by $4016 bit 2 (OUT2 pin).
/// - Mirroring: Four-screen (hardwired).
/// - PRG-RAM: 2KB at $6000-$7FFF.
pub struct Mapper99 {
    base: BaseMapper,
    prg_chr_select_bit: u8,
    has_extended_prg: bool,
}

impl Mapper99 {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_SIZE: usize = 0x2000; // 8 KiB

    pub fn new(ctx: MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let has_extended_prg = ctx.prg_rom.len() > 0x8000; // >32KB = 40KB Gumshoe variant
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            max_prg_ram_kb: 2,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_chr_select_bit: 0,
            has_extended_prg,
        };

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // CHR: select 8KB bank based on OUT2 bit
        self.base.select_chr_page(0, self.prg_chr_select_bit as i16);

        // PRG: banks 0-3 are normally fixed
        let prg_outer = if self.has_extended_prg {
            // 40KB variant: OUT2 also selects $8000 bank (bank 0 or bank 4)
            (self.prg_chr_select_bit as i16) << 2
        } else {
            0
        };
        self.base.select_prg_page(0, prg_outer);
        self.base.select_prg_page(1, 1);
        self.base.select_prg_page(2, 2);
        self.base.select_prg_page(3, 3);
    }
}

impl Mapper for Mapper99 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => self.base.try_read_prg_ram(addr).unwrap_or(0),
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.base.try_write_prg_ram(addr, value);
    }

    fn on_controller_port_write(&mut self, addr: u16, value: u8) {
        if addr != 0x4016 {
            return;
        }
        self.prg_chr_select_bit = (value >> 2) & 0x01;
        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_chr_select_bit]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if !data.is_empty() {
            self.prg_chr_select_bit = data[0];
            self.update_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::create_mapper;
    use crate::cartridge::test_helpers::banked_data;

    // Standard 32KB PRG (4 × 8KB) + 16KB CHR (2 × 8KB)
    fn make_mapper() -> Mapper99 {
        let prg = banked_data(8 * 1024, 4);
        let chr = banked_data(8 * 1024, 2);
        Mapper99::new(MapperContext::new_for_test(
            99,
            prg,
            chr,
            NametableLayout::FourScreen,
        ))
    }

    // 40KB PRG (5 × 8KB) + 16KB CHR — Vs. Gumshoe variant
    fn make_mapper_40kb_prg() -> Mapper99 {
        let prg = banked_data(8 * 1024, 5);
        let chr = banked_data(8 * 1024, 2);
        Mapper99::new(MapperContext::new_for_test(
            99,
            prg,
            chr,
            NametableLayout::FourScreen,
        ))
    }

    // -----------------------------------------------------------------------
    // Registration
    // -----------------------------------------------------------------------

    #[test]
    fn mapper_99_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            99,
            banked_data(8 * 1024, 4),
            banked_data(8 * 1024, 2),
            NametableLayout::FourScreen,
        ));
        assert!(
            result.is_ok(),
            "Mapper 99 must be registered in the factory"
        );
    }

    // -----------------------------------------------------------------------
    // Power-on state
    // -----------------------------------------------------------------------

    #[test]
    fn power_on_chr_bank_0_selected() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank 0 must be selected at power-on"
        );
    }

    #[test]
    fn power_on_prg_banks_fixed() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 = PRG bank 0");
        assert_eq!(mapper.read_prg(0xA000), 1, "$A000 = PRG bank 1");
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 = PRG bank 2");
        assert_eq!(mapper.read_prg(0xE000), 3, "$E000 = PRG bank 3");
    }

    #[test]
    fn power_on_mirroring_is_four_screen() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::FourScreen,
            "Mirroring must be four-screen"
        );
    }

    // -----------------------------------------------------------------------
    // CHR bank switching via $4016 bit 2
    // -----------------------------------------------------------------------

    #[test]
    fn controller_port_write_bit2_selects_chr_bank_1() {
        let mut mapper = make_mapper();
        mapper.on_controller_port_write(0x4016, 0x04); // bit 2 = 1
        assert_eq!(mapper.read_chr(0x0000), 1, "OUT2=1 must select CHR bank 1");
    }

    #[test]
    fn controller_port_write_bit2_clear_selects_chr_bank_0() {
        let mut mapper = make_mapper();
        mapper.on_controller_port_write(0x4016, 0x04); // set
        mapper.on_controller_port_write(0x4016, 0x00); // clear
        assert_eq!(mapper.read_chr(0x0000), 0, "OUT2=0 must select CHR bank 0");
    }

    #[test]
    fn controller_port_write_other_bits_do_not_affect_chr() {
        let mut mapper = make_mapper();
        // Write with all bits except bit 2 set
        mapper.on_controller_port_write(0x4016, 0xFB); // 0b1111_1011
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Only bit 2 should affect CHR bank selection"
        );
    }

    #[test]
    fn controller_port_write_4017_is_ignored() {
        let mut mapper = make_mapper();
        mapper.on_controller_port_write(0x4017, 0x04); // bit 2 = 1, but on $4017
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "$4017 writes must not affect CHR bank selection"
        );
    }

    // -----------------------------------------------------------------------
    // 40KB PRG variant (Vs. Gumshoe) — OUT2 also switches $8000
    // -----------------------------------------------------------------------

    #[test]
    fn gumshoe_out2_switches_prg_bank_at_8000() {
        let mut mapper = make_mapper_40kb_prg();
        mapper.on_controller_port_write(0x4016, 0x04); // bit 2 = 1
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "OUT2=1 must switch $8000 to PRG bank 4 on 40KB variant"
        );
        // Other PRG banks remain unchanged
        assert_eq!(mapper.read_prg(0xA000), 1, "$A000 unchanged");
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 unchanged");
        assert_eq!(mapper.read_prg(0xE000), 3, "$E000 unchanged");
    }

    #[test]
    fn gumshoe_out2_clear_restores_prg_bank_0_at_8000() {
        let mut mapper = make_mapper_40kb_prg();
        mapper.on_controller_port_write(0x4016, 0x04); // set
        mapper.on_controller_port_write(0x4016, 0x00); // clear
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "OUT2=0 must restore $8000 to PRG bank 0 on 40KB variant"
        );
    }

    #[test]
    fn standard_32kb_out2_does_not_switch_prg() {
        let mut mapper = make_mapper();
        mapper.on_controller_port_write(0x4016, 0x04); // bit 2 = 1
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "OUT2 must NOT switch PRG on standard 32KB variant"
        );
    }

    // -----------------------------------------------------------------------
    // PRG-RAM at $6000-$7FFF
    // -----------------------------------------------------------------------

    #[test]
    fn prg_ram_write_and_read_at_6000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "PRG-RAM write/read at $6000 must work"
        );
    }

    // -----------------------------------------------------------------------
    // $8000-$FFFF writes are no-ops
    // -----------------------------------------------------------------------

    #[test]
    fn writes_to_prg_space_are_nops() {
        let mut mapper = make_mapper();
        let before = mapper.read_chr(0x0000);
        mapper.write_prg(0x8000, 0x01); // should be a no-op
        assert_eq!(
            mapper.read_chr(0x0000),
            before,
            "PRG space writes must not affect CHR bank"
        );
    }

    // -----------------------------------------------------------------------
    // Snapshot round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn registers_snapshot_round_trips() {
        let mut original = make_mapper();
        original.on_controller_port_write(0x4016, 0x04); // select CHR bank 1
        assert_eq!(original.read_chr(0x0000), 1, "setup: CHR bank 1 active");

        let snap = original.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_chr(0x0000),
            1,
            "CHR bank selection must be preserved after restore"
        );
    }

    #[test]
    fn registers_snapshot_round_trips_40kb_prg() {
        let mut original = make_mapper_40kb_prg();
        original.on_controller_port_write(0x4016, 0x04);
        assert_eq!(original.read_prg(0x8000), 4, "setup: PRG bank 4 at $8000");

        let snap = original.registers_snapshot();
        let mut restored = make_mapper_40kb_prg();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            4,
            "PRG bank at $8000 must be preserved after restore"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            1,
            "CHR bank must be preserved after restore"
        );
    }
}
