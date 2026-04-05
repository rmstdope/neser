//! Mapper 311
//!
//! # Specification sources
//!
//! - **Primary source**: NesDev wiki — unavailable (Cloudflare 403 in this environment).
//! - **Backup mirror**: NesDev mirror — no page found for mapper 311 (404).
//! - **Mesen2**: mapper 311 not implemented (commented out in MapperFactory.cpp).
//! - **Fallback source (used)**: puNES `mapper_311.c`
//!   <https://raw.githubusercontent.com/punesemu/puNES/master/src/core/mappers/mapper_311.c>
//!
//! # Hardware overview
//!
//! Mapper 311 is an unlicensed mapper with a 32 KiB switchable PRG-ROM window,
//! two fixed PRG-ROM windows at $5000 and $6000, a CPU-cycle IRQ counter, and
//! special I/O reads at $4042–$4055.
//!
//! # PRG banking
//!
//! | CPU address | Window size | Bank                          |
//! |-------------|-------------|-------------------------------|
//! | $5000–$5FFF | 4 KiB       | Fixed — bank 17 in 4 KiB units |
//! | $6000–$7FFF | 8 KiB       | Fixed — bank 9 in 8 KiB units  |
//! | $8000–$FFFF | 32 KiB      | Switchable — bit 0 of $4022   |
//!
//! # CHR
//!
//! CHR uses a single 8 KiB window (ROM or RAM, no banking).
//!
//! # Mirroring
//!
//! Not controlled by the mapper; uses the layout from the iNES header.
//!
//! # Registers
//!
//! | Address | Effect                                                                   |
//! |---------|--------------------------------------------------------------------------|
//! | $4022   | PRG bank select: bit 0 → 32 KiB bank at $8000; re-applies fixed windows |
//! | $4122   | IRQ control: bit 0 = enable; always resets counter to 0 and clears IRQ  |
//!
//! # Special reads
//!
//! Reads from $4042–$4055 return `0xFF`.
//!
//! # IRQ
//!
//! When IRQ is enabled, a 32-bit counter increments on every CPU cycle. The
//! IRQ line is asserted (level-sensitive) while the counter ≥ 4096. Writing
//! to $4122 resets the counter to 0, clears the IRQ, and sets the enable bit.
//!
//! # Power-on / reset state
//!
//! All registers zero; IRQ disabled; all PRG windows at bank 0 / fixed offsets.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

const MAPPER_NUMBER: u16 = 311;
const PRG_32K_BANK_SIZE: usize = 0x8000;
const PRG_6000_BANK: i16 = 9; // 8 KiB bank 9 mapped at $6000–$7FFF
const PRG_5000_BANK_4K: usize = 17; // 4 KiB bank 17 mapped at $5000–$5FFF
const PRG_5000_BANK_SIZE: usize = 0x1000; // 4 KiB
const IRQ_THRESHOLD: u32 = 4096;

/// Mapper 311
///
/// Unlicensed mapper with a 1-bit 32 KiB PRG bank select, two fixed PRG
/// windows ($5000 and $6000), a CPU-cycle IRQ counter, and special I/O reads.
pub struct Mapper311 {
    base: BaseMapper,
    /// Bit 0 of $4022: selects the 32 KiB PRG bank at $8000–$FFFF.
    prg_bank: u8,
    /// Whether the IRQ counter is running.
    irq_enabled: bool,
    /// 32-bit CPU cycle counter; IRQ is asserted when count ≥ IRQ_THRESHOLD.
    irq_count: u32,
}

impl Mapper311 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_irq: true,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_32K_BANK_SIZE);
        base.configure_prg_6000_banking();

        let mut mapper = Self {
            base,
            prg_bank: 0,
            irq_enabled: false,
            irq_count: 0,
        };
        mapper.apply_prg_banking();
        mapper
    }

    /// Apply all PRG bank selections from the current register state.
    fn apply_prg_banking(&mut self) {
        self.base.select_prg_page(0, self.prg_bank as i16);
        self.base.select_prg_6000_page(PRG_6000_BANK);
    }

    /// Read a byte from the fixed 4 KiB PRG-ROM window at $5000–$5FFF.
    ///
    /// Uses bank 17 (in 4 KiB units) with modulo wrap to stay within ROM bounds.
    fn read_prg_5000(&self, addr: u16) -> u8 {
        let prg = self.base.prg_rom();
        if prg.is_empty() {
            return 0;
        }
        let base_offset = PRG_5000_BANK_4K * PRG_5000_BANK_SIZE;
        let page_offset = (addr as usize) - 0x5000;
        let index = (base_offset + page_offset) % prg.len();
        prg[index]
    }
}

