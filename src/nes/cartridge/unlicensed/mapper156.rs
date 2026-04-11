//! Mapper 156 - Daou Infosys (DIS23C01 DAOU 245 ASIC)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::NametableLayout;
use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

#[allow(dead_code)]
const MAPPER_NUMBER: u16 = 156;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 1024;
/// Number of 1 KB CHR slots
const CHR_SLOTS: usize = 8;

/// Mapper 156 - Daou Infosys (DIS23C01 DAOU 245 ASIC)
///
/// Hardware: DAOU ROM Controller DIS23C01 DAOU 245 ASIC by Daou Infosys (Korea).
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_156>
/// - PRG-ROM: 16 KB switchable at $8000–$BFFF; 16 KB fixed to last bank at $C000–$FFFF
/// - PRG-RAM: 8 KB at $6000–$7FFF
/// - CHR-ROM: 8 × 1 KB switchable banks; bank numbers are 9 bits wide
/// - Mirroring: Software-controlled; defaults to one-screen (page 0) at power-on
///
/// Registers (writes to CPU $C000–$C014):
///
/// ```text
/// $C000–$C003: Low 8 bits of 9-bit CHR bank for slots 0–3
/// $C004–$C007: High bit (bit 8) of 9-bit CHR bank for slots 0–3
/// $C008–$C00B: Low 8 bits of 9-bit CHR bank for slots 4–7
/// $C00C–$C00F: High bit (bit 8) of 9-bit CHR bank for slots 4–7
/// $C010:       [.... PPPP] 16 KB PRG bank at $8000–$BFFF (bits 3:0)
/// $C014:       [.... ..MM] Nametable mirroring: 00=Vertical, 01=Horizontal, 1x=One-screen page 0
/// ```
pub struct Mapper156 {
    base: BaseMapper,
    /// 9-bit CHR bank numbers for the 8 × 1 KB PPU slots.
    chr_banks: [u16; CHR_SLOTS],
    /// PRG bank selection register (bits 3:0 used).
    prg_reg: u8,
    /// Raw mirroring register value (bits 1:0 decoded to NametableLayout).
    mirr_reg: u8,
}

/// Default power-on mirroring: one-screen page 0 (per NesDev spec).
const INITIAL_MIRR_REG: u8 = 0b10;

impl Mapper156 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            ..Default::default()
        };
        let mut ctx = ctx;
        ctx.prg_ram_banks_8k = 1;
        ctx.prg_ram_size_specified = true;
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);
        let mut mapper = Self {
            base,
            chr_banks: [0u16; CHR_SLOTS],
            prg_reg: 0,
            mirr_reg: INITIAL_MIRR_REG,
        };
        mapper.apply_state();
        mapper
    }

    fn apply_state(&mut self) {
        // PRG: slot 0 switchable, slot 1 fixed to last 16 KB bank
        self.base.select_prg_page(0, (self.prg_reg & 0x0F) as i16);
        self.base.select_prg_page(1, -1);

        // CHR: apply all 8 × 1 KB bank selections
        for slot in 0..CHR_SLOTS {
            self.base.select_chr_page(slot, self.chr_banks[slot] as i16);
        }

        // Mirroring
        self.apply_mirroring(self.mirr_reg);
    }

    fn apply_mirroring(&mut self, value: u8) {
        self.mirr_reg = value;
        let layout = match value & 0x03 {
            0 => NametableLayout::Vertical,
            1 => NametableLayout::Horizontal,
            _ => NametableLayout::SingleScreenLower,
        };
        self.base.set_mirroring(layout);
    }
}

