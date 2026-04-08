//! Mapper 80 - Taito X1-005
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_080>
//!
//! Register map used by this implementation:
//! - `$7EF0-$7EF5`: CHR bank registers (1KB pages; first two form 2KB pairs)
//! - `$7EF6-$7EF7`: Mirroring control, bit 0 (`0=Horizontal`, `1=Vertical`)
//! - `$7EF8-$7EF9`: RAM permission register (`$A3` enables `$7F00-$7FFF`)
//! - `$7EFA-$7EFB`: PRG bank at `$8000-$9FFF`
//! - `$7EFC-$7EFD`: PRG bank at `$A000-$BFFF`
//! - `$7EFE-$7EFF`: PRG bank at `$C000-$DFFF` (`$E000-$FFFF` fixed to last)
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};
use crate::trace_mapper;

const PRG_BANK_SIZE: usize = 8 * 1024;
const CHR_BANK_SIZE: usize = 1024;
const CHR_REG_START: u16 = 0x7EF0;
const CHR_REG_END: u16 = 0x7EF5;
const MIRROR_REG_START: u16 = 0x7EF6;
const MIRROR_REG_END: u16 = 0x7EF7;
const PRG0_REG_START: u16 = 0x7EFA;
const PRG0_REG_END: u16 = 0x7EFB;
const PRG1_REG_START: u16 = 0x7EFC;
const PRG1_REG_END: u16 = 0x7EFD;
const PRG2_REG_START: u16 = 0x7EFE;
const PRG2_REG_END: u16 = 0x7EFF;
const PRG_REG_COUNT: usize = 3;
const CHR_REG_COUNT: usize = 6;
const SNAPSHOT_SIZE: usize = PRG_REG_COUNT + CHR_REG_COUNT + 2;
const RAM_START: u16 = 0x7F00;
const RAM_END: u16 = 0x7FFF;
const RAM_SIZE: usize = 0x100;
const PRG_RAM_START: u16 = 0x6000;
const PRG_RAM_END: u16 = 0x7EEF;
const PRG_RAM_SIZE: usize = (PRG_RAM_END - PRG_RAM_START + 1) as usize;
const RAM_ENABLE_VALUE: u8 = 0xA3;

const DEFAULT_PRG_BANKS: [u8; PRG_REG_COUNT] = [0, 1, 2];
const DEFAULT_CHR_BANKS: [u8; CHR_REG_COUNT] = [0, 2, 4, 5, 6, 7];

/// Mapper 80 - Taito X1-005
pub struct TaitoX1005Mapper {
    base: BaseMapper,
    prg_banks: [u8; PRG_REG_COUNT],
    chr_banks: [u8; CHR_REG_COUNT],
    ram_permission: u8,
    ram: [u8; RAM_SIZE],
    prg_ram: [u8; PRG_RAM_SIZE],
    unhandled_write_trace_budget: u16,
}

impl TaitoX1005Mapper {
    pub fn new(ctx: crate::cartridge::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(CHR_BANK_SIZE);

        let mut mapper = Self {
            base,
            prg_banks: DEFAULT_PRG_BANKS,
            chr_banks: DEFAULT_CHR_BANKS,
            ram_permission: 0,
            ram: [0; RAM_SIZE],
            prg_ram: [0; PRG_RAM_SIZE],
            unhandled_write_trace_budget: 128,
        };
        mapper.apply_banks();
        trace_mapper!(1; "[80] initialized (PRG=8KB pages, CHR=1KB pages)");
        mapper
    }

    fn chr_register_index(addr: u16) -> usize {
        (addr - CHR_REG_START) as usize
    }

    fn decode_control_register(addr: u16) -> Option<u16> {
        if (0x7EF0..=0x7EFF).contains(&addr) {
            return Some(addr);
        }
        if (0x7E70..=0x7E7F).contains(&addr) {
            return Some(addr | 0x0080);
        }
        None
    }

    fn apply_banks(&mut self) {
        for (slot, &bank) in self.prg_banks.iter().enumerate() {
            self.base.select_prg_page(slot, bank as i16);
        }
        self.base.select_prg_page(3, -1);

        let chr0 = self.chr_banks[0] as i16;
        self.base.select_chr_page(0, chr0);
        self.base.select_chr_page(1, chr0 + 1);

        let chr1 = self.chr_banks[1] as i16;
        self.base.select_chr_page(2, chr1);
        self.base.select_chr_page(3, chr1 + 1);

        self.base.select_chr_page(4, self.chr_banks[2] as i16);
        self.base.select_chr_page(5, self.chr_banks[3] as i16);
        self.base.select_chr_page(6, self.chr_banks[4] as i16);
        self.base.select_chr_page(7, self.chr_banks[5] as i16);
    }

