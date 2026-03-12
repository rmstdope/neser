//! Mapper 313 – ResetTxrom (MMC3-based multicart with reset-select)
//!
//! Specifications:
//! - Primary source: NesDev wiki (unavailable due to Cloudflare 403 in this environment).
//! - Fallback source: Mesen2 `ResetTxrom.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Mmc3Variants/ResetTxrom.h>
//!
//! # Hardware overview
//!
//! Standard MMC3 mapper with an outer 2-bit bank counter that advances on each
//! soft (button) reset.  Hard power-on resets the counter to 0.
//!
//! # PRG banking
//!
//! Each MMC3 8 KB page selection is extended with the outer counter:
//!
//! ```text
//! physical_bank = (reset_counter << 4) | (mmc3_bank & 0x0F)
//! ```
//!
//! This gives each game slot 16 × 8 KB = 128 KB of PRG space.
//!
//! # CHR banking
//!
//! Each MMC3 1 KB page selection is extended with the outer counter:
//!
//! ```text
//! physical_bank = (reset_counter << 7) | (mmc3_bank & 0x7F)
//! ```
//!
//! This gives each game slot 128 × 1 KB = 128 KB of CHR space.
//!
//! # Mirroring / IRQ
//!
//! Fully standard MMC3 (controlled via $A000 and $C000–$E001).
//!
//! # Reset behaviour
//!
//! | Reset type | Counter behaviour                          |
//! |------------|--------------------------------------------|
//! | Hard reset | counter = 0                                |
//! | Soft reset | counter = (counter + 1) & 0x03             |
//!
//! The hard/soft distinction is detected via the `initialize_ram` call: the
//! bus only invokes `initialize_ram` for hard resets, so that method sets a
//! pending-hard-reset flag that `reset()` consumes.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::cartridge::mmc3::MMC3Mapper;

const MAPPER_NUMBER: u16 = 313;
const PRG_INNER_BANK_MASK: usize = 0x0F;
const PRG_OUTER_BANK_SHIFT: usize = 4;
const CHR_INNER_BANK_MASK: usize = 0x7F;
const CHR_OUTER_BANK_SHIFT: usize = 7;
const COUNTER_MASK: u8 = 0x03;
const PRG_BANK_OFFSET_MASK: usize = 0x1FFF;
const CHR_BANK_OFFSET_MASK: usize = 0x03FF;

/// Mapper 313 – ResetTxrom
pub struct Mapper313 {
    mmc3: MMC3Mapper,
    reset_counter: u8,
    hard_reset_pending: bool,
}

impl Mapper313 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(ctx.prg_rom, ctx.chr_rom, ctx.mirroring, false),
            reset_counter: 0,
            hard_reset_pending: false,
        }
    }

    fn outer_prg_bank(&self, inner_bank: usize) -> usize {
        ((self.reset_counter as usize) << PRG_OUTER_BANK_SHIFT) | (inner_bank & PRG_INNER_BANK_MASK)
    }

    fn outer_chr_bank(&self, inner_bank: usize) -> usize {
        ((self.reset_counter as usize) << CHR_OUTER_BANK_SHIFT) | (inner_bank & CHR_INNER_BANK_MASK)
    }
}

