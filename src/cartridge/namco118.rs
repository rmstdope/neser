//! Mapper 206 - Namco 118 / Namco 108
//!
//! Known Limitations:
//! - No mapper-specific gameplay-blocking functional limitations are currently documented.
//! - Edge-case behavior may still differ from hardware in untested timing and board-variant scenarios.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 206 - Namco 118 / Namco 108 (DxROM boards)
///
/// Hardware: Namco's mapper derived from MMC3 without IRQ functionality
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_206>
/// - Namco 108: <https://www.nesdev.org/wiki/Namco_108>
/// - Namco 118: <https://www.nesdev.org/wiki/Namco_118>
/// - PRG-ROM: Up to 512KB (64 8KB banks)
/// - PRG-RAM: 8KB at $6000-$7FFF
/// - CHR: Up to 256KB (256 1KB banks) or CHR-RAM
/// - Mirroring: Fixed horizontal or vertical (solder pads, not programmable)
///
/// Common boards: Namco DxROM (derived from TxROM/MMC3)
///
/// Notes:
/// - Identical to MMC3 register format ($8000 bank select, $8001 bank data)
/// - Same PRG/CHR banking modes as MMC3
/// - **No IRQ counter** (unlike MMC3)
/// - **Mirroring not programmable** (writes to $A000 ignored, uses header value)
/// - Used in Gauntlet, Dragon Buster, Family Circuit '91
///
/// Implementation:
/// - Simplified MMC3 without IRQ functionality
/// - Bank switching compatible with MMC3 behavior
pub struct Namco118Mapper {
    base: BaseMapper,

    bank_select: u8,
    regs: [u8; 8],
}

impl Namco118Mapper {
    const PRG_MODE_MASK: u8 = 0b0100_0000;
    const CHR_MODE_MASK: u8 = 0b1000_0000;
    const REG_SELECT_MASK: u8 = 0b0000_0111;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
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
        // PRG banking
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

        // CHR banking
        let r0 = (self.regs[0] & 0xFE) as i16; // even-aligned
        let r1 = (self.regs[1] & 0xFE) as i16;
        let r2 = self.regs[2] as i16;
        let r3 = self.regs[3] as i16;
        let r4 = self.regs[4] as i16;
        let r5 = self.regs[5] as i16;

        if !self.chr_mode() {
            // Mode 0: R0/R1 2KB at low, R2-R5 1KB at high
            self.base.select_chr_page(0, r0);
            self.base.select_chr_page(1, r0 + 1);
            self.base.select_chr_page(2, r1);
            self.base.select_chr_page(3, r1 + 1);
            self.base.select_chr_page(4, r2);
            self.base.select_chr_page(5, r3);
            self.base.select_chr_page(6, r4);
            self.base.select_chr_page(7, r5);
        } else {
            // Mode 1: R2-R5 1KB at low, R0/R1 2KB at high
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
}

impl Mapper for Namco118Mapper {
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
        match addr {
            0x8000..=0x9FFF => {
                if (addr & 1) == 0 {
                    // Bank select
                    self.bank_select = value;
                } else {
                    // Bank data
                    let reg = self.selected_reg();
                    self.regs[reg] = value;
                }
                self.update_banks();
            }
            // Mirroring / PRG-RAM protect and IRQ registers are ignored
            0xA000..=0xFFFF => {}
            _ => {}
        }
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
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::namco118::Namco118Mapper;
    use crate::cartridge::test_helpers::banked_data;

    fn create_namco118_mapper(
        prg_rom: Vec<u8>,
        chr_rom: Vec<u8>,
        mirroring: NametableLayout,
    ) -> std::io::Result<Box<dyn Mapper>> {
        create_mapper(MapperContext::new_for_test(
            206, prg_rom, chr_rom, mirroring,
        ))
    }

    #[test]
    fn namco118_prg_chr_banking_matches_mmc3_subset() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_namco118_mapper(prg_rom, chr_rom, NametableLayout::Vertical)
            .expect("Mapper 206 should be implemented");

        // PRG mode 0 (bit 6 clear): R6 @ $8000, R7 @ $A000, fixed second-last @ $C000, last @ $E000.
        mapper.write_prg(0x8000, 0b0000_0110); // select R6
        mapper.write_prg(0x8001, 1);
        mapper.write_prg(0x8000, 0b0000_0111); // select R7
        mapper.write_prg(0x8001, 2);

