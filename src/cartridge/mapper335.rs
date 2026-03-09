//! Mapper 335 - BMC-CTC-09 (10-in-1 multicart)
//!
//! Specifications:
//! - Primary reference: FCEUmm `bmcctc09.c` (libretro-fceumm)
//! - NesDev: <https://www.nesdev.org/wiki/NES_2.0_Mapper_335>
//! - UNIF board name: BMC-CTC-09
//!
//! ## Register Map
//!
//! | Address       | Function          |
//! |---------------|-------------------|
//! | $8000–$BFFF   | CHR bank register |
//! | $C000–$FFFF   | PRG bank register |
//!
//! ## PRG Banking (two 16 KB windows)
//!
//! Let `prg = regs[PRG]`:
//! - If `prg & 0x10` (NROM-128 mode): both $8000 and $C000 →
//!   bank `((prg & 0x07) << 1) | ((prg >> 3) & 1)`
//! - Otherwise (NROM-256 mode): $8000 → `(prg & 0x07) * 2`,
//!   $C000 → `(prg & 0x07) * 2 + 1`
//!
//! ## CHR Banking
//!
//! 8 KB CHR page: `regs[CHR] & 0x0F`
//!
//! ## Mirroring
//!
//! Controlled by bit 5 of the PRG register:
//! - Bit 5 = 0 → Vertical
//! - Bit 5 = 1 → Horizontal
//!
//! ## Power-on / Reset
//!
//! Both registers initialised to 0: PRG banks 0/1, CHR bank 0, vertical mirroring.
//!
//! ## Known Limitations
//!
//! No known gameplay-blocking limitations.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 335;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;
const CHR_WRITE_END: u16 = 0xBFFF;
const PRG_WRITE_START: u16 = 0xC000;
const NROM128_BIT: u8 = 0x10;
const PRG_BANK_MASK: u8 = 0x07;
const CHR_BANK_MASK: u8 = 0x0F;
const MIRROR_BIT: u8 = 0x20;
const REGISTERS_SNAPSHOT_LEN: usize = 2;

pub struct Mapper335 {
    base: BaseMapper,
    prg_reg: u8,
    chr_reg: u8,
}

impl Mapper335 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            has_chr_banking: true,
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
        };
        mapper.apply_state(0, 0);
        mapper
    }

    fn prg_bank_8000(prg_reg: u8) -> i16 {
        if prg_reg & NROM128_BIT != 0 {
            (((prg_reg & PRG_BANK_MASK) << 1) | ((prg_reg >> 3) & 1)) as i16
        } else {
            ((prg_reg & PRG_BANK_MASK) as i16) * 2
        }
    }

    fn prg_bank_c000(prg_reg: u8) -> i16 {
        if prg_reg & NROM128_BIT != 0 {
            (((prg_reg & PRG_BANK_MASK) << 1) | ((prg_reg >> 3) & 1)) as i16
        } else {
            ((prg_reg & PRG_BANK_MASK) as i16) * 2 + 1
        }
    }

    fn apply_state(&mut self, prg_reg: u8, chr_reg: u8) {
        self.prg_reg = prg_reg;
        self.chr_reg = chr_reg;

        self.base.select_prg_page(0, Self::prg_bank_8000(prg_reg));
        self.base.select_prg_page(1, Self::prg_bank_c000(prg_reg));
        self.base
            .select_chr_page(0, (chr_reg & CHR_BANK_MASK) as i16);
        self.base.set_mirroring_hv((prg_reg & MIRROR_BIT) != 0);
    }
}

impl Mapper for Mapper335 {
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
        if addr <= CHR_WRITE_END {
            self.apply_state(self.prg_reg, value);
        } else if addr >= PRG_WRITE_START {
            self.apply_state(value, self.chr_reg);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_reg, self.chr_reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < REGISTERS_SNAPSHOT_LEN {
            return;
        }
        self.apply_state(data[0], data[1]);
    }

    fn reset(&mut self) {
        self.apply_state(0, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS_16K: usize = 48;
    const CHR_BANKS_8K: usize = 16;

    fn make_mapper() -> Mapper335 {
        Mapper335::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_335_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 335 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────

    #[test]
    fn power_on_prg_banks_are_0_and_1_with_vertical_mirroring() {
        let mapper = make_mapper();
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should be PRG bank 0");
        assert_eq!(mapper.read_prg(0xC000), 1, "$C000 should be PRG bank 1");
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be vertical at power-on"
        );
    }

    #[test]
    fn power_on_chr_bank_is_0() {
        let mut mapper = make_mapper();
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR bank should be 0 at power-on"
        );
    }

    // ── PRG banking: NROM-256 mode (bit 4 clear) ─────────────────────────────

    #[test]
    fn nrom256_mode_prg_bank_3_gives_pages_6_and_7() {
        // prg_reg = 0x03: bit4=0 → NROM-256; bank = 3 → page0 = 6, page1 = 7
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 6, "$8000 should be 16KB page 6");
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 should be 16KB page 7");
    }

