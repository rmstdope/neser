//! Mapper 095 - Namcot-3425 (Namco 108/118 variant with CIRAM control)
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::{Mapper, MapperCapabilities, NametableLayout};

/// Mapper 095 - Namcot-3425
///
/// Hardware: Namco 108/118 derivative (MMC3 register format, no IRQ) with
/// CIRAM A10 controlled by CHR register bit 5.
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_095>
/// - Related: <https://www.nesdev.org/wiki/Namco_108>
/// - PRG-ROM: Up to 512KB (64 8KB banks)
/// - CHR: Up to 256KB (256 1KB banks) or CHR-RAM
/// - Mirroring: Derived from bit 5 of R0 and R1 (on bank-data writes)
///
/// Notes:
/// - Register interface mirrors all writes in `$8000-$FFFF` to `$8000/$8001`
/// - PRG/CHR mode bits are forced off on bank-select writes (`value &= 0x3F`)
/// - Mirroring is not controlled through `$A000`; writes there are ignored
/// - On odd write (`$8001` equivalent), mirroring is recomputed from:
///   - `r0 = (R0 >> 5) & 1`
///   - `r1 = (R1 >> 5) & 1`
///   - `(0,0) -> SingleScreenLower`, `(1,1) -> SingleScreenUpper`, mixed -> Horizontal
pub struct Mapper95 {
    base: BaseMapper,

    bank_select: u8,
    regs: [u8; 8],
}

impl Mapper95 {
    const PRG_MODE_MASK: u8 = 0b0100_0000;
    const CHR_MODE_MASK: u8 = 0b1000_0000;
    const REG_SELECT_MASK: u8 = 0b0000_0111;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(8 * 1024);
        base.configure_chr_banking(1024);

        let mut mapper = Self {
            base,
            bank_select: 0,
            regs: [0; 8],
        };
        mapper.update_banks();
        mapper.update_mirroring_from_r0_r1();
        mapper
    }

    fn prg_mode(&self) -> bool {
        (self.bank_select & Self::PRG_MODE_MASK) != 0
    }

    fn chr_mode(&self) -> bool {
        (self.bank_select & Self::CHR_MODE_MASK) != 0
    }

    fn selected_reg(&self) -> usize {
        (self.bank_select & Self::REG_SELECT_MASK) as usize
    }

    fn update_banks(&mut self) {
        let r6 = self.regs[6] as i16;
        let r7 = self.regs[7] as i16;
        if !self.prg_mode() {
            self.base.select_prg_page(0, r6);
            self.base.select_prg_page(1, r7);
            self.base.select_prg_page(2, -2);
            self.base.select_prg_page(3, -1);
        } else {
            self.base.select_prg_page(0, -2);
            self.base.select_prg_page(1, r7);
            self.base.select_prg_page(2, r6);
            self.base.select_prg_page(3, -1);
        }

        let r0 = (self.regs[0] & 0xFE) as i16;
        let r1 = (self.regs[1] & 0xFE) as i16;
        let r2 = self.regs[2] as i16;
        let r3 = self.regs[3] as i16;
        let r4 = self.regs[4] as i16;
        let r5 = self.regs[5] as i16;

        if !self.chr_mode() {
            self.base.select_chr_page(0, r0);
            self.base.select_chr_page(1, r0 + 1);
            self.base.select_chr_page(2, r1);
            self.base.select_chr_page(3, r1 + 1);
            self.base.select_chr_page(4, r2);
            self.base.select_chr_page(5, r3);
            self.base.select_chr_page(6, r4);
            self.base.select_chr_page(7, r5);
        } else {
            self.base.select_chr_page(0, r2);
            self.base.select_chr_page(1, r3);
            self.base.select_chr_page(2, r4);
            self.base.select_chr_page(3, r5);
            self.base.select_chr_page(4, r0);
            self.base.select_chr_page(5, r0 + 1);
            self.base.select_chr_page(6, r1);
            self.base.select_chr_page(7, r1 + 1);
        }
    }

    fn update_mirroring_from_r0_r1(&mut self) {
        let nt0 = (self.regs[0] >> 5) & 0x01;
        let nt1 = (self.regs[1] >> 5) & 0x01;
        let mirroring = match (nt0, nt1) {
            (0, 0) => NametableLayout::SingleScreenLower,
            (1, 1) => NametableLayout::SingleScreenUpper,
            _ => NametableLayout::Horizontal,
        };
        self.base.set_mirroring(mirroring);
    }
}

