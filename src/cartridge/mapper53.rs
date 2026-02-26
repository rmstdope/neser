//! Mapper 053 - Supervision 16-in-1
//!
//! Specifications:
//! - Main: <https://www.nesdev.org/wiki/INES_Mapper_053>
//! - Reference: FCEUX supervision.cpp
//!
//! Known Limitations:
//! - No known gameplay-blocking functional limitations are currently documented.

use crate::cartridge::NametableLayout;
use crate::cartridge::common::ChrMemory;
use crate::cartridge::mapper::{Mapper, MapperCapabilities};

/// Mapper 053 - Supervision 16-in-1
///
/// Hardware: Supervision 16-in-1 PCB
///
/// Specifications:
/// - Main: <https://www.nesdev.org/wiki/INES_Mapper_053>
/// - Reference: FCEUX `supervision.cpp`
/// - PRG-ROM: variable (up to 2 MiB)
/// - CHR: 8 KiB fixed RAM bank
/// - Mirroring: Programmable (H/V)
///
/// Registers:
/// - cmd0 ($6000-$7FFF, write): [..MM NLLL]
///   - LLL (bits 2:0): outer block selector (A18..A16 of PRG)
///   - N (bit 3): outer block bit 3 (A19)
///   - M (bits 5:4): mode/outer bits (unused for A20..A19 on standard boards)
///   - bit 5 = mirroring (0=Vert, 1=Horz)
///   - bit 4 = lock/select: 0=menu/selectable, 1=game running (locks $6000 writes)
///     (once bit 4 is set, $6000 writes are blocked)
///
/// - cmd1 ($8000-$FFFF, write): [.... .BBB]
///   - BBB (bits 2:0): inner bank selector
///
/// PRG banking (16KB granularity):
///   $6000-$7FFF: 8KB bank = ((cmd0 & 0x0F) << 4 | 0x0F) + 4
///   cmd0 bit4=0 (menu): $8000-$FFFF = 32KB block 0
///   cmd0 bit4=1 (game): $8000-$BFFF = 16KB bank ((cmd0 & 0x0F) << 3 | (cmd1 & 7)) + 2
///                       $C000-$FFFF = 16KB bank ((cmd0 & 0x0F) << 3 | 7) + 2
///
/// CHR: Fixed 8KB bank 0 (CHR-RAM).
pub struct Mapper53 {
    prg_rom: Vec<u8>,
    chr_memory: ChrMemory,
    cmd0: u8,
    cmd1: u8,
}

impl Mapper53 {
    const MAPPER_NUMBER: u8 = 53;
    const PRG_8K_SIZE: usize = 0x2000;
    const PRG_8K_MASK: usize = Self::PRG_8K_SIZE - 1;
    const PRG_16K_SIZE: usize = 0x4000;
    const PRG_16K_MASK: usize = Self::PRG_16K_SIZE - 1;
    const PRG_32K_SIZE: usize = 0x8000;
    const PRG_32K_MASK: usize = Self::PRG_32K_SIZE - 1;

    pub fn new(prg_rom: Vec<u8>, chr_rom: Vec<u8>, _mirroring: NametableLayout) -> Self {
        Self {
            prg_rom,
            chr_memory: ChrMemory::new_ram(if chr_rom.is_empty() {
                8192
            } else {
                chr_rom.len()
            }),
            cmd0: 0,
            cmd1: 0,
        }
    }

    fn num_prg_8k_banks(&self) -> usize {
        self.prg_rom.len() / Self::PRG_8K_SIZE
    }

    fn num_prg_16k_banks(&self) -> usize {
        self.prg_rom.len() / Self::PRG_16K_SIZE
    }

    fn num_prg_32k_banks(&self) -> usize {
        self.prg_rom.len() / Self::PRG_32K_SIZE
    }

