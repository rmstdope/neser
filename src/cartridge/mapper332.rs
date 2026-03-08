//! Mapper 332 – Super 40-in-1 WS (BMC multicart)
//!
//! Specifications:
//! - NesDev wiki: inaccessible (Cloudflare 403).
//! - Fallback: Mesen2 `Super40in1Ws.h` (primary reference).
//!
//! Hardware behaviour:
//! - 2×16 KB PRG windows at $8000–$BFFF and $C000–$FFFF.
//! - 1×8 KB CHR window at $0000–$1FFF (CHR-ROM).
//! - Registers mapped to $6000–$6FFF:
//!   - Even address: PRG + mirroring control.  Bit 5 arms the register lock;
//!     subsequent writes are ignored while the lock is held.
//!   - Odd address: CHR bank select.
//! - PRG banking:
//!   - Bit 3 clear → aligned 32 KB NROM pair: page 0 = value & 0xFE,
//!     page 1 = (value & 0xFE) | 1.
//!   - Bit 3 set  → 16 KB "UNROM" repeated mode: both pages = value.
//! - Mirroring: bit 4 = 1 → Horizontal; bit 4 = 0 → Vertical.
//! - Power-on equivalent: WriteRegister($6000, 0) → pages 0/1, Vertical.
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 332;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const REG_START: u16 = 0x6000;
const REG_END: u16 = 0x6FFF;
const REGISTERS_SNAPSHOT_LEN: usize = 3;

pub struct Mapper332 {
    base: BaseMapper,
    prg_reg: u8,
    chr_reg: u8,
    reg_lock: bool,
}

impl Mapper332 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self {
            base,
            prg_reg: 0,
            chr_reg: 0,
            reg_lock: false,
        };
        mapper.apply_state(0, 0, false);
        mapper
    }

    fn apply_state(&mut self, prg_reg: u8, chr_reg: u8, reg_lock: bool) {
        self.prg_reg = prg_reg;
        self.chr_reg = chr_reg;
        self.reg_lock = reg_lock;
        self.update_banks();
    }

    fn update_banks(&mut self) {
        let (page0, page1) = Self::prg_pages(self.prg_reg);
        self.base.select_prg_page(0, page0);
        self.base.select_prg_page(1, page1);
        self.base.select_chr_page(0, self.chr_reg as i16);
        self.base.set_mirroring_hv((self.prg_reg & 0x10) != 0);
    }

    fn prg_pages(value: u8) -> (i16, i16) {
        if value & 0x08 != 0 {
            // 16 KB repeated mode
            (value as i16, value as i16)
        } else {
            // Aligned 32 KB NROM pair
            ((value & 0xFE) as i16, (value | 0x01) as i16)
        }
    }
}