        assert_eq!(mapper.read_prg(0x8000), 1);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 6);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // Switch to PRG mode 1 (bit 6 set): fixed second-last @ $8000, R7 @ $A000, R6 @ $C000, fixed last @ $E000.
        mapper.write_prg(0x8000, 0b0100_0110); // select R6 with PRG mode 1
        mapper.write_prg(0x8001, 4);

        assert_eq!(mapper.read_prg(0x8000), 6);
        assert_eq!(mapper.read_prg(0xA000), 2);
        assert_eq!(mapper.read_prg(0xC000), 4);
        assert_eq!(mapper.read_prg(0xE000), 7);

        // CHR mode 0 (bit 7 clear): R0/1 are 2KB even-aligned, R2-5 are 1KB.
        mapper.write_prg(0x8000, 0b0000_0000); // select R0, CHR mode 0
        mapper.write_prg(0x8001, 4); // R0 maps banks 4+5 at $0000-$07FF
        mapper.write_prg(0x8000, 0b0000_0001); // select R1
        mapper.write_prg(0x8001, 6); // R1 maps banks 6+7 at $0800-$0FFF

        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0011); // R3
        mapper.write_prg(0x8001, 9);
        mapper.write_prg(0x8000, 0b0000_0100); // R4
        mapper.write_prg(0x8001, 10);
        mapper.write_prg(0x8000, 0b0000_0101); // R5
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
    fn namco118_mirroring_and_irq_registers_are_noops() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = banked_data(1024, 8);

        let mut mapper = create_namco118_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 206 should be implemented");

        // Mirroring should stay hardwired to the cartridge header; writes to $A000 must not change it.
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
        mapper.write_prg(0xA000, 1);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);

        // IRQ-related registers should have no effect; mapper never asserts IRQ.
        mapper.write_prg(0xC000, 1);
        mapper.write_prg(0xC001, 0);
        mapper.write_prg(0xE000, 0);
        mapper.write_prg(0xE001, 0);

        for _ in 0..3 {
            mapper.ppu_address_changed(0x1000);
            mapper.ppu_scanline(0, true);
            mapper.cpu_cycle();
            assert!(!mapper.irq_pending());
        }
    }

    #[test]
    fn namco118_prg_ram_works_and_snapshots() {
        let prg_rom = banked_data(8 * 1024, 2);
        let chr_rom = Vec::new(); // CHR-RAM path

        let mut mapper = Namco118Mapper::new(MapperContext::new_for_test(
            206,
            prg_rom,
            chr_rom,
            NametableLayout::Vertical,
        ));

        mapper.write_prg(0x6000, 0xAA);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);

        let snap = mapper.wram_snapshot();
        assert_eq!(snap[0], 0xAA);

        mapper.write_prg(0x6000, 0x00);
        mapper.load_wram_snapshot(&snap);
        assert_eq!(mapper.read_prg(0x6000), 0xAA);
    }

    #[test]
    fn namco118_registers_snapshot_restores_bank_mapping() {
        let prg_rom = banked_data(8 * 1024, 8);
        let chr_rom = banked_data(1024, 16);

        let mut mapper = create_namco118_mapper(
            prg_rom.clone(),
            chr_rom.clone(),
            NametableLayout::Horizontal,
        )
        .expect("Mapper 206 should be implemented");

        // Set PRG bank registers R6/R7.
        mapper.write_prg(0x8000, 0b0000_0110);
        mapper.write_prg(0x8001, 3);
        mapper.write_prg(0x8000, 0b0000_0111);
        mapper.write_prg(0x8001, 4);

        // Set CHR registers R0/R1 (2KB) and R2 (1KB).
        mapper.write_prg(0x8000, 0b0000_0000);
        mapper.write_prg(0x8001, 6);
        mapper.write_prg(0x8000, 0b0000_0001);
        mapper.write_prg(0x8001, 8);
        mapper.write_prg(0x8000, 0b0000_0010);
        mapper.write_prg(0x8001, 10);

        let regs = mapper.registers_snapshot();

        let mut restored = create_namco118_mapper(prg_rom, chr_rom, NametableLayout::Horizontal)
            .expect("Mapper 206 should be implemented");
        restored.restore_registers(&regs);

        assert_eq!(restored.read_prg(0x8000), 3);
        assert_eq!(restored.read_prg(0xA000), 4);
        assert_eq!(restored.read_chr(0x0000), 6);
        assert_eq!(restored.read_chr(0x0400), 7);
        assert_eq!(restored.read_chr(0x0800), 8);
        assert_eq!(restored.read_chr(0x1000), 10);
    }
}
