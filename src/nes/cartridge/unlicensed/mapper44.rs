//! Mapper 044 - MMC3 multicart (Super HIK 7-in-1 and others)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_044>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::nes::cartridge::base_mapper::BaseMapper;
use crate::nes::cartridge::mmc3::MMC3Mapper;
use crate::nes::cartridge::{Mapper, MapperCapabilities};

/// Mapper 044 - MMC3-based 7-in-1 multicart
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_044>
/// - PRG-ROM: 512 KiB (6 × 128 KiB + 1 × 256 KiB = 7 logical "blocks")
/// - CHR: 2048 KiB (6 × 256 KiB + 1 × 512 KiB = 7 logical "blocks")
///
/// Block register: stored in $A001 bits[2:0] (written alongside MMC3's $A001)
/// - Blocks 0–5: each is a 128 KiB PRG / 256 KiB CHR block
/// - Block 6 and 7 both map to the combined last block (256 KiB PRG / 512 KiB CHR)
///
/// AND/OR masking for PRG (8KB banks) and CHR (1KB banks):
/// | Block | PRG_AND | PRG_OR          | CHR_AND | CHR_OR             |
/// |-------|---------|-----------------|---------|-------------------|
/// |  0    | 0x0F    | 0x00            | 0x7F    | 0x000             |
/// |  1    | 0x0F    | 0x10            | 0x7F    | 0x080             |
/// |  2    | 0x0F    | 0x20            | 0x7F    | 0x100             |
/// |  3    | 0x0F    | 0x30            | 0x7F    | 0x180             |
/// |  4    | 0x0F    | 0x40            | 0x7F    | 0x200             |
/// |  5    | 0x0F    | 0x50            | 0x7F    | 0x280             |
/// | 6/7   | 0x1F    | 0x60            | 0xFF    | 0x300             |
///
/// Known games: Super HIK 7-in-1 (A001), Super Marvelous 7-in-1
pub struct Mapper44 {
    pub(crate) mmc3: MMC3Mapper,
    block: u8, // 0-6 (7 maps to 6)
}

impl Mapper44 {
    const MAPPER_NUMBER: u8 = 44;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: crate::nes::cartridge::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            block: 0,
        }
    }

    fn effective_block(&self) -> u8 {
        if self.block >= 6 { 6 } else { self.block }
    }

    fn prg_and_or(&self) -> (usize, usize) {
        let b = self.effective_block() as usize;
        if b < 6 {
            (0x0F, b * 0x10)
        } else {
            (0x1F, 0x60)
        }
    }

    fn chr_and_or(&self) -> (usize, usize) {
        let b = self.effective_block() as usize;
        if b < 6 {
            (0x7F, b * 0x80)
        } else {
            (0xFF, 0x300)
        }
    }
}