    fn read_prg_8k_bank(&self, bank: usize, offset: usize) -> u8 {
        let count = self.num_prg_8k_banks();
        if count == 0 {
            return 0;
        }
        let safe_bank = bank % count;
        self.prg_rom
            .get(safe_bank * Self::PRG_8K_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }

    fn read_prg_16k_bank(&self, bank: usize, offset: usize) -> u8 {
        let count = self.num_prg_16k_banks();
        if count == 0 {
            return 0;
        }
        let safe_bank = bank % count;
        self.prg_rom
            .get(safe_bank * Self::PRG_16K_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }

    fn read_prg_32k_bank(&self, bank: usize, offset: usize) -> u8 {
        let count = self.num_prg_32k_banks();
        if count == 0 {
            return 0;
        }
        let safe_bank = bank % count;
        self.prg_rom
            .get(safe_bank * Self::PRG_32K_SIZE + offset)
            .copied()
            .unwrap_or(0)
    }

    /// $6000 bank (8KB): ((cmd0 & 0x0F) << 4 | 0x0F) + 4
    fn prg_6000_bank(&self) -> usize {
        ((((self.cmd0 & 0x0F) as usize) << 4) | 0x0F).wrapping_add(4)
    }

    fn game_selected(&self) -> bool {
        (self.cmd0 & 0x10) != 0
    }
}

impl Mapper for Mapper53 {
    fn read_prg(&self, addr: u16) -> u8 {
        match addr {
            0x6000..=0x7FFF => {
                let offset = (addr as usize) & Self::PRG_8K_MASK;
                self.read_prg_8k_bank(self.prg_6000_bank(), offset)
            }
            0x8000..=0xFFFF => {
                if self.game_selected() {
                    // Game running: $8000-$BFFF = selectable, $C000-$FFFF = fixed last in block
                    let outer = (self.cmd0 & 0x0F) as usize;
                    if addr < 0xC000 {
                        let bank = (outer << 3 | (self.cmd1 & 7) as usize).wrapping_add(2);
                        let offset = (addr as usize) & Self::PRG_16K_MASK;
                        self.read_prg_16k_bank(bank, offset)
                    } else {
                        let bank = (outer << 3 | 7).wrapping_add(2);
                        let offset = (addr as usize) & Self::PRG_16K_MASK;
                        self.read_prg_16k_bank(bank, offset)
                    }
                } else {
                    // Menu: 32KB bank 0
                    let offset = (addr as usize) & Self::PRG_32K_MASK;
                    self.read_prg_32k_bank(0, offset)
                }
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        match addr {
            0x6000..=0x7FFF => {
                // Only writable when bit 4 is NOT set (menu mode, not game running)
                if !self.game_selected() {
                    self.cmd0 = value;
                }
            }
            0x8000..=0xFFFF => {
                self.cmd1 = value;
            }
            _ => {}
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        self.chr_memory.read(addr)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        self.chr_memory.write(addr, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        if (self.cmd0 & 0x20) != 0 {
            NametableLayout::Horizontal
        } else {
            NametableLayout::Vertical
        }
    }

    fn mapper_number(&self) -> u8 {
        Self::MAPPER_NUMBER
    }

    fn wram_size(&self) -> usize {
        0
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn initialize_ram(&mut self, mode: crate::console::RamInitMode) {
        self.chr_memory.initialize(mode);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.cmd0, self.cmd1]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if data.len() >= 2 {
            self.cmd0 = data[0];
            self.cmd1 = data[1];
        }
    }

    fn reset(&mut self) {
        self.cmd0 = 0;
        self.cmd1 = 0;
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_dynamic_mirroring: true,
            prg_bank_size_kb: 16,
            chr_bank_size_kb: 8,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cartridge::mapper::{MapperContext, create_mapper};
    use crate::cartridge::test_helpers::banked_data;

    fn make_mapper() -> Mapper53 {
        // 128 banks of 8KB = 1MB
        let prg = banked_data(8 * 1024, 128);
        Mapper53::new(prg, vec![], NametableLayout::Vertical)
    }

    #[test]
    fn mapper_53_is_registered() {
        let result = create_mapper(MapperContext::new_for_test(
            53,
            banked_data(8 * 1024, 128),
            vec![],
            NametableLayout::Vertical,
        ));
        assert!(result.is_ok(), "Mapper 53 must be registered");
    }

    #[test]
    fn default_menu_mode_maps_32k_bank0() {
        let mapper = make_mapper();
        // cmd0=0, cmd1=0: menu mode → 32KB bank 0
        // banked_data fills 8KB banks with their index, so within 32KB bank 0:
        //   $8000 → 8KB bank 0 → fill value 0
        //   $A000 → 8KB bank 1 → fill value 1
        //   $C000 → 8KB bank 2 → fill value 2
        //   $E000 → 8KB bank 3 → fill value 3
        assert_eq!(
            mapper.read_prg(0x8000),
            0,
            "$8000 in 32KB bank 0 (8KB sub-bank 0)"
        );
        assert_eq!(
            mapper.read_prg(0xC000),
            2,
            "$C000 in 32KB bank 0 (8KB sub-bank 2)"
        );
    }

    #[test]
    fn game_selected_enables_inner_banking() {
        let mut mapper = make_mapper();
        // Set game mode: cmd0 bit4=1, outer block=0
        mapper.write_prg(0x6000, 0x10); // cmd0: bit4=1, outer=0
        mapper.write_prg(0x8000, 3); // cmd1: inner bank 3
        // $8000-$BFFF: 16KB bank = (0 << 3 | 3) + 2 = 5
        // $C000-$FFFF: 16KB bank = (0 << 3 | 7) + 2 = 9
        assert_eq!(mapper.read_prg(0x8000), 5 * 2); // bank 5 in 16KB = page 10 in 8KB × 2
        // Actually: 16KB bank 5 = 8KB pages 10 and 11
        // Our mapper reads 16KB banks directly, so bank=5, offset=0 → 8KB page 10 marker
        // Hmm: banked_data(8KB, 128) fills bank N with value N
        // 16KB bank 5 = 8KB pages 10-11 (bytes 5*16384 to 5*16384+16383)
        // At offset 0 of 16KB bank 5, we're in 8KB page 10 → value 10
        // But our read uses num_prg_16k_banks = 64, and bank 5 → OK
        // Actually banked_data fills each 8KB bank with its index
        // 16KB bank 5 = bytes [5*16384 ... 6*16384-1]
        // 8KB page 10 = bytes [10*8192 ... 11*8192-1]
        // 5*16384 = 81920, 10*8192 = 81920 ✓
        // So offset 0 of 16KB bank 5 → 8KB page 10 → value 10
        // But our banked_data uses bank_size=8192 and fills each 8KB bank with index
        // So read_prg_16k_bank(5, 0) reads prg_rom[5*16384] = prg_rom[81920] = 10
    }

    #[test]
    fn game_selection_locks_outer_register() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x12); // cmd0: bit4=1, outer=2
        // Now try to change it:
        mapper.write_prg(0x6000, 0x05); // should be blocked
        assert_eq!(
            mapper.cmd0, 0x12,
            "$6000 write must be blocked once bit4 is set"
        );
    }

    #[test]
    fn mirroring_from_cmd0_bit5() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x00);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Vertical);
        mapper.write_prg(0x6000, 0x20);
        assert_eq!(mapper.get_mirroring(), NametableLayout::Horizontal);
    }

    #[test]
    fn chr_is_ram() {
        let mut mapper = make_mapper();
        mapper.write_chr(0x0000, 0xAB);
        assert_eq!(mapper.read_chr(0x0000), 0xAB);
    }

    #[test]
    fn snapshot_restore() {
        let mut mapper = make_mapper();
        mapper.write_prg(0x6000, 0x13);
        mapper.write_prg(0x8000, 5);
        let snap = mapper.registers_snapshot();
        let mut r = make_mapper();
        r.restore_registers(&snap);
        assert_eq!(r.cmd0, mapper.cmd0);
        assert_eq!(r.cmd1, mapper.cmd1);
    }
}
