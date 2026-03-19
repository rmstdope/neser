//! Mapper 222 – CTC-31 (Dragon Ninja, Don Doko Don 2 / Super Mario Bros. 8)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_222>
//! - Fallback: Mesen2 `Core/NES/Mappers/Unlicensed/Mapper222.h`
//!
//! Known Limitations:
//! - IRQ counter behavior is based on Mesen2 reference implementation; NESdev
//!   notes no better information than FCEUX's source was available at time of
//!   specification.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::A12RisingEdgeDetector;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 222;

/// Mapper 222 – CTC-31
///
/// Used by pirate ports of Dragon Ninja (Namco) and Don Doko Don 2 (Taito).
/// Combines VRC2-style PRG/CHR/mirroring banking with a custom A12-triggered
/// IRQ counter using 7400-series discrete logic.
///
/// ## Register map (address mask `$F003`)
///
/// | Address | Function                                     |
/// |---------|----------------------------------------------|
/// | `$8000` | PRG bank 0 (8 KB at `$8000`)                 |
/// | `$9000` | Mirroring – bit 0: 0=Vertical, 1=Horizontal |
/// | `$A000` | PRG bank 1 (8 KB at `$A000`)                 |
/// | `$B000` | CHR bank 0 (1 KB at `$0000`)                 |
/// | `$B002` | CHR bank 1 (1 KB at `$0400`)                 |
/// | `$C000` | CHR bank 2 (1 KB at `$0800`)                 |
/// | `$C002` | CHR bank 3 (1 KB at `$0C00`)                 |
/// | `$D000` | CHR bank 4 (1 KB at `$1000`)                 |
/// | `$D002` | CHR bank 5 (1 KB at `$1400`)                 |
/// | `$E000` | CHR bank 6 (1 KB at `$1800`)                 |
/// | `$E002` | CHR bank 7 (1 KB at `$1C00`)                 |
/// | `$F000` | IRQ counter load; writing clears pending IRQ |
///
/// ## PRG banking (4 × 8 KB)
///
/// - `$8000` and `$A000`: individually switchable 8 KB windows.
/// - `$C000–$FFFF`: fixed to the last two 8 KB banks of PRG-ROM.
///
/// ## CHR banking (8 × 1 KB)
///
/// Eight individually switchable 1 KB windows covering `$0000–$1FFF`.
///
/// ## Mirroring
///
/// Dynamically controlled. Bit 0 of the value written to `$9000`:
/// 0 = Vertical, 1 = Horizontal.
///
/// ## IRQ
///
/// Counter is loaded by writing to `$F000`; writing also clears the IRQ line.
/// On each PPU A12 rising edge (with debounce), if the counter is non-zero it
/// is incremented. When the counter reaches ≥ 240, IRQ is asserted and the
/// counter resets to 0. Writing 0 to `$F000` disables counting.
///
/// ## Power-on / Reset
///
/// PRG banks 0 and 1 select bank 0; `$C000–$FFFF` fixed to last two banks.
/// CHR banks default to 0. Vertical mirroring. IRQ counter = 0.
pub struct Mapper222 {
    base: BaseMapper,
    irq_counter: u16,
    irq_pending: bool,
    a12: A12RisingEdgeDetector,
}

impl Mapper222 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(1024);
        let mut mapper = Self {
            base,
            irq_counter: 0,
            irq_pending: false,
            a12: A12RisingEdgeDetector::new(3),
        };
        mapper.reset_state();
        mapper
    }

    fn reset_state(&mut self) {
        self.base.select_prg_page(0, 0);
        self.base.select_prg_page(1, 0);
        // Fix last two 8 KB banks to $C000–$FFFF
        self.base.select_prg_page(2, -2);
        self.base.select_prg_page(3, -1);
        for ch in 0..8 {
            self.base.select_chr_page(ch, 0);
        }
        self.base.set_mirroring_hv(false);
        self.irq_counter = 0;
        self.irq_pending = false;
        self.a12 = A12RisingEdgeDetector::new(3);
    }

    fn write_register(&mut self, addr: u16, value: u8) {
        match addr & 0xF003 {
            0x8000 => self.base.select_prg_page(0, value as i16),
            0x9000 => self.base.set_mirroring_hv(value & 0x01 != 0),
            0xA000 => self.base.select_prg_page(1, value as i16),
            0xB000 => self.base.select_chr_page(0, value as i16),
            0xB002 => self.base.select_chr_page(1, value as i16),
            0xC000 => self.base.select_chr_page(2, value as i16),
            0xC002 => self.base.select_chr_page(3, value as i16),
            0xD000 => self.base.select_chr_page(4, value as i16),
            0xD002 => self.base.select_chr_page(5, value as i16),
            0xE000 => self.base.select_chr_page(6, value as i16),
            0xE002 => self.base.select_chr_page(7, value as i16),
            0xF000 => {
                self.irq_counter = u16::from(value);
                self.irq_pending = false;
            }
            _ => {}
        }
    }
}

