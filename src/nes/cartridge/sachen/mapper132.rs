//! Mapper 132 - TXC 22111 / UNL-22211
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_132>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mapper::{Mapper, MapperCapabilities};

const REG_SELECT_R: u16 = 0;
const REG_INVERT_MODE: u16 = 1;
const REG_SET_P_AND_S: u16 = 2;
const REG_INCREMENT_MODE: u16 = 3;

pub struct Mapper132 {
    base: BaseMapper,
    p: u8,
    r: u8,
    s: bool,
    increment_mode: bool,
    invert_mode: bool,
    output: u8,
}

impl Mapper132 {
    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
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
        Self {
            base,
            p: 0,
            r: 0,
            s: false,
            increment_mode: false,
            invert_mode: false,
            output: 0,
        }
    }

    fn status_nibble(&self) -> u8 {
        (((self.s ^ self.invert_mode) as u8) << 3) | (self.r & 0x07)
    }

    fn is_4100_masked_window(addr: u16) -> bool {
        addr & 0xE100 == 0x4100
    }

    fn is_status_read_address(addr: u16) -> bool {
        Self::is_4100_masked_window(addr) && (addr & 0x0003) == REG_SELECT_R
    }

    fn apply_output_to_banks(&mut self) {
        self.base
            .select_prg_page(0, ((self.output >> 2) & 0x01) as i16);
        self.base.select_chr_page(0, (self.output & 0x03) as i16);
    }
}