    #[test]
    fn nrom256_mode_prg_bank_5_gives_pages_10_and_11() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x05);
        assert_eq!(mapper.read_prg(0x8000), 10, "$8000 should be 16KB page 10");
        assert_eq!(mapper.read_prg(0xC000), 11, "$C000 should be 16KB page 11");
    }

    #[test]
    fn nrom256_mode_upper_prg_bits_are_ignored() {
        // Only bits [2:0] are used for bank in NROM-256 mode
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x43); // bit 6 set, bits [2:0] = 3 → pages 6/7
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xC000), 7);
    }

    // ── PRG banking: NROM-128 mode (bit 4 set) ───────────────────────────────

    #[test]
    fn nrom128_mode_both_windows_use_same_bank() {
        // prg_reg = 0x10: bit4=1 → NROM-128; bits[2:0]=0, bit3=0 → bank=(0<<1)|0=0
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x10);
        let bank8 = mapper.read_prg(0x8000);
        let bankc = mapper.read_prg(0xC000);
        assert_eq!(
            bank8, bankc,
            "$8000 and $C000 must use the same bank in NROM-128"
        );
    }

    #[test]
    fn nrom128_bank_formula_uses_bits2_0_and_bit3() {
        // prg_reg = 0x1B: bit4=1, bits[2:0]=3, bit3=1 → bank=(3<<1)|1=7
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x1B);
        assert_eq!(mapper.read_prg(0x8000), 7, "NROM-128 bank should be 7");
        assert_eq!(mapper.read_prg(0xC000), 7, "$C000 should also be bank 7");
    }

    #[test]
    fn nrom128_bank_with_bits2_0_eq_2_and_bit3_set_gives_bank_5() {
        // prg_reg = 0x1A: bit4=1, bits[2:0]=2, bit3=1 → bank=(2<<1)|1=5
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x1A);
        assert_eq!(mapper.read_prg(0x8000), 5, "NROM-128 bank should be 5");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should also be bank 5");
    }

    #[test]
    fn nrom128_bank_with_bits2_0_eq_1_and_bit3_clear_gives_bank_2() {
        // prg_reg = 0x11: bit4=1, bits[2:0]=1, bit3=0 → bank=(1<<1)|0=2
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x11);
        assert_eq!(mapper.read_prg(0x8000), 2, "NROM-128 bank should be 2");
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 should also be bank 2");
    }

    // ── Write handlers ────────────────────────────────────────────────────────

    #[test]
    fn writes_to_8000_bfff_change_chr_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x05);
        assert_eq!(mapper.read_chr(0x0000), 5, "CHR should switch to bank 5");
    }

    #[test]
    fn writes_to_c000_ffff_change_prg_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xFFFF, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 should be PRG page 4");
        assert_eq!(mapper.read_prg(0xC000), 5, "$C000 should be PRG page 5");
    }

    #[test]
    fn chr_write_does_not_affect_prg_banks() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x03); // PRG bank 3 → pages 6/7
        mapper.write_prg(0x8000, 0x07); // CHR bank change
        assert_eq!(mapper.read_prg(0x8000), 6, "PRG should remain at page 6");
        assert_eq!(mapper.read_prg(0xC000), 7, "PRG should remain at page 7");
    }

    #[test]
    fn prg_write_does_not_affect_chr_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x0A); // CHR bank 10
        mapper.write_prg(0xC000, 0x05); // PRG change
        assert_eq!(mapper.read_chr(0x0000), 10, "CHR should remain at bank 10");
    }

    // ── CHR banking ───────────────────────────────────────────────────────────

    #[test]
    fn chr_bank_register_uses_only_low_4_bits() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xF5); // bits[3:0] = 5
        assert_eq!(
            mapper.read_chr(0x0000),
            5,
            "CHR should be bank 5 (bits[3:0])"
        );
    }

    #[test]
    fn chr_bank_15_is_selectable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x0F);
        assert_eq!(
            mapper.read_chr(0x0000),
            15,
            "CHR bank 15 should be selectable"
        );
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn prg_bit5_set_gives_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x20); // bit 5 = 1
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "bit5=1 should select horizontal mirroring"
        );
    }

    #[test]
    fn prg_bit5_clear_gives_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x20); // set horizontal
        mapper.write_prg(0xC000, 0x00); // clear bit 5
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "bit5=0 should select vertical mirroring"
        );
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_prg_chr_and_mirroring() {
        let mut mapper = make_mapper();
        // prg_reg = 0x35: bit5=1(H), bit4=1(NROM-128), bit3=0, bits[2:0]=5 → bank=(5<<1)|0=10
        mapper.write_prg(0xC000, 0x35);
        mapper.write_prg(0x8000, 0x0C); // chr_reg: bank 12
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            10,
            "restored PRG bank should be 10"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            10,
            "restored PRG $C000 should be 10 (NROM-128)"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            12,
            "restored CHR bank should be 12"
        );
        assert_eq!(
            restored.get_mirroring(),
            NametableLayout::Horizontal,
            "restored mirroring should be horizontal"
        );
    }

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 0x25);
        mapper.write_prg(0x8000, 0x0C);
        mapper.reset();
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xC000), 1);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq);
        assert!(!caps.has_expansion_audio);
        assert!(caps.has_dynamic_mirroring);
        assert!(caps.has_chr_banking);
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }
}