impl Mapper for Mapper222 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn reset(&mut self) {
        self.reset_state();
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        if addr >= 0x8000 {
            self.write_register(addr, value);
        }
    }

    fn ppu_address_changed(&mut self, addr: u16) {
        if self.a12.update(addr) && self.irq_counter != 0 {
            self.irq_counter += 1;
            if self.irq_counter >= 240 {
                self.irq_pending = true;
                self.irq_counter = 0;
            }
        }
    }

    fn cpu_cycle(&mut self) {
        self.a12.cpu_tick();
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_dynamic_mirroring: true,
            has_chr_banking: true,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // Layout:
        // [0..N]  banking_snapshot() from BaseMapper
        // [N]     irq_counter low byte
        // [N+1]   irq_counter high byte
        // [N+2]   flags: irq_pending(bit0)
        // [N+3]   a12 prev_a12
        // [N+4]   a12 current_a12
        // [N+5]   a12 a12_low_cycles
        let mut snap = self.base.banking_snapshot();
        snap.push((self.irq_counter & 0xFF) as u8);
        snap.push(((self.irq_counter >> 8) & 0xFF) as u8);
        snap.push(self.irq_pending as u8);
        snap.push(self.a12.prev_a12() as u8);
        snap.push(self.a12.current_a12() as u8);
        snap.push(self.a12.a12_low_cycles());
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let banking_len = self.base.banking_snapshot().len();
        if data.len() < banking_len {
            return;
        }
        self.base.restore_banking(&data[..banking_len]);
        if data.len() >= banking_len + 3 {
            self.irq_counter =
                u16::from(data[banking_len]) | (u16::from(data[banking_len + 1]) << 8);
            self.irq_pending = data[banking_len + 2] != 0;
        }
        if data.len() >= banking_len + 6 {
            self.a12.set_prev_a12(data[banking_len + 3] != 0);
            self.a12.set_current_a12(data[banking_len + 4] != 0);
            self.a12.set_a12_low_cycles(data[banking_len + 5]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// 11 × 8 KB PRG banks (non-power-of-two to prevent silent modulo wrap)
    const PRG_BANKS: usize = 11;
    /// 11 × 1 KB CHR banks
    const CHR_BANKS: usize = 11;

    fn make_mapper(prg_rom: Vec<u8>, chr_rom: Vec<u8>) -> Mapper222 {
        Mapper222::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ))
    }

    fn make_default_mapper() -> Mapper222 {
        make_mapper(
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
        )
    }

    fn trigger_a12_rising_edge(mapper: &mut Mapper222) {
        // Bring A12 low with enough CPU cycles for debounce, then high
        mapper.ppu_address_changed(0x0000);
        for _ in 0..4 {
            mapper.cpu_cycle();
        }
        mapper.ppu_address_changed(0x1000);
    }

    // ─── Factory registration ────────────────────────────────────────────────

    #[test]
    fn mapper_222_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 222 should be registered in factory");
    }

    // ─── Power-on state ──────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_bank0_window_at_bank_0() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should start at PRG bank 0 on power-on"
        );
    }

    #[test]
    fn power_on_prg_bank1_window_at_bank_0() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.read_prg(0xA000),
            0,
            "$A000 should start at PRG bank 0 on power-on"
        );
    }

    #[test]
    fn power_on_upper_fixed_at_last_two_banks() {
        let mapper = make_default_mapper();
        // With PRG_BANKS=11, last two banks are 9 and 10
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "$C000 should be fixed to second-to-last PRG bank"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 should be fixed to last PRG bank"
        );
    }

    #[test]
    fn power_on_chr_all_banks_at_0() {
        let mut mapper = make_default_mapper();
        for offset in [
            0x0000u16, 0x0400, 0x0800, 0x0C00, 0x1000, 0x1400, 0x1800, 0x1C00,
        ] {
            assert_eq!(
                mapper.read_chr(offset),
                0,
                "CHR at ${offset:04X} should be bank 0 on power-on"
            );
        }
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_default_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring should be Vertical on power-on"
        );
    }

    #[test]
    fn power_on_irq_not_pending() {
        let mapper = make_default_mapper();
        assert!(
            !mapper.irq_pending(),
            "IRQ should not be pending on power-on"
        );
    }

    #[test]
    fn power_on_has_irq_capability() {
        let mapper = make_default_mapper();
        assert!(
            mapper.capabilities().has_irq,
            "Mapper 222 must report has_irq capability"
        );
    }

    // ─── PRG banking ($8000 and $A000) ───────────────────────────────────────

    #[test]
    fn write_8000_selects_prg_bank0_window() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 5);
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "$8000 write should switch PRG bank 0 window"
        );
    }

    #[test]
    fn write_a000_selects_prg_bank1_window() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xA000, 3);
        assert_eq!(
            mapper.read_prg(0xA000),
            3,
            "$A000 write should switch PRG bank 1 window"
        );
    }

    #[test]
    fn prg_bank0_write_does_not_affect_bank1_window() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xA000, 3);
        mapper.write_prg(0x8000, 5);
        assert_eq!(
            mapper.read_prg(0xA000),
            3,
            "Writing $8000 must not change $A000 PRG window"
        );
    }

    #[test]
    fn fixed_upper_banks_unaffected_by_prg_register_writes() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0xA000, 3);
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "Fixed $C000 bank must not change on register write"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "Fixed $E000 bank must not change on register write"
        );
    }

    // ─── CHR banking ─────────────────────────────────────────────────────────

    #[test]
    fn write_b000_selects_chr_bank0() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xB000, 3);
        assert_eq!(mapper.read_chr(0x0000), 3, "$B000 write sets CHR bank 0");
    }

    #[test]
    fn write_b002_selects_chr_bank1() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xB002, 4);
        assert_eq!(mapper.read_chr(0x0400), 4, "$B002 write sets CHR bank 1");
    }

    #[test]
    fn write_c000_selects_chr_bank2() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xC000, 2);
        assert_eq!(mapper.read_chr(0x0800), 2, "$C000 write sets CHR bank 2");
    }

    #[test]
    fn write_c002_selects_chr_bank3() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xC002, 5);
        assert_eq!(mapper.read_chr(0x0C00), 5, "$C002 write sets CHR bank 3");
    }

    #[test]
    fn write_d000_selects_chr_bank4() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xD000, 6);
        assert_eq!(mapper.read_chr(0x1000), 6, "$D000 write sets CHR bank 4");
    }

    #[test]
    fn write_d002_selects_chr_bank5() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xD002, 7);
        assert_eq!(mapper.read_chr(0x1400), 7, "$D002 write sets CHR bank 5");
    }

    #[test]
    fn write_e000_selects_chr_bank6() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xE000, 8);
        assert_eq!(mapper.read_chr(0x1800), 8, "$E000 write sets CHR bank 6");
    }

    #[test]
    fn write_e002_selects_chr_bank7() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xE002, 9);
        assert_eq!(mapper.read_chr(0x1C00), 9, "$E002 write sets CHR bank 7");
    }

    #[test]
    fn chr_banks_are_independent() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xB000, 1);
        mapper.write_prg(0xB002, 2);
        mapper.write_prg(0xC000, 3);
        assert_eq!(mapper.read_chr(0x0000), 1, "CHR bank 0 independent");
        assert_eq!(mapper.read_chr(0x0400), 2, "CHR bank 1 independent");
        assert_eq!(mapper.read_chr(0x0800), 3, "CHR bank 2 independent");
    }

    // ─── Mirroring ($9000) ───────────────────────────────────────────────────

    #[test]
    fn write_9000_bit0_clear_selects_vertical() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x9000, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit0=0 → Vertical mirroring"
        );
    }

    #[test]
    fn write_9000_bit0_set_selects_horizontal() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit0=1 → Horizontal mirroring"
        );
    }

    #[test]
    fn mirroring_can_be_toggled() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0x9000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ─── IRQ ($F000) ─────────────────────────────────────────────────────────

    #[test]
    fn write_f000_nonzero_enables_irq_counter() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xF000, 100);
        // Counter started; should not be pending yet
        assert!(!mapper.irq_pending(), "IRQ should not fire immediately");
    }

    #[test]
    fn write_f000_zero_disables_irq_counting() {
        let mut mapper = make_default_mapper();
        // Start counting, then disable
        mapper.write_prg(0xF000, 100);
        mapper.write_prg(0xF000, 0);
        // Trigger many A12 edges — counter should stay at 0, no IRQ
        for _ in 0..250 {
            trigger_a12_rising_edge(&mut mapper);
        }
        assert!(!mapper.irq_pending(), "IRQ must not fire when counter=0");
    }

    #[test]
    fn irq_fires_after_counter_reaches_240() {
        let mut mapper = make_default_mapper();
        // Start counter at 1; needs 239 more increments to reach 240
        mapper.write_prg(0xF000, 1);
        for _ in 0..238 {
            trigger_a12_rising_edge(&mut mapper);
            assert!(
                !mapper.irq_pending(),
                "IRQ must not fire before reaching 240"
            );
        }
        trigger_a12_rising_edge(&mut mapper); // counter = 240 → fire
        assert!(
            mapper.irq_pending(),
            "IRQ must fire when counter reaches 240"
        );
    }

    #[test]
    fn irq_counter_resets_to_zero_after_firing() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xF000, 1);
        for _ in 0..239 {
            trigger_a12_rising_edge(&mut mapper);
        }
        assert!(mapper.irq_pending());
        // Writing 0 to $F000 clears IRQ; counter stays 0 so no new counting
        mapper.write_prg(0xF000, 0);
        assert!(!mapper.irq_pending(), "Writing $F000 clears IRQ");
    }

    #[test]
    fn write_f000_clears_pending_irq() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xF000, 1);
        for _ in 0..239 {
            trigger_a12_rising_edge(&mut mapper);
        }
        assert!(mapper.irq_pending());
        mapper.write_prg(0xF000, 50); // reloads counter, clears IRQ
        assert!(
            !mapper.irq_pending(),
            "Writing $F000 with any value clears IRQ"
        );
    }

    #[test]
    fn irq_high_start_value_fires_on_first_edge_at_or_above_239() {
        let mut mapper = make_default_mapper();
        // Counter starts at 239; after 1 edge: 240 → fire
        mapper.write_prg(0xF000, 239);
        trigger_a12_rising_edge(&mut mapper);
        assert!(
            mapper.irq_pending(),
            "Counter starting at 239 fires on first A12 edge (239+1=240)"
        );
    }

    // ─── Reset ───────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_prg_banking() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0xA000, 3);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 reset to bank 0");
        assert_eq!(mapper.read_prg(0xA000), 0, "$A000 reset to bank 0");
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "$C000 fixed after reset"
        );
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 fixed after reset"
        );
    }

    #[test]
    fn reset_restores_chr_banking() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xB000, 5);
        mapper.reset();
        assert_eq!(mapper.read_chr(0x0000), 0, "CHR bank 0 reset to 0");
    }

    #[test]
    fn reset_restores_mirroring() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0x9000, 0x01);
        mapper.reset();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring restored to Vertical after reset"
        );
    }

    #[test]
    fn reset_clears_irq() {
        let mut mapper = make_default_mapper();
        mapper.write_prg(0xF000, 1);
        for _ in 0..239 {
            trigger_a12_rising_edge(&mut mapper);
        }
        assert!(mapper.irq_pending());
        mapper.reset();
        assert!(!mapper.irq_pending(), "IRQ cleared after reset");
    }

    // ─── Save state ──────────────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_and_restore_banking() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        let mut mapper = make_mapper(prg.clone(), chr.clone());
        mapper.write_prg(0x8000, 5);
        mapper.write_prg(0xA000, 3);
        mapper.write_prg(0xB000, 7);
        mapper.write_prg(0xE002, 9);
        mapper.write_prg(0x9000, 0x01);

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper(prg, chr);
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 5, "PRG bank 0 after restore");
        assert_eq!(restored.read_prg(0xA000), 3, "PRG bank 1 after restore");
        assert_eq!(restored.read_chr(0x0000), 7, "CHR bank 0 after restore");
        assert_eq!(restored.read_chr(0x1C00), 9, "CHR bank 7 after restore");
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring after restore"
        );
    }

    #[test]
    fn registers_snapshot_and_restore_irq_state() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        let mut mapper = make_mapper(prg.clone(), chr.clone());
        mapper.write_prg(0xF000, 1);
        for _ in 0..239 {
            trigger_a12_rising_edge(&mut mapper);
        }
        assert!(mapper.irq_pending());

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper(prg, chr);
        restored.restore_registers(&snap);

        assert!(restored.irq_pending(), "IRQ pending state restored");
    }
}
