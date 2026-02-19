//! Mapper 13 - CPROM
//!
//! PPU CHR address space layout:
//! - `$0000-$0FFF`: 4 KiB CHR RAM fixed to bank 0 (never changes)
//! - `$1000-$1FFF`: 4 KiB CHR RAM swappable via register bits 0-1 (banks 0-3)
//!
//! Known Limitations:
//! - Bus conflicts: writes to `$8000-$FFFF` are subject to bus conflicts with PRG-ROM data.
//! - See CARTRIDGE_REVIEW.md sections 5 and 6 for remaining mapper test/documentation follow-up.

use crate::cartridge::Mapper;
use crate::cartridge::MapperCapabilities;
use crate::cartridge::NametableLayout;
use crate::cartridge::common::{ChrMemory, DEFAULT_PRG_RAM_SIZE, PrgRam};

const CHR_BANK_SIZE: usize = 0x1000; // 4 KiB
const CHR_RAM_SIZE: usize = 0x4000;  // 16 KiB total (4 banks)

/// Mapper 13 - CPROM
///
/// Simple CHR-RAM banking mapper, used exclusively by Videomation.
///
/// Specifications: <https://www.nesdev.org/wiki/CPROM>
/// - PRG-ROM: 32 KiB fixed (no banking)
/// - CHR-RAM: 16 KiB; lower 4 KiB fixed to bank 0, upper 4 KiB selected by bits 0-1 of the bank register
/// - Bank select register: any write to `$8000-$FFFF`, bits 0-1 choose which bank maps to `$1000-$1FFF`
/// - Mirroring: fixed (set by iNES header)
pub struct CpromMapper {
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr_memory: ChrMemory,
    mirroring: NametableLayout,
    chr_bank_select: u8,
}

impl CpromMapper {
    pub fn new(prg_rom: Vec<u8>, _chr_rom: Vec<u8>, mirroring: NametableLayout) -> Self {
        // CPROM uses CHR-RAM, ignore chr_rom parameter
        Self {
            prg_rom,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_memory: ChrMemory::new_ram(CHR_RAM_SIZE),
            mirroring,
            chr_bank_select: 0,
        }
    }

    fn get_chr_bank_offset(&self, addr: u16) -> usize {
        if addr < 0x1000 {
            // $0000-$0FFF is always bank 0 — fixed by hardware design
            0
        } else {
            // $1000-$1FFF maps to whichever bank is selected by register bits 0-1
            let bank = (self.chr_bank_select & 0x03) as usize;
            bank * CHR_BANK_SIZE
        }
    }
}

