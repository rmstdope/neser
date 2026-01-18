use crate::cartridge::Mapper;
use crate::cartridge::MirroringMode;
use crate::cartridge::common::{DEFAULT_PRG_RAM_SIZE, PrgRam};

// Memory size constants
const CHR_BANK_SIZE: usize = 0x1000; // 4KB
const CHR_RAM_SIZE: usize = 0x4000; // 16KB

/// CPROM mapper (Mapper 13)
///
/// Simple CHR-RAM bank switching mapper.
/// Supports:
/// - 32KB fixed PRG ROM (no PRG banking)
/// - 8KB PRG-RAM at $6000-$7FFF
/// - 16KB CHR-RAM with 4KB bank switching
/// - Bank select via writes to $8000-$FFFF
/// - Fixed nametable mirroring
///
/// Known games: Videomation (only commercial game using this mapper)
pub struct CpromMapper {
    prg_rom: Vec<u8>,
    prg_ram: PrgRam,
    chr_ram: Vec<u8>,
    mirroring: MirroringMode,
    chr_bank_select: u8,
}

impl CpromMapper {
    pub fn new(prg_rom: Vec<u8>, _chr_rom: Vec<u8>, mirroring: MirroringMode) -> Self {
        // CPROM uses CHR-RAM, ignore chr_rom parameter
        Self {
            prg_rom,
            prg_ram: PrgRam::new(DEFAULT_PRG_RAM_SIZE),
            chr_ram: vec![0; CHR_RAM_SIZE],
            mirroring,
            chr_bank_select: 0,
        }
    }

    fn get_chr_bank_offset(&self, addr: u16) -> usize {
        // CHR address space is $0000-$1FFF (8KB)
        // Lower 4KB ($0000-$0FFF) uses selected bank
        // Upper 4KB ($1000-$1FFF) is fixed to bank 3
        if addr < 0x1000 {
            // Lower 4KB: switchable bank (0-3)
            let bank = (self.chr_bank_select & 0x03) as usize;
            bank * CHR_BANK_SIZE
        } else {
            // Upper 4KB: fixed to bank 3
            3 * CHR_BANK_SIZE
        }
    }
}

impl Mapper for CpromMapper {
    fn read_prg(&self, addr: u16) -> u8 {
        // PRG-RAM at $6000-$7FFF
        if let Some(value) = self.prg_ram.try_read(addr) {
            return value;
        }

        // PRG ROM is fixed at $8000-$FFFF (32KB)
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
        // PRG-RAM at $6000-$7FFF
        if self.prg_ram.try_write(addr, value) {
            return;
        }

        // Any write to $8000-$FFFF sets the CHR bank select (lower 2 bits)
        if (0x8000..=0xFFFF).contains(&addr) {
            self.chr_bank_select = value & 0x03;
        }
    }

    fn read_chr(&self, addr: u16) -> u8 {
        let bank_offset = self.get_chr_bank_offset(addr);
        let offset = (addr & 0x0FFF) as usize;
        let index = bank_offset + offset;
        self.chr_ram.get(index).copied().unwrap_or(0)
    }

    fn write_chr(&mut self, addr: u16, value: u8) {
        let bank_offset = self.get_chr_bank_offset(addr);
        let offset = (addr & 0x0FFF) as usize;
        let index = bank_offset + offset;
        if index < self.chr_ram.len() {
            self.chr_ram[index] = value;
        }
    }

    fn ppu_address_changed(&mut self, _addr: u16) {
        // CPROM doesn't care about PPU address changes (no IRQ)
    }

    fn get_mirroring(&self) -> MirroringMode {
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
        self.chr_ram.clone()
    }

    fn restore_chr_ram(&mut self, data: &[u8]) {
        let to_copy = data.len().min(self.chr_ram.len());
        self.chr_ram[..to_copy].copy_from_slice(&data[..to_copy]);
    }

    fn registers_snapshot(&self) -> Vec<u8> {
        vec![self.chr_bank_select]
    }

