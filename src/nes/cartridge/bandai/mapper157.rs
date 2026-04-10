//! Mapper 157 — Bandai Datach Joint ROM System
//!
//! Specifications:
//! - NESdev wiki: <https://www.nesdev.org/wiki/INES_Mapper_157>
//!
//! Memory map:
//! - CPU `$8000-$BFFF`: 16 KiB switchable PRG-ROM bank
//! - CPU `$C000-$FFFF`: 16 KiB PRG-ROM bank, fixed to the last bank
//! - PPU `$0000-$1FFF`: 8 KiB unbanked CHR-RAM
//!
//! This mapper belongs to the Bandai FCG family and uses the LZ93D50 ASIC
//! with registers mapped to `$8000-$800F`.  Registers `$8008`–`$800C` are
//! identical to iNES Mapper 016 submapper 5 (PRG bank, mirroring, CPU-cycle
//! IRQ with 16-bit latched counter).
//!
//! The Datach main unit also contains a barcode scanner and an internal
//! 256-byte X24C02 serial EEPROM shared across all Datach games.  One game
//! (*Battle Rush*) carries an additional external 128-byte X24C01 EEPROM on
//! the subcartridge.
//!
//! Known limitations:
//! - Serial EEPROM (X24C01/X24C02) is not emulated; writes to register `$800D`
//!   are silently ignored and reads from `$6000-$7FFF` return open bus.
//! - Barcode scanner interface is not emulated.
//! - External EEPROM clock register (`$8000-$8003`) is silently ignored.

use crate::nes::cartridge::bandai::bandai_fcg::{BandaiFcgMapper, BandaiFcgVariant};
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 157;

/// Mapper 157 — thin wrapper around [`BandaiFcgMapper`] (LZ93D50 variant).
///
/// All register and IRQ behaviour is delegated to the Bandai FCG LZ93D50
/// implementation.  CHR-RAM is used because the Datach system stores all
/// graphics in 8 KiB of RAM rather than banked CHR-ROM.
pub struct Mapper157 {
    inner: BandaiFcgMapper,
}

impl Mapper157 {
    pub fn new(ctx: MapperContext) -> Self {
        Self {
            inner: BandaiFcgMapper::new_with_variant(ctx, BandaiFcgVariant::Lz93d50),
        }
    }
}

impl Mapper for Mapper157 {
    fn base(&self) -> &BaseMapper {
        self.inner.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.inner.base_mut()
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.inner.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.inner.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        self.inner.read_chr(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.inner.write_chr(addr, value);
    }

    fn cpu_cycle(&mut self) {
        self.inner.cpu_cycle();
    }

    fn irq_pending(&self) -> bool {
        self.inner.irq_pending()
    }

    fn capabilities(&self) -> MapperCapabilities {
        self.inner.capabilities()
    }

    fn wram_size(&self) -> usize {
        self.inner.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.inner.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.inner.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.inner.registers_snapshot()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.inner.restore_registers(data);
    }

    fn initialize_ram(&mut self, mode: crate::nes::console::RamInitMode) {
        self.inner.initialize_ram(mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    /// Datach games have up to 256 KiB PRG-ROM and 8 KiB CHR-RAM.
    const PRG_BANKS_16K: usize = 8; // 8 × 16 KiB = 128 KiB

    fn make_mapper() -> Mapper157 {
        Mapper157::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS_16K),
            vec![], // CHR-RAM: no CHR-ROM
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_157_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS_16K),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 157 must be creatable via factory");
    }

    // ── PRG banking ───────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_bank_0_at_8000_last_bank_fixed_at_c000() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "bank 0 at $8000 on power-on");
        assert_eq!(
            mapper.read_prg(0xC000),
            PRG_BANKS_16K as u8 - 1,
            "last bank fixed at $C000"
        );
    }

    #[test]
    fn register_8008_switches_prg_bank_at_8000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "bank 3 at $8000 after writing $8008"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            PRG_BANKS_16K as u8 - 1,
            "last bank at $C000 unchanged"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn register_8009_controls_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8009, 0); // Vertical
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);

        mapper.write_prg(0x8009, 1); // Horizontal
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── CHR-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn chr_ram_is_writable() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(mapper.read_chr(0x0000), 0xAB, "CHR-RAM write/read at $0000");
        mapper.write_chr(0x1FFF, 0x42);
        assert_eq!(mapper.read_chr(0x1FFF), 0x42, "CHR-RAM write/read at $1FFF");
    }

    // ── IRQ ───────────────────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_startup() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "no IRQ at power-on");
    }

    #[test]
    fn irq_fires_after_countdown() {
        let mut mapper = make_mapper();
        // Enable IRQ with a latch of 3 (fires after 3 CPU cycles)
        mapper.write_prg(0x800B, 3); // low byte of latch
        mapper.write_prg(0x800C, 0); // high byte of latch
        mapper.write_prg(0x800A, 1); // enable IRQ + reload counter

        assert!(!mapper.irq_pending(), "IRQ not yet pending");
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        mapper.cpu_cycle();
        assert!(mapper.irq_pending(), "IRQ should fire after 3 cycles");
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_include_irq_and_chr_banking() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_irq, "mapper 157 has IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert_eq!(caps.prg_bank_size_kb, 16, "16 KiB PRG bank size");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_round_trips_prg_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 5);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(
            restored.read_prg(0x8000),
            5,
            "PRG bank must be restored to 5"
        );
    }
}