impl Mapper for Mapper95 {
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

        if addr < 0x8000 {
            return;
        }

        match addr & 0x8001 {
            0x8000 => {
                self.bank_select = value & 0x3F;
            }
            0x8001 => {
                let reg = self.selected_reg();
                self.regs[reg] = value;
                self.update_mirroring_from_r0_r1();
            }
            _ => {}
        }

        self.update_banks();
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snapshot = Vec::with_capacity(9);
        snapshot.push(self.bank_select);
        snapshot.extend_from_slice(&self.regs);
        snapshot
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 9 {
            self.bank_select = data[0];
            self.regs.copy_from_slice(&data[1..9]);
            self.update_banks();
            self.update_mirroring_from_r0_r1();
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;
    use crate::cartridge::{MapperCapabilities, NametableLayout};

    fn create_mapper95(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(95, prg_rom, chr_rom, mirroring))
    }

    #[test]
    fn mapper95_exposes_expected_capabilities() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mapper = create_mapper95(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 95 should be implemented");

        assert_eq!(
            mapper.capabilities(),
            MapperCapabilities {
                has_irq: false,
                has_chr_banking: true,
                has_dynamic_mirroring: true,
                has_expansion_audio: false,
                max_prg_ram_kb: 8,
                prg_bank_size_kb: 8,
                chr_bank_size_kb: 1,
                trainer_jsr: false,
                trainer_load_address: 0x7000,
            }
        );
    }

    #[test]
    fn mapper95_prg_chr_banking_matches_namco118_core_behavior() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mapper95(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 95 should be implemented");

        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0111);
        mapper.write_prg(0x8001, 2);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);

        mapper.write_prg(0x8000, 0b0000_0000);
        mapper.write_prg(0x8001, 4);
        mapper.write_prg(0x8000, 0b0000_0001);
        mapper.write_prg(0x8001, 6);

        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0011);
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b0000_0100);
        mapper.write_prg(0x8001, 10);
        mapper.write_prg(0x8000, 0b0000_0101);
        mapper.write_prg(0x8001, 11);

        assert_eq!(mapper.read_chr(0x0000), 4);
        assert_eq!(mapper.read_chr(0x0400), 5);
        assert_eq!(mapper.read_chr(0x0800), 6);
        assert_eq!(mapper.read_chr(0x0C00), 7);
        assert_eq!(mapper.read_chr(0x1000), 8);
        assert_eq!(mapper.read_chr(0x1400), 9);
        assert_eq!(mapper.read_chr(0x1800), 10);
        assert_eq!(mapper.read_chr(0x1C00), 11);
    }

    #[test]
    fn mapper95_mirroring_uses_r0_r1_bit5() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mapper95(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 95 should be implemented");

        mapper.write_prg(0x8000, 0b0000_0000);
        mapper.write_prg(0x8001, 0x00);
        mapper.write_prg(0x8000, 0b0000_0001);
        mapper.write_prg(0x8001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenLower);

        mapper.write_prg(0x8000, 0b0000_0000);
        mapper.write_prg(0x8001, 0x20);
        mapper.write_prg(0x8000, 0b0000_0001);
        mapper.write_prg(0x8001, 0x20);
        assert_eq!(mapper.get_mirroring(), NametableLayout::SingleScreenUpper);

        mapper.write_prg(0x8000, 0b0000_0000);
        mapper.write_prg(0x8001, 0x20);
        mapper.write_prg(0x8000, 0b0000_0001);
        mapper.write_prg(0x8001, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn mapper95_register_writes_are_mirrored_across_8000_ffff() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mapper95(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 95 should be implemented");

        // Bank-select from A000-range even address (mirrors to $8000)
        mapper.write_prg(0xA000, 0b0000_0110); // select R6
        // Bank-data from C000-range odd address (mirrors to $8001)
        mapper.write_prg(0xC001, 3); // R6 = 3

        assert_eq!(mapper.read_prg(0x8000), 3);
    }

    #[test]
    fn mapper95_forces_prg_chr_mode_bits_off() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_mapper95(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 95 should be implemented");

        // Attempt PRG mode 1 with R6 select. Mapper should force mode 0.
        mapper.write_prg(0x8000, 0b0100_0110);
        mapper.write_prg(0x8001, 4);

        assert_eq!(mapper.read_prg(0x8000), 4);
        assert_eq!(mapper.read_prg(0xC000), 6);
    }
}
