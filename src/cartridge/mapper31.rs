//! Mapper 031 - NSF playing cart / 8-in-1 (Homebrew)
//!
//! Specifications:
//! - Fallback: Mesen2 `Core/NES/Mappers/Homebrew/NsfCart31.h`
//! - NesDev wiki: <https://www.nesdev.org/wiki/INES_Mapper_031>
//!
//! Register: write to `$5000-$5FFF`; bits 2-0 of address select one of eight 4 KB PRG windows.
//! - Window 0 (`$8000-$8FFF`): any address with `addr & 0x07 == 0` (e.g. `$5FF8`)
//! - Window 1 (`$9000-$9FFF`): `addr & 0x07 == 1` (e.g. `$5FF9`)
//! - …
//! - Window 7 (`$F000-$FFFF`): `addr & 0x07 == 7` (e.g. `$5FFF`)
//!
//! Power-on state: window 7 = bank `$FF` (wraps to available banks); windows 0–6 = bank 0.
//! No PRG-RAM. CHR: fixed 8 KB at bank 0 (CHR-ROM or CHR-RAM). Mirroring: fixed from header.

use crate::cartridge::base_mapper::BaseMapper;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

const CHR_RAM_SIZE: usize = 8 * 1024;
const PRG_BANK_SIZE: usize = 4 * 1024;
const NUM_WINDOWS: usize = 8;
const INIT_WINDOW7_BANK: u8 = 0xFF;

/// Mapper 031 – NSF playing cart / 8-in-1 homebrew board.
pub struct Mapper31 {
    base: BaseMapper,
    regs: [u8; NUM_WINDOWS],
}

impl Mapper31 {
    pub fn new(ctx: super::mapper::MapperContext) -> Self {
        let capabilities = MapperCapabilities {
            has_chr_banking: true,
            prg_bank_size_kb: 4,
            chr_bank_size_kb: 8,
            max_prg_ram_kb: 0,
            ..Default::default()
        };

        let mut base = BaseMapper::new(&ctx, capabilities);
        if ctx.chr_rom.is_empty() {
            base.set_chr_memory(ChrMemory::new_ram(CHR_RAM_SIZE));
        }
        base.configure_prg_banking(PRG_BANK_SIZE);
        base.configure_chr_banking(8 * 1024);

        // Power-on state: windows 0–6 = bank 0, window 7 = bank 0xFF
        let mut regs = [0u8; NUM_WINDOWS];
        regs[7] = INIT_WINDOW7_BANK;
        for i in 0..7usize {
            base.select_prg_page(i, 0);
        }
        base.select_prg_page(7, INIT_WINDOW7_BANK as i16);
        base.select_chr_page(0, 0);

        Self { base, regs }
    }

    fn apply_register(&mut self, window: usize, bank: u8) {
        self.regs[window] = bank;
        self.base.select_prg_page(window, bank as i16);
    }
}

