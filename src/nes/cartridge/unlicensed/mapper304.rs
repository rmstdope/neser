//! Mapper 304 – SMB2J (Super Mario Bros. 2 pirate port)
//!
//! Specifications:
//! - Primary reference: NesDev wiki (unavailable, HTTP 403).
//! - Fallback: Mesen2 `Core/NES/Mappers/Whirlwind/Smb2j.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Whirlwind/Smb2j.h>
//!
//! # Hardware overview
//!
//! Used for a pirate port of Super Mario Bros. 2 (the Japanese original, later
//! known as "The Lost Levels" in the West).
//!
//! - PRG-ROM: 8 × 4 KiB pages covering $8000–$FFFF, plus 3 fixed pages at
//!   $5000–$7FFF (always the last 3 pages of PRG-ROM).
//! - CHR-ROM: single 8 KiB bank, unbanked.
//! - Mirroring: fixed from ROM header.
//! - IRQ: CPU-clock counter. Writing any value with bits [1:0] != 0 to $4122
//!   enables the counter and resets it; writing bits [1:0] == 0 disables it.
//!   The counter fires (and disables itself) after 4096 CPU clocks.
//! - PRG-RAM: none.
//! - Bus conflicts: none.
//!
//! # Register map
//!
//! | Address | Condition          | Effect                                                  |
//! |---------|--------------------|---------------------------------------------------------|
//! | $4022   | PRG-ROM >= 64 KiB  | bit 0: 0 → banks 0-7 at $8000-$FFFF; 1 → banks 4-11    |
//! | $4122   | always             | bits [1:0]: enable (≠0) / disable (=0) IRQ, reset ctr  |
//!
//! # Memory map
//!
//! ```text
//! CPU $5000–$5FFF  PRG-ROM 4 KiB, fixed to page count-3
//! CPU $6000–$6FFF  PRG-ROM 4 KiB, fixed to page count-2
//! CPU $7000–$7FFF  PRG-ROM 4 KiB, fixed to page count-1
//! CPU $8000–$BFFF  PRG-ROM 4 × 4 KiB switchable (banks 0-3 or 4-7)
//! CPU $C000–$FFFF  PRG-ROM 4 × 4 KiB switchable (banks 4-7 or 8-11)
//! PPU $0000–$1FFF  CHR-ROM 8 KiB, bank 0 (fixed)
//! ```
//!
//! # Power-on / reset state
//!
//! - $8000–$BFFF: banks 0-3; $C000–$FFFF: banks 4-7.
//! - $5000–$7FFF: last 3 pages (fixed).
//! - CHR: bank 0.
//! - IRQ: disabled, counter = 0.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 304;
const PRG_PAGE_SIZE: usize = 4 * 1024; // 4 KiB
const PRG_SLOTS: usize = 8; // 8 × 4 KiB covers $8000–$FFFF
/// PRG-ROM size threshold above which the $4022 bank-select register is active.
const LARGE_PRG_THRESHOLD: usize = 64 * 1024;
/// IRQ fires after this many CPU clock increments.
const IRQ_PERIOD: u16 = 0x1000; // 4096

/// Snapshot byte layout: [prg_pages × 8][irq_counter_lo][irq_counter_hi][irq_enabled][irq_pending][prg_bank_select]
/// CHR is unbanked (not in banking snapshot).
const BANKING_BYTES: usize = PRG_SLOTS; // 8 PRG pages only

/// Mapper 304 – SMB2J
pub struct Mapper304 {
    base: BaseMapper,
    irq_counter: u16,
    irq_enabled: bool,
    irq_pending: bool,
    /// Bit 0 of the last $4022 write (only meaningful when PRG >= 64 KiB).
    prg_bank_select: u8,
    /// Whether the $4022 register is active (PRG-ROM >= 64 KiB).
    large_prg: bool,
}

