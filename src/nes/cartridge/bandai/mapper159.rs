//! Mapper 159 — Bandai FCG (LZ93D50 with X24C01 128-byte serial EEPROM)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_159>
//! - Bandai FCG family: <https://www.nesdev.org/wiki/INES_Mapper_016>
//! - Reference implementation: Mesen2 `BandaiFcg.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Bandai/BandaiFcg.h>
//!
//! ## Hardware behavior
//!
//! Mapper 159 uses the **Bandai LZ93D50** ASIC with a 128-byte **Xicor X24C01**
//! serial EEPROM for save data.  All registers are at `$8000–$FFFF` (same as
//! iNES Mapper 016 submapper 5 / LZ93D50 variant):
//!
//! | Address       | Function                     |
//! |---------------|------------------------------|
//! | `$8000–$8007` | CHR bank 0–7 (1 KiB each)   |
//! | `$8008`       | PRG bank (16 KiB, `$8000`)  |
//! | `$8009`       | Nametable mirroring          |
//! | `$800A`       | IRQ enable                   |
//! | `$800B`       | IRQ counter latch low        |
//! | `$800C`       | IRQ counter latch high       |
//! | `$800D`       | X24C01 EEPROM data (ignored) |
//!
//! - **PRG-ROM:** Up to 256 KiB; 16 KiB switchable at `$8000–$BFFF`, last bank
//!   fixed at `$C000–$FFFF`.
//! - **CHR:** Up to 128 KiB (8 × 1 KiB switchable banks), or CHR-RAM.
//! - **IRQ:** CPU-cycle driven, 16-bit latched counter.
//!
//! ## Known limitations
//!
//! - **X24C01 EEPROM not implemented**: the 128-byte serial EEPROM used for save
//!   data in Dragon Ball Z and SD Gundam titles is not emulated.  Writes to
//!   register `$800D` are silently ignored and reads from `$6000–$7FFF` return
//!   open bus.  Games requiring EEPROM saves cannot persist progress.

use crate::nes::cartridge::bandai::bandai_fcg::{BandaiFcgMapper, BandaiFcgVariant};
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 159;

/// Mapper 159 — thin wrapper around [`BandaiFcgMapper`] (LZ93D50 variant).
///
/// All register, CHR banking, IRQ, and mirroring behaviour is delegated to the
/// Bandai FCG LZ93D50 implementation.  The X24C01 EEPROM is not emulated.
pub struct Mapper159 {
    inner: BandaiFcgMapper,
}

impl Mapper159 {
    pub fn new(ctx: MapperContext) -> Self {
        Self {
            inner: BandaiFcgMapper::new_with_variant(ctx, BandaiFcgVariant::Lz93d50),
        }
    }
}

impl Mapper for Mapper159 {
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

    const PRG_BANKS_16K: usize = 8; // 8 × 16 KiB = 128 KiB
    const CHR_BANKS_1K: usize = 8; // 8 × 1 KiB = 8 KiB

    fn make_mapper() -> Mapper159 {
        Mapper159::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS_16K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_159_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(16 * 1024, PRG_BANKS_16K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 159 must be creatable via factory");
    }

    // ── PRG banking ───────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_bank_0_at_8000_last_bank_fixed_at_c000() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "bank 0 at $8000 on power-on");
        assert_eq!(
            mapper.read_prg(0xC000),
            PRG_BANKS_16K as u8 - 1,
            "last PRG bank fixed at $C000"
        );
    }

    #[test]
    fn register_8008_switches_prg_bank_at_8000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 5);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "bank 5 at $8000 after write to $8008"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            PRG_BANKS_16K as u8 - 1,
            "last bank fixed at $C000 unchanged"
        );
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn registers_8000_to_8007_switch_chr_banks() {
        let mut mapper = make_mapper();
        for slot in 0u16..8 {
            mapper.write_prg(0x8000 + slot, slot as u8);
            let ppu_addr = slot * 0x400;
            assert_eq!(
                mapper.read_chr(ppu_addr),
                slot as u8,
                "CHR slot {slot} should map to bank {slot}"
            );
        }
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn register_8009_controls_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8009, 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8009, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── IRQ ───────────────────────────────────────────────────────────────────

    #[test]
    fn irq_fires_after_countdown_when_enabled() {
        let mut mapper = make_mapper();
        // Set latch to 3, then enable IRQ.
        mapper.write_prg(0x800B, 3); // latch low
        mapper.write_prg(0x800C, 0); // latch high
        mapper.write_prg(0x800A, 1); // enable

        assert!(
            !mapper.irq_pending(),
            "IRQ should not be pending before countdown"
        );
        for _ in 0..3 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ should fire after 3 CPU cycles");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8008, 3);
        mapper.write_prg(0x8009, 1);
        mapper.write_prg(0x8000, 2); // CHR slot 0 → bank 2

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            mapper.read_prg(0x8000),
            "Restored PRG bank must match"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Restored mirroring must match"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            mapper.read_chr(0x0000),
            "Restored CHR bank must match"
        );
    }
}
