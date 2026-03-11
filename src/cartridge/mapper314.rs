//! Mapper 314 – BMC-64IN1NOREPEATE (64-in-1 no-repeat multicart)
//!
//! Specifications:
//! - Primary source: NesDev wiki (unavailable due to Cloudflare 403 in this environment).
//! - Fallback source: Mesen2 `Bmc64in1NoRepeat.h`
//!   <https://github.com/SourMesen/Mesen2/blob/master/Core/NES/Mappers/Unlicensed/Bmc64in1NoRepeat.h>
//!
//! # Hardware overview
//!
//! Four write-only registers at CPU $5000–$5003:
//!
//! | Address | Register | Reset |
//! |---------|----------|-------|
//! | $5000   | reg[0]   | $80   |
//! | $5001   | reg[1]   | $43   |
//! | $5002   | reg[2]   | $00   |
//! | $5003   | reg[3]   | $00   |
//!
//! Writes to $8000–$FFFF also update reg[3].
//!
//! # PRG banking (16 KiB pages)
//!
//! Mode is gated by **reg[0] bit 7**:
//!
//! - **reg[0] bit 7 = 1** (outer mode active):
//!   - **reg[1] bit 7 = 1** → 32 KiB mode: bank pair = `(reg[1] & 0x1F) << 1`;
//!     page 0 ($8000) = bank_pair, page 1 ($C000) = bank_pair + 1.
//!   - **reg[1] bit 7 = 0** → 16 KiB NROM mode: bank = `((reg[1] & 0x1F) << 1) | ((reg[1] >> 6) & 0x01)`;
//!     both page 0 and page 1 map to the same bank.
//! - **reg[0] bit 7 = 0** (inner mode): only page 1 ($C000) is updated:
//!   bank = `((reg[1] & 0x1F) << 1) | ((reg[1] >> 6) & 0x01)`.
//!   Page 0 ($8000) is unchanged.
//!
//! # CHR banking (8 KiB)
//!
//! `chr_bank = (reg[2] << 2) | ((reg[0] >> 1) & 0x03)`
//!
//! # Mirroring
//!
//! reg[0] bit 5: 1 = Horizontal, 0 = Vertical
//!
//! # Power-on / reset state
//!
//! reg[0]=0x80, reg[1]=0x43, reg[2]=0x00, reg[3]=0x00
//!
//! At reset: page 0 and page 1 both map to PRG bank 7, CHR bank 0, Vertical mirroring.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 314;
const PRG_BANK_SIZE_BYTES: usize = 16 * 1024;
const CHR_BANK_SIZE_BYTES: usize = 8 * 1024;

/// Mapper 314 – BMC-64IN1NOREPEATE
pub struct Mapper314 {
    base: BaseMapper,
    regs: [u8; 4],
}

impl Mapper314 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE_BYTES);
        base.configure_chr_banking(CHR_BANK_SIZE_BYTES);

        let mut mapper = Self { base, regs: [0; 4] };
        mapper.apply_reset();
        mapper
    }

    fn apply_reset(&mut self) {
        self.regs = [0x80, 0x43, 0x00, 0x00];
        self.apply_state();
    }

    fn apply_state(&mut self) {
        let r0 = self.regs[0];
        let r1 = self.regs[1];
        let r2 = self.regs[2];

        // CHR bank
        let chr_bank = ((r2 as u16) << 2) | (((r0 >> 1) & 0x03) as u16);
        self.base.select_chr_page(0, chr_bank as i16);

        // Mirroring: bit 5 of reg[0]
        self.base.set_mirroring_hv((r0 & 0x20) != 0);

        // PRG banking
        if r0 & 0x80 != 0 {
            if r1 & 0x80 != 0 {
                // 32 KB mode: consecutive pair
                let bank_pair = ((r1 & 0x1F) as i16) << 1;
                self.base.select_prg_page(0, bank_pair);
                self.base.select_prg_page(1, bank_pair + 1);
            } else {
                // 16 KB NROM mode: both windows same bank
                let bank = (((r1 & 0x1F) as i16) << 1) | (((r1 >> 6) & 0x01) as i16);
                self.base.select_prg_page(0, bank);
                self.base.select_prg_page(1, bank);
            }
        } else {
            // Inner mode: only $C000 (page 1) changes
            let bank = (((r1 & 0x1F) as i16) << 1) | (((r1 >> 6) & 0x01) as i16);
            self.base.select_prg_page(1, bank);
        }
    }
}

