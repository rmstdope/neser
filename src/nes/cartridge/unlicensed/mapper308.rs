//! Mapper 308 – UNL-TH2131-1 (Batman bootleg)
//!
//! Specifications:
//! - Primary: NesDev wiki (NES 2.0 Mapper 308)
//!   <https://wiki.nesdev.org/w/index.php/NES_2.0_Mapper_308>
//!   (archived/mirror source used: EmIsGreat/Monsoon-Emulator wiki-clone)
//! - Fallback implementation reference: negativeExponent/libretro-fceumm_next mapper308.c
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};
use crate::nes::cartridge::vrc2_vrc4::Vrc2Vrc4Mapper;
use crate::nes::console::RamInitMode;

/// Mapper 308 – UNL-TH2131-1 (Batman bootleg)
///
/// Hardware: UNL-TH2131-1 discrete logic board
///
/// This is a VRC2b clone (Mapper 23, Submapper 3) with custom M2-cycle IRQ added on top.
/// Used for a bootleg version of the Sunsoft game Batman.
///
/// Specifications (NesDev wiki, libretro-fceumm_next reference):
/// - PRG-ROM: Up to 256 KiB — two 8 KiB switchable banks + two fixed
///   - $8000–$9FFF: PRG bank 0 (switchable via $8000 write)
///   - $A000–$BFFF: PRG bank 1 (switchable via $A000 write)
///   - $C000–$DFFF: Fixed to second-to-last 8 KiB bank
///   - $E000–$FFFF: Fixed to last 8 KiB bank
/// - PRG-RAM: 8 KiB at $6000–$7FFF (always accessible, VRC2 style)
/// - CHR: Eight 1 KiB switchable banks ($B000–$E003, low/high nibble registers)
/// - Mirroring: H / V programmable via $9000 bit 0 (0 = Vertical, 1 = Horizontal)
/// - IRQ: Custom M2-cycle IRQ (NOT the standard VRC IRQ system)
///
/// VRC2b Register Map ($8000–$EFFF):
/// - $8000: PRG bank 0 select (bits [4:0])
/// - $9000: Mirroring (bit 0: 0 = Vertical, 1 = Horizontal)
/// - $A000: PRG bank 1 select (bits [4:0])
/// - $B000/$B001: CHR bank 0 low / high nibble
/// - $B002/$B003: CHR bank 1 low / high nibble
/// - $C000/$C001: CHR bank 2 low / high nibble
/// - $C002/$C003: CHR bank 3 low / high nibble
/// - $D000/$D001: CHR bank 4 low / high nibble
/// - $D002/$D003: CHR bank 5 low / high nibble
/// - $E000/$E001: CHR bank 6 low / high nibble
/// - $E002/$E003: CHR bank 7 low / high nibble
///
/// Custom IRQ Register Map ($F000–$FFFF, decoded via addr & 0xF003):
/// - $F000: IRQ Acknowledge and Reset — ack IRQ, disable counting, reset low counter to 0
/// - $F001: IRQ Counter Enable — enable counting
/// - $F003: IRQ High Counter Value — bits [7:4] load the 4-bit high counter
///
/// IRQ Operation:
/// - When enabled, the 12-bit low counter increments on every M2 (CPU) cycle.
/// - When the low counter transitions from < 2048 to ≥ 2048 (bit 11: 0 → 1),
///   the 4-bit high counter is decremented.
/// - When the high counter == 0 AND the low counter bit 11 == 0, an IRQ is asserted.
/// - Writing $F000 clears the assertion, disables counting, and resets the low counter.
///
/// Power-on state: all registers zero; no IRQ; mirroring from cartridge header.
pub struct Mapper308 {
    /// Inner VRC2b mapper handling all PRG/CHR/mirroring registers.
    inner: Vrc2Vrc4Mapper,
    /// 4-bit high counter (counts down on low-counter bit-11 transitions).
    irq_count_high: u8,
    /// 12-bit low counter stored in a u16; only the lower 12 bits are used.
    irq_count_low: u16,
    /// True when M2-cycle counting is enabled.
    irq_enabled: bool,
    /// True when the IRQ line is currently asserted.
    irq_asserted: bool,
}