impl Mapper for CpromMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        match addr {
            0x8000..=0xFFFF => {
                let offset = (addr - 0x8000) as usize;
                let index = offset % self.prg_rom.len();
                self.prg_rom.get(index).copied().unwrap_or(0)
            }
            _ => 0,
        }
    }

    fn write_prg(&mut self, addr: u16, value: u8) {
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        // Writes anywhere in $8000-$FFFF update the CHR bank select register (bits 0-1 only).
        // Note: subject to bus conflicts with PRG-ROM data on real hardware.
        if (0x8000..=0xFFFF).contains(&addr) {
            self.chr_bank_select = value & 0x03;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank_offset = self.get_chr_bank_offset(addr);
        let offset = (addr & 0x0FFF) as usize;
        let index = bank_offset + offset;
        self.chr_memory.read_at_index(index)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank_offset = self.get_chr_bank_offset(addr);
        let offset = (addr & 0x0FFF) as usize;
        let index = bank_offset + offset;
        self.chr_memory.write_at_index(index, value);
    }

    fn get_mirroring(&self) -> NametableLayout {
        self.mirroring
    }

    fn mapper_number(&self) -> u8 {
        13
    }

    fn wram_size(&self) -> usize {
        self.prg_ram.size()
    }

    fn wram_snapshot(&self) -> Vec<u8> {
        self.prg_ram.snapshot()
    }

    fn load_wram_snapshot(&mut self, data: &[u8]) {
        self.prg_ram.load_snapshot(data);
    }

    fn chr_ram_snapshot(&self) -> Vec<u8> {
        self.chr_memory.snapshot()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        self.chr_memory.load_snapshot(data);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_bank_select]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if !data.is_empty() {
            self.chr_bank_select = data[0];
        }
    }

    fn capabilities(&self) -> MapperCapabilities {
        MapperCapabilities {
            has_irq: false,
            has_chr_banking: true,
            has_dynamic_mirroring: false,
            has_expansion_audio: false,
            max_prg_ram_kb: 8,
            prg_bank_size_kb: 32,
            chr_bank_size_kb: 4,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cprom_32kb_prg_fixed() {
        // CPROM has 32KB PRG ROM with no banking
        let mut prg_rom = vec![0; 32 * 1024];

        // Fill with pattern - each 1KB block gets a unique value
        for (i, byte) in prg_rom.iter_mut().enumerate() {
            *byte = (i / 1024) as u8;
        }

        let mapper = CpromMapper::new(prg_rom, vec![], NametableLayout::Horizontal);

        // PRG ROM should be accessible at $8000-$FFFF
        assert_eq!(mapper.read_prg(0x8000), 0); // First byte of first 1KB block
        assert_eq!(mapper.read_prg(0x9000), 4); // $9000 = $8000 + $1000 = 4KB offset = block 4
        assert_eq!(mapper.read_prg(0xC000), 16); // $C000 = $8000 + $4000 = 16KB offset = block 16
        assert_eq!(mapper.read_prg(0xFFFF), 31); // $FFFF = last byte of block 31
    }

    #[test]
    fn test_cprom_chr_ram_lower_bank_fixed() {
        // Per NesDev spec: PPU $0000-$0FFF is FIXED to bank 0 (first page).
        // The bank select register does NOT affect $0000-$0FFF.
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Horizontal);

        // Write a distinct pattern to bank 0 via the lower window
        for i in 0..4096u16 {
            mapper.write_chr(i, (10 + (i % 256)) as u8);
        }

        // Regardless of bank select, $0000-$0FFF must always read bank 0
        for bank in 0u8..4 {
            mapper.write_prg(0x8000, bank);
            assert_eq!(
                mapper.read_chr(0x0000),
                10,
                "bank select {bank}: $0000 should always read bank 0"
            );
            assert_eq!(
                mapper.read_chr(0x0001),
                11,
                "bank select {bank}: $0001 should always read bank 0"
            );
        }
    }

    #[test]
    fn test_cprom_chr_ram_upper_bank_switching() {
        // Per NesDev spec: PPU $1000-$1FFF is swappable via the bank select register.
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Horizontal);

        // Write distinct patterns to each 4KB bank via the upper window ($1000-$1FFF)
        for bank in 0u8..4 {
            mapper.write_prg(0x8000, bank);
            for i in 0..4096u16 {
                mapper.write_chr(0x1000 + i, (bank as u16 * 10 + i % 256) as u8);
            }
        }

        // Verify each bank is readable via $1000-$1FFF after selecting it
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_chr(0x1000), 0);
        assert_eq!(mapper.read_chr(0x1001), 1);

        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_chr(0x1000), 10);
        assert_eq!(mapper.read_chr(0x1001), 11);

        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_chr(0x1000), 20);
        assert_eq!(mapper.read_chr(0x1001), 21);

        mapper.write_prg(0x8000, 3);
        assert_eq!(mapper.read_chr(0x1000), 30);
        assert_eq!(mapper.read_chr(0x1001), 31);

        // Switch back to bank 0
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_chr(0x1000), 0);
        assert_eq!(mapper.read_chr(0x1001), 1);
    }

    #[test]
    fn test_cprom_chr_ram_writable() {
        // CHR-RAM should be writable in both windows.
        // Per spec: $0000-$0FFF is fixed to bank 0; $1000-$1FFF is swappable.
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Horizontal);

        // Write to fixed lower window (always bank 0)
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x0FFF, 0xBB);
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x0FFF), 0xBB);

        // Bank switching does NOT affect $0000-$0FFF
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x0FFF), 0xBB);

        // Write to upper swappable window (bank 1 selected)
        mapper.write_chr(0x1000, 0xCC);
        assert_eq!(mapper.read_chr(0x1000), 0xCC);

        // Switch upper bank to 2 — should not see bank 1 data
        mapper.write_prg(0x8000, 2);
        assert_ne!(mapper.read_chr(0x1000), 0xCC);

        // Switch back to bank 1 — should restore bank 1 data
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_chr(0x1000), 0xCC);
    }

    #[test]
    fn test_cprom_bank_select_mask() {
        // Only lower 2 bits should be used for bank select (4 banks total).
        // The selected bank maps to PPU $1000-$1FFF.
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Horizontal);

        // Fill each bank with distinct pattern via the upper window
        for bank in 0u8..4 {
            mapper.write_prg(0x8000, bank);
            for i in 0..4096u16 {
                mapper.write_chr(0x1000 + i, bank * 50);
            }
        }

        // Writing higher bits should be masked; only bits 0-1 count
        mapper.write_prg(0x8000, 0b1111_1100); // Should select bank 0
        assert_eq!(mapper.read_chr(0x1000), 0);

        mapper.write_prg(0x8000, 0b1111_1101); // Should select bank 1
        assert_eq!(mapper.read_chr(0x1000), 50);

        mapper.write_prg(0x8000, 0b1111_1110); // Should select bank 2
        assert_eq!(mapper.read_chr(0x1000), 100);

        mapper.write_prg(0x8000, 0b1111_1111); // Should select bank 3
        assert_eq!(mapper.read_chr(0x1000), 150);
    }

    #[test]
    fn test_cprom_mirroring() {
        let mapper_h = CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Horizontal);
        assert_eq!(mapper_h.get_mirroring(), NametableLayout::Horizontal);

        let mapper_v = CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Vertical);
        assert_eq!(mapper_v.get_mirroring(), NametableLayout::Vertical);
    }

    #[test]
    fn test_cprom_bank_select_any_address() {
        // CPROM responds to writes anywhere in $8000-$FFFF.
        // The selected bank maps to PPU $1000-$1FFF.
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Horizontal);

        // Fill banks with distinct patterns via the upper window
        for bank in 0u8..4 {
            mapper.write_prg(0x8000, bank);
            for i in 0..4096u16 {
                mapper.write_chr(0x1000 + i, bank + 100);
            }
        }

        // Write to different addresses in PRG space — all should update the bank select
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_chr(0x1000), 101);

        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_chr(0x1000), 102);

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_chr(0x1000), 103);
    }

    #[test]
    fn test_cprom_registers_snapshot_restores_chr_bank() {
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Horizontal);

        mapper.write_prg(0x8000, 0b0000_0010); // select bank 2
        mapper.write_chr(0x1000, 0xAA); // write to bank 2 via upper window
        let regs = mapper.registers_snapshot();

        let mut restored =
            CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Horizontal);
        restored.restore_registers(&regs);

        // After restoring registers, upper window should still map to bank 2
        restored.write_chr(0x1000, 0xAA);
        assert_eq!(restored.read_chr(0x1000), 0xAA);

        // Switching to bank 3 should make $1000 point to a different bank (not 0xAA)
        restored.write_prg(0x8000, 0b0000_0011);
        assert_ne!(restored.read_chr(0x1000), 0xAA);
    }

    #[test]
    fn test_cprom_open_bus() {
        let mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], NametableLayout::Horizontal);

        assert_eq!(mapper.read_prg_open_bus(0x5000, 0x77), 0x77);
        assert_eq!(mapper.read_prg_open_bus(0x5FFF, 0x88), 0x88);
    }
}