    fn apply_mirroring(&mut self, value: u8) {
        self.base.set_mirroring(if (value & 0x01) != 0 {
            NametableLayout::Vertical
        } else {
            NametableLayout::Horizontal
        });
    }

    fn ram_enabled(&self) -> bool {
        self.ram_permission == RAM_ENABLE_VALUE
    }

    fn ram_index(addr: u16) -> usize {
        (addr & 0x00FF) as usize
    }

    fn prg_ram_index(addr: u16) -> usize {
        (addr - PRG_RAM_START) as usize
    }
}

impl Mapper for TaitoX1005Mapper {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x7E70..=0x7E7F).contains(&addr) {
            self.prg_ram[Self::prg_ram_index(addr)] = value;
            trace_mapper!(2; "[80] PRG-RAM shadow write ${:04X}=${:02X}", addr, value);
        }

        if let Some(reg_addr) = Self::decode_control_register(addr) {
            match reg_addr {
                CHR_REG_START..=CHR_REG_END => {
                    self.chr_banks[Self::chr_register_index(reg_addr)] = value;
                    self.apply_banks();
                    trace_mapper!(1; "[80] CHR reg ${:04X}<=${:04X}=${:02X} -> chr={:?}", reg_addr, addr, value, self.chr_banks);
                }
                MIRROR_REG_START..=MIRROR_REG_END => {
                    self.apply_mirroring(value);
                    trace_mapper!(1; "[80] MIRROR reg ${:04X}<=${:04X}=${:02X} -> {:?}", reg_addr, addr, value, self.base.mirroring());
                }
                0x7EF8..=0x7EF9 => {
                    self.ram_permission = value;
                    trace_mapper!(1; "[80] RAM permission reg ${:04X}<=${:04X}=${:02X} (enabled={})", reg_addr, addr, value, self.ram_enabled());
                }
                PRG0_REG_START..=PRG0_REG_END => {
                    self.prg_banks[0] = value;
                    self.apply_banks();
                    trace_mapper!(1; "[80] PRG0 reg ${:04X}<=${:04X}=${:02X} -> prg={:?}", reg_addr, addr, value, self.prg_banks);
                }
                PRG1_REG_START..=PRG1_REG_END => {
                    self.prg_banks[1] = value;
                    self.apply_banks();
                    trace_mapper!(1; "[80] PRG1 reg ${:04X}<=${:04X}=${:02X} -> prg={:?}", reg_addr, addr, value, self.prg_banks);
                }
                PRG2_REG_START..=PRG2_REG_END => {
                    self.prg_banks[2] = value;
                    self.apply_banks();
                    trace_mapper!(1; "[80] PRG2 reg ${:04X}<=${:04X}=${:02X} -> prg={:?}", reg_addr, addr, value, self.prg_banks);
                }
                _ => {}
            }
            return;
        }

        match addr {
            RAM_START..=RAM_END => {
                if self.ram_enabled() {
                    let idx = Self::ram_index(addr);
                    self.ram[idx] = value;
                    self.ram[idx ^ 0x80] = value;
                    trace_mapper!(2; "[80] RAM write ${:04X}=${:02X} (mirrored)", addr, value);
                }
            }
            PRG_RAM_START..=PRG_RAM_END => {
                self.prg_ram[Self::prg_ram_index(addr)] = value;
                trace_mapper!(2; "[80] PRG-RAM write ${:04X}=${:02X}", addr, value);
            }
            _ => {
                if (0x4020..=0xFFFF).contains(&addr) && self.unhandled_write_trace_budget > 0 {
                    trace_mapper!(1; "[80] unhandled write ${:04X}=${:02X}", addr, value);
                    self.unhandled_write_trace_budget -= 1;
                }
            }
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = Vec::with_capacity(SNAPSHOT_SIZE);
        snapshot.extend_from_slice(&self.prg_banks);
        snapshot.extend_from_slice(&self.chr_banks);
        snapshot.push(matches!(self.base.mirroring(), NametableLayout::Vertical) as u8);
        snapshot.push(self.ram_permission);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() < SNAPSHOT_SIZE {
            return;
        }
        self.prg_banks.copy_from_slice(&data[0..PRG_REG_COUNT]);
        self.chr_banks
            .copy_from_slice(&data[PRG_REG_COUNT..(PRG_REG_COUNT + CHR_REG_COUNT)]);
        self.apply_mirroring(data[PRG_REG_COUNT + CHR_REG_COUNT]);
        self.ram_permission = data[PRG_REG_COUNT + CHR_REG_COUNT + 1];
        self.apply_banks();
    }

    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            RAM_START..=RAM_END => {
                if self.ram_enabled() {
                    self.ram[Self::ram_index(addr)]
                } else {
                    0
                }
            }
            PRG_RAM_START..=PRG_RAM_END => self.prg_ram[Self::prg_ram_index(addr)],
            0x8000..=0xFFFF => self.base.read_prg_rom(addr),
            _ => 0,
        }
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        match addr {
            RAM_START..=RAM_END => {
                if self.ram_enabled() {
                    self.ram[Self::ram_index(addr)]
                } else {
                    open_bus
                }
            }
            PRG_RAM_START..=PRG_RAM_END => self.prg_ram[Self::prg_ram_index(addr)],
            _ => self
                .base
                .read_prg_open_bus(addr, open_bus, |a| self.read_prg(a)),
        }
    }

    fn reset(&mut self) {
        self.prg_banks = DEFAULT_PRG_BANKS;
        self.chr_banks = DEFAULT_CHR_BANKS;
        self.ram_permission = 0;
        self.unhandled_write_trace_budget = 128;
        self.apply_banks();
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.ram.fill(0);
        self.base.initialize_ram(mode);
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.len() + self.ram.len()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(self.prg_ram.len() + self.ram.len());
        data.extend_from_slice(&self.prg_ram);
        data.extend_from_slice(&self.ram);
        data
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        let prg_len = data.len().min(self.prg_ram.len());
        self.prg_ram[..prg_len].copy_from_slice(&data[..prg_len]);

        if data.len() > self.prg_ram.len() {
            let ram_data = &data[self.prg_ram.len()..];
            let len = ram_data.len().min(self.ram.len());
            self.ram[..len].copy_from_slice(&ram_data[..len]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CHR_BANK_SIZE, PRG_BANK_SIZE, RAM_SIZE, RAM_START};
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_BANKS: usize = 13;
    const CHR_BANKS: usize = 11;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);
        create_mapper(MapperContext::new_for_test(
            80,
            prg,
            chr,
            NametableLayout::Horizontal,
        ))
        .expect("Mapper 80 must be creatable via factory")
    }

    #[test]
    fn mapper_80_is_registered_in_factory() {
        let prg = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let chr = banked_data(CHR_BANK_SIZE, CHR_BANKS);

        let mapper = create_mapper(MapperContext::new_for_test(
            80,
            prg,
            chr,
            NametableLayout::Horizontal,
        ));

        assert!(mapper.is_ok(), "Mapper 80 must be creatable via factory");
    }

    #[test]
    fn registers_7efa_to_7eff_select_prg_windows_with_last_bank_fixed() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7EFA, 2);
        mapper.write_prg(0x7EFC, 4);
        mapper.write_prg(0x7EFE, 6);

        assert_eq!(mapper.read_prg(0x8000), (2 % PRG_BANKS) as u8);
        assert_eq!(mapper.read_prg(0xA000), (4 % PRG_BANKS) as u8);
        assert_eq!(mapper.read_prg(0xC000), (6 % PRG_BANKS) as u8);
        assert_eq!(mapper.read_prg(0xE000), (PRG_BANKS - 1) as u8);
    }

    #[test]
    fn registers_7ef0_to_7ef5_select_chr_windows_with_two_2k_pairs() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7EF0, 3);
        mapper.write_prg(0x7EF1, 5);
        mapper.write_prg(0x7EF2, 7);
        mapper.write_prg(0x7EF3, 8);
        mapper.write_prg(0x7EF4, 9);
        mapper.write_prg(0x7EF5, 10);

        assert_eq!(mapper.read_chr(0x0000), (3 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x0400), (4 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x0800), (5 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x0C00), (6 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x1000), (7 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x1400), (8 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x1800), (9 % CHR_BANKS) as u8);
        assert_eq!(mapper.read_chr(0x1C00), (10 % CHR_BANKS) as u8);
    }

    #[test]
    fn registers_7ef6_to_7ef7_bit0_controls_mirroring_vh() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7EF6, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Bit 0 cleared should select horizontal mirroring"
        );

        mapper.write_prg(0x7EF6, 0x01);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Vertical,
            "Bit 0 set should select vertical mirroring"
        );

        mapper.write_prg(0x7EF7, 0x00);
        assert_eq!(
            mapper.get_mirroring(),
            NametableLayout::Horizontal,
            "Mirroring register write range should include $7EF7"
        );
    }

    #[test]
    fn ram_7f00_is_gated_by_a3_and_mirrored_every_0x80_bytes() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7F00, 0x5A);
        assert_eq!(
            mapper.read_prg(0x7F00),
            0,
            "RAM should be inaccessible before permission is set"
        );

        mapper.write_prg(0x7EF8, 0xA3);
        mapper.write_prg(0x7F00, 0x5A);

        assert_eq!(mapper.read_prg(0x7F00), 0x5A);
        assert_eq!(
            mapper.read_prg(0x7F80),
            0x5A,
            "upper half should mirror lower"
        );

        mapper.write_prg(0x7F80, 0x33);
        assert_eq!(
            mapper.read_prg(0x7F00),
            0x33,
            "lower half should mirror upper"
        );
        assert_eq!(mapper.read_prg(0x7F80), 0x33);

        mapper.write_prg(0x7EF9, 0x00);
        mapper.write_prg(0x7F00, 0x99);
        assert_eq!(
            mapper.read_prg(0x7F00),
            0,
            "RAM should be inaccessible again after clearing permission"
        );
    }

    #[test]
    fn writes_to_7000_range_use_standard_prg_ram() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7000, 0x4C);
        mapper.write_prg(0x7045, 0x8D);

        assert_eq!(mapper.read_prg(0x7000), 0x4C);
        assert_eq!(mapper.read_prg(0x7045), 0x8D);
    }

    #[test]
    fn writes_across_7000_and_7100_do_not_alias() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7004, 0x9D);
        mapper.write_prg(0x7104, 0x5E);

        assert_eq!(mapper.read_prg(0x7004), 0x9D);
        assert_eq!(mapper.read_prg(0x7104), 0x5E);
    }

    #[test]
    fn power_on_prg_layout_is_contiguous_with_last_bank_fixed() {
        let mapper = make_mapper();

        assert_eq!(mapper.read_prg(0x8000), 0);
        assert_eq!(mapper.read_prg(0xA000), 1);
        assert_eq!(mapper.read_prg(0xC000), 2);
        assert_eq!(mapper.read_prg(0xE000), (PRG_BANKS - 1) as u8);
    }

    #[test]
    fn power_on_chr_layout_is_contiguous_8kb() {
        let mut mapper = make_mapper();

        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x0400), 1);
        assert_eq!(mapper.read_chr(0x0800), 2);
        assert_eq!(mapper.read_chr(0x0C00), 3);
        assert_eq!(mapper.read_chr(0x1000), 4);
        assert_eq!(mapper.read_chr(0x1400), 5);
        assert_eq!(mapper.read_chr(0x1800), 6);
        assert_eq!(mapper.read_chr(0x1C00), 7);
    }

    #[test]
    fn registers_are_mirrored_at_7e7x_when_a7_is_ignored() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7E7A, 4);
        mapper.write_prg(0x7E7C, 6);
        mapper.write_prg(0x7E7E, 8);

        assert_eq!(mapper.read_prg(0x8000), (4 % PRG_BANKS) as u8);
        assert_eq!(mapper.read_prg(0xA000), (6 % PRG_BANKS) as u8);
        assert_eq!(mapper.read_prg(0xC000), (8 % PRG_BANKS) as u8);
    }

    #[test]
    fn aliased_7e7x_register_writes_are_retained_in_prg_ram() {
        let mut mapper = make_mapper();

        mapper.write_prg(0x7E7A, 0x4D);
        mapper.write_prg(0x7E7C, 0x6E);
        mapper.write_prg(0x7E7E, 0x8F);

        assert_eq!(mapper.read_prg(0x7E7A), 0x4D);
        assert_eq!(mapper.read_prg(0x7E7C), 0x6E);
        assert_eq!(mapper.read_prg(0x7E7E), 0x8F);
    }

    #[test]
    fn snapshot_restore_preserves_mirroring_without_inversion() {
        for (write_value, expected) in [
            (0x00, NametableLayout::Horizontal),
            (0x01, NametableLayout::Vertical),
        ] {
            let mut mapper = make_mapper();
            mapper.write_prg(0x7EF6, write_value);
            let snapshot = mapper.registers_snapshot();

            mapper.write_prg(0x7EF6, write_value ^ 0x01);
            mapper.restore_registers(&snapshot);

            assert_eq!(
                mapper.get_mirroring(),
                expected,
                "Snapshot/restore must keep mirroring unchanged"
            );
        }
    }

    #[test]
    fn initialize_ram_zeroes_mapper_owned_ram_buffers() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x7004, 0x9D);
        mapper.write_prg(0x7024, 0xEE);
        mapper.initialize_ram(crate::console::RamInitMode::Random);
        mapper.write_prg(0x7EF8, 0xA3);

        assert_eq!(mapper.read_prg(0x7004), 0x9D);
        assert_eq!(mapper.read_prg(0x7024), 0xEE);

        for offset in 0..RAM_SIZE {
            assert_eq!(mapper.read_prg(RAM_START + offset as u16), 0);
        }
    }
}