impl Mapper for Mapper314 {
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
        if (0x5000..=0x5003).contains(&addr) {
            self.regs[(addr & 0x03) as usize] = value;
        } else if addr >= 0x8000 {
            self.regs[3] = value;
        }
        self.apply_state();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.regs.to_vec()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 4 {
            return;
        }
        self.regs.copy_from_slice(&data[..4]);
        self.apply_state();
    }

    fn reset(&mut self) {
        self.apply_reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // Non-power-of-two to prevent false-pass modulo wrapping.
    const PRG_BANKS_16K: usize = 11; // 11 × 16 KiB
    const CHR_BANKS_8K: usize = 9; // 9 × 8 KiB

    fn make_mapper() -> Mapper314 {
        Mapper314::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ))
    }

    // ── Factory registration ──────────────────────────────────────────────────

    #[test]
    fn mapper_314_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE_BYTES, PRG_BANKS_16K),
            banked_data(CHR_BANK_SIZE_BYTES, CHR_BANKS_8K),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 314 must be registered in factory");
    }

    // ── Power-on / reset state ────────────────────────────────────────────────
    // On reset: reg[0]=0x80, reg[1]=0x43
    // reg[0]&0x80 = set → outer mode
    // reg[1]&0x80 = clear → 16KB NROM mode
    // bank = ((0x43 & 0x1F) << 1) | ((0x43 >> 6) & 0x01)
    //      = (0x03 << 1) | (0x01)
    //      = 0x06 | 0x01 = 7
    // Both $8000 and $C000 → bank 7
    // CHR: (0 << 2) | ((0x80 >> 1) & 0x03) = 0 | (0x40 & 0x03) = 0
    // Mirroring: 0x80 & 0x20 = 0 → Vertical

    #[test]
    fn power_on_prg_page0_is_bank_7() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            7,
            "$8000 should be PRG bank 7 at power-on"
        );
    }

    #[test]
    fn power_on_prg_page1_is_bank_7() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "$C000 should be PRG bank 7 at power-on"
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

    #[test]
    fn power_on_mirroring_is_vertical() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be Vertical at power-on (reg[0] bit5 = 0)"
        );
    }

    // ── Registers at $5000–$5003 ──────────────────────────────────────────────

    #[test]
    fn reg0_controls_outer_mode_bit() {
        let mut mapper = make_mapper();
        // Set reg[0] bit7 = 0 (inner mode), reg[1] = 0x00 → bank 0
        // page 0 stays at reset bank 7; page 1 → bank 0
        mapper.write_prg(0x5000, 0x00); // reg[0] = 0x00
        mapper.write_prg(0x5001, 0x00); // reg[1] = 0x00 → page1 bank 0
        assert_eq!(
            mapper.read_prg(0xC000),
            0,
            "page 1 should be bank 0 in inner mode with reg[1]=0"
        );
        // page 0 unchanged from its previous state (bank 7 from reset in outer NROM mode)
        assert_eq!(
            mapper.read_prg(0x8000),
            7,
            "page 0 must not change when reg[0] bit7 is 0"
        );
    }

    #[test]
    fn reg0_and_reg1_set_32kb_mode() {
        let mut mapper = make_mapper();
        // reg[0]=0x80 (outer mode on), reg[1]=0x82 (bit7=1 → 32KB, bits[4:0]=2)
        // bank_pair = (2 & 0x1F) << 1 = 4; page0=4, page1=5
        mapper.write_prg(0x5000, 0x80);
        mapper.write_prg(0x5001, 0x82);
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "page 0 should be bank 4 in 32KB mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            5,
            "page 1 should be bank 5 in 32KB mode (consecutive)"
        );
    }

    #[test]
    fn reg0_and_reg1_set_16kb_nrom_mode() {
        let mut mapper = make_mapper();
        // reg[0]=0x80, reg[1]=0x03 (bit7=0, bit6=0, bits[4:0]=3)
        // bank = (3 << 1) | 0 = 6; both pages = 6
        mapper.write_prg(0x5000, 0x80);
        mapper.write_prg(0x5001, 0x03);
        assert_eq!(
            mapper.read_prg(0x8000),
            6,
            "page 0 should be bank 6 in 16KB NROM mode"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            6,
            "page 1 should be bank 6 in 16KB NROM mode (same as page 0)"
        );
    }

    #[test]
    fn reg1_bit6_acts_as_prg_lsb() {
        let mut mapper = make_mapper();
        // reg[0]=0x80, reg[1]=0x43 (bit7=0, bit6=1, bits[4:0]=3)
        // bank = (3 << 1) | 1 = 7
        mapper.write_prg(0x5000, 0x80);
        mapper.write_prg(0x5001, 0x43);
        assert_eq!(
            mapper.read_prg(0x8000),
            7,
            "reg[1] bit6 contributes to PRG bank as LSB"
        );
    }

    #[test]
    fn reg1_in_inner_mode_selects_page1_bank() {
        let mut mapper = make_mapper();
        // reg[0]=0x00 (inner mode), reg[1]=0x02 → bank = (2<<1)|0 = 4
        mapper.write_prg(0x5000, 0x00);
        mapper.write_prg(0x5001, 0x02);
        assert_eq!(
            mapper.read_prg(0xC000),
            4,
            "page 1 should be bank 4 in inner mode"
        );
    }

    #[test]
    fn reg2_and_reg0_bits_control_chr_bank() {
        let mut mapper = make_mapper();
        // CHR = (reg[2] << 2) | ((reg[0] >> 1) & 0x03)
        // reg[2]=0x01, reg[0]=0x80 → CHR = (1<<2) | ((0x80>>1)&0x03) = 4 | (0x40&0x03) = 4 | 0 = 4
        mapper.write_prg(0x5000, 0x80);
        mapper.write_prg(0x5002, 0x01);
        assert_eq!(mapper.read_chr(0x0000), 4, "CHR bank should be 4");
    }

    #[test]
    fn reg0_bits_1_0_contribute_to_chr_bank() {
        let mut mapper = make_mapper();
        // reg[0]=0x82 → bits[2:1] = 0x01 → (reg[0]>>1)&0x03 = (0x82>>1)&0x03 = 0x41&0x03 = 0x01
        // reg[2]=0 → CHR = (0<<2) | 0x01 = 1
        mapper.write_prg(0x5000, 0x82);
        mapper.write_prg(0x5002, 0x00);
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "reg[0] bits [2:1] contribute to CHR bank"
        );
    }

    #[test]
    fn reg0_bit5_set_selects_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x20); // bit5 = 1 → Horizontal
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "reg[0] bit5=1 should select horizontal mirroring"
        );
    }

    #[test]
    fn reg0_bit5_clear_selects_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x20); // set horizontal
        mapper.write_prg(0x5000, 0x00); // clear bit5 → vertical
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "reg[0] bit5=0 should select vertical mirroring"
        );
    }

    #[test]
    fn write_to_8000_ffff_updates_reg3() {
        // Writing to $8000-$FFFF sets reg[3]; it shouldn't crash / should update state
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x42);
        assert_eq!(
            mapper.registers_snapshot()[3],
            0x42,
            "reg[3] should be updated"
        );
    }

    #[test]
    fn write_to_5003_updates_reg3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5003, 0x77);
        assert_eq!(
            mapper.registers_snapshot()[3],
            0x77,
            "reg[3] should be updated via $5003"
        );
    }

    // ── Mirroring stays independent of PRG ───────────────────────────────────

    #[test]
    fn mirroring_and_banking_can_change_together() {
        let mut mapper = make_mapper();
        // reg[0]=0xA0 (bit7=outer mode, bit5=H mirror), reg[1]=0x03 → bank 6
        mapper.write_prg(0x5000, 0xA0);
        mapper.write_prg(0x5001, 0x03);
        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    // ── Snapshot / restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_all_registers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x80);
        mapper.write_prg(0x5001, 0x82); // 32KB mode
        mapper.write_prg(0x5002, 0x01); // CHR
        mapper.write_prg(0x5003, 0x07);
        let snap = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snap);

        assert_eq!(
            restored.read_prg(0x8000),
            4,
            "restored page 0 should be bank 4"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            5,
            "restored page 1 should be bank 5"
        );
        assert_eq!(
            restored.read_chr(0x0000),
            4,
            "restored CHR bank should be 4"
        );
    }

    #[test]
    fn restore_with_short_data_is_noop() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x80);
        mapper.write_prg(0x5001, 0x82); // page0=4, page1=5
        mapper.restore_registers(&[0x00, 0x00, 0x00]); // only 3 bytes — must be ignored
        assert_eq!(
            mapper.read_prg(0x8000),
            4,
            "state must be unchanged after short restore"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x5000, 0x00);
        mapper.write_prg(0x5001, 0x00);
        mapper.reset();
        assert_eq!(
            mapper.read_prg(0x8000),
            7,
            "PRG page 0 should be bank 7 after reset"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            7,
            "PRG page 1 should be bank 7 after reset"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR should be bank 0 after reset"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "mirroring should be vertical after reset"
        );
    }

    // ── Capabilities ──────────────────────────────────────────────────────────

    #[test]
    fn capabilities_match_specification() {
        let mapper = make_mapper();
        let caps = mapper.capabilities();
        assert!(!caps.has_irq, "no IRQ");
        assert!(!caps.has_expansion_audio, "no expansion audio");
        assert!(caps.has_dynamic_mirroring, "dynamic mirroring");
        assert!(caps.has_chr_banking, "CHR banking");
        assert_eq!(caps.prg_bank_size_kb, 16);
        assert_eq!(caps.chr_bank_size_kb, 8);
        assert_eq!(caps.max_prg_ram_kb, 0);
    }

    // ── No IRQ ────────────────────────────────────────────────────────────────

    #[test]
    fn irq_never_pending() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0xFF);
        assert!(!mapper.irq_pending(), "Mapper 314 must never assert IRQ");
    }
}
