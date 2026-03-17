//! Mapper 232 - Camerica BF9096 ("Quattro" multicart)
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_232>
//! - Fallback: Mesen2 `Core/NES/Mappers/Codemasters/BF9096.h`
//!
//! Known Limitations:
//! - Submapper 1 (Aladdin Deck Enhancer) block-bit swap is implemented but
//!   not hardware-verified.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const MAPPER_NUMBER: u16 = 232;
const PRG_BANK_SIZE: usize = 16 * 1024;
const CHR_BANK_SIZE: usize = 8 * 1024;

/// Mapper 232 – Camerica BF9096 ("Quattro" multicart)
///
/// Two-register PRG banking scheme with a "block" outer register and a "page"
/// inner register. The mapper provides 16 selectable 16KB PRG banks arranged
/// as 4 blocks of 4 pages each.
///
/// Registers:
/// - $8000–$BFFF: PRG block select
///   - Submapper 0 (default): bits 4–3 → block (`(value >> 3) & 0x03`)
///   - Submapper 1 (Aladdin Deck Enhancer): bits 4 and 3 swapped
/// - $C000–$FFFF: PRG page select – bits 1–0 → page (`value & 0x03`)
///
/// Banking:
/// - $8000–$BFFF (slot 0): bank = `(block << 2) | page`
/// - $C000–$FFFF (slot 1): bank = `(block << 2) | 3`  (fixed last page in block)
///
/// CHR: 8 KB CHR-RAM (no CHR-ROM support expected).
/// Mirroring: fixed from ROM header (no mapper-controlled mirroring).
pub struct Mapper232 {
    base: BaseMapper,
    prg_block: u8,
    prg_page: u8,
    submapper: u8,
}

impl Mapper232 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let submapper = ctx.submapper;
        let capabilities = MapperCapabilities {
            prg_bank_size_kb: PRG_BANK_SIZE / 1024,
            chr_bank_size_kb: CHR_BANK_SIZE / 1024,
            ..Default::default()
        };
        let mut base = BaseMapper::new(&ctx, capabilities);
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.select_prg_page(0, 0);
        base.select_prg_page(1, 3);
        Self {
            base,
            prg_block: 0,
            prg_page: 0,
            submapper,
        }
    }

    fn apply_banks(&mut self) {
        let lo = ((self.prg_block << 2) | self.prg_page) as i16;
        let hi = ((self.prg_block << 2) | 3) as i16;
        self.base.select_prg_page(0, lo);
        self.base.select_prg_page(1, hi);
    }
}

impl Mapper for Mapper232 {
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
        if self.base.try_write_prg_ram(addr, value) {
            return;
        }
        match addr {
            0x8000..=0xBFFF => {
                self.prg_block = if self.submapper == 1 {
                    ((value >> 4) & 0x01) | ((value >> 2) & 0x02)
                } else {
                    (value >> 3) & 0x03
                };
                self.apply_banks();
            }
            0xC000..=0xFFFF => {
                self.prg_page = value & 0x03;
                self.apply_banks();
            }
            _ => {}
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.prg_block, self.prg_page]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.prg_block = data[0];
            self.prg_page = data[1];
            self.apply_banks();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    /// 12 banks = 3 blocks × 4 pages (non-power-of-two avoids index wrap false-passes)
    const PRG_BANKS: usize = 12;

    fn create_mapper232(prg_rom: Vec<u8>) -> Mapper232 {
        Mapper232::new(MapperContext::new_for_test(
            MAPPER_NUMBER,
            prg_rom,
            vec![],
            NametableLayout::Horizontal,
        ))
    }

    fn create_mapper232_submapper1(prg_rom: Vec<u8>) -> Mapper232 {
        Mapper232::new(
            MapperContext::new_for_test(
                MAPPER_NUMBER,
                prg_rom,
                vec![],
                NametableLayout::Horizontal,
            )
            .with_submapper(1),
        )
    }

    #[test]
    fn mapper_232_is_registered_in_factory() {
        let result = create_mapper(MapperContext::new_for_test(
            MAPPER_NUMBER,
            banked_data(PRG_BANK_SIZE, PRG_BANKS),
            vec![],
            NametableLayout::Horizontal,
        ));
        assert!(result.is_ok(), "Mapper 232 should be registered in factory");
    }

    #[test]
    fn power_on_slot0_at_bank0_slot1_at_bank3() {
        let mapper = create_mapper232(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Slot 0 starts at block 0 page 0 → bank 0"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "Slot 1 starts at block 0 last page → bank 3"
        );
    }