/// Number of bytes used by Mapper308's own IRQ state in the snapshot.
const IRQ_SNAPSHOT_BYTES: usize = 4;
/// Mask to keep the high IRQ counter within 4-bit range (0x00–0x0F).
const IRQ_COUNT_HIGH_MASK: u8 = 0x0F;

impl Mapper308 {
    pub fn new(ctx: MapperContext) -> Self {
        // Construct inner mapper as VRC2b (Mapper 23 Submapper 3):
        // A0→chip A0, A1→chip A1, no built-in IRQ, 4-bit CHR, H/V mirroring.
        let inner_ctx = MapperContext {
            mapper: 23,
            submapper: 3,
            ..ctx
        };
        Self {
            inner: Vrc2Vrc4Mapper::new(inner_ctx),
            irq_count_high: 0,
            irq_count_low: 0,
            irq_enabled: false,
            irq_asserted: false,
        }
    }

    /// Handle writes to the custom IRQ register range $F000–$FFFF.
    ///
    /// The effective register is decoded from the lower two address bits:
    /// `addr & 0xF003` gives $F000, $F001, $F002, or $F003.
    fn handle_irq_write(&mut self, addr: u16, value: u8) {
        match addr & 0xF003 {
            0xF000 => {
                // Acknowledge IRQ: clear assertion, disable counting, reset low counter.
                self.irq_asserted = false;
                self.irq_enabled = false;
                self.irq_count_low = 0;
            }
            0xF001 => {
                // Enable IRQ counting.
                self.irq_enabled = true;
            }
            0xF003 => {
                // Load high counter from bits [7:4] of the written value.
                self.irq_count_high = (value >> 4) & IRQ_COUNT_HIGH_MASK;
            }
            _ => {}
        }
    }

    /// Advance the custom IRQ counter by one M2 cycle and update the IRQ line.
    fn tick_irq(&mut self) {
        if !self.irq_enabled {
            return;
        }
        let prev = self.irq_count_low & 0x0FFF;
        self.irq_count_low = self.irq_count_low.wrapping_add(1);
        let curr = self.irq_count_low & 0x0FFF;

        // Decrement high counter when bit 11 transitions 0 → 1.
        if (prev & 0x0800) == 0 && (curr & 0x0800) != 0 {
            self.irq_count_high = self.irq_count_high.wrapping_sub(1) & IRQ_COUNT_HIGH_MASK;
        }
        // Assert IRQ when high counter is zero and bit 11 is clear.
        if self.irq_count_high == 0 && (curr & 0x0800) == 0 {
            self.irq_asserted = true;
        }
    }
}

impl Mapper for Mapper308 {
    fn base(&self) -> &BaseMapper {
        self.inner.base()
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        self.inner.base_mut()
    }

    fn mapper_number(&self) -> u16 {
        308
    }

    fn capabilities(&self) -> MapperCapabilities {
        let mut caps = self.inner.capabilities();
        caps.has_irq = true;
        caps
    }