impl Mapper304 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let large_prg = ctx.prg_rom.len() >= LARGE_PRG_THRESHOLD;
        let capabilities = MapperCapabilities {
            has_irq: true,
            has_chr_banking: false,
            prg_bank_size_kb: 4,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_PAGE_SIZE);

        let mut mapper = Self {
            base,
            irq_counter: 0,
            irq_enabled: false,
            irq_pending: false,
            prg_bank_select: 0,
            large_prg,
        };
        mapper.set_power_on_state();
        mapper
    }

    fn set_power_on_state(&mut self) {
        self.prg_bank_select = 0;
        self.irq_counter = 0;
        self.irq_enabled = false;
        self.irq_pending = false;
        self.apply_prg_banking();
    }

    /// Apply PRG banking to $8000–$FFFF based on `prg_bank_select`.
    ///
    /// - bit 0 = 0: banks 0-3 at $8000-$BFFF, banks 4-7 at $C000-$FFFF.
    /// - bit 0 = 1: banks 4-7 at $8000-$BFFF, banks 8-11 at $C000-$FFFF.
    fn apply_prg_banking(&mut self) {
        let start = (self.prg_bank_select as i16) << 2;
        for i in 0..4i16 {
            self.base.select_prg_page(i as usize, start + i);
            self.base.select_prg_page(i as usize + 4, start + 4 + i);
        }
    }
}