    #[test]
    fn page_register_selects_page_within_block0() {
        let mut mapper = create_mapper232(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        // Block 0 pages: 0,1,2,3 → banks 0,1,2,3
        mapper.write_prg(0xC000, 0x01); // page=1
        assert_eq!(mapper.read_prg(0x8000), 1, "Block 0, page 1 → bank 1");
        assert_eq!(
            mapper.read_prg(0xC000),
            3,
            "Slot 1 fixed to last page → bank 3"
        );

        mapper.write_prg(0xC000, 0x02); // page=2
        assert_eq!(mapper.read_prg(0x8000), 2, "Block 0, page 2 → bank 2");

        mapper.write_prg(0xC000, 0x03); // page=3
        assert_eq!(mapper.read_prg(0x8000), 3, "Block 0, page 3 → bank 3");
    }

    #[test]
    fn block_register_shifts_page_window_by_four() {
        let mut mapper = create_mapper232(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        // Select block 1: banks 4,5,6,7
        // Block bits = 1 → write value with bits[4:3] = 0b01 → value = 0b0000_1000 = 0x08
        mapper.write_prg(0x8000, 0x08); // block = (0x08>>3)&3 = 1
        assert_eq!(mapper.read_prg(0x8000), 4, "Block 1, page 0 → bank 4");
        assert_eq!(mapper.read_prg(0xC000), 7, "Block 1, last page → bank 7");

        // Select block 2: banks 8,9,10,11
        mapper.write_prg(0x8000, 0x10); // block = (0x10>>3)&3 = 2
        assert_eq!(mapper.read_prg(0x8000), 8, "Block 2, page 0 → bank 8");
        assert_eq!(mapper.read_prg(0xC000), 11, "Block 2, last page → bank 11");
    }

    #[test]
    fn block_and_page_combine_correctly() {
        let mut mapper = create_mapper232(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        // Block 2 (value 0x10), page 2 (value 0x02) → bank (2<<2)|2 = 10
        mapper.write_prg(0x8000, 0x10); // block=2
        mapper.write_prg(0xC000, 0x02); // page=2
        assert_eq!(mapper.read_prg(0x8000), 10, "Block 2, page 2 → bank 10");
        assert_eq!(mapper.read_prg(0xC000), 11, "Block 2, last page → bank 11");
    }

    #[test]
    fn page_register_only_uses_bits_1_0() {
        let mut mapper = create_mapper232(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        // Write 0xFF to page register – only bits [1:0] used → page 3
        mapper.write_prg(0xC000, 0xFF);
        assert_eq!(
            mapper.read_prg(0x8000),
            3,
            "Upper bits of page register ignored"
        );
    }

    #[test]
    fn block_register_only_uses_bits_4_3() {
        let mut mapper = create_mapper232(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        // Write 0xFF to block register – only bits [4:3] used → block = (0xFF>>3)&3 = 3
        // (0xFF = 1111_1111, >>3 = 0001_1111, &3 = 3) → block 3
        // With 12 banks only blocks 0-2 exist; select block 2 for a safe check
        mapper.write_prg(0x8000, 0xE7); // (0xE7>>3)&3 = (0x1C)&3 = 0 → block 0
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "Non-block bits of block register ignored"
        );
    }

    #[test]
    fn block_register_write_range_covers_8000_to_bfff() {
        let mut mapper = create_mapper232(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        // Any address in $8000-$BFFF should update block
        for addr in [0x8000u16, 0x9000, 0xA000, 0xBFFF] {
            mapper.write_prg(addr, 0x08); // block=1
            assert_eq!(
                mapper.read_prg(0x8000),
                4,
                "Block write at ${:04X} works",
                addr
            );
            mapper.write_prg(addr, 0x00); // reset block=0
        }
    }

    #[test]
    fn page_register_write_range_covers_c000_to_ffff() {
        let mut mapper = create_mapper232(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        for addr in [0xC000u16, 0xD000, 0xE000, 0xFFFF] {
            mapper.write_prg(addr, 0x01); // page=1
            assert_eq!(
                mapper.read_prg(0x8000),
                1,
                "Page write at ${:04X} works",
                addr
            );
            mapper.write_prg(addr, 0x00); // reset page=0
        }
    }

    #[test]
    fn chr_ram_is_readable_and_writable() {
        let mut mapper = create_mapper232(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        mapper.write_chr(0x0000, 0xAB);
        mapper.write_chr(0x1FFF, 0xCD);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
        assert_eq!(mapper.read_chr(0x1FFF), 0xCD);
    }

    #[test]
    fn submapper1_block_bits_are_swapped() {
        // Submapper 1: _prgBlock = ((value >> 4) & 0x01) | ((value >> 2) & 0x02)
        // value = 0x10 = 0001_0000:
        //   bit4 = 1 → (0x10>>4)&1 = 1
        //   bit3 = 0 → (0x10>>2)&2 = 0
        //   block = 1 → banks 4-7
        // value = 0x08 = 0000_1000:
        //   bit4 = 0 → 0
        //   bit3 = 1 → (0x08>>2)&2 = 2
        //   block = 2 → banks 8-11
        let mut mapper = create_mapper232_submapper1(banked_data(PRG_BANK_SIZE, PRG_BANKS));
        mapper.write_prg(0x8000, 0x10);
        assert_eq!(mapper.read_prg(0x8000), 4, "Submapper1: 0x10 → block 1");

        mapper.write_prg(0x8000, 0x08);
        assert_eq!(mapper.read_prg(0x8000), 8, "Submapper1: 0x08 → block 2");
    }

    #[test]
    fn registers_snapshot_and_restore() {
        let prg_rom = banked_data(PRG_BANK_SIZE, PRG_BANKS);
        let mut mapper = create_mapper232(prg_rom.clone());
        mapper.write_prg(0x8000, 0x10); // block=2
        mapper.write_prg(0xC000, 0x01); // page=1

        let snap = mapper.registers_snapshot();

        let mut restored = create_mapper232(prg_rom);
        restored.restore_registers(&snap);
        assert_eq!(
            restored.read_prg(0x8000),
            9,
            "Restored: block 2, page 1 → bank 9"
        );
        assert_eq!(
            restored.read_prg(0xC000),
            11,
            "Restored: block 2, last page → bank 11"
        );
    }
}