    fn read_prg(&self, addr: u16) -> u8 {
        self.inner.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0xF000..=0xFFFF => self.handle_irq_write(addr, value),
            _ => self.inner.write_prg(addr, value),
        }
    }

    fn cpu_cycle(&mut self) {
        self.tick_irq();
    }

    fn irq_pending(&self) -> bool {
        self.irq_asserted
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

    fn initialize_ram(&mut self, mode: RamInitMode) {
        self.inner.initialize_ram(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let [lo, hi] = self.irq_count_low.to_le_bytes();
        let flags = (self.irq_enabled as u8) | ((self.irq_asserted as u8) << 1);
        let mut snapshot = vec![self.irq_count_high, lo, hi, flags];
        snapshot.extend(self.inner.registers_snapshot());
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= IRQ_SNAPSHOT_BYTES {
            self.irq_count_high = data[0];
            self.irq_count_low = u16::from_le_bytes([data[1], data[2]]);
            let flags = data[3];
            self.irq_enabled = (flags & 0x01) != 0;
            self.irq_asserted = (flags & 0x02) != 0;
            self.inner.restore_registers(&data[IRQ_SNAPSHOT_BYTES..]);
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.irq_count_high = 0;
        self.irq_count_low = 0;
        self.irq_enabled = false;
        self.irq_asserted = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    /// Non-power-of-two bank counts to prevent false-pass modulo wrapping.
    const PRG_BANKS: usize = 11; // 11 × 8 KiB = 88 KiB
    const CHR_BANKS: usize = 13; // 13 × 1 KiB = 13 KiB

    fn make_mapper() -> Mapper308 {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        Mapper308::new(MapperContext::new_for_test(
            308,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
    }

    // ── Registration ────────────────────────────────────────────────────────

    #[test]
    fn mapper_308_is_registered_in_factory() {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            308,
            prg,
            chr,
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 308 must be registered in the factory"
        );
    }

    // ── PRG banking ─────────────────────────────────────────────────────────

    #[test]
    fn power_on_prg_8000_maps_to_bank_0() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 must map to PRG bank 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_e000_maps_to_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xE000),
            (PRG_BANKS - 1) as u8,
            "$E000 must map to the last PRG bank at power-on"
        );
    }

    #[test]
    fn power_on_prg_c000_maps_to_second_to_last_bank() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            (PRG_BANKS - 2) as u8,
            "$C000 must map to the second-to-last PRG bank at power-on"
        );
    }

    #[test]
    fn write_8000_switches_prg_bank_at_8000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 3);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Writing 3 to $8000 must switch the $8000–$9FFF PRG window to bank 3"
        );
    }

    #[test]
    fn write_a000_switches_prg_bank_at_a000() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 5);
        assert_eq!(
            mapper.read_prg(0xA000),
            5,
            "Writing 5 to $A000 must switch the $A000–$BFFF PRG window to bank 5"
        );
    }

    #[test]
    fn write_8000_does_not_affect_a000_window() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA000, 2); // set $A000 window to bank 2 first
        mapper.write_prg(0x8000, 3); // change $8000 window to bank 3
        assert_eq!(
            mapper.read_prg(0xA000),
            2,
            "Changing PRG bank 0 must not affect the $A000 window"
        );
    }

    // ── PRG-RAM ─────────────────────────────────────────────────────────────

    #[test]
    fn prg_ram_at_6000_is_read_write_accessible() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "PRG-RAM at $6000 must be readable after write (VRC2 always-enabled)"
        );
    }

    // ── CHR banking ─────────────────────────────────────────────────────────

    #[test]
    fn power_on_chr_0000_maps_to_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR $0000 must map to bank 0 at power-on"
        );
    }

    #[test]
    fn write_b000_b001_sets_chr_bank_0() {
        let mut mapper = make_mapper();
        // Low nibble of bank 5 = 0x05; high nibble = 0x00
        mapper.write_prg(0xB000, 5); // low nibble → bank = 5
        mapper.write_prg(0xB001, 0); // high nibble → unchanged
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR bank 0 at $0000 must be bank 5 after $B000/$B001 write"
        );
    }

    #[test]
    fn write_b002_b003_sets_chr_bank_1() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xB002, 7);
        mapper.write_prg(0xB003, 0);
        assert_eq!(
            mapper.read_chr(0x0400),
            7,
            "CHR bank 1 at $0400 must be bank 7 after $B002/$B003 write"
        );
    }

    #[test]
    fn write_e002_e003_sets_chr_bank_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xE002, 9); // low nibble, CHR bank 7 slot
        mapper.write_prg(0xE003, 0); // high nibble
        assert_eq!(
            mapper.read_chr(0x1C00),
            9,
            "CHR bank 7 at $1C00 must be bank 9 after $E002/$E003 write"
        );
    }

    // ── Mirroring ────────────────────────────────────────────────────────────

    #[test]
    fn power_on_mirroring_from_header() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Mirroring must be initialised from the cartridge header"
        );
    }

    #[test]
    fn write_9000_bit0_one_sets_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "$9000 bit 0 = 1 must select Horizontal mirroring"
        );
    }

    #[test]
    fn write_9000_bit0_zero_sets_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x9000, 0x01); // set to H first
        mapper.write_prg(0x9000, 0x00); // then back to V
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "$9000 bit 0 = 0 must select Vertical mirroring"
        );
    }

    // ── Custom IRQ ───────────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_at_power_on() {
        let mapper = make_mapper();
        assert!(!mapper.irq_pending(), "IRQ must not be pending at power-on");
    }

    #[test]
    fn irq_does_not_fire_when_disabled() {
        let mut mapper = make_mapper();
        // Set high counter = 0 (IRQ would fire immediately if enabled)
        mapper.write_prg(0xF003, 0x00);
        // Do NOT enable via $F001
        for _ in 0..100 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire when counting is disabled"
        );
    }

    #[test]
    fn f003_write_loads_high_counter_from_bits_7_to_4() {
        // Verify that writing 0xA0 to $F003 loads high counter = 0xA (= 10).
        // We verify indirectly: with high=10, IRQ should NOT fire in 4096 cycles
        // (it would take 10 × 4096 = 40960 cycles for high=10 to reach 0).
        let mut mapper = make_mapper();
        mapper.write_prg(0xF003, 0xA0); // high counter = (0xA0 >> 4) = 10
        mapper.write_prg(0xF001, 0x01); // enable
        // Run 4095 cycles (less than one full low-counter period) → no IRQ
        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire within one low-counter period when high=10"
        );
    }

    #[test]
    fn irq_fires_after_4096_cycles_with_high_counter_one() {
        // With high_counter=1:
        //   - After 2048 cycles, bit 11 transitions 0→1 → decrement high to 0.
        //   - After another 2048 cycles (total 4096), counter wraps back to 0 (bit11=0),
        //     high=0 → IRQ asserted.
        let mut mapper = make_mapper();
        mapper.write_prg(0xF003, 0x10); // high counter = (0x10 >> 4) = 1
        mapper.write_prg(0xF001, 0x01); // enable counting

        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before 4096 cycles with high=1"
        );

        mapper.cpu_cycle(); // cycle 4096
        assert!(
            mapper.irq_pending(),
            "IRQ must fire at exactly 4096 cycles with high=1"
        );
    }

    #[test]
    fn f000_write_acknowledges_and_disables_irq() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF003, 0x10); // high = 1
        mapper.write_prg(0xF001, 0x01); // enable
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ must be asserted before ack");

        // Acknowledge: clear IRQ, disable counting, reset low counter.
        mapper.write_prg(0xF000, 0x00);
        assert!(
            !mapper.irq_pending(),
            "$F000 write must clear IRQ assertion"
        );

        // Counting must also be disabled after ack (no new IRQ without re-enable).
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must remain clear after ack (counting disabled)"
        );
    }

    #[test]
    fn f000_write_resets_low_counter() {
        // After ack, low counter resets to 0 so the next cycle with high=1 takes 4096 more cycles.
        let mut mapper = make_mapper();
        mapper.write_prg(0xF003, 0x10); // high = 1
        mapper.write_prg(0xF001, 0x01); // enable
        // Run 2000 cycles to advance low counter partially.
        for _ in 0..2000 {
            mapper.cpu_cycle();
        }
        mapper.write_prg(0xF000, 0x00); // ack (resets low counter)
        mapper.write_prg(0xF003, 0x10); // reload high = 1
        mapper.write_prg(0xF001, 0x01); // re-enable
        // Must require a full 4096 cycles again (not 4096 - 2000).
        for _ in 0..4095 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "After ack, low counter must reset so timing starts fresh"
        );
        mapper.cpu_cycle(); // cycle 4096
        assert!(
            mapper.irq_pending(),
            "IRQ must fire after full 4096 cycles following ack/re-enable"
        );
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn registers_snapshot_captures_prg_bank_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 4);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(
            restored.read_prg(0x8000),
            4,
            "Snapshot must preserve PRG bank 0 register"
        );
    }

    #[test]
    fn registers_snapshot_captures_irq_state() {
        let mut mapper = make_mapper();
        // Advance to a partially-counted state.
        mapper.write_prg(0xF003, 0x20); // high = 2
        mapper.write_prg(0xF001, 0x01); // enable
        for _ in 0..3000 {
            mapper.cpu_cycle();
        }
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // After restore, IRQ state should match original at cycle 3000.
        // Exactly 1096 more cycles needed to reach the first bit-11 transition.
        // Running 1096 more: total = 4096 → but high was 2, not 1.
        // At cycle 2048: high goes 2→1. At 4096: high goes 1→0, then at 4096 counters wraps,
        // actually IRQ would fire at cycle 4096 only when high=0 at that moment.
        // Simpler: just verify the restored mapper behaves identically.
        assert_eq!(
            mapper.irq_pending(),
            restored.irq_pending(),
            "Restored mapper must have same IRQ pending state"
        );
        // Run a few more ticks and compare.
        for _ in 0..100 {
            mapper.cpu_cycle();
            restored.cpu_cycle();
        }
        assert_eq!(
            mapper.irq_pending(),
            restored.irq_pending(),
            "Restored mapper IRQ behavior must match original after additional ticks"
        );
    }

    #[test]
    fn snapshot_length_is_correct() {
        let mapper = make_mapper();
        // Layout: IRQ_SNAPSHOT_BYTES (4) first, then the full VRC2b inner snapshot (27 bytes).
        assert_eq!(
            mapper.registers_snapshot().len(),
            IRQ_SNAPSHOT_BYTES + 27,
            "Snapshot must be exactly {} bytes",
            IRQ_SNAPSHOT_BYTES + 27
        );
    }

    // ── Reset ────────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_irq_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF003, 0x10); // high = 1
        mapper.write_prg(0xF001, 0x01); // enable
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ must be asserted before reset");
        mapper.reset();
        assert!(!mapper.irq_pending(), "IRQ must clear after reset");
    }

    #[test]
    fn reset_disables_irq_counting() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xF003, 0x10); // high = 1
        mapper.write_prg(0xF001, 0x01); // enable
        mapper.reset();
        // After reset, counting must be disabled: run 4096 cycles and expect no IRQ.
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire after reset (counting disabled)"
        );
    }

    // ── VRC2b pin mapping for $F000 writes ──────────────────────────────────

    #[test]
    fn f002_write_is_decoded_as_irq_enable() {
        // For VRC2b pin wiring (A0→chipA0, A1→chipA1), address $F002 has A1=1, A0=0.
        // $F002 & 0xF003 = 0xF002, which is not a handled register, so it is a no-op.
        // Verify that $F001 (bit 0 set) enables IRQ and $F002 does not.
        let mut mapper = make_mapper();
        mapper.write_prg(0xF003, 0x10); // high = 1
        mapper.write_prg(0xF002, 0x01); // NOT an enable (addr & 0xF003 = 0xF002)
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "$F002 must not enable IRQ counting (not a defined register)"
        );
    }

    #[test]
    fn f001_aliased_addresses_also_enable_irq() {
        // For VRC2b: A0→chipA0, A1→chipA1. Address $F001 has A0=1, A1=0 → chip pos 1.
        // Any address with (addr & 0xF003) == 0xF001 should also enable IRQ.
        // e.g. $F005, $F009, $F00D … (all have A0=1, A1=0 in the $F000 block).
        let mut mapper = make_mapper();
        mapper.write_prg(0xF003, 0x10); // high = 1
        mapper.write_prg(0xF101, 0x01); // $F101 → $F101 & 0xF003 = $F001 → enable
        for _ in 0..4096 {
            mapper.cpu_cycle();
        }
        assert!(
            mapper.irq_pending(),
            "$F101 (alias of $F001 via pin masking) must enable IRQ"
        );
    }
}