    fn restore_registers(&mut self, data: &[u8]) {
        if !data.is_empty() {
            self.chr_bank_select = data[0];
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

        let mapper = CpromMapper::new(prg_rom, vec![], MirroringMode::Horizontal);

        // PRG ROM should be accessible at $8000-$FFFF
        assert_eq!(mapper.read_prg(0x8000), 0); // First byte of first 1KB block
        assert_eq!(mapper.read_prg(0x9000), 4); // $9000 = $8000 + $1000 = 4KB offset = block 4
        assert_eq!(mapper.read_prg(0xC000), 16); // $C000 = $8000 + $4000 = 16KB offset = block 16
        assert_eq!(mapper.read_prg(0xFFFF), 31); // $FFFF = last byte of block 31
    }

    #[test]
    fn test_cprom_chr_ram_lower_bank_switching() {
        // CPROM has 16KB CHR-RAM with 4KB bank switching in lower half
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Horizontal);

        // Write distinct patterns to each 4KB bank in CHR-RAM
        for bank in 0..4 {
            for i in 0..4096 {
                let addr = bank * 0x1000 + i;
                mapper.chr_ram[addr] = (bank * 10 + i % 256) as u8;
            }
        }

        // Initially bank 0 should be at $0000-$0FFF
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x0001), 1);

        // Switch to bank 1
        mapper.write_prg(0x8000, 0b0000_0001);
        assert_eq!(mapper.read_chr(0x0000), 10);
        assert_eq!(mapper.read_chr(0x0001), 11);

        // Switch to bank 2
        mapper.write_prg(0x8000, 0b0000_0010);
        assert_eq!(mapper.read_chr(0x0000), 20);
        assert_eq!(mapper.read_chr(0x0001), 21);

        // Switch to bank 3
        mapper.write_prg(0x8000, 0b0000_0011);
        assert_eq!(mapper.read_chr(0x0000), 30);
        assert_eq!(mapper.read_chr(0x0001), 31);

        // Switch back to bank 0
        mapper.write_prg(0x8000, 0b0000_0000);
        assert_eq!(mapper.read_chr(0x0000), 0);
        assert_eq!(mapper.read_chr(0x0001), 1);
    }

    #[test]
    fn test_cprom_chr_ram_upper_bank_fixed() {
        // Upper 4KB should always be fixed to bank 3
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Horizontal);

        // Fill bank 3 with a distinct pattern
        for i in 0..4096 {
            mapper.chr_ram[3 * 0x1000 + i] = (100 + i % 256) as u8;
        }

        // Verify upper 4KB reads bank 3 regardless of bank select
        assert_eq!(mapper.read_chr(0x1000), 100);
        assert_eq!(mapper.read_chr(0x1001), 101);

        // Switch lower bank to 0
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_chr(0x1000), 100);
        assert_eq!(mapper.read_chr(0x1001), 101);

        // Switch lower bank to 1
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_chr(0x1000), 100);
        assert_eq!(mapper.read_chr(0x1001), 101);

        // Switch lower bank to 2
        mapper.write_prg(0x8000, 2);
        assert_eq!(mapper.read_chr(0x1000), 100);
        assert_eq!(mapper.read_chr(0x1001), 101);
    }

    #[test]
    fn test_cprom_chr_ram_writable() {
        // CHR-RAM should be writable
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Horizontal);

        // Write to lower bank (initially bank 0)
        mapper.write_chr(0x0000, 0xAA);
        mapper.write_chr(0x0FFF, 0xBB);
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x0FFF), 0xBB);

        // Switch to bank 1 and write
        mapper.write_prg(0x8000, 1);
        mapper.write_chr(0x0000, 0xCC);
        assert_eq!(mapper.read_chr(0x0000), 0xCC);

        // Switch back to bank 0 - should still have old values
        mapper.write_prg(0x8000, 0);
        assert_eq!(mapper.read_chr(0x0000), 0xAA);
        assert_eq!(mapper.read_chr(0x0FFF), 0xBB);

        // Write to upper bank (fixed to bank 3)
        mapper.write_chr(0x1000, 0xDD);
        mapper.write_chr(0x1FFF, 0xEE);
        assert_eq!(mapper.read_chr(0x1000), 0xDD);
        assert_eq!(mapper.read_chr(0x1FFF), 0xEE);
    }

    #[test]
    fn test_cprom_bank_select_mask() {
        // Only lower 2 bits should be used for bank select (4 banks total)
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Horizontal);

        // Fill each bank with distinct pattern
        for bank in 0..4 {
            for i in 0..4096 {
                mapper.chr_ram[bank * 0x1000 + i] = (bank * 50) as u8;
            }
        }

        // Writing higher bits should be masked
        mapper.write_prg(0x8000, 0b1111_1100); // Should select bank 0
        assert_eq!(mapper.read_chr(0x0000), 0);

        mapper.write_prg(0x8000, 0b1111_1101); // Should select bank 1
        assert_eq!(mapper.read_chr(0x0000), 50);

        mapper.write_prg(0x8000, 0b1111_1110); // Should select bank 2
        assert_eq!(mapper.read_chr(0x0000), 100);

        mapper.write_prg(0x8000, 0b1111_1111); // Should select bank 3
        assert_eq!(mapper.read_chr(0x0000), 150);
    }

    #[test]
    fn test_cprom_mirroring() {
        let mapper_h = CpromMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Horizontal);
        assert_eq!(mapper_h.get_mirroring(), MirroringMode::Horizontal);

        let mapper_v = CpromMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Vertical);
        assert_eq!(mapper_v.get_mirroring(), MirroringMode::Vertical);
    }

    #[test]
    fn test_cprom_bank_select_any_address() {
        // CPROM responds to writes anywhere in $8000-$FFFF
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Horizontal);

        // Fill banks with distinct patterns
        for bank in 0..4 {
            for i in 0..4096 {
                mapper.chr_ram[bank * 0x1000 + i] = (bank + 100) as u8;
            }
        }

        // Write to different addresses in PRG space
        mapper.write_prg(0x8000, 1);
        assert_eq!(mapper.read_chr(0x0000), 101);

        mapper.write_prg(0xA000, 2);
        assert_eq!(mapper.read_chr(0x0000), 102);

        mapper.write_prg(0xFFFF, 3);
        assert_eq!(mapper.read_chr(0x0000), 103);
    }

    #[test]
    fn test_cprom_registers_snapshot_restores_chr_bank() {
        let mut mapper = CpromMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Horizontal);

        mapper.write_prg(0x8000, 0b0000_0010); // select bank 2
        let regs = mapper.registers_snapshot();

        let mut restored = CpromMapper::new(vec![0; 32 * 1024], vec![], MirroringMode::Horizontal);
        restored.restore_registers(&regs);

        restored.write_chr(0x0000, 0xAA);
        assert_eq!(restored.read_chr(0x0000), 0xAA);

        restored.write_prg(0x8000, 0b0000_0011); // switch to bank 3
        assert_ne!(restored.read_chr(0x0000), 0xAA);
    }
}