impl Mapper for Mapper156 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        let low_nibble = addr & 0x000F;
        match addr {
            // CHR low bytes for slots 0–3 ($C000–$C003)
            0xC000..=0xC003 => {
                let slot = low_nibble as usize;
                self.chr_banks[slot] = (self.chr_banks[slot] & 0x100) | value as u16;
                self.base.select_chr_page(slot, self.chr_banks[slot] as i16);
            }
            // CHR high bits for slots 0–3 ($C004–$C007)
            0xC004..=0xC007 => {
                let slot = (low_nibble & 0x03) as usize;
                self.chr_banks[slot] = (self.chr_banks[slot] & 0x0FF) | ((value as u16 & 1) << 8);
                self.base.select_chr_page(slot, self.chr_banks[slot] as i16);
            }
            // CHR low bytes for slots 4–7 ($C008–$C00B)
            0xC008..=0xC00B => {
                let slot = (low_nibble & 0x03) as usize + 4;
                self.chr_banks[slot] = (self.chr_banks[slot] & 0x100) | value as u16;
                self.base.select_chr_page(slot, self.chr_banks[slot] as i16);
            }
            // CHR high bits for slots 4–7 ($C00C–$C00F)
            0xC00C..=0xC00F => {
                let slot = (low_nibble & 0x03) as usize + 4;
                self.chr_banks[slot] = (self.chr_banks[slot] & 0x0FF) | ((value as u16 & 1) << 8);
                self.base.select_chr_page(slot, self.chr_banks[slot] as i16);
            }
            // PRG bank select ($C010)
            0xC010 => {
                self.prg_reg = value;
                self.base.select_prg_page(0, (value & 0x0F) as i16);
            }
            // Mirroring ($C014)
            0xC014 => {
                self.apply_mirroring(value);
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = Vec::with_capacity(2 + CHR_SLOTS * 2);
        snap.push(self.prg_reg);
        snap.push(self.mirr_reg);
        for &bank in &self.chr_banks {
            snap.push((bank & 0xFF) as u8);
            snap.push(((bank >> 8) & 1) as u8);
        }
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        let expected = 2 + CHR_SLOTS * 2;
        if data.len() < expected {
            return;
        }
        self.prg_reg = data[0];
        self.mirr_reg = data[1];
        for slot in 0..CHR_SLOTS {
            let lo = data[2 + slot * 2] as u16;
            let hi = (data[3 + slot * 2] as u16 & 1) << 8;
            self.chr_banks[slot] = lo | hi;
        }
        self.apply_state();
    }

    fn reset(&mut self) {
        self.chr_banks = [0u16; CHR_SLOTS];
        self.prg_reg = 0;
        self.mirr_reg = INITIAL_MIRR_REG;
        self.apply_state();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::mapper::{MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::{banked_data, banked_data_with_upper_marker};

    // 4 × 16 KB = 64 KB PRG; 16 × 1 KB = 16 KB CHR (enough for non-9-bit tests)
    const PRG_BANKS: usize = 4;
    const CHR_BANKS: usize = 16;

    fn make_mapper() -> Mapper156 {
        Mapper156::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ))
    }

    // ── Factory ──────────────────────────────────────────────────────────────

    #[test]
    fn mapper156_registers_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            banked_data(CHR_BANK_SIZE, CHR_BANKS),
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 156 must be creatable via factory");
    }

    // ── PRG – power-on state ─────────────────────────────────────────────────

    #[test]
    fn prg_slot0_defaults_to_bank0_at_8000() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 should read PRG bank 0 on reset"
        );
    }

    #[test]
    fn prg_slot1_fixed_to_last_bank_at_c000() {
        let mapper = make_mapper();
        // 4 PRG banks → last bank = 3
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 should be fixed to last PRG bank"
        );
        assert_eq!(
            mapper.read_prg(0xFFFF),
            3,
            "$FFFF should be in last PRG bank"
        );
    }

    // ── PRG – switching ───────────────────────────────────────────────────────

    #[test]
    fn write_to_c010_switches_prg_slot0() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC010, 2);
        assert_eq!(
            mapper.read_prg(0x8000),
            2,
            "$8000 should read PRG bank 2 after $C010 = 2"
        );
    }

    #[test]
    fn prg_reg_uses_low_4_bits_only() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC010, 0xF1); // only low nibble = 1
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "PRG reg must mask to low 4 bits"
        );
    }

    #[test]
    fn prg_slot1_remains_fixed_after_slot0_switch() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC010, 1);
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "$C000 must remain fixed at last bank"
        );
    }

    // ── PRG-RAM ───────────────────────────────────────────────────────────────

    #[test]
    fn prg_ram_at_6000_readable_and_writable() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0xAB);
        assert_eq!(
            mapper.read_prg(0x6000),
            0xAB,
            "$6000 PRG-RAM write/read must work"
        );
        mapper.write_prg(0x7FFF, 0xCD);
        assert_eq!(
            mapper.read_prg(0x7FFF),
            0xCD,
            "$7FFF PRG-RAM write/read must work"
        );
    }

    // ── CHR – power-on state ──────────────────────────────────────────────────

    #[test]
    fn chr_all_slots_default_to_bank0() {
        let mut mapper = make_mapper();
        for slot in 0..CHR_SLOTS {
            let addr = (slot as u16) * (CHR_BANK_SIZE as u16);
            assert_eq!(
                mapper.read_chr(addr),
                0,
                "CHR slot {slot} should default to bank 0"
            );
        }
    }

    // ── CHR – low byte selection ($C000-$C003 for slots 0-3) ─────────────────

    #[test]
    fn write_low_byte_to_c000_c003_selects_chr_slots_0_to_3() {
        let mut mapper = make_mapper();
        for slot in 0u16..4 {
            mapper.write_prg(0xC000 + slot, (slot + 5) as u8); // banks 5,6,7,8 (wrap to 5-8)
            let addr = slot * CHR_BANK_SIZE as u16;
            assert_eq!(
                mapper.read_chr(addr),
                (slot + 5) as u8,
                "CHR slot {slot} should select bank {}",
                slot + 5
            );
        }
    }

    // ── CHR – low byte selection ($C008-$C00B for slots 4-7) ─────────────────

    #[test]
    fn write_low_byte_to_c008_c00b_selects_chr_slots_4_to_7() {
        let mut mapper = make_mapper();
        for i in 0u16..4 {
            let slot = i + 4;
            mapper.write_prg(0xC008 + i, (slot + 2) as u8); // banks 6,7,8,9
            let addr = slot * CHR_BANK_SIZE as u16;
            assert_eq!(
                mapper.read_chr(addr),
                (slot + 2) as u8,
                "CHR slot {slot} should select bank {}",
                slot + 2
            );
        }
    }

    // ── CHR – 9-bit bank number via high bit ($C004-$C007, $C00C-$C00F) ──────

    #[test]
    fn write_high_bit_to_c004_extends_chr_bank_to_9_bits_for_slots_0_to_3() {
        // Use 512-bank CHR ROM where banks 256+ have upper marker byte 1
        let chr_rom = banked_data_with_upper_marker(CHR_BANK_SIZE, 512);
        let mut mapper = Mapper156::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            chr_rom,
            NametableLayout::Horizontal,
        ));
        // Set low byte = 0 (bank 0 without high bit), then set high bit → bank 256
        mapper.write_prg(0xC000, 0x00); // low byte = 0
        mapper.write_prg(0xC004, 0x01); // high bit = 1 → bank 256
        // Bank 256 with upper marker fills with 1
        assert_eq!(
            mapper.read_chr(0x0000),
            1,
            "Setting high bit should select bank 256 (upper marker = 1)"
        );
    }

    #[test]
    fn write_high_bit_to_c00c_extends_chr_bank_to_9_bits_for_slots_4_to_7() {
        let chr_rom = banked_data_with_upper_marker(CHR_BANK_SIZE, 512);
        let mut mapper = Mapper156::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            chr_rom,
            NametableLayout::Horizontal,
        ));
        mapper.write_prg(0xC008, 0x00); // low byte = 0 for slot 4
        mapper.write_prg(0xC00C, 0x01); // high bit = 1 for slot 4 → bank 256
        assert_eq!(
            mapper.read_chr(0x1000),
            1,
            "Setting high bit via $C00C should select bank 256 for slot 4"
        );
    }

    #[test]
    fn high_bit_cleared_returns_to_low_bank() {
        let chr_rom = banked_data_with_upper_marker(CHR_BANK_SIZE, 512);
        let mut mapper = Mapper156::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            chr_rom,
            NametableLayout::Horizontal,
        ));
        mapper.write_prg(0xC000, 0x00);
        mapper.write_prg(0xC004, 0x01); // high bit = 1 → bank 256
        mapper.write_prg(0xC004, 0x00); // clear high bit → bank 0
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Clearing high bit should return to low bank (upper marker = 0)"
        );
    }

    // ── CHR – independence of slots ────────────────────────────────────────────

    #[test]
    fn chr_slots_are_independent() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC000, 3);
        mapper.write_prg(0xC001, 7);
        mapper.write_prg(0xC002, 5);
        assert_eq!(mapper.read_chr(0x0000), 3, "Slot 0 bank 3");
        assert_eq!(mapper.read_chr(0x0400), 7, "Slot 1 bank 7");
        assert_eq!(mapper.read_chr(0x0800), 5, "Slot 2 bank 5");
        // Slots 3-7 still at default bank 0
        assert_eq!(mapper.read_chr(0x0C00), 0, "Slot 3 bank 0 (unchanged)");
    }

    // ── Mirroring ─────────────────────────────────────────────────────────────

    #[test]
    fn default_mirroring_is_one_screen_lower() {
        let mapper = make_mapper();
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "Default mirroring must be one-screen page 0 (SingleScreenLower)"
        );
    }

    #[test]
    fn c014_00_sets_vertical_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC014, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "$C014 = 0b00 → Vertical"
        );
    }

    #[test]
    fn c014_01_sets_horizontal_mirroring() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC014, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "$C014 = 0b01 → Horizontal"
        );
    }

    #[test]
    fn c014_1x_sets_one_screen_lower() {
        let mut mapper = make_mapper();
        for val in [0x02u8, 0x03] {
            mapper.write_prg(0xC014, val);
            assert_eq!(
                mapper.get_mirroring(),
                NametableLayout::SingleScreenLower,
                "$C014 = {val:#04x} → one-screen page 0"
            );
        }
    }

    // ── Snapshot / Restore ────────────────────────────────────────────────────

    #[test]
    fn snapshot_restore_preserves_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC010, 2);
        mapper.write_prg(0xC000, 5);
        mapper.write_prg(0xC001, 9);
        mapper.write_prg(0xC014, 0x01);

        let snap = mapper.registers_snapshot();

        let mut fresh = make_mapper();
        fresh.restore_registers(&snap);

        assert_eq!(fresh.read_prg(0x8000), 2, "PRG bank restored");
        assert_eq!(fresh.read_chr(0x0000), 5, "CHR slot 0 restored");
        assert_eq!(fresh.read_chr(0x0400), 9, "CHR slot 1 restored");
        assert_eq!(
            fresh.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring restored"
        );
    }

    // ── Reset ─────────────────────────────────────────────────────────────────

    #[test]
    fn reset_restores_power_on_state() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xC010, 3);
        mapper.write_prg(0xC000, 7);
        mapper.write_prg(0xC014, 0x01);

        mapper.reset();

        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "PRG slot 0 must reset to bank 0"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "CHR slot 0 must reset to bank 0"
        );
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::SingleScreenLower,
            "Mirroring must reset to one-screen (default)"
        );
    }
}