impl Mapper for Mapper31 {
    fn base(&self) -> &BaseMapper {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseMapper {
        &mut self.base
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if (0x5000..=0x5FFF).contains(&addr) {
            let window = (addr & 0x07) as usize;
            self.apply_register(window, value);
        }
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        self.regs.to_vec()
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= NUM_WINDOWS {
            for (i, &bank) in data[0..NUM_WINDOWS].iter().enumerate() {
                self.apply_register(i, bank);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::cartridge::NametableLayout;
    use crate::cartridge::mapper::{Mapper, MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    // 48 banks (non-power-of-two) prevents false passes from modulo wrapping
    const NUM_BANKS: usize = 48;
    const BANK_SIZE: usize = 4 * 1024;

    fn create_mapper31() -> Box<dyn Mapper> {
        let prg_rom = banked_data(BANK_SIZE, NUM_BANKS);
        create_mapper(MapperContext::new_for_test(
            31,
            prg_rom,
            Vec::new(),
            NametableLayout::Vertical,
        ))
        .expect("mapper 31 should be implemented")
    }

    #[test]
    fn mapper_31_is_registered() {
        let prg_rom = banked_data(BANK_SIZE, NUM_BANKS);
        let result = create_mapper(MapperContext::new_for_test(
            31,
            prg_rom,
            Vec::new(),
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "mapper 31 must be available in factory");
    }

    #[test]
    fn window_0_defaults_to_bank_0() {
        let mapper = create_mapper31();
        // Window 0 ($8000-$8FFF) should be bank 0 (all 0x00 bytes)
        assert_eq!(mapper.read_prg(0x8000), 0x00);
    }

    #[test]
    fn window_7_defaults_to_bank_0xff_wrapped() {
        let mapper = create_mapper31();
        // Window 7 ($F000-$FFFF) is initialized to bank 0xFF = 255.
        // With 48 banks: 255 % 48 = 15, so reads return 15.
        assert_eq!(mapper.read_prg(0xF000), 15);
    }

    #[test]
    fn write_addr_with_bits_000_selects_window_0() {
        let mut mapper = create_mapper31();
        // $5FF8 & 0x07 = 0 → window 0 ($8000-$8FFF)
        mapper.write_prg(0x5FF8, 5);
        assert_eq!(mapper.read_prg(0x8000), 5);
        // Windows 1–7 must be unaffected
        assert_eq!(mapper.read_prg(0x9000), 0); // window 1 still bank 0
    }

    #[test]
    fn write_addr_with_bits_001_selects_window_1() {
        let mut mapper = create_mapper31();
        // $5FF9 & 0x07 = 1 → window 1 ($9000-$9FFF)
        mapper.write_prg(0x5FF9, 3);
        assert_eq!(mapper.read_prg(0x9000), 3);
        assert_eq!(mapper.read_prg(0x8000), 0); // window 0 unaffected
    }

    #[test]
    fn write_addr_with_bits_010_selects_window_2() {
        let mut mapper = create_mapper31();
        // $5FFA & 0x07 = 2 → window 2 ($A000-$AFFF)
        mapper.write_prg(0x5FFA, 7);
        assert_eq!(mapper.read_prg(0xA000), 7);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn write_addr_with_bits_011_selects_window_3() {
        let mut mapper = create_mapper31();
        // $5FFB & 0x07 = 3 → window 3 ($B000-$BFFF)
        mapper.write_prg(0x5FFB, 11);
        assert_eq!(mapper.read_prg(0xB000), 11);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn write_addr_with_bits_100_selects_window_4() {
        let mut mapper = create_mapper31();
        // $5FFC & 0x07 = 4 → window 4 ($C000-$CFFF)
        mapper.write_prg(0x5FFC, 13);
        assert_eq!(mapper.read_prg(0xC000), 13);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn write_addr_with_bits_101_selects_window_5() {
        let mut mapper = create_mapper31();
        // $5FFD & 0x07 = 5 → window 5 ($D000-$DFFF)
        mapper.write_prg(0x5FFD, 17);
        assert_eq!(mapper.read_prg(0xD000), 17);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn write_addr_with_bits_110_selects_window_6() {
        let mut mapper = create_mapper31();
        // $5FFE & 0x07 = 6 → window 6 ($E000-$EFFF)
        mapper.write_prg(0x5FFE, 23);
        assert_eq!(mapper.read_prg(0xE000), 23);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn write_addr_with_bits_111_selects_window_7() {
        let mut mapper = create_mapper31();
        // $5FFF & 0x07 = 7 → window 7 ($F000-$FFFF)
        mapper.write_prg(0x5FFF, 31);
        assert_eq!(mapper.read_prg(0xF000), 31);
        assert_eq!(mapper.read_prg(0x8000), 0);
    }

    #[test]
    fn bank_index_wraps_modulo_available_banks() {
        let mut mapper = create_mapper31();
        // With 48 banks, bank index 49 = 49 % 48 = 1 → same as bank 1
        mapper.write_prg(0x5FF8, 49);
        assert_eq!(mapper.read_prg(0x8000), 1);
    }

    #[test]
    fn write_outside_5000_5fff_is_ignored() {
        let mut mapper = create_mapper31();
        // Writes to $8000 or above must not affect banking
        mapper.write_prg(0x8000, 9); // PRG ROM range – should be ignored
        assert_eq!(mapper.read_prg(0x8000), 0); // still bank 0
    }

    #[test]
    fn mirroring_is_fixed_from_header() {
        let mapper = create_mapper31();
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn chr_ram_is_allocated_when_no_chr_rom() {
        let mapper = create_mapper31();
        // Mapper allocates 8 KB CHR-RAM when no CHR-ROM is present
        assert_eq!(mapper.chr_ram_snapshot().len(), 8 * 1024);
    }

    #[test]
    fn chr_ram_is_readable_and_writable() {
        let mut mapper = create_mapper31();
        mapper.write_chr(0x0200, 0xA5);
        assert_eq!(mapper.read_chr(0x0200), 0xA5);
    }

    #[test]
    fn registers_snapshot_restore_roundtrip() {
        let mut mapper = create_mapper31();
        // Set distinct banks for each window
        mapper.write_prg(0x5FF8, 2);
        mapper.write_prg(0x5FF9, 4);
        mapper.write_prg(0x5FFA, 6);
        mapper.write_prg(0x5FFB, 8);
        mapper.write_prg(0x5FFC, 10);
        mapper.write_prg(0x5FFD, 12);
        mapper.write_prg(0x5FFE, 14);
        mapper.write_prg(0x5FFF, 16);

        let snapshot = mapper.registers_snapshot();

        let mut restored = create_mapper31();
        restored.restore_registers(&snapshot);

        assert_eq!(restored.read_prg(0x8000), mapper.read_prg(0x8000));
        assert_eq!(restored.read_prg(0x9000), mapper.read_prg(0x9000));
        assert_eq!(restored.read_prg(0xA000), mapper.read_prg(0xA000));
        assert_eq!(restored.read_prg(0xB000), mapper.read_prg(0xB000));
        assert_eq!(restored.read_prg(0xC000), mapper.read_prg(0xC000));
        assert_eq!(restored.read_prg(0xD000), mapper.read_prg(0xD000));
        assert_eq!(restored.read_prg(0xE000), mapper.read_prg(0xE000));
        assert_eq!(restored.read_prg(0xF000), mapper.read_prg(0xF000));
    }
}