impl Mapper for Mapper332 {
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
        if !(REG_START..=REG_END).contains(&addr) {
            return;
        }
        if self.reg_lock {
            return;
        }
        if addr & 0x01 != 0 {
            // Odd address: CHR select
            self.apply_state(self.prg_reg, value, self.reg_lock);
        } else {
            // Even address: PRG + mirroring + lock
            let lock = (value & 0x20) != 0;
            self.apply_state(value, self.chr_reg, lock);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_reg, self.chr_reg, self.reg_lock as u8]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < REGISTERS_SNAPSHOT_LEN {
            return;
        }
        self.apply_state(data[0], data[1], data[2] != 0);
    }

    fn reset(&mut self) {
        self.apply_state(0, 0, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two bank count to prevent false-pass modulo wrapping.
    const PRG_BANKS_16K: usize = 48;
    const CHR_BANKS_8K: usize = 11;

    fn make_mapper() -> Mapper332 {
        Mapper332::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    // --- Factory registration ---

    #[test]
    fn mapper_332_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 332 must be registered in factory");
    }

    // --- Power-on state ---

    #[test]
    fn power_on_prg_8000_reads_bank_0() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 must map to PRG bank 0");
    }

    #[test]
    fn power_on_prg_c000_reads_bank_1() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 must map to PRG bank 1");
    }

    #[test]
    fn power_on_chr_0000_reads_bank_0() {
        let mut mapper = make_mapper();
        assert_eq!(mapper.read_chr(0x0000), 0, "$0000 must map to CHR bank 0");
    }

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // --- PRG banking: aligned 32 KB pair (bit 3 = 0) ---

    #[test]
    fn nrom_32_mode_selects_aligned_pair() {
        let mut mapper = make_mapper();
        // value = 0x04 (bit3=0): page0 = 0x04 & 0xFE = 4, page1 = 4 | 1 = 5
        mapper.write_prg(0x6000, 0x04);
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xC000), 5);
    }

    #[test]
    fn nrom_32_mode_odd_value_rounds_down_to_even_page() {
        let mut mapper = make_mapper();
        // value = 0x05 (bit3=0): page0 = 0x05 & 0xFE = 4, page1 = 5
        mapper.write_prg(0x6000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xC000), 5);
    }

    // --- PRG banking: 16 KB repeated mode (bit 3 = 1) ---

    #[test]
    fn unrom_mode_selects_same_bank_in_both_windows() {
        let mut mapper = make_mapper();
        // value = 0x0A (bit3=1): page0 = page1 = 10
        mapper.write_prg(0x6000, 0x0A);
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_prg(0xC000), 10);
    }

    #[test]
    fn unrom_mode_with_different_bank_index_selects_correct_bank() {
        let mut mapper = make_mapper();
        // value = 0x1B (bit3=1, value=27): both pages = 27
        mapper.write_prg(0x6000, 0x1B);
        assert_eq!(mapper.read_prg(0x8000), 27);
        assert_eq!(mapper.read_prg(0xC000), 27);
    }

    // --- CHR banking ---

    #[test]
    fn odd_address_write_selects_chr_bank() {
        let mut mapper = make_mapper();
        // Odd address $6001: CHR bank select
        mapper.write_prg(0x6001, 3);
        assert_eq!(mapper.read_chr(0x0000), 3);
    }

    #[test]
    fn chr_bank_changes_independently_of_prg() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x0A); // PRG: UNROM mode bank 10
        mapper.write_prg(0x6001, 5); // CHR: bank 5
        assert_eq!(mapper.read_prg(0x8000), 10);
        assert_eq!(mapper.read_chr(0x0000), 5);
    }

    // --- Mirroring ---

    #[test]
    fn bit4_set_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn bit4_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x10); // Set horizontal first
        mapper.write_prg(0x6000, 0x00); // Then clear
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // --- Register lock ---

    #[test]
    fn bit5_set_locks_register_and_ignores_further_writes() {
        let mut mapper = make_mapper();
        // Write with bit5 set: lock active, PRG = 0x24 & 0xFE = 0x24, page0=0x24=36, page1=37
        mapper.write_prg(0x6000, 0x24); // 0x24 = 0b00100100, bit5=1, bit3=0
        let page0_after_lock = mapper.read_prg(0x8000);
        // Subsequent write must be ignored
        mapper.write_prg(0x6000, 0x00);
        assert_eq!(
            mapper.read_prg(0x8000),
            page0_after_lock,
            "Writes must be ignored after lock"
        );
    }

    #[test]
    fn lock_does_not_affect_already_applied_values() {
        let mut mapper = make_mapper();
        // value = 0x24: bit5=1 (lock), bit4=0 (vert), bit3=0 (NROM), bank = 0x24 & 0xFE = 36
        mapper.write_prg(0x6000, 0x24);
        assert_eq!(mapper.read_prg(0x8000), 36);
        assert_eq!(mapper.read_prg(0xC000), 37);
    }

    #[test]
    fn writes_outside_6000_6fff_are_ignored() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5FFF, 0x0A);
        mapper.write_prg(0x7000, 0x0A);
        mapper.write_prg(0x8000, 0x0A);
        // Power-on: pages 0/1
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
    }

    // --- Reset ---

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x24); // lock + bank 36
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // --- Snapshot / restore ---

    #[test]
    fn snapshot_restore_preserves_prg_chr_and_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x1A); // UNROM mode (bit3=1), bank 26, bit4=1 → Horizontal
        mapper.write_prg(0x6001, 7); // CHR bank 7

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(restored.read_prg(0x8000), 26);
        assert_eq!(restored.read_prg(0xC000), 26);
        assert_eq!(restored.read_chr(0x0000), 7);
        assert_eq!(restored.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn snapshot_restore_preserves_lock_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x24); // bit5=1 → lock

        let snap = mapper.registers_snapshot();
        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        // After restore with lock, writes must be ignored
        restored.write_prg(0x6000, 0x00);
        assert_eq!(restored.read_prg(0x8000), 36);
    }

    // --- No IRQ ---

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 332 must never assert IRQ");
    }

    // --- Capabilities ---

    #[test]
    fn capabilities_match_spec() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(caps.has_dynamic_mirroring);
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
