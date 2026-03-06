//! Mapper 350 - BMC-891227 multicart
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/NES_2.0_Mapper_350>
//! - Reference: Mesen mapper factory entry `891227`
//!
//! Known Limitations:
//! - CHR-RAM write protection is implemented for NROM banking modes (0/1) and
//!   writable in UNROM modes (2/3). No additional undocumented protection
//!   behavior is currently emulated.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 350 - BMC-891227 multicart.
///
/// Hardware summary:
/// - PRG-ROM: up to 1 MiB
/// - CHR: 8 KiB CHR-RAM (unbanked)
/// - Mirroring: programmable H/V
/// - IRQ: none
/// - Audio: none
///
/// Registers:
/// - `$8000-$BFFF` (outer): `D~7654 3210`
///   - bits 0..=2: outer 128 KiB PRG block
///   - bit 3: chip select (adds high PRG bit in UNROM modes only)
///   - bits 4..=5: PRG mode
///     - 0: NROM-128 (16 KiB mirrored at $8000/$C000)
///     - 1: NROM-256 (32 KiB, inner bit0 replaced by CPU A14)
///     - 2/3: UNROM (16 KiB switchable at $8000, fixed bank 7 at $C000)
///   - bit 6: mirroring (0=V, 1=H)
/// - `$C000-$FFFF` (inner): bits 0..=2 select 16 KiB inner bank.
///
/// Additional mapping:
/// - `$6000-$7FFF`: fixed 8 KiB PRG-ROM bank #1.
pub struct Mapper350 {
    base: BaseMapper,
    outer_reg: u8,
    inner_reg: u8,
}

impl Mapper350 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let chr_seed = ctx.chr_rom.clone();
        let capabilities = MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(16 * 1024);
        base.configure_prg_6000_banking();
        base.configure_chr_banking(8 * 1024);

        // Mapper 350 uses 8KB CHR-RAM (not CHR-ROM). Keep any dumped CHR payload
        // as initial RAM contents for compatibility with bad dumps.
        let mut chr_ram = ChrMemory::new_ram(8 * 1024);
        if !chr_seed.is_empty() {
            chr_ram.load_snapshot(&chr_seed);
        }
        base.set_chr_memory(chr_ram);

        let mut mapper = Self {
            base,
            outer_reg: 0,
            inner_reg: 0,
        };
        mapper.update_banks();
        mapper
    }

    fn mode(&self) -> u8 {
        (self.outer_reg >> 4) & 0x03
    }

    fn chr_write_protected(&self) -> bool {
        self.mode() <= 1
    }

    fn outer_block_128k(&self) -> i16 {
        let mut block = (self.outer_reg & 0x07) as i16;
        // Chip select is only used in UNROM modes.
        if self.mode() >= 2 && (self.outer_reg & 0x08) != 0 {
            block |= 0x08;
        }
        block
    }

    fn update_banks(&mut self) {
        self.base.select_prg_6000_page(1);

        let base_16k = self.outer_block_128k() * 8;
        let inner_16k = (self.inner_reg & 0x07) as i16;

        match self.mode() {
            // NROM-128
            0 => {
                let bank = base_16k + inner_16k;
                self.base.select_prg_page(0, bank);
                self.base.select_prg_page(1, bank);
            }
            // NROM-256: inner bit0 replaced by CPU A14
            1 => {
                let even = base_16k + (inner_16k & !1);
                self.base.select_prg_page(0, even);
                self.base.select_prg_page(1, even | 1);
            }
            // UNROM variants (2/3)
            _ => {
                self.base.select_prg_page(0, base_16k + inner_16k);
                self.base.select_prg_page(1, base_16k + 7);
            }
        }

        self.base.set_mirroring_hv((self.outer_reg & 0x40) != 0);
        self.base.select_chr_page(0, 0);
    }
}

impl Mapper for Mapper350 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x8000..=0xBFFF => {
                self.outer_reg = value;
                self.update_banks();
            }
            0xC000..=0xFFFF => {
                self.inner_reg = value;
                self.update_banks();
            }
            _ => {}
        }
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        if !self.chr_write_protected() {
            self.base_mut().write_chr(addr, value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.outer_reg, self.inner_reg]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.outer_reg = data[0];
            self.inner_reg = data[1];
            self.update_banks();
        }
    }

    fn reset(&mut self) {
        self.outer_reg = 0;
        self.inner_reg = 0;
        self.update_banks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    const PRG_16K_BANKS: usize = 48;

    fn make_mapper() -> Mapper350 {
        Mapper350::new(MapperContext::new_for_test(
            350,
            banked_data(16 * 1024, PRG_16K_BANKS),
            vec![],
            NametableLayout::Vertical,
        ))
    }

    fn make_mapper_with_8k_markers() -> Mapper350 {
        Mapper350::new(MapperContext::new_for_test(
            350,
            banked_data(8 * 1024, PRG_16K_BANKS * 2),
            vec![],
            NametableLayout::Vertical,
        ))
    }

    #[test]
    fn mapper_350_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            350,
            banked_data(16 * 1024, PRG_16K_BANKS),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 350 must be registered");
    }

    #[test]
    fn fixed_6000_window_reads_prg_bank_1() {
        let mapper = make_mapper_with_8k_markers();
        assert_eq!(mapper.read_prg(0x6000), 1);
        assert_eq!(mapper.read_prg(0x7FFF), 1);
    }

    #[test]
    fn mode0_nrom128_uses_same_inner_bank_in_both_windows() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x02); // mode 0, outer block 2
        mapper.write_prg(0xC000, 0x05); // inner bank 5
        assert_eq!(mapper.read_prg(0x8000), 21);
        assert_eq!(mapper.read_prg(0xC000), 21);
    }

    #[test]
    fn mode1_nrom256_replaces_inner_lsb_with_cpu_a14() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x13); // mode 1, outer block 3
        mapper.write_prg(0xC000, 0x05); // inner=5 => even pair 4/5
        assert_eq!(mapper.read_prg(0x8000), 28);
        assert_eq!(mapper.read_prg(0xC000), 29);
    }

    #[test]
    fn mode2_unrom_uses_chip_select_and_fixes_upper_to_bank7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x29); // mode 2 + chip select + outer block 1
        mapper.write_prg(0xC000, 0x02);
        assert_eq!(mapper.read_prg(0x8000), 26);
        assert_eq!(mapper.read_prg(0xC000), 31);
    }

    #[test]
    fn mirroring_comes_from_outer_register_bit6() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x8000, 0x40);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn chr_writes_are_blocked_in_nrom_modes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x00); // mode 0
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(mapper.read_chr(0x0000), 0x00);
    }

    #[test]
    fn chr_writes_work_in_unrom_modes() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x20); // mode 2
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
    }

    #[test]
    fn snapshot_restore_roundtrip() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0x6A); // mode 2, H mirroring, chip+outer
        mapper.write_prg(0xC000, 0x06);
        let snapshot = mapper.registers_snapshot();

        let mut restored = make_mapper();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(restored.get_mirroring(), mapper.get_mirroring());
    }
}
