//! Mapper 049 - MMC3 multicart (Super HIK 4-in-1 and others)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_049>
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mmc3::MMC3Mapper;
use crate::cartridge::{Mapper, MapperCapabilities};

/// Mapper 049 - MMC3-based multicart
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_049>
/// - PRG-ROM: 512 KiB (4 × 128 KiB blocks)
/// - CHR: 512 KiB (4 × 128 KiB blocks)
///
/// Outer register ($6000-$7FFF): [BB PP ... O]
/// - Writable only when MMC3's PRG-RAM is enabled and writable
/// - BB (bits 7:6): block selector (0-3) for CHR (always) and MMC3-mode PRG
/// - PP (bits 5:4): 32 KiB PRG page within block when O=0
/// - O (bit 0): 0 = fixed 32 KiB mode, 1 = MMC3 banking mode
///
/// O=0 (fixed 32 KiB mode):
/// - PRG: 32 KiB at $8000–$FFFF = block BB × 4 + page PP (each page = 4 × 8 KiB banks)
/// - CHR: (BB << 7) | (MMC3_1k_bank & 0x7F) always
///
/// O=1 (MMC3 mode):
/// - PRG: (BB << 4) | (MMC3_bank & 0x0F)
/// - CHR: (BB << 7) | (MMC3_1k_bank & 0x7F)
///
/// Known games: Super HIK 4-in-1, Super 8-in-1, various unlicensed multicarts
pub struct Mapper49 {
    pub(crate) mmc3: MMC3Mapper,
    block: u8, // bits 7:6 of outer reg
    page: u8,  // bits 5:4 of outer reg, used in fixed-32K mode
    mmc3_mode: bool,
}

impl Mapper49 {
    const MAPPER_NUMBER: u8 = 49;
    const PRG_BANK_SIZE: usize = 0x2000; // 8 KiB
    const PRG_BANK_MASK: usize = Self::PRG_BANK_SIZE - 1;
    const CHR_1K_BANK_SIZE: usize = 0x0400; // 1 KiB
    const CHR_BANK_MASK: usize = Self::CHR_1K_BANK_SIZE - 1;

    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let prg_rom = ctx.prg_rom;
        let chr_rom = ctx.chr_rom;
        let mirroring = ctx.mirroring;
        Self {
            mmc3: MMC3Mapper::new_with_irq_mode(prg_rom, chr_rom, mirroring, false),
            block: 0,
            page: 0,
            mmc3_mode: false,
        }
    }

    /// Base bank for the first 8KB slot in fixed-32K mode.
    /// 32KB page = 4 × 8KB banks. Block offset: BB × 16 banks.
    fn fixed_prg_base_bank(&self) -> usize {
        (self.block as usize) * 16 + (self.page as usize) * 4
    }

    fn apply_chr_block(&self, raw_1k_bank: usize) -> usize {
        (raw_1k_bank & 0x7F) | ((self.block as usize) << 7)
    }
}

