//! Mapper 055 – BTL-MARIOBABY (Mario Baby Bootleg)
//!
//! Specifications:
//! - Fallback: MAME/HBMAME `bootleg.cpp` (`nes_mbaby_device`)
//!   (NesDev wiki unavailable due to network restriction)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

use crate::nes::cartridge::cpu_cycle_irq::{CpuCycleIrq, CpuCycleIrqMode};

/// Mapper 055 – BTL-MARIOBABY
///
/// Hardware: Bootleg cartridge PCB used by Mario Baby.
///
/// Specifications:
/// - Fallback source: MAME `nes_mbaby_device` in `bootleg.cpp`
/// - PRG-ROM: 32 KiB fixed at $8000–$FFFF (last 32 KiB bank); 8 KiB switchable
///   bank at $6000–$7FFF selected by register at $F000.
/// - PRG-RAM: None
/// - CHR: 8 KiB fixed (no switching)
/// - Mirroring: H/V switchable via register at $F001 (bit 3: 0=Vertical, 1=Horizontal)
/// - IRQ: 15-bit CPU-cycle counter; asserted while counter >= $6000.
///   Controlled by register at $F002 (bit 1: 1=enable, 0=disable+ack+reset).
/// - Bus conflicts: None
///
/// Register map ($F000–$FFFF, write-only, decoded by addr & 0x03):
/// - $F000 (offset & 0x03 == 0): PRG latch — 8 KiB bank number for $6000 window
/// - $F001 (offset & 0x03 == 1): Mirroring — bit 3: 0=Vertical, 1=Horizontal
/// - $F002 (offset & 0x03 == 2): IRQ control — bit 1: enable
///
/// Power-on state: PRG latch = 0, mirroring from header, IRQ disabled.
pub struct Mapper55 {
    base: BaseMapper,
    prg_bank: u8,
    irq: CpuCycleIrq,
    initial_mirroring: NametableLayout,
}

impl Mapper55 {
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const CHR_BANK_SIZE: usize = 0x2000; // 8 KiB

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let mirroring = ctx.mirroring;
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 8,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(Self::PRG_BANK_SIZE);
        base.configure_prg_6000_banking();
        base.configure_chr_banking(Self::CHR_BANK_SIZE);
        base.set_mirroring(mirroring);

        let mut mapper = Self {
            base,
            prg_bank: 0,
            irq: CpuCycleIrq::new(CpuCycleIrqMode::UpLevel {
                threshold: 0x6000,
                mask: 0x7FFF,
            }),
            initial_mirroring: mirroring,
        };

        mapper.update_banks();
        mapper
    }

    fn update_banks(&mut self) {
        // $6000–$7FFF: switchable 8 KiB PRG-ROM bank
        self.base.select_prg_6000_page(self.prg_bank as i16);

        // $8000–$FFFF: fixed last 4 × 8 KiB banks
        self.base.select_prg_page(0, -4);
        self.base.select_prg_page(1, -3);
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);

        // CHR: single fixed 8 KiB bank (bank 0)
        self.base.select_chr_page(0, 0);
    }
}

