//! Mapper 136 – Sachen 3011 (JV001 ASIC)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_136>
//!
//! The board uses the JV001 ASIC, a 28-pin adder / inverter / latch.
//! Copy protection relies on a sequence of latch, increment, and inversion
//! operations, checked by reading back the 6-bit register at $4100.
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities, MapperContext};

pub struct Mapper136 {
    base: BaseMapper,
    /// Six-bit input register set by writes to $4102 (masked $E103).
    input: u8,
    /// Six-bit internal register; bits 0–3 latched/incremented, bits 4–5 from input.
    register: u8,
    /// When `true` a write to $4100 increments bits 0–3 instead of latching.
    mode: bool,
    /// When `true`, bits 0–3 of `input` are inverted on latch, and bits 4–5 of
    /// `register` are inverted on read.
    invert: bool,
    /// Output latched by writing to $8000–$FFFF; selects PRG/CHR banks.
    output: u8,
}

impl Mapper136 {
    pub fn new(ctx: MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 8,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(32 * 1024);
        base.configure_chr_banking(8 * 1024);
        let mut mapper = Self {
            base,
            input: 0,
            register: 0,
            mode: false,
            invert: false,
            output: 0,
        };
        mapper.apply_output_to_banks();
        mapper
    }

    /// True when `addr` falls in the masked $4100 register window ($E103 decode).
    fn in_register_window(addr: u16) -> bool {
        addr & 0xE100 == 0x4100
    }

    /// True when `addr` reads back the register (bit 8 set, bits 1–0 == 00).
    fn is_register_read(addr: u16) -> bool {
        addr & 0x0101 == 0x0100
    }

    /// The 6-bit value returned when the register is read.
    /// Bits 4–5 are inverted if the invert flag is set.
    fn register_read_value(&self) -> u8 {
        let inv_mask = if self.invert { 0x30u8 } else { 0u8 };
        (self.register ^ inv_mask) & 0x3F
    }

    fn apply_output_to_banks(&mut self) {
        self.base
            .select_prg_page(0, ((self.output >> 4) & 0x01) as i16);
        self.base.select_chr_page(0, (self.output & 0x07) as i16);
    }
}