impl Mapper for Mapper311 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            // Special I/O reads: return 0xFF
            0x4042..=0x4055 => 0xFF,
            // Fixed 4 KiB PRG-ROM window at $5000–$5FFF
            0x5000..=0x5FFF => self.read_prg_5000(addr),
            // $6000–$7FFF: fixed 8 KiB PRG-ROM window (via prg_6000_banking)
            // $8000–$FFFF: switchable 32 KiB PRG-ROM window (via prg_banking)
            _ => self
                .base
                .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            // $4022: PRG bank select (bit 0 only)
            0x4022 => {
                self.prg_bank = value & 0x01;
                self.apply_prg_banking();
            }
            // $4122: IRQ control — bit 0 = enable; always resets counter and clears IRQ
            0x4122 => {
                self.irq_enabled = (value & 0x01) != 0;
                self.irq_count = 0;
            }
            _ => {}
        }
    }

    fn cpu_cycle(&mut self) {
        if self.irq_enabled {
            self.irq_count = self.irq_count.wrapping_add(1);
        }
    }

    fn irq_pending(&self) -> bool {
        self.irq_enabled && self.irq_count >= IRQ_THRESHOLD
    }

    fn reset(&mut self) {
        self.prg_bank = 0;
        self.irq_enabled = false;
        self.irq_count = 0;
        self.apply_prg_banking();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        // [0] prg_bank
        // [1] flags: bit 0 = irq_enabled
        // [2..5] irq_count (little-endian u32)
        let mut snap = vec![self.prg_bank, self.irq_enabled as u8];
        snap.extend_from_slice(&self.irq_count.to_le_bytes());
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 6 {
            return;
        }
        self.prg_bank = data[0] & 0x01;
        self.irq_enabled = (data[1] & 0x01) != 0;
        self.irq_count = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
        self.apply_prg_banking();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};

    /// Build a 128 KiB PRG-ROM with distinctive fill patterns for each
    /// relevant window so tests can distinguish each one unambiguously.
    ///
    /// Layout:
    /// - Bytes 0x0000–0x7FFF (bank 0 of 32 KiB):  0xAA
    /// - Bytes 0x8000–0xFFFF (bank 1 of 32 KiB):  0xBB
    /// - Bytes 0x11000–0x11FFF (4 KiB bank 17):   0x17 (fixed at $5000)
    /// - Bytes 0x12000–0x13FFF (8 KiB bank 9):    0x09 (fixed at $6000)
    fn make_prg_rom_128k() -> Vec<u8> {
        let mut rom = vec![0u8; 128 * 1024];
        rom[0x00000..0x08000].fill(0xAA); // 32 KiB bank 0 → $8000 when prg_bank=0
        rom[0x08000..0x10000].fill(0xBB); // 32 KiB bank 1 → $8000 when prg_bank=1
        rom[0x11000..0x12000].fill(0x17); // 4 KiB bank 17 (offset 17 * 4096) → fixed at $5000
        rom[0x12000..0x14000].fill(0x09); // 8 KiB bank 9 (offset 9 * 8192) → fixed at $6000
        rom
    }

    fn make_mapper() -> Box<dyn Mapper> {
        create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            make_prg_rom_128k(),
            vec![0u8; 8192],
            NametableLayout::Vertical,
        ))
        .expect("Mapper 311 must be registered")
    }

    fn make_mapper_direct() -> Mapper311 {
        Mapper311::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                make_prg_rom_128k(),
                vec![0u8; 8192],
                NametableLayout::Vertical,
            )
            .with_prg_ram_banks(0),
        )
    }

    // ── Factory registration ─────────────────────────────────────────────────

    #[test]
    fn mapper_311_is_registered_in_factory() {
        assert!(
            make_mapper().mapper_number() == MAPPER_NUMBER,
            "Mapper 311 must be registered in the factory"
        );
    }

    // ── PRG $8000 banking ────────────────────────────────────────────────────

    #[test]
    fn prg_8000_defaults_to_bank_0_on_power_on() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0xAA,
            "$8000 must read bank 0 (0xAA) on power-on"
        );
    }

    #[test]
    fn prg_8000_switches_to_bank_1_when_4022_bit0_set() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4022, 0x01);
        assert_eq!(
            mapper.read_prg(0x8000),
            0xBB,
            "$8000 must read bank 1 (0xBB) after writing 0x01 to $4022"
        );
    }

    #[test]
    fn prg_8000_only_bit0_of_4022_is_used() {
        let mut mapper = make_mapper();
        // Write 0xFF; only bit 0 = 1 should be used → bank 1
        mapper.write_prg(0x4022, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            0xBB,
            "Only bit 0 of $4022 must select the 32 KiB PRG bank"
        );
    }

    #[test]
    fn prg_8000_switch_back_to_bank_0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4022, 0x01);
        mapper.write_prg(0x4022, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            0xAA,
            "$8000 must return to bank 0 (0xAA) after clearing bit 0 of $4022"
        );
    }

    #[test]
    fn prg_ffff_mirrors_same_32k_bank() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0xAA,
            "$FFFF must mirror the same 32 KiB bank as $8000 (bank 0)"
        );
        mapper.write_prg(0x4022, 0x01);
        assert_eq!(
            mapper.read_prg(0xFFFF),
            0xBB,
            "$FFFF must mirror the same 32 KiB bank (bank 1)"
        );
    }

    // ── Fixed $5000 window ───────────────────────────────────────────────────

    #[test]
    fn prg_5000_reads_fixed_4k_bank_17() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg_open_bus(0x5000, 0x00),
            0x17,
            "$5000 must read from fixed 4 KiB bank 17 (0x17)"
        );
    }

    #[test]
    fn prg_5000_window_does_not_change_with_4022() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4022, 0x01);
        assert_eq!(
            mapper.read_prg_open_bus(0x5000, 0x00),
            0x17,
            "$5000 window must be fixed regardless of $4022"
        );
    }

    #[test]
    fn prg_5fff_is_last_byte_of_fixed_4k_bank_17() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg_open_bus(0x5FFF, 0x00),
            0x17,
            "$5FFF must be the last byte of the fixed 4 KiB window"
        );
    }

    // ── Fixed $6000 window ───────────────────────────────────────────────────

    #[test]
    fn prg_6000_reads_fixed_8k_bank_9() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, 0x00),
            0x09,
            "$6000 must read from fixed 8 KiB bank 9 (0x09)"
        );
    }

    #[test]
    fn prg_6000_window_does_not_change_with_4022() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4022, 0x01);
        assert_eq!(
            mapper.read_prg_open_bus(0x6000, 0x00),
            0x09,
            "$6000 window must be fixed regardless of $4022"
        );
    }

    #[test]
    fn prg_7fff_is_last_byte_of_fixed_8k_bank_9() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg_open_bus(0x7FFF, 0x00),
            0x09,
            "$7FFF must be the last byte of the fixed 8 KiB window"
        );
    }

    // ── Special I/O reads ────────────────────────────────────────────────────

    #[test]
    fn read_4042_returns_0xff() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg_open_bus(0x4042, 0x55),
            0xFF,
            "Read from $4042 must return 0xFF"
        );
    }

    #[test]
    fn read_4055_returns_0xff() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg_open_bus(0x4055, 0x55),
            0xFF,
            "Read from $4055 must return 0xFF"
        );
    }

    #[test]
    fn read_4050_returns_0xff() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg_open_bus(0x4050, 0x22),
            0xFF,
            "Read from $4050 must return 0xFF"
        );
    }

    #[test]
    fn read_4041_returns_open_bus() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg_open_bus(0x4041, 0xCC),
            0xCC,
            "Read from $4041 (just below special range) must return open bus"
        );
    }

    #[test]
    fn read_4056_returns_open_bus() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg_open_bus(0x4056, 0xDD),
            0xDD,
            "Read from $4056 (just above special range) must return open bus"
        );
    }

    // ── IRQ ──────────────────────────────────────────────────────────────────

    #[test]
    fn irq_not_pending_on_power_on() {
        let mapper = make_mapper_direct();
        assert!(!mapper.irq_pending(), "IRQ must not be pending on power-on");
    }

    #[test]
    fn irq_does_not_fire_while_disabled() {
        let mut mapper = make_mapper_direct();
        for _ in 0..10_000 {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire while disabled (even after many cycles)"
        );
    }

    #[test]
    fn irq_does_not_fire_before_threshold() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4122, 0x01); // enable
        for _ in 0..(IRQ_THRESHOLD - 1) {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before reaching threshold"
        );
    }

    #[test]
    fn irq_fires_at_threshold() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4122, 0x01); // enable
        for _ in 0..IRQ_THRESHOLD {
            mapper.cpu_cycle();
        }
        assert!(
            mapper.irq_pending(),
            "IRQ must fire when counter reaches {IRQ_THRESHOLD}"
        );
    }

    #[test]
    fn irq_cleared_by_writing_4122() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4122, 0x01); // enable
        for _ in 0..IRQ_THRESHOLD {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ should be pending before ack");
        mapper.write_prg(0x4122, 0x00); // disable + clear
        assert!(
            !mapper.irq_pending(),
            "IRQ must be cleared after writing to $4122"
        );
    }

    #[test]
    fn irq_counter_reset_by_writing_4122() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4122, 0x01); // enable
        for _ in 0..IRQ_THRESHOLD {
            mapper.cpu_cycle();
        }
        mapper.write_prg(0x4122, 0x01); // re-enable (also resets counter)
        // Should not fire again until another full threshold of cycles
        for _ in 0..(IRQ_THRESHOLD - 1) {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "IRQ must not fire before threshold after counter reset"
        );
    }

    #[test]
    fn irq_fires_again_after_reenable() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4122, 0x01); // enable
        for _ in 0..IRQ_THRESHOLD {
            mapper.cpu_cycle();
        }
        mapper.write_prg(0x4122, 0x01); // re-enable
        for _ in 0..IRQ_THRESHOLD {
            mapper.cpu_cycle();
        }
        assert!(
            mapper.irq_pending(),
            "IRQ must fire again after re-enable and threshold cycles"
        );
    }

    #[test]
    fn irq_enable_bit0_only_in_4122() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4122, 0xFE); // bit 0 = 0 → disabled
        for _ in 0..IRQ_THRESHOLD {
            mapper.cpu_cycle();
        }
        assert!(
            !mapper.irq_pending(),
            "Only bit 0 of $4122 enables IRQ; upper bits must be ignored"
        );
    }

    // ── Capabilities ─────────────────────────────────────────────────────────

    #[test]
    fn capabilities_report_irq() {
        let mapper = make_mapper();
        assert!(
            mapper.capabilities().has_irq,
            "Mapper 311 must report has_irq = true"
        );
    }

    #[test]
    fn capabilities_report_no_prg_ram() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.capabilities().max_prg_ram_kb,
            0,
            "Mapper 311 must report no PRG-RAM"
        );
    }

    // ── Reset ────────────────────────────────────────────────────────────────

    #[test]
    fn reset_clears_prg_bank_selection() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x4022, 0x01);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            0xAA,
            "$8000 must read bank 0 (0xAA) after reset"
        );
    }

    #[test]
    fn reset_clears_irq_state() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4122, 0x01); // enable
        for _ in 0..IRQ_THRESHOLD {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending());
        mapper.reset();
        assert!(!mapper.irq_pending(), "IRQ must be cleared after reset");
    }

    // ── Snapshot / restore ───────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4022, 0x01); // bank 1
        mapper.write_prg(0x4122, 0x01); // enable IRQ
        for _ in 0..100 {
            mapper.cpu_cycle();
        }

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper_direct();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            0xBB,
            "Restored PRG bank must be 1 ($8000 reads 0xBB)"
        );
        assert!(
            restored.irq_enabled,
            "Restored IRQ enabled state must match"
        );
        assert_eq!(restored.irq_count, 100, "Restored IRQ counter must match");
    }

    #[test]
    fn snapshot_restore_derives_irq_pending_from_count() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4122, 0x01); // enable IRQ
        for _ in 0..IRQ_THRESHOLD {
            mapper.cpu_cycle();
        }
        assert!(mapper.irq_pending(), "IRQ must be pending before snapshot");

        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper_direct();
        restored.restore_registers(&snap);

        assert!(
            restored.irq_pending(),
            "Restored mapper must derive irq_pending from count >= threshold"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper_direct();
        mapper.write_prg(0x4022, 0x01);
        // snapshot is 6 bytes; truncate to 5 → should be ignored
        let snap = mapper.registers_snapshot();
        let short = &snap[..5];

        let mut restored = make_mapper_direct();
        restored.restore_registers(short);

        assert_eq!(
            restored.read_prg(0x8000),
            0xAA,
            "State must be unchanged after truncated restore"
        );
    }
}