impl Mapper for Mapper44 {
    fn base(&self) -> &BaseMapper {
        &self.mmc3.base
    }
    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.mmc3.base
    }

    fn mmc3_delegate(&self) -> Option<&MMC3Mapper> {
        Some(&self.mmc3)
    }

    fn mmc3_delegate_mut(&mut self) -> Option<&mut MMC3Mapper> {
        Some(&mut self.mmc3)
    }

    fn read_prg(&self, addr: u16) -> u8 {
        if (0x6000..=0x7FFF).contains(&addr) {
            return self.mmc3.read_prg(addr);
        }
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let raw = self.mmc3.mapped_prg_bank(addr);
        let (and, or) = self.prg_and_or();
        let bank = (raw & and) | or;
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn read_prg_open_bus(&self, addr: u16, open_bus: u8) -> u8 {
        if (0x6000..=0x7FFF).contains(&addr) {
            return self.mmc3.read_prg_open_bus(addr, open_bus);
        }
        if !(0x8000..=0xFFFF).contains(&addr) {
            return open_bus;
        }
        self.read_prg(addr)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (addr & 0xE001) == 0xA001 {
            // Extract block bits [2:0] before passing to MMC3
            self.block = value & 0x07;
            if self.block == 7 {
                self.block = 6;
            }
        }
        // Always delegate to MMC3 (it handles $A001 itself)
        self.mmc3.write_prg(addr, value);
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let raw = self.mmc3.mapped_chr_1k_bank(addr);
        let (and, or) = self.chr_and_or();
        let bank = (raw & and) | or;
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let raw = self.mmc3.mapped_chr_1k_bank(addr);
        let (and, or) = self.chr_and_or();
        let bank = (raw & and) | or;
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn mapper_number(&self) -> u16 {
        u16::from(Self::MAPPER_NUMBER)
    }

    fn wram_size(&self) -> usize {
        8 * 1024
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.block);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&block, mmc3_data)) = data.split_last() {
            self.block = block & 0x07;
            self.mmc3.restore_registers(mmc3_data);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::nes::cartridge::NametableLayout;
    use crate::nes::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::nes::cartridge::test_helpers::banked_data;

    // 32 PRG banks = 256 KiB (covers blocks 0-1 for testing)
    const PRG_BANKS: usize = 32;
    // 256 CHR 1K banks = 256 KiB (covers blocks 0-1)
    const CHR_1K_BANKS: usize = 256;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        let chr = banked_data(1024, CHR_1K_BANKS);
        create_mapper(MapperContext::new_for_test(
            44,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 44 should be implemented")
    }

    // --- Factory ---

    #[test]
    fn mapper_44_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            44,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, CHR_1K_BANKS),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 44 must be registered in the factory"
        );
    }

    // --- PRG block selection via $A001 ---

    #[test]
    fn prg_defaults_to_block_0_fixed_last() {
        let mapper = make_mapper();
        // Fixed last bank in 32-bank ROM: bank 31. Block0 AND=0x0F: 31&0x0F=15
        assert_eq!(
            mapper.read_prg(0xE000),
            15,
            "Block 0 fixed-last PRG must be bank 15"
        );
    }

    #[test]
    fn prg_block1_shifts_fixed_last_by_0x10() {
        let mut mapper = make_mapper();
        // Write $A001 with block=1 in bits[2:0]
        mapper.write_prg(0xA001, 0x81); // PRG-RAM enable + block 1
        // Fixed-last = 31. AND=0x0F, OR=0x10 → (31&0x0F)|0x10 = 31
        assert_eq!(
            mapper.read_prg(0xE000),
            31,
            "Block 1 fixed-last PRG must be bank 31"
        );
    }

    #[test]
    fn prg_block1_r6_bank_3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x81); // enable RAM + block 1
        mapper.write_prg(0x8000, 0b0000_0110); // R6
        mapper.write_prg(0x8001, 3); // R6=3 → (3 & 0x0F) | 0x10 = 19
        assert_eq!(
            mapper.read_prg(0x8000),
            19,
            "Block 1 R6=3 must map to bank 19"
        );
    }

    #[test]
    fn prg_block7_treated_as_block6() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x87); // block = 7 (should act as 6)
        // Block 6: AND=0x1F, OR=0x60. fixed-last=31 → (31&0x1F)|0x60 = 31|0x60 = 0x7F = 127?
        // Wait: 31 = 0x1F. (0x1F & 0x1F) | 0x60 = 0x1F | 0x60 = 0x7F = 127
        // But PRG_BANKS = 32, so 127 is out of range. Use modulo: 127 % 32 = 31.
        // Actually the read_prg_at_bank does modulo internally.
        // Let's just verify block 6 and 7 produce the same result.
        let val_with_7 = mapper.read_prg(0xE000);
        let mut mapper2 = make_mapper();
        mapper2.write_prg(0xA001, 0x86); // block = 6
        let val_with_6 = mapper2.read_prg(0xE000);
        assert_eq!(
            val_with_7, val_with_6,
            "Block 7 and block 6 must produce the same PRG mapping"
        );
    }

    // --- CHR block selection ---

    #[test]
    fn chr_block0_r2_bank_5() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5); // R2=5 → block0 CHR_AND=0x7F, OR=0 → bank 5
        assert_eq!(mapper.read_chr(0x1000), 5, "CHR block0 R2=5 = bank 5");
    }

    #[test]
    fn chr_block1_shifts_bank_by_0x80() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x81); // block 1
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5); // R2=5 → (5&0x7F)|0x80 = 133
        assert_eq!(
            mapper.read_chr(0x1000),
            133,
            "CHR block1 R2=5 = bank 133 (upper marker = 0)"
        );
    }
}