impl Mapper for Mapper49 {
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
        if !(0x8000..=0xFFFF).contains(&addr) {
            return 0;
        }
        let offset = (addr as usize) & Self::PRG_BANK_MASK;
        let bank = if self.mmc3_mode {
            let raw = self.mmc3.mapped_prg_bank(addr);
            (raw & 0x0F) | ((self.block as usize) << 4)
        } else {
            // fixed 32 KiB: 4 sequential 8KB banks starting at base
            let slot = ((addr as usize) >> 13) & 0x03; // 0-3 within $8000-$FFFF
            self.fixed_prg_base_bank() + slot
        };
        self.mmc3.read_prg_at_bank(bank, offset)
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x6000..=0x7FFF).contains(&addr) {
            if self.mmc3.is_prg_ram_writable() {
                self.block = (value >> 6) & 0x03;
                self.page = (value >> 4) & 0x03;
                self.mmc3_mode = (value & 0x01) != 0;
            }
        } else {
            self.mmc3.write_prg(addr, value);
        }
    }

    fn read_chr(&mut self, addr: u16) -> u8 {
        let raw = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_block(raw);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.read_chr_1k_at(bank, offset)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let raw = self.mmc3.mapped_chr_1k_bank(addr);
        let bank = self.apply_chr_block(raw);
        let offset = (addr as usize) & Self::CHR_BANK_MASK;
        self.mmc3.write_chr_1k_at(bank, offset, value);
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        let mut snap = self.mmc3.registers_snapshot();
        snap.push(self.block | (self.page << 2) | (self.mmc3_mode as u8) << 4);
        snap
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if let Some((&packed, mmc3_data)) = data.split_last() {
            self.block = packed & 0x03;
            self.page = (packed >> 2) & 0x03;
            self.mmc3_mode = (packed >> 4) & 0x01 != 0;
            self.mmc3.restore_registers(mmc3_data);
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: true,
            has_chr_banking: true,
            has_dynamic_mirroring: true,
            has_expansion_audio: false,
            max_prg_ram_kb: 0,
            prg_bank_size_kb: 8,
            chr_bank_size_kb: 1,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 4 blocks × 16 PRG banks = 64 banks × 8 KiB = 512 KiB PRG
    const PRG_BANKS: usize = 64;

    fn make_mapper() -> Box<dyn Mapper> {
        let prg = banked_data(8 * 1024, PRG_BANKS);
        // CHR 512 banks overflows u8 — use 384 (non-power-of-two), which still covers blocks 0–2
        let chr = banked_data(1024, 256);
        create_mapper(MapperContext::new_for_test(
            49,
            prg,
            chr,
            NametableLayout::Vertical,
        ))
        .expect("Mapper 49 should be implemented")
    }

    // --- Factory ---

    #[test]
    fn mapper_49_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            49,
            banked_data(8 * 1024, PRG_BANKS),
            banked_data(1024, 256),
            NametableLayout::Vertical,
        ));
        assert!(
            result.is_ok(),
            "Mapper 49 must be registered in the factory"
        );
    }

    // --- Fixed 32K mode (O=0) ---

    #[test]
    fn fixed_mode_block0_page0_maps_banks_0_3() {
        let mapper = make_mapper();
        // Default: block=0, page=0, mmc3_mode=false
        // $8000→bank0, $A000→bank1, $C000→bank2, $E000→bank3
        assert_eq!(mapper.read_prg(0x8000), 0, "$8000 should read bank 0");
        assert_eq!(mapper.read_prg(0xA000), 1, "$A000 should read bank 1");
        assert_eq!(mapper.read_prg(0xC000), 2, "$C000 should read bank 2");
        assert_eq!(mapper.read_prg(0xE000), 3, "$E000 should read bank 3");
    }

    #[test]
    fn fixed_mode_block0_page1_maps_banks_4_7() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80); // enable PRG-RAM write
        // block=0, page=1, O=0 → outer byte = (0<<6)|(1<<4)|0 = 0x10
        mapper.write_prg(0x6000, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 4, "$8000 block0 page1 = bank 4");
        assert_eq!(mapper.read_prg(0xE000), 7, "$E000 block0 page1 = bank 7");
    }

    #[test]
    fn fixed_mode_block1_page0_maps_banks_16_19() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80);
        // block=1, page=0, O=0 → (1<<6)|(0<<4)|0 = 0x40
        mapper.write_prg(0x6000, 0x40);
        assert_eq!(mapper.read_prg(0x8000), 16, "$8000 block1 page0 = bank 16");
        assert_eq!(mapper.read_prg(0xE000), 19, "$E000 block1 page0 = bank 19");
    }

    #[test]
    fn fixed_mode_block2_page3_maps_banks_44_47() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80);
        // block=2, page=3, O=0 → (2<<6)|(3<<4)|0 = 0x80|0x30 = 0xB0
        mapper.write_prg(0x6000, 0xB0);
        assert_eq!(mapper.read_prg(0x8000), 44, "$8000 block2 page3 = bank 44");
        assert_eq!(mapper.read_prg(0xE000), 47, "$E000 block2 page3 = bank 47");
    }

    // --- MMC3 mode (O=1) ---

    #[test]
    fn mmc3_mode_block0_fixed_last_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80);
        // block=0, O=1 → 0x01
        mapper.write_prg(0x6000, 0x01);
        // fixed last bank in 64-bank ROM: bank 63 → (63 & 0x0F) | (0<<4) = 15
        // But wait: MMC3 sees full 64-bank ROM, fixed_last = 63. (63 & 0x0F) = 15.
        // Block 0 → bank 15
        assert_eq!(
            mapper.read_prg(0xE000),
            15,
            "MMC3 mode block0 fixed-last must be bank 15"
        );
    }

    #[test]
    fn mmc3_mode_block1_r6_bank_3() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x41); // block=1, O=1
        mapper.write_prg(0x8000, 0b0000_0110); // R6
        mapper.write_prg(0x8001, 3); // R6=3 → final = (3 & 0x0F) | (1<<4) = 19
        assert_eq!(
            mapper.read_prg(0x8000),
            19,
            "MMC3 mode block1 R6=3 must map to bank 19"
        );
    }

    #[test]
    fn mmc3_mode_block2_fixed_last_bank() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x81); // block=2, O=1
        // fixed_last=63, (63&0x0F)=15, block=2 → 15 | 32 = 47
        assert_eq!(
            mapper.read_prg(0xE000),
            47,
            "MMC3 mode block2 fixed-last must be bank 47"
        );
    }

    // --- Block register gating ---

    #[test]
    fn outer_reg_not_writable_when_prg_ram_disabled() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x00); // PRG-RAM disabled
        mapper.write_prg(0x6000, 0x41); // should be ignored
        assert_eq!(
            mapper.read_prg(0xE000),
            3,
            "Block register must not change when PRG-RAM disabled"
        );
    }

    // --- CHR block selection ---

    #[test]
    fn chr_block0_default() {
        let mut mapper = make_mapper();
        // Enable MMC3 mode, block=0
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x01); // block=0, O=1
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5); // R2=5 → CHR bank = (5 & 0x7F) | (0<<7) = 5
        assert_eq!(mapper.read_chr(0x1000), 5, "CHR block0 R2=5 = bank 5");
    }

    #[test]
    fn chr_block1_shifts_bank_by_128() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x41); // block=1, O=1
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5); // R2=5 → CHR bank = (5 & 0x7F) | (1<<7) = 133
        assert_eq!(
            mapper.read_chr(0x1000),
            133,
            "CHR block1 R2=5 must be bank 133"
        );
    }

    #[test]
    fn chr_block_applies_in_fixed_prg_mode_too() {
        let mut mapper = make_mapper();
        mapper.write_prg(0xA001, 0x80);
        mapper.write_prg(0x6000, 0x40); // block=1, O=0
        mapper.write_prg(0x8000, 0b0000_0010); // R2
        mapper.write_prg(0x8001, 5); // CHR R2=5 → (5 & 0x7F) | 0x80 = 133
        assert_eq!(
            mapper.read_chr(0x1000),
            133,
            "CHR block applies even in fixed PRG mode"
        );
    }
}