impl Mapper for Mapper55 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    /// Registers are only decoded at $F000–$FFFF (addr >= $F000).
    /// The register index is addr & 0x03.
    fn write_prg(&mut self, addr: u16, value: u8) {
        if addr < 0xF000 {
            return;
        }
        match addr & 0x03 {
            0x00 => {
                // PRG latch: selects 8 KiB PRG bank at $6000–$7FFF
                self.prg_bank = value;
                self.base.select_prg_6000_page(value as i16);
            }
            0x01 => {
                // Mirroring: bit 3 (0=Vertical, 1=Horizontal)
                self.base.set_mirroring_hv((value & 0x08) != 0);
            }
            0x02 => {
                // IRQ control: bit 1 enables; disabling also acknowledges and resets counter
                self.irq.set_enabled((value & 0x02) != 0);
                if !self.irq.enabled() {
                    self.irq.acknowledge();
                    self.irq.set_counter(0);
                }
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        self.irq.tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq.is_pending()
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout: [0] prg_bank,
        //         [1] flags (irq_enabled | irq_pending<<1),
        //         [2-3] irq_counter (little-endian),
        //         [4] mirroring (0=Vertical, 1=Horizontal)
        let flags = (self.irq.enabled() as u8) | ((self.irq.is_pending() as u8) << 1);
        let mirror_byte = matches!(self.base.mirroring(), NametableLayout::Horizontal) as u8;
        let counter = self.irq.counter();
        vec![
            self.prg_bank,
            flags,
            (counter & 0xFF) as u8,
            (counter >> 8) as u8,
            mirror_byte,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 5 {
            self.prg_bank = data[0];
            self.irq.set_enabled((data[1] & 1) != 0);
            self.irq.set_pending((data[1] & 2) != 0);
            self.irq
                .set_counter((data[2] as u16) | ((data[3] as u16) << 8));
            self.base.set_mirroring_hv(data[4] != 0);
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.irq.set_enabled(false);
        self.irq.acknowledge();
        self.irq.set_counter(0);
        self.base.set_mirroring(self.initial_mirroring);
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to avoid false-pass via modulo wrapping.
    // 24 banks × 8KB = 192 KB PRG (24 is non-power-of-two).
    const PRG_BANKS: usize = 24;
    const CHR_BANKS: usize = 1; // Fixed, no banking

    fn make_mapper() -> Mapper55 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        Mapper55::new(MapperContext::new_for_test(
            55,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ──────────────────────────────────────────────────────────

    #[test]
    fn mapper_55_is_registered() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(8 * 1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            55,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 55 must be registered in the factory"
        );
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_6000_is_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "$6000 window must default to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_8000_fixed_to_last_32kb() {
        let mapper = make_mapper();
        // $8000–$9FFF = bank PRG_BANKS-4
        let expected = (PRG_BANKS - 4) as u8;
        assert_eq!(
            mapper.read_prg(0x8000),
            expected,
            "$8000 must be fixed to 4th-from-last 8KB bank"
        );
        // $E000–$FFFF = last bank
        let expected_last = (PRG_BANKS - 1) as u8;
        assert_eq!(
            mapper.read_prg(0xE000),
            expected_last,
            "$E000 must be fixed to last 8KB bank"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR must be fixed to bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_irq_not_pending() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    // ── PRG bank switching at $6000–$7FFF ────────────────────────────────────

    #[test]
    fn prg_6000_window_switches_on_f000_write() {
        let mut mapper = make_mapper();
        // Write bank 5 to register $F000
        mapper.write_prg(0xF000, 5);
        assert_eq!(
            mapper.read_prg(0x6000),
            5,
            "$6000 window must switch to bank 5 after writing $F000"
        );
    }

    #[test]
    fn prg_6000_window_switches_on_f004_write() {
        let mut mapper = make_mapper();
        // $F004 has addr & 0x03 == 0, same register
        mapper.write_prg(0xF004, 7);
        assert_eq!(
            mapper.read_prg(0x6000),
            7,
            "$6000 window must switch to bank 7 after writing $F004"
        );
    }

    #[test]
    fn prg_6000_window_covers_full_8kb() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 3);
        assert_eq!(mapper.read_prg(0x6000), 3, "start of $6000 window");
        assert_eq!(mapper.read_prg(0x7FFF), 3, "end of $7FFF window");
    }

    #[test]
    fn prg_8000_fixed_unaffected_by_latch_write() {
        let mut mapper = make_mapper();
        let before = mapper.read_prg(0x8000);
        mapper.write_prg(0xF000, 15);
        assert_eq!(
            mapper.read_prg(0x8000),
            before,
            "$8000 fixed window must not change after latch write"
        );
    }

    #[test]
    fn registers_below_f000_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE000, 5);
        assert_eq!(
            mapper.read_prg(0x6000),
            0,
            "Writes below $F000 must be ignored (PRG bank stays 0)"
        );
    }

    // ── CHR: no banking ───────────────────────────────────────────────────────

    #[test]
    fn chr_is_always_bank_0() {
        let mut mapper = make_mapper();
        // Writing the latch register must not affect CHR
        mapper.write_prg(0xF000, 0xFF);
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR must remain bank 0 regardless of latch value"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn mirroring_defaults_from_header() {
        let mapper = make_mapper(); // created with Vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must reflect header at power-on"
        );
    }

    #[test]
    fn write_f001_bit3_set_selects_horizontal() {
        let mut mapper = make_mapper(); // starts Vertical
        mapper.write_prg(0xF001, 0x08);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Writing $F001 with bit 3 set must switch to Horizontal"
        );
    }

    #[test]
    fn write_f001_bit3_clear_selects_vertical() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF001, 0x08); // set horizontal
        mapper.write_prg(0xF001, 0x00); // clear back to vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Writing $F001 with bit 3 clear must switch to Vertical"
        );
    }

    // ── IRQ ───────────────────────────────────────────────────────────────────

    #[test]
    fn irq_does_not_fire_while_disabled() {
        let mut mapper = make_mapper();
        for _ in 0..0x8000 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire while IRQ is disabled"
        );
    }

    #[test]
    fn irq_fires_after_0x6000_cycles_when_enabled() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF002, 0x02); // enable IRQ
        for _ in 0..0x5FFF {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before reaching threshold"
        );
        mapper.cpu_cycle(); // 0x6000th cycle
        assert!(mapper.irq_pending(), "IRQ must fire at cycle 0x6000");
    }

    #[test]
    fn irq_disable_clears_pending_and_resets_counter() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF002, 0x02); // enable
        for _ in 0..0x6000 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ should be pending before disable");

        mapper.write_prg(0xF002, 0x00); // disable → should ack and reset
        assert!(!mapper.irq_pending(), "IRQ must be cleared after disabling");

        // Re-enable and verify counter was reset (needs 0x6000 new cycles)
        mapper.write_prg(0xF002, 0x02); // re-enable
        for _ in 0..0x5FFF {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before threshold after counter reset"
        );
    }

    #[test]
    fn irq_register_decoded_at_f002_and_multiples() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF006, 0x02); // addr & 0x03 == 2 → IRQ register
        for _ in 0..0x6000 {
            mapper.cpu_cycle();
        }
        assert!(
            mapper.irq_pending(),
            "IRQ enable at $F006 (addr & 3 == 2) must work"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_returns_to_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 10); // change PRG bank
        mapper.write_prg(0xF001, 0x08); // change mirroring
        mapper.write_prg(0xF002, 0x02); // enable IRQ
        for _ in 0..0x6000 {
            mapper.cpu_cycle();
        }
        mapper.reset();
        assert_eq!(mapper.read_prg(0x6000), 0, "PRG latch must reset to 0");
        assert!(!mapper.irq_pending(), "IRQ must not be pending after reset");
        assert_eq!(
            mapper.read_prg(0x8000),
            (PRG_BANKS - 4) as u8,
            "$8000 must still be fixed to last-4 bank after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must reset to header value (Vertical) after reset"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_round_trips() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF000, 7); // PRG bank 7
        mapper.write_prg(0xF001, 0x08); // horizontal
        mapper.write_prg(0xF002, 0x02); // enable IRQ
        for _ in 0..0x3000 {
            mapper.cpu_cycle(); // advance counter
        }

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x6000),
            mapper.read_prg(0x6000),
            "Restored PRG bank must match"
        );
        assert_eq!(
            restored.get_mirroring(),
            mapper.get_mirroring(),
            "Restored mirroring must match"
        );
        assert_eq!(
            restored.irq_pending(),
            mapper.irq_pending(),
            "Restored IRQ pending state must match"
        );
    }

    // ── CHR-RAM fallback ──────────────────────────────────────────────────────

    #[test]
    fn chr_ram_works_when_no_chr_rom() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let mut mapper = Mapper55::new(MapperContext::new_for_test(
            55,
            prg,
            vec![],
            NametableLayout::Vertical,
        ));
        mapper.write_chr(0x0100, 0xAB);
        assert_eq!(
            mapper.read_chr(0x0100),
            0xAB,
            "CHR-RAM must be writable when no CHR-ROM is present"
        );
    }
}