impl Mapper for Mapper313 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn mapper_number(&self) -> u16 {
        MAPPER_NUMBER
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return self.mmc3.read_prg(addr);
        }
        let inner_bank = self.mmc3.mapped_prg_bank(addr);
        let bank = self.outer_prg_bank(inner_bank);
        let offset = (addr as usize) & PRG_BANK_OFFSET_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if !(0x8000..=0xFFFF).contains(&addr) {
            return self.mmc3.read_prg_open_bus(addr, open_bus);
        }
        self.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        self.mmc3.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let inner_bank = self.mmc3.raw_chr_1k_bank(addr);
        let bank = self.outer_chr_bank(inner_bank);
        let offset = (addr as usize) & CHR_BANK_OFFSET_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let inner_bank = self.mmc3.raw_chr_1k_bank(addr);
        let bank = self.outer_chr_bank(inner_bank);
        let offset = (addr as usize) & CHR_BANK_OFFSET_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn wram_size(&self) -> usize {
        self.mmc3.wram_size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.mmc3.wram_snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.mmc3.load_wram_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.reset_counter);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let Some((&counter, mmc3_data)) = data.split_last() else {
            return;
        };
        self.mmc3.restore_registers(mmc3_data);
        self.reset_counter = counter & COUNTER_MASK;
        self.hard_reset_pending = false;
    }

    fn reset(&mut self) {
        self.mmc3.reset();
        if self.hard_reset_pending {
            self.reset_counter = 0;
            self.hard_reset_pending = false;
        } else {
            self.reset_counter = self.reset_counter.wrapping_add(1) & COUNTER_MASK;
        }
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.mmc3.initialize_ram(mode);
        self.hard_reset_pending = true;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 64 8 KB PRG banks → 512 KB; each game slot (16 banks) is distinct
    const PRG_BANKS_8K: usize = 64;
    // 256 1 KB CHR banks → 256 KB; counter=1 inner=5 → bank 133, no modulo collision
    const CHR_BANKS_1K: usize = 256;

    fn make_mapper() -> Mapper313 {
        Mapper313::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_313_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(8 * 1024, PRG_BANKS_8K),
            banked_data(1024, CHR_BANKS_1K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 313 must be registered in factory");
    }

    // ── Power-on state ────────────────────────────────────────────────────────
    // At power-on reset_counter=0, so PRG/CHR outer bank = 0.
    // MMC3 default: R6=$00, last bank at $E000-$FFFF.
    // With 64 8K banks, mapped_prg_bank(0x8000) = R6 % 64 = 0.
    // outer_prg_bank(0) = (0 << 4) | 0 = 0 → read returns 0.

    #[test]
    fn power_on_counter_is_zero() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.reset_counter, 0,
            "reset_counter must be 0 at power-on"
        );
    }

    #[test]
    fn power_on_prg_uses_mmc3_inner_bank_unmodified() {
        let mut mapper = make_mapper();
        // Set MMC3 R6=5 via bank-select $06, bank-data 5
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);
        // counter=0 → outer = (0<<4)|5 = 5 → bank 5 → read returns 5
        assert_eq!(
            mapper.read_prg(0x8000),
            5,
            "With counter=0, MMC3 inner bank 5 maps to outer bank 5"
        );
    }

    // ── Soft reset increments counter ─────────────────────────────────────────

    #[test]
    fn soft_reset_increments_counter_to_1() {
        let mut mapper = make_mapper();
        mapper.reset(); // soft reset (no initialize_ram called first)
        assert_eq!(
            mapper.reset_counter, 1,
            "Soft reset should increment counter from 0 to 1"
        );
    }

    #[test]
    fn soft_reset_wraps_counter_after_3() {
        let mut mapper = make_mapper();
        for _ in 0..4 {
            mapper.reset();
        }
        assert_eq!(
            mapper.reset_counter, 0,
            "Four soft resets must wrap counter back to 0"
        );
    }

    #[test]
    fn prg_outer_bank_shifts_with_counter() {
        let mut mapper = make_mapper();
        // Set MMC3 R6=5
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);
        let bank_before = mapper.read_prg(0x8000); // counter=0 → outer bank 5

        mapper.reset(); // soft reset → counter=1
        let bank_after = mapper.read_prg(0x8000); // counter=1 → outer = (1<<4)|5 = 21

        assert_ne!(
            bank_before, bank_after,
            "Soft reset must shift PRG outer bank via counter"
        );
        assert_eq!(bank_before, 5, "Before soft reset, outer bank should be 5");
        assert_eq!(
            bank_after, 21,
            "After soft reset (counter=1), outer bank should be 21"
        );
    }

    #[test]
    fn chr_outer_bank_shifts_with_counter() {
        let mut mapper = make_mapper();
        // Set MMC3 CHR mode 0, R2=5 (1KB at $1000)
        mapper.write_prg(0x8000, 0x02); // select R2
        mapper.write_prg(0x8001, 5); // R2=5
        let chr_before = mapper.read_chr(0x1000); // counter=0 → bank (0<<7)|5 = 5 → returns 5

        mapper.reset(); // soft reset → counter=1
        let chr_after = mapper.read_chr(0x1000); // counter=1 → bank (1<<7)|5 = 133 → returns 133

        assert_ne!(
            chr_before, chr_after,
            "Soft reset must shift CHR outer bank via counter"
        );
        assert_eq!(chr_before, 5, "Before soft reset, CHR bank should be 5");
        assert_eq!(
            chr_after, 133,
            "After soft reset (counter=1), CHR bank should be 133"
        );
    }

    // ── Hard reset resets counter to 0 ───────────────────────────────────────

    #[test]
    fn hard_reset_resets_counter_to_zero() {
        let mut mapper = make_mapper();
        mapper.reset(); // soft reset → counter=1
        mapper.reset(); // soft reset → counter=2
        assert_eq!(
            mapper.reset_counter, 2,
            "Counter should be 2 before hard reset"
        );

        mapper.initialize_ram(crate::console::RamInitMode::Zero);
        mapper.reset(); // hard reset → counter=0
        assert_eq!(
            mapper.reset_counter, 0,
            "Hard reset must reset counter to 0"
        );
    }

    #[test]
    fn subsequent_soft_reset_after_hard_reset_increments_to_1() {
        let mut mapper = make_mapper();
        mapper.reset(); // soft → counter=1
        mapper.initialize_ram(crate::console::RamInitMode::Zero);
        mapper.reset(); // hard → counter=0
        mapper.reset(); // soft → counter=1
        assert_eq!(
            mapper.reset_counter, 1,
            "Soft reset after hard reset should go to counter=1"
        );
    }

    // ── Outer bank masking ────────────────────────────────────────────────────

    #[test]
    fn prg_inner_bank_uses_only_lower_4_bits() {
        let mut mapper = make_mapper();
        mapper.reset(); // counter=1
        // Set R6=0x1F (inner bits = 0x0F, masked to 0x0F)
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 0x1F); // inner & 0x0F = 0x0F=15
        // outer = (1<<4)|15 = 31
        assert_eq!(
            mapper.read_prg(0x8000),
            31,
            "PRG inner bank uses only lower 4 bits; outer = (counter<<4)|inner4"
        );
    }

    #[test]
    fn chr_inner_bank_uses_only_lower_7_bits() {
        let mut mapper = make_mapper();
        mapper.reset(); // counter=1
        // R2=0xFF → inner & 0x7F = 0x7F=127
        mapper.write_prg(0x8000, 0x02);
        mapper.write_prg(0x8001, 0x7F);
        // outer = (1<<7)|127 = 255
        assert_eq!(
            mapper.read_chr(0x1000),
            255,
            "CHR inner bank uses only lower 7 bits; outer = (counter<<7)|inner7"
        );
    }

    // ── MMC3 features still work ──────────────────────────────────────────────

    #[test]
    fn mirroring_control_is_delegated_to_mmc3() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn irq_signalling_is_delegated_to_mmc3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE001, 0);
        for _ in 0..2 {
            mapper.ppu_address_changed(0x0FFF);
            for _ in 0..3 {
                mapper.cpu_cycle();
            }
            mapper.ppu_address_changed(0x1000);
        }
        assert!(
            mapper.irq_pending(),
            "Mapper 313 must pass MMC3 IRQ through"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_counter() {
        let mut mapper = make_mapper();
        mapper.reset(); // counter=1
        mapper.write_prg(0x8000, 0x06);
        mapper.write_prg(0x8001, 5);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);
        assert_eq!(restored.reset_counter, 1, "Restored counter should be 1");
        assert_eq!(
            restored.read_prg(0x8000),
            21,
            "Restored PRG bank should be 21 (counter=1, R6=5)"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.reset(); // counter=1
        mapper.restore_registers(&[]);
        assert_eq!(
            mapper.reset_counter, 1,
            "Empty restore data must not change state"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_report_irq_and_chr_banking() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(caps.has_irq, "Mapper 313 must report IRQ capability");
        assert!(caps.has_chr_banking, "Mapper 313 must report CHR banking");
        assert!(
            caps.has_dynamic_mirroring,
            "Mapper 313 must report dynamic mirroring"
        );
        assert!(
            !caps.has_expansion_audio,
            "Mapper 313 has no expansion audio"
        );
        assert_eq!(caps.prg_bank_size_kb, 8);
        assert_eq!(caps.chr_bank_size_kb, 1);
    }
}