impl Mapper for Mapper132 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if Self::is_status_read_address(addr) {
            return self.status_nibble();
        }

        if (0x8000..=0xFFFF).contains(&addr) {
            return self.base.read_prg_rom(addr);
        }

        0
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if Self::is_status_read_address(addr) {
            return (open_bus & 0xF0) | self.status_nibble();
        }

        self.base
            .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a))
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if Self::is_4100_masked_window(addr) {
            match addr & 0x0003 {
                REG_SELECT_R => {
                    if self.increment_mode {
                        self.r = self.r.wrapping_add(1) & 0x07;
                    } else if self.invert_mode {
                        self.r = (!self.p) & 0x07;
                    } else {
                        self.r = self.p & 0x07;
                    }
                }
                REG_INVERT_MODE => {
                    self.invert_mode = value & 0x01 != 0;
                }
                REG_SET_P_AND_S => {
                    self.s = value & 0x08 != 0;
                    self.p = value & 0x07;
                }
                REG_INCREMENT_MODE => {
                    self.increment_mode = value & 0x01 != 0;
                }
                _ => {}
            }
            return;
        }

        if (addr & 0x8000) != 0 {
            self.output = self.r & 0x07;
            self.apply_output_to_banks();
        }
    }

    fn reset(&mut self) {
        self.p = 0;
        self.r = 0;
        self.s = false;
        self.increment_mode = false;
        self.invert_mode = false;
        self.output = 0;
        self.apply_output_to_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![
            self.p,
            self.r,
            self.s as u8,
            self.increment_mode as u8,
            self.invert_mode as u8,
            self.output,
        ]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < 6 {
            return;
        }

        self.p = data[0] & 0x07;
        self.r = data[1] & 0x07;
        self.s = data[2] & 0x01 != 0;
        self.increment_mode = data[3] & 0x01 != 0;
        self.invert_mode = data[4] & 0x01 != 0;
        self.output = data[5] & 0x07;
        self.apply_output_to_banks();
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::nes::cartridge::Cartridge;
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    const PRG_BANKS_32K: usize = 2;
    const CHR_BANKS_8K: usize = 4;

    fn create_mapper132(prg_banks: usize, chr_banks: usize) -> Box<dyn Mapper> {
        let prg_rom = banked_data(32 * 1024, prg_banks);
        let chr_rom = banked_data(8 * 1024, chr_banks);
        create_mapper(MapperContext::new_for_test(
            132,
            prg_rom,
            chr_rom,
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 132 should be creatable")
    }

    fn write_r_from_p(mapper: &mut dyn Mapper, p: u8, invert: bool, increment: bool, s: bool) {
        mapper.write_prg(0x4102, ((s as u8) << 3) | (p & 0x07));
        mapper.write_prg(0x4101, u8::from(invert));
        mapper.write_prg(0x4103, u8::from(increment));
        mapper.write_prg(0x4100, 0);
    }

    fn unique_temp_rom_path(filename: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after UNIX_EPOCH")
            .as_nanos();
        path.push(format!("neser-{nonce}-{filename}"));
        path
    }

    fn build_mapper132_fixture_rom() -> Vec<u8> {
        let mut rom = Vec::new();

        let mapper: u8 = 132;
        let prg_16k_banks: u8 = 4;
        let chr_8k_banks: u8 = 4;
        let flags6 = (mapper & 0x0F) << 4;
        let flags7 = mapper & 0xF0;

        rom.extend_from_slice(b"NES\x1A");
        rom.push(prg_16k_banks);
        rom.push(chr_8k_banks);
        rom.push(flags6);
        rom.push(flags7);
        rom.extend_from_slice(&[0u8; 8]);

        rom.extend(std::iter::repeat_n(0u8, 32 * 1024));
        rom.extend(std::iter::repeat_n(1u8, 32 * 1024));

        for bank in 0..chr_8k_banks {
            rom.extend(std::iter::repeat_n(bank, 8 * 1024));
        }

        rom
    }

    fn configure_mapper132_r_and_apply_output(cartridge: &mut Cartridge) {
        cartridge.mapper_mut().write_prg(0x4102, 0b0111);
        cartridge.mapper_mut().write_prg(0x4101, 0);
        cartridge.mapper_mut().write_prg(0x4103, 0);
        cartridge.mapper_mut().write_prg(0x4100, 0);
        cartridge.mapper_mut().write_prg(0x8000, 0);
    }

    #[test]
    fn mapper_132_is_registered() {
        let mapper = create_mapper132(PRG_BANKS_32K, CHR_BANKS_8K);
        assert_eq!(mapper.mapper_number(), 132);
    }

    #[test]
    fn read_4100_returns_status_nibble_sxorv_rrr() {
        let mut mapper = create_mapper132(PRG_BANKS_32K, CHR_BANKS_8K);

        write_r_from_p(mapper.as_mut(), 0b101, false, false, true);
        assert_eq!(mapper.read_prg(0x4100) & 0x0F, 0b1101);

        mapper.write_prg(0x4101, 1); // invert=1, S stays set
        assert_eq!(mapper.read_prg(0x4100) & 0x0F, 0b0101);
    }

    #[test]
    fn write_decode_uses_masked_4100_register_window() {
        let mut mapper = create_mapper132(PRG_BANKS_32K, CHR_BANKS_8K);

        write_r_from_p(mapper.as_mut(), 0b100, false, false, false);
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0x8000), 1, "R2 should select PRG bank 1");

        mapper.write_prg(0x4202, 0b001); // should be ignored by $E103 decode
        mapper.write_prg(0x4100, 0);
        mapper.write_prg(0x8000, 0);
        assert_eq!(
            mapper.read_prg(0x8000),
            1,
            "Ignored write must not alter latched P value"
        );

        mapper.write_prg(0x4302, 0b001); // matches masked decode
        mapper.write_prg(0x4100, 0);
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_prg(0x8000), 0, "Decoded write must update P");
    }

    #[test]
    fn write_8000_applies_output_to_prg_and_chr_banks() {
        let mut mapper = create_mapper132(PRG_BANKS_32K, CHR_BANKS_8K);

        write_r_from_p(mapper.as_mut(), 0b111, false, false, false);
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Output is not applied until $8000"
        );
        assert_eq!(
            mapper.read_chr(0x0000),
            0,
            "Output is not applied until $8000"
        );

        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.read_prg(0x8000), 1, "PRG bank uses R bit2");
        assert_eq!(mapper.read_chr(0x0000), 3, "CHR bank uses R bits1:0");
    }

    #[test]
    fn increment_mode_updates_rrr_on_4100_write() {
        let mut mapper = create_mapper132(PRG_BANKS_32K, CHR_BANKS_8K);

        write_r_from_p(mapper.as_mut(), 0b001, false, false, false);
        mapper.write_prg(0x4103, 1);
        mapper.write_prg(0x4100, 0x00);
        mapper.write_prg(0x8000, 0x00);

        assert_eq!(mapper.read_prg(0x8000), 0, "R=2 keeps PRG bank 0");
        assert_eq!(mapper.read_chr(0x0000), 2, "R increments to 0b010");
    }

    #[test]
    fn reset_and_restore_keep_state_deterministic() {
        let mut mapper = create_mapper132(PRG_BANKS_32K, CHR_BANKS_8K);

        write_r_from_p(mapper.as_mut(), 0b111, false, false, true);
        mapper.write_prg(0x8000, 0);
        let snapshot = mapper.registers_snapshot();

        mapper.reset();
        assert_eq!(mapper.read_prg(0x4100) & 0x0F, 0);
        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_chr(0x0000), 0);

        mapper.restore_registers(&snapshot);
        assert_eq!(mapper.read_prg(0x4100) & 0x0F, 0b1111);
        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_chr(0x0000), 3);
    }

    #[test]
    fn synthetic_fixture_routes_and_switches_banks() {
        let rom_path = unique_temp_rom_path("mapper132-fixture.nes");
        let rom_data = build_mapper132_fixture_rom();

        let mut cartridge =
            Cartridge::load_from_file(&rom_data, &rom_path, crate::app_context::AppContext::new())
                .expect("mapper 132 fixture should load");

        assert_eq!(cartridge.mapper().mapper_number(), 132);
        assert_eq!(cartridge.mapper().read_prg(0x8000), 0);
        assert_eq!(cartridge.mapper_mut().read_chr(0x0000), 0);

        configure_mapper132_r_and_apply_output(&mut cartridge);

        assert_eq!(cartridge.mapper().read_prg(0x4100) & 0x0F, 0b0111);
        assert_eq!(cartridge.mapper().read_prg(0x8000), 1);
        assert_eq!(cartridge.mapper_mut().read_chr(0x0000), 3);
    }
}