impl Mapper for Mapper136 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if (0x8000..=0xFFFF).contains(&addr) {
            return self.base.read_prg_rom(addr);
        }
        0
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if Self::in_register_window(addr) && Self::is_register_read(addr) {
            return (open_bus & 0xC0) | self.register_read_value();
        }
        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::in_register_window(addr) {
            match addr & 0xE103 {
                0x4100 => {
                    if self.mode {
                        let low = (self.register & 0x0F).wrapping_add(1) & 0x0F;
                        self.register = (self.register & 0x30) | low;
                    } else {
                        let high = self.input & 0x30;
                        let low = (self.input & 0x0F) ^ (if self.invert { 0x0F } else { 0 });
                        self.register = high | low;
                    }
                }
                0x4101 => self.invert = value & 0x01 != 0,
                0x4102 => self.input = value & 0x3F,
                0x4103 => self.mode = value & 0x01 != 0,
                _ => {}
            }
            return;
        }

        if addr & 0x8000 != 0 {
            self.output = self.register;
            self.apply_output_to_banks();
        }
    }

    fn reset(&mut self) {
        self.input = 0;
        self.register = 0;
        self.mode = false;
        self.invert = false;
        self.output = 0;
        self.apply_output_to_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.input,
            self.register,
            self.mode as u8,
            self.invert as u8,
            self.output,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 5 {
            return;
        }
        self.input = data[0] & 0x3F;
        self.register = data[1] & 0x3F;
        self.mode = data[2] & 0x01 != 0;
        self.invert = data[3] & 0x01 != 0;
        self.output = data[4] & 0x3F;
        self.apply_output_to_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS_32K: usize = 2;
    const CHR_BANKS_8K: usize = 8;

    fn make_mapper136(prg_banks: usize, chr_banks: usize) -> Box<dyn Mapper> {
        let prg_rom = banked_data(32 * 1024, prg_banks);
        let chr_rom = banked_data(8 * 1024, chr_banks);
        create_mapper(MapperContext::new_for_test(
            136,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ))
        .expect("mapper 136 should be creatable")
    }

    fn latch_from_input(mapper: &mut dyn Mapper, input: u8, invert: bool, mode: bool) {
        mapper.write_prg(0x4102, input);
        mapper.write_prg(0x4101, u8::from(invert));
        mapper.write_prg(0x4103, u8::from(mode));
        mapper.write_prg(0x4100, 0);
    }

    #[test]
    fn mapper_136_is_registered() {
        let m = make_mapper136(PRG_BANKS_32K, CHR_BANKS_8K);
        assert_eq!(m.mapper_number(), 136);
    }

    #[test]
    fn write_8000_latches_register_to_output_and_selects_banks() {
        let mut m = make_mapper136(PRG_BANKS_32K, CHR_BANKS_8K);

        // input = 0b01_0000_0111 = 0x17 → bits 4-5: 01, bits 0-2: 111
        // CHR bank = output & 7 = 7, PRG bank = (output >> 4) & 1 = 1
        latch_from_input(m.as_mut(), 0x17, false, false);
        m.write_prg(0x8000, 0); // latch output = register = 0x17

        assert_eq!(m.read_chr(0x0000), 7, "CHR bank from output bits 0-2");
        assert_eq!(m.read_prg(0x8000), 1, "PRG bank from output bit 4");
    }

    #[test]
    fn read_4100_returns_register_with_bits_4_5_inverted_when_invert_set() {
        let mut m = make_mapper136(PRG_BANKS_32K, CHR_BANKS_8K);

        // input = 0x15 = 0b01_0101: high=0x10, low=0x05
        // invert=false: register = 0x15; read: 0x15 (no inversion on bits 4-5)
        latch_from_input(m.as_mut(), 0x15, false, false);
        assert_eq!(m.read_prg_open_bus(0x4100, 0xC0), 0xC0 | 0x15);

        // invert=true on latch: bits 0-3 inverted → low = 0x05 ^ 0x0F = 0x0A
        // register = 0x1A; read with invert: 0x1A ^ 0x30 = 0x2A
        latch_from_input(m.as_mut(), 0x15, true, false);
        assert_eq!(m.read_prg_open_bus(0x4100, 0x00), 0x2A);
    }

    #[test]
    fn increment_mode_wraps_bits_0_to_3_and_preserves_bits_4_5() {
        let mut m = make_mapper136(PRG_BANKS_32K, CHR_BANKS_8K);

        // Latch 0x1F (bits 4-5=01, bits 0-3=0xF)
        latch_from_input(m.as_mut(), 0x1F, false, false);
        assert_eq!(m.read_prg_open_bus(0x4100, 0), 0x1F);

        // Switch to increment mode
        m.write_prg(0x4103, 1);
        m.write_prg(0x4100, 0); // increment: bits 0-3 wrap 0xF → 0x0; bits 4-5 stay
        assert_eq!(
            m.read_prg_open_bus(0x4100, 0),
            0x10,
            "bits 0-3 wrapped to 0"
        );

        m.write_prg(0x4100, 0); // increment again: 0x0 → 0x1
        assert_eq!(
            m.read_prg_open_bus(0x4100, 0),
            0x11,
            "bits 0-3 incremented to 1"
        );
    }

    #[test]
    fn register_window_decode_uses_e103_mask() {
        let mut m = make_mapper136(PRG_BANKS_32K, CHR_BANKS_8K);

        // $4102 decoded by $E103 mask
        m.write_prg(0x4102, 0b11_0101); // alias also matched
        m.write_prg(0x4302, 0b11_0101); // $E103 & $4302 = $4102 → also decoded
        m.write_prg(0x4100, 0);
        assert_eq!(m.read_prg_open_bus(0x4100, 0), 0b11_0101);
    }

    #[test]
    fn output_is_not_applied_until_8000_write() {
        let mut m = make_mapper136(PRG_BANKS_32K, CHR_BANKS_8K);

        latch_from_input(m.as_mut(), 0x17, false, false);
        assert_eq!(m.read_chr(0x0000), 0, "bank unchanged before $8000 write");

        m.write_prg(0x8000, 0);
        assert_eq!(m.read_chr(0x0000), 7, "bank applied after $8000 write");
    }

    #[test]
    fn reset_clears_all_state_and_banks() {
        let mut m = make_mapper136(PRG_BANKS_32K, CHR_BANKS_8K);

        latch_from_input(m.as_mut(), 0x37, false, false);
        m.write_prg(0x8000, 0);

        m.reset();
        assert_eq!(m.read_prg_open_bus(0x4100, 0), 0);
        assert_eq!(m.read_prg(0x8000), 0);
        assert_eq!(m.read_chr(0x0000), 0);
    }

    #[test]
    fn snapshot_and_restore_preserve_full_state() {
        let mut m = make_mapper136(PRG_BANKS_32K, CHR_BANKS_8K);

        // input=0x15, invert=true → register=0x1A, output=0x1A
        // CHR bank = 0x1A & 0x07 = 2; PRG bank = (0x1A >> 4) & 1 = 1
        latch_from_input(m.as_mut(), 0x15, true, false);
        m.write_prg(0x8000, 0);
        let snap = m.registers_snapshot();

        m.reset();
        m.restore_registers(&snap);

        // read with invert=true: (0x1A ^ 0x30) & 0x3F = 0x2A
        assert_eq!(m.read_prg_open_bus(0x4100, 0), 0x2A);
        assert_eq!(
            m.read_chr(0x0000),
            2,
            "CHR bank from restored output bits 0-2"
        );
    }
}