impl Mapper for Mapper304 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x5000..=0x7FFF => {
                let prg = self.base.prg_rom();
                let n = prg.len() / PRG_PAGE_SIZE;
                if n < 3 {
                    return 0;
                }
                let offset = (addr - 0x5000) as usize;
                prg.get((n - 3) * PRG_PAGE_SIZE + offset)
                    .copied()
                    .unwrap_or(0)
            }
            0x8000..=0xFFFF => self.base.read_prg_banked(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            0x5000..=0x7FFF => self.read_prg(addr),
            _ => self
                .base
                .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr {
            0x4022 if self.large_prg => {
                self.prg_bank_select = value & 0x01;
                self.apply_prg_banking();
            }
            0x4122 => {
                self.irq_enabled = (value & 0x03) != 0;
                self.irq_counter = 0;
                self.irq_pending = false;
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        if self.irq_enabled {
            self.irq_counter = (self.irq_counter + 1) & (IRQ_PERIOD - 1);
            if self.irq_counter == 0 {
                self.irq_enabled = false;
                self.irq_pending = true;
            }
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_pending
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut data = self.base.banking_snapshot();
        data.push((self.irq_counter & 0xFF) as u8);
        data.push((self.irq_counter >> 8) as u8);
        data.push(self.irq_enabled as u8);
        data.push(self.irq_pending as u8);
        data.push(self.prg_bank_select);
        data
    }

    fn restore_registers(&mut self, data: &[u8]) {
        self.base.restore_banking(data);
        if data.len() >= BANKING_BYTES + 5 {
            self.irq_counter =
                (data[BANKING_BYTES] as u16) | ((data[BANKING_BYTES + 1] as u16) << 8);
            self.irq_enabled = data[BANKING_BYTES + 2] != 0;
            self.irq_pending = data[BANKING_BYTES + 3] != 0;
            self.prg_bank_select = data[BANKING_BYTES + 4] & 0x01;
        }
    }

    fn reset(&mut self) {
        self.set_power_on_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank counts to prevent modulo false-pass.
    const PRG_BANKS_SMALL: usize = 11; // 11 × 4 KiB = 44 KiB (< 64 KiB, no $4022)
    const PRG_BANKS_LARGE: usize = 21; // 21 × 4 KiB = 84 KiB (>= 64 KiB, $4022 active)
    const CHR_SIZE: usize = 8 * 1024; // single 8 KiB CHR bank

    fn make_mapper_small() -> Mapper304 {
        Mapper304::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_PAGE_SIZE, PRG_BANKS_SMALL),
            vec![0u8; CHR_SIZE],
            NametableLayout::Vertical,
        ))
    }

    fn make_mapper_large() -> Mapper304 {
        Mapper304::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_PAGE_SIZE, PRG_BANKS_LARGE),
            vec![0u8; CHR_SIZE],
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_304_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_PAGE_SIZE, PRG_BANKS_SMALL),
            vec![0u8; CHR_SIZE],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 304 must be registered in factory");
    }

    // ── Power-on state ────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_is_bank_0() {
        let mapper = make_mapper_small();
        // banked_data fills page N with byte N
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should be page 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_is_bank_4() {
        let mapper = make_mapper_small();
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 should be page 4 at power-on"
        );
    }

    #[test]
    fn power_on_chr_is_bank_0() {
        let mut mapper = make_mapper_small();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_irq_not_pending() {
        let mapper = make_mapper_small();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    // ── Fixed PRG pages at $5000–$7FFF ────────────────────────────────────────

    #[test]
    fn fixed_page_at_5000_is_last_minus_3() {
        // PRG_BANKS_SMALL = 11 → page count-3 = page 8
        let mapper = make_mapper_small();
        assert_eq!(
            mapper.read_prg(0x5000),
            (PRG_BANKS_SMALL - 3) as u8,
            "$5000 should map to page count-3"
        );
    }

    #[test]
    fn fixed_page_at_6000_is_last_minus_1() {
        let mapper = make_mapper_small();
        assert_eq!(
            mapper.read_prg(0x6000),
            (PRG_BANKS_SMALL - 2) as u8,
            "$6000 should map to page count-2"
        );
    }

    #[test]
    fn fixed_page_at_7000_is_last() {
        let mapper = make_mapper_small();
        assert_eq!(
            mapper.read_prg(0x7000),
            (PRG_BANKS_SMALL - 1) as u8,
            "$7000 should map to page count-1"
        );
    }

    #[test]
    fn fixed_pages_do_not_change_after_prg_banking() {
        // After changing banking via $4022 on large ROM, $5000-$7FFF must stay fixed.
        let mut mapper = make_mapper_large();
        mapper.write_prg(0x4022, 0x01); // switch to bank group 1
        assert_eq!(
            mapper.read_prg(0x5000),
            (PRG_BANKS_LARGE - 3) as u8,
            "$5000 must stay fixed to page count-3 after bank switch"
        );
        assert_eq!(
            mapper.read_prg(0x6000),
            (PRG_BANKS_LARGE - 2) as u8,
            "$6000 must stay fixed to page count-2 after bank switch"
        );
        assert_eq!(
            mapper.read_prg(0x7000),
            (PRG_BANKS_LARGE - 1) as u8,
            "$7000 must stay fixed to page count-1 after bank switch"
        );
    }

    // ── PRG banking ($4022, large ROM only) ───────────────────────────────────

    #[test]
    fn large_rom_default_banking_matches_small_rom() {
        let mapper = make_mapper_large();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 = page 0 on large ROM default"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 = page 4 on large ROM default"
        );
    }

    #[test]
    fn register_4022_bit0_set_shifts_prg_banking_by_4() {
        let mut mapper = make_mapper_large();
        mapper.write_prg(0x4022, 0x01);
        // bit 0 = 1 → start = 4 → $8000-$BFFF = pages 4-7, $C000-$FFFF = pages 8-11
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "$8000 should be page 4 after $4022=1"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            8,
            "$C000 should be page 8 after $4022=1"
        );
    }

    #[test]
    fn register_4022_bit0_clear_restores_default_banking() {
        let mut mapper = make_mapper_large();
        mapper.write_prg(0x4022, 0x01);
        mapper.write_prg(0x4022, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should be page 0 after $4022=0"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 should be page 4 after $4022=0"
        );
    }

    #[test]
    fn register_4022_ignored_on_small_rom() {
        let mut mapper = make_mapper_small();
        mapper.write_prg(0x4022, 0x01); // should be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$4022 must be ignored when PRG < 64 KiB"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "$C000 must remain page 4 when $4022 is ignored"
        );
    }

    #[test]
    fn prg_8000_to_bfff_pages_are_consecutive() {
        let mapper = make_mapper_small();
        // Pages 0-3 map to $8000-$BFFF (each 4 KiB)
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0x9000), 1);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xB000), 3);
    }

    #[test]
    fn prg_c000_to_ffff_pages_are_consecutive() {
        let mapper = make_mapper_small();
        // Pages 4-7 map to $C000-$FFFF (each 4 KiB)
        assert_eq!(mapper.read_prg(0xC000), 4);
        assert_eq!(mapper.read_prg(0xD000), 5);
        assert_eq!(mapper.read_prg(0xE000), 6);
        assert_eq!(mapper.read_prg(0xF000), 7);
    }

    // ── IRQ: $4122 register ───────────────────────────────────────────────────

    #[test]
    fn write_4122_with_nonzero_bits_enables_irq() {
        let mut mapper = make_mapper_small();
        mapper.write_prg(0x4122, 0x01);
        // advance 4095 cycles — no IRQ yet
        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before 4096 cycles"
        );
        mapper.cpu_cycle(); // cycle 4096
        assert!(mapper.irq_pending(), "IRQ must fire after 4096 CPU cycles");
    }

    #[test]
    fn write_4122_resets_counter_and_clears_irq() {
        let mut mapper = make_mapper_small();
        mapper.write_prg(0x4122, 0x01);
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ should be pending");
        mapper.write_prg(0x4122, 0x01); // re-enable resets counter and clears IRQ
        assert!(
            !mapper.irq_pending(),
            "IRQ should be cleared after re-writing $4122"
        );
    }

    #[test]
    fn write_4122_zero_disables_irq() {
        let mut mapper = make_mapper_small();
        mapper.write_prg(0x4122, 0x01); // enable
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        mapper.write_prg(0x4122, 0x00); // disable
        // counter is reset; advancing 4096 more should NOT fire
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire when disabled via $4122=0"
        );
    }

    #[test]
    fn irq_disables_itself_after_firing() {
        let mut mapper = make_mapper_small();
        mapper.write_prg(0x4122, 0x03);
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ fired");
        // Advance another full period — IRQ should not fire again (self-disabled)
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        // IRQ remains pending (not cleared by further cycles — must be cleared via $4122)
        assert!(
            mapper.irq_pending(),
            "IRQ should remain pending until $4122 is written"
        );
    }

    #[test]
    fn irq_uses_bits_1_0_of_4122() {
        let mut mapper = make_mapper_small();
        mapper.write_prg(0x4122, 0x02); // bit 1 set, bit 0 clear → still enables
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ must fire when bits[1:0] != 0");
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_banking_and_irq() {
        let mut mapper = make_mapper_large();
        mapper.write_prg(0x4022, 0x01); // bank group 1
        mapper.write_prg(0x4122, 0x01); // enable IRQ
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper_large();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            4,
            "restored: $8000 should be page 4"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            8,
            "restored: $C000 should be page 8"
        );
        assert!(
            !restored.irq_pending(),
            "restored: IRQ should not be pending"
        );
    }

    #[test]
    fn snapshot_restore_preserves_irq_pending() {
        let mut mapper = make_mapper_small();
        mapper.write_prg(0x4122, 0x01);
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper_small();
        restored.restore_registers(&snap);

        assert!(
            restored.irq_pending(),
            "restored: IRQ pending state should be preserved"
        );
    }

    #[test]
    fn restore_with_empty_data_is_noop() {
        let mut mapper = make_mapper_small();
        mapper.write_prg(0x4122, 0x01);
        mapper.restore_registers(&[]); // too short — must be ignored
        // Banking should still be default since restore is a noop
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "state must be unchanged after empty restore"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper_large();
        mapper.write_prg(0x4022, 0x01);
        mapper.write_prg(0x4122, 0x01);
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG $8000 should be page 0 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "PRG $C000 should be page 4 after reset"
        );
        assert!(
            !mapper.irq_pending(),
            "IRQ should not be pending after reset"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper_small();
        let caps = mapper.capabilities();
        assert!(caps.has_irq, "mapper 304 has IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(
            !caps.has_dynamic_mirroring,
            "mirroring is fixed from header"
        );
        assert_eq!(caps.prg_bank_size_kb, 4, "4 KiB PRG pages");
        assert_eq!(caps.max_prg_ram_kb, 0, "no PRG-RAM");
    }
}
